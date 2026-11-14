use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use pyrosync_core::calendar::build_calendar;
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::dedup::remove_interscan_duplicates_indexed_with;
use pyrosync_core::effort::estimate_effort;
use pyrosync_core::fleet::{
    consolidate_modis_fleet_indexed_with, consolidate_viirs_fleet_indexed_with,
};
use pyrosync_core::harmonize::harmonize_with_config;
use pyrosync_core::models::{CalendarCell, HarmonizedHotspot, HotspotSource, Sensor};
use pyrosync_firms::archive::ArchiveClient;
use pyrosync_firms::parse::retain_wildfires;
use tracing_subscriber::EnvFilter;

const ARCHIVE_FETCH_SENSORS: [Sensor; 3] =
    [Sensor::ModisTerra, Sensor::ViirsSNPP, Sensor::ViirsNOAA20];
const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";

struct Args {
    country: String,
    from: i32,
    to: i32,
}

struct LoadStat {
    sensor: Sensor,
    year: i32,
    detections: usize,
    elapsed_ms: f64,
}

struct Fnv1a64(u64);

impl Fnv1a64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;

    fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn write_u64(&mut self, v: u64) {
        self.write_bytes(&v.to_le_bytes());
    }

    fn write_i64(&mut self, v: i64) {
        self.write_bytes(&v.to_le_bytes());
    }

    fn write_u32(&mut self, v: u32) {
        self.write_bytes(&v.to_le_bytes());
    }

    fn write_u16(&mut self, v: u16) {
        self.write_bytes(&v.to_le_bytes());
    }

    fn write_f64(&mut self, v: f64) {
        self.write_u64(v.to_bits());
    }

    fn write_opt_f64(&mut self, v: Option<f64>) {
        match v {
            None => self.write_bytes(&[0]),
            Some(x) => {
                self.write_bytes(&[1]);
                self.write_f64(x);
            }
        }
    }

    fn write_debug<T: std::fmt::Debug>(&mut self, v: &T) {
        let s = format!("{v:?}");
        self.write_u64(s.len() as u64);
        self.write_bytes(s.as_bytes());
    }

    fn finish(self) -> u64 {
        self.0
    }
}

fn fingerprint_harmonized(hotspots: &[HarmonizedHotspot]) -> u64 {
    let mut h = Fnv1a64::new();
    h.write_u64(hotspots.len() as u64);
    for item in hotspots {
        h.write_f64(item.lat);
        h.write_f64(item.lon);
        h.write_f64(item.frp);
        h.write_f64(item.frp_corrected);
        h.write_opt_f64(item.vza_factor);
        h.write_u16(item.n_obs);
        h.write_u64(item.platforms.len() as u64);
        for p in &item.platforms {
            h.write_debug(p);
        }
        h.write_debug(&item.confidence);
        h.write_debug(&item.source);
        h.write_debug(&item.daynight);
        h.write_i64(item.acquired.timestamp());
        h.write_u32(item.acquired.timestamp_subsec_nanos());
    }
    h.finish()
}

fn fingerprint_calendar(cells: &[CalendarCell]) -> u64 {
    let mut h = Fnv1a64::new();
    h.write_u64(cells.len() as u64);
    for cell in cells {
        h.write_debug(&cell.year);
        h.write_debug(&cell.month);
        h.write_debug(&cell.count);
        h.write_opt_f64(cell.count_normalized);
        h.write_opt_f64(cell.effort_omega);
        h.write_f64(cell.mean_frp);
        h.write_f64(cell.mean_frp_raw);
        h.write_opt_f64(cell.anomaly);
    }
    h.finish()
}

fn print_usage() {
    println!(
        "\
Usage: bench_harmonize <Country> <year> [--to <year>]

Measure each stage of the harmonization pipeline on offline archived FIRMS CSVs.
Archive directory: ARCHIVE_CACHE_DIR env var, or {DEFAULT_ARCHIVE_CACHE_DIR}."
    );
}

fn parse_args() -> Result<Option<Args>> {
    let mut raw_args = std::env::args().skip(1);
    let mut positionals: Vec<String> = Vec::new();
    let mut to_year: Option<i32> = None;

    while let Some(arg) = raw_args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return Ok(None);
            }
            "--to" => {
                let val = raw_args.next().context("missing value for --to")?;
                to_year = Some(
                    val.parse()
                        .with_context(|| format!("invalid --to year: {val}"))?,
                );
            }
            other if other.starts_with('-') => {
                bail!("unknown flag: {other} (try --help)");
            }
            other => positionals.push(other.to_string()),
        }
    }

    if positionals.len() != 2 {
        bail!("expected <Country> <year> [--to <year>] (try --help)");
    }

    let country = positionals.remove(0);
    let year_str = positionals.remove(0);
    let from: i32 = year_str
        .parse()
        .with_context(|| format!("invalid year: {year_str}"))?;
    let to = to_year.unwrap_or(from);

    if from > to {
        bail!("start year ({from}) must be <= end year ({to})");
    }

    Ok(Some(Args { country, from, to }))
}

fn archive_cache_dir() -> PathBuf {
    match std::env::var_os("ARCHIVE_CACHE_DIR") {
        Some(val) if !val.is_empty() => PathBuf::from(val),
        _ => PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let Some(args) = parse_args()? else {
        return Ok(());
    };

    let cache_dir = archive_cache_dir();
    let archive = ArchiveClient::offline(cache_dir.clone());

    let mut modis = Vec::new();
    let mut viirs = Vec::new();
    let mut load_stats = Vec::new();

    let load_total_start = Instant::now();
    for year in args.from..=args.to {
        for sensor in ARCHIVE_FETCH_SENSORS {
            if !archive.covers(sensor, year) {
                continue;
            }
            let t0 = Instant::now();
            match archive.fetch_country_year(sensor, &args.country, year).await {
                Ok(mut hotspots) => {
                    retain_wildfires(&mut hotspots);
                    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
                    let detections = hotspots.len();
                    load_stats.push(LoadStat {
                        sensor,
                        year,
                        detections,
                        elapsed_ms,
                    });
                    if sensor.is_modis() {
                        modis.append(&mut hotspots);
                    } else {
                        viirs.append(&mut hotspots);
                    }
                }
                Err(err) => {
                    eprintln!("warning: {sensor:?} {year} skipped: {err}");
                }
            }
        }
    }
    let load_total_ms = load_total_start.elapsed().as_secs_f64() * 1000.0;

    if modis.is_empty() && viirs.is_empty() {
        eprintln!(
            "error: no wildfire hotspot data loaded for {} ({}..={}) from {}",
            args.country,
            args.from,
            args.to,
            cache_dir.display()
        );
        std::process::exit(1);
    }

    let raw_modis = modis.len();
    let raw_viirs = viirs.len();
    let raw_total = raw_modis + raw_viirs;

    // Clone loaded inputs before running stage-by-stage breakdown so `harmonize_with_config`
    // executes on the exact same input without including clone overhead in its timing.
    let modis_full = modis.clone();
    let viirs_full = viirs.clone();

    let config = PipelineConfig::default();

    let t0 = Instant::now();
    let _ = remove_interscan_duplicates_indexed_with(&mut modis, &config.dedup);
    let dedup_modis_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let dedup_modis_len = modis.len();

    let t0 = Instant::now();
    let _ = remove_interscan_duplicates_indexed_with(&mut viirs, &config.dedup);
    let dedup_viirs_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let dedup_viirs_len = viirs.len();

    let t0 = Instant::now();
    let (modis_fleet, _) =
        consolidate_modis_fleet_indexed_with(&modis, &config.fleet, &config.vza);
    let fleet_modis_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let fleet_modis_len = modis_fleet.len();

    let t0 = Instant::now();
    let (viirs_fleet, _) =
        consolidate_viirs_fleet_indexed_with(&viirs, &config.fleet, &config.vza);
    let fleet_viirs_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let fleet_viirs_len = viirs_fleet.len();

    // Free stage intermediates before running full harmonization to avoid holding 2x peak memory.
    drop(modis_fleet);
    drop(viirs_fleet);
    drop(modis);
    drop(viirs);

    let t0 = Instant::now();
    let harmonized = harmonize_with_config(modis_full, viirs_full, &config);
    let harmonize_total_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let matching_derived_ms =
        harmonize_total_ms - (dedup_modis_ms + dedup_viirs_ms + fleet_modis_ms + fleet_viirs_ms);

    let t0 = Instant::now();
    let calendar = build_calendar(&harmonized);
    let calendar_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let t0 = Instant::now();
    let effort_factors = estimate_effort(&harmonized);
    let effort_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let (mut matched, mut modis_only, mut viirs_only) = (0usize, 0usize, 0usize);
    for h in &harmonized {
        match h.source {
            HotspotSource::Matched => matched += 1,
            HotspotSource::ModisOnly => modis_only += 1,
            HotspotSource::ViirsOnly => viirs_only += 1,
        }
    }

    let harmonized_fp = fingerprint_harmonized(&harmonized);
    let calendar_fp = fingerprint_calendar(&calendar);

    println!(
        "Benchmark: country={} years={}..={} archive={}",
        args.country,
        args.from,
        args.to,
        cache_dir.display()
    );
    println!("Input counts: modis={raw_modis} viirs={raw_viirs} total={raw_total}");

    println!("\n[1] Load (read + parse + retain_wildfires)");
    println!("{:<14} {:>6} {:>12} {:>12}", "sensor", "year", "detections", "time_ms");
    println!("{}", "-".repeat(47));
    for stat in &load_stats {
        println!(
            "{:<14} {:>6} {:>12} {:>12.3}",
            format!("{:?}", stat.sensor),
            stat.year,
            stat.detections,
            stat.elapsed_ms
        );
    }
    println!("{}", "-".repeat(47));
    println!("{:<21} {:>12} {:>12.3}", "load_total", raw_total, load_total_ms);

    println!("\n[2] Pipeline stages");
    println!("{:<24} {:>12} {:>14}", "stage", "time_ms", "output_len");
    println!("{}", "-".repeat(52));
    println!("{:<24} {:>12.3} {:>14}", "dedup_modis", dedup_modis_ms, dedup_modis_len);
    println!("{:<24} {:>12.3} {:>14}", "dedup_viirs", dedup_viirs_ms, dedup_viirs_len);
    println!("{:<24} {:>12.3} {:>14}", "fleet_modis", fleet_modis_ms, fleet_modis_len);
    println!("{:<24} {:>12.3} {:>14}", "fleet_viirs", fleet_viirs_ms, fleet_viirs_len);
    println!(
        "{:<24} {:>12.3} {:>14}",
        "matching (derived)",
        matching_derived_ms,
        harmonized.len()
    );
    println!(
        "{:<24} {:>12.3} {:>14}",
        "harmonize_total",
        harmonize_total_ms,
        harmonized.len()
    );
    println!("{:<24} {:>12.3} {:>14}", "calendar", calendar_ms, calendar.len());
    println!("{:<24} {:>12.3} {:>14}", "effort", effort_ms, effort_factors.len());

    println!("\n[3] Harmonized source breakdown");
    println!(
        "total={}  Matched={}  ModisOnly={}  ViirsOnly={}",
        harmonized.len(),
        matched,
        modis_only,
        viirs_only
    );

    println!("\n[4] Effort factors");
    println!("{:<44} {:>8} {:>12}", "platforms", "months", "omega");
    println!("{}", "-".repeat(66));
    for f in &effort_factors {
        let platforms = if f.platforms.is_empty() {
            "-".to_string()
        } else {
            f.platforms
                .iter()
                .map(|s| format!("{s:?}"))
                .collect::<Vec<_>>()
                .join("+")
        };
        let omega = match f.omega {
            Some(w) => format!("{w:.6}"),
            None => "None".to_string(),
        };
        println!("{:<44} {:>8} {:>12}", platforms, f.months, omega);
    }

    println!();
    println!("FINGERPRINT harmonized={harmonized_fp:016x} count={}", harmonized.len());
    println!("FINGERPRINT calendar={calendar_fp:016x} cells={}", calendar.len());

    Ok(())
}
