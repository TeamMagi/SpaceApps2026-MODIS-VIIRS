use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{Datelike, NaiveDate, Timelike};
use pyrosync_core::calibration::calibration_for;
use pyrosync_core::config::CalibrationConfig;
use pyrosync_core::dedup::remove_interscan_duplicates;
use pyrosync_core::models::{DayNight, Hotspot, Sensor};
use pyrosync_firms::archive::ArchiveClient;
use pyrosync_firms::parse::retain_wildfires;
use tracing_subscriber::EnvFilter;

const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const DEFAULT_MAX_DT_MIN: f64 = 20.0;

struct Args {
    country: String,
    year: i32,
    month: Option<u32>,
    lat_min: Option<f64>,
    lat_max: Option<f64>,
    max_dt_min: f64,
}

#[derive(Default)]
struct SensorCellStats {
    sum_frp: f64,
    count: u64,
    sum_sec_of_day: f64,
}

impl SensorCellStats {
    fn add(&mut self, h: &Hotspot) {
        self.sum_frp += h.frp;
        self.count += 1;
        let sec = f64::from(h.acquired.num_seconds_from_midnight())
            + f64::from(h.acquired.nanosecond()) * 1e-9;
        self.sum_sec_of_day += sec;
    }

    fn mean_sec(&self) -> f64 {
        self.sum_sec_of_day / self.count as f64
    }
}

#[derive(Default)]
struct BandAgg {
    n_cells: usize,
    sum_modis_frp: f64,
    sum_viirs_frp: f64,
    sum_modis_count: u64,
    sum_viirs_count: u64,
    cell_ratios: Vec<f64>,
    model_ratios: Vec<f64>,
}

type CellKey = (NaiveDate, i32, i32);

fn print_usage() {
    println!(
        "\
Usage: calibration_grid_check <Country> <year> [--month <1..=12>] [--lat-min <deg>] [--lat-max <deg>] [--max-dt-min <min>]

Validate Li et al. (2018) latitude-band calibration on 1x1 deg daily grid cells
comparing daytime, interscan-deduped raw FRP from Aqua MODIS vs S-NPP VIIRS.
Defaults: --max-dt-min {DEFAULT_MAX_DT_MIN}
Archive directory: ARCHIVE_CACHE_DIR env var, or {DEFAULT_ARCHIVE_CACHE_DIR}."
    );
}

fn parse_args() -> Result<Option<Args>> {
    let mut raw_args = std::env::args().skip(1);
    let mut positionals: Vec<String> = Vec::new();
    let mut month: Option<u32> = None;
    let mut lat_min: Option<f64> = None;
    let mut lat_max: Option<f64> = None;
    let mut max_dt_min: f64 = DEFAULT_MAX_DT_MIN;

    while let Some(arg) = raw_args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return Ok(None);
            }
            "--month" => {
                let val = raw_args.next().context("missing value for --month")?;
                let m: u32 = val
                    .parse()
                    .with_context(|| format!("invalid --month: {val}"))?;
                if !(1..=12).contains(&m) {
                    bail!("--month must be in 1..=12, got {m}");
                }
                month = Some(m);
            }
            "--lat-min" => {
                let val = raw_args.next().context("missing value for --lat-min")?;
                lat_min = Some(
                    val.parse()
                        .with_context(|| format!("invalid --lat-min: {val}"))?,
                );
            }
            "--lat-max" => {
                let val = raw_args.next().context("missing value for --lat-max")?;
                lat_max = Some(
                    val.parse()
                        .with_context(|| format!("invalid --lat-max: {val}"))?,
                );
            }
            "--max-dt-min" => {
                let val = raw_args.next().context("missing value for --max-dt-min")?;
                let dt: f64 = val
                    .parse()
                    .with_context(|| format!("invalid --max-dt-min: {val}"))?;
                if !dt.is_finite() || dt < 0.0 {
                    bail!("--max-dt-min must be finite and >= 0, got {dt}");
                }
                max_dt_min = dt;
            }
            other if other.starts_with('-') => {
                bail!("unknown flag: {other} (try --help)");
            }
            other => positionals.push(other.to_string()),
        }
    }

    if positionals.len() != 2 {
        bail!("expected <Country> <year> [options] (try --help)");
    }

    let country = positionals.remove(0);
    let year_str = positionals.remove(0);
    let year: i32 = year_str
        .parse()
        .with_context(|| format!("invalid year: {year_str}"))?;

    if let (Some(lo), Some(hi)) = (lat_min, lat_max)
        && lo > hi
    {
        bail!("--lat-min ({lo}) must be <= --lat-max ({hi})");
    }

    Ok(Some(Args {
        country,
        year,
        month,
        lat_min,
        lat_max,
        max_dt_min,
    }))
}

fn archive_cache_dir() -> PathBuf {
    match std::env::var_os("ARCHIVE_CACHE_DIR") {
        Some(val) if !val.is_empty() => PathBuf::from(val),
        _ => PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR),
    }
}

fn retain_filters(hotspots: &mut Vec<Hotspot>, expected_sensor: Sensor, args: &Args) {
    retain_wildfires(hotspots);
    hotspots.retain(|h| {
        if h.sensor != expected_sensor || h.daynight != DayNight::Day {
            return false;
        }
        if !h.lat.is_finite() || !h.lon.is_finite() || !h.frp.is_finite() || h.frp < 0.0 {
            return false;
        }
        if let Some(m) = args.month
            && h.acquired.month() != m
        {
            return false;
        }
        if let Some(min_lat) = args.lat_min
            && h.lat < min_lat
        {
            return false;
        }
        if let Some(max_lat) = args.lat_max
            && h.lat > max_lat
        {
            return false;
        }
        true
    });
}

async fn load_sensor(
    archive: &ArchiveClient,
    sensor: Sensor,
    args: &Args,
) -> Vec<Hotspot> {
    if !archive.covers(sensor, args.year) {
        eprintln!("warning: archive does not cover {sensor:?} {}", args.year);
        return Vec::new();
    }
    match archive
        .fetch_country_year(sensor, &args.country, args.year)
        .await
    {
        Ok(mut hotspots) => {
            retain_filters(&mut hotspots, sensor, args);
            hotspots
        }
        Err(err) => {
            eprintln!("warning: {sensor:?} {} skipped: {err}", args.year);
            Vec::new()
        }
    }
}

fn aggregate_cells(hotspots: &[Hotspot]) -> HashMap<CellKey, SensorCellStats> {
    let mut cells: HashMap<CellKey, SensorCellStats> = HashMap::new();
    for h in hotspots {
        let key = (
            h.acquired.date_naive(),
            h.lat.floor() as i32,
            h.lon.floor() as i32,
        );
        cells.entry(key).or_default().add(h);
    }
    cells
}

fn band_bucket(abs_lat: f64) -> usize {
    if abs_lat <= 30.0 {
        0
    } else if abs_lat <= 50.0 {
        1
    } else if abs_lat < 55.0 {
        2
    } else if abs_lat <= 70.0 {
        3
    } else {
        4
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    debug_assert!(!sorted.is_empty());
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = p * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    sorted[lo] + frac * (sorted[hi] - sorted[lo])
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

    // Li et al. (2018) compares nearly simultaneous afternoon orbits (~13:30 LT):
    // Aqua MODIS (filtered from the combined MODIS archive) vs S-NPP VIIRS.
    let mut aqua = load_sensor(&archive, Sensor::ModisAqua, &args).await;
    let mut snpp = load_sensor(&archive, Sensor::ViirsSNPP, &args).await;

    let aqua_pre_dedup = aqua.len();
    let snpp_pre_dedup = snpp.len();

    remove_interscan_duplicates(&mut aqua);
    remove_interscan_duplicates(&mut snpp);

    let aqua_cells = aggregate_cells(&aqua);
    let snpp_cells = aggregate_cells(&snpp);

    let max_dt_secs = args.max_dt_min * 60.0;
    let cal_cfg = CalibrationConfig::default();

    let mut bands: [BandAgg; 5] = Default::default();
    let mut overlap_any_dt = 0usize;
    let mut rejected_dt = 0usize;
    let mut skipped_nonpositive = 0usize;
    let mut common_cells = 0usize;

    for (key, m_stats) in &aqua_cells {
        let Some(v_stats) = snpp_cells.get(key) else {
            continue;
        };
        overlap_any_dt += 1;

        let dt_secs = (m_stats.mean_sec() - v_stats.mean_sec()).abs();
        if dt_secs > max_dt_secs {
            rejected_dt += 1;
            continue;
        }

        if m_stats.sum_frp <= 0.0 || v_stats.sum_frp <= 0.0 {
            skipped_nonpositive += 1;
            continue;
        }

        common_cells += 1;
        let center_lat = (f64::from(key.1) + 0.5).clamp(-90.0, 90.0);
        let bucket = band_bucket(center_lat.abs());
        let agg = &mut bands[bucket];
        agg.n_cells += 1;
        agg.sum_modis_frp += m_stats.sum_frp;
        agg.sum_viirs_frp += v_stats.sum_frp;
        agg.sum_modis_count += m_stats.count;
        agg.sum_viirs_count += v_stats.count;
        agg.cell_ratios.push(v_stats.sum_frp / m_stats.sum_frp);
        agg.model_ratios.push(calibration_for(center_lat, &cal_cfg).ratio);
    }

    println!(
        "Grid calibration check: country={} year={} month={:?} lat={:?}..={:?} max_dt_min={:.1} archive={}",
        args.country,
        args.year,
        args.month,
        args.lat_min,
        args.lat_max,
        args.max_dt_min,
        cache_dir.display()
    );
    println!(
        "Detections (daytime, wildfire): Aqua pre_dedup={} post_dedup={} | S-NPP pre_dedup={} post_dedup={}",
        aqua_pre_dedup,
        aqua.len(),
        snpp_pre_dedup,
        snpp.len()
    );
    println!(
        "Cell-days (1x1 deg, UTC): aqua={} snpp={} overlap_any_dt={} common(dt<={:.1}m)={} rejected_dt={}{}",
        aqua_cells.len(),
        snpp_cells.len(),
        overlap_any_dt,
        args.max_dt_min,
        common_cells,
        rejected_dt,
        if skipped_nonpositive > 0 {
            format!(" skipped_nonpositive={skipped_nonpositive}")
        } else {
            String::new()
        }
    );

    if common_cells == 0 {
        println!();
        println!(
            "No common Aqua MODIS / S-NPP VIIRS cell-days found within max_dt_min={:.1}.",
            args.max_dt_min
        );
        return Ok(());
    }

    const BAND_LABELS: [&str; 5] = [
        "Tropics (0..=30)",
        "Mid-lat (30..=50)",
        "Transition (50..55)",
        "Boreal (55..=70)",
        "Polar (>70)",
    ];

    println!();
    println!(
        "{:<20} {:>7} {:>10} {:>10} {:>10} {:>10} {:>8} {:>9} {:>12} {:>12}",
        "band",
        "n_cells",
        "ratio_sum",
        "cell_med",
        "q25",
        "q75",
        "model",
        "det_v/m",
        "modis_mw",
        "viirs_mw"
    );
    println!("{}", "-".repeat(116));

    let mut active_bands = 0usize;
    let mut total_modis_frp = 0.0;
    let mut total_viirs_frp = 0.0;
    let mut total_modis_count = 0u64;
    let mut total_viirs_count = 0u64;

    for (b, label) in BAND_LABELS.iter().enumerate() {
        let agg = &mut bands[b];
        if agg.n_cells == 0 {
            continue;
        }
        active_bands += 1;
        total_modis_frp += agg.sum_modis_frp;
        total_viirs_frp += agg.sum_viirs_frp;
        total_modis_count += agg.sum_modis_count;
        total_viirs_count += agg.sum_viirs_count;

        agg.cell_ratios.sort_by(f64::total_cmp);
        agg.model_ratios.sort_by(f64::total_cmp);

        let ratio_of_sums = agg.sum_viirs_frp / agg.sum_modis_frp;
        let med = percentile(&agg.cell_ratios, 0.50);
        let q25 = percentile(&agg.cell_ratios, 0.25);
        let q75 = percentile(&agg.cell_ratios, 0.75);
        let model_med = percentile(&agg.model_ratios, 0.50);
        let det_ratio = agg.sum_viirs_count as f64 / agg.sum_modis_count as f64;

        println!(
            "{:<20} {:>7} {:>10.4} {:>10.4} {:>10.4} {:>10.4} {:>8.4} {:>9.4} {:>12.1} {:>12.1}",
            label,
            agg.n_cells,
            ratio_of_sums,
            med,
            q25,
            q75,
            model_med,
            det_ratio,
            agg.sum_modis_frp,
            agg.sum_viirs_frp
        );
    }

    if active_bands > 1 {
        println!("{}", "-".repeat(116));
        println!(
            "{:<20} {:>7} {:>10.4} {:>10} {:>10} {:>10} {:>8} {:>9.4} {:>12.1} {:>12.1}",
            "All bands",
            common_cells,
            total_viirs_frp / total_modis_frp,
            "-",
            "-",
            "-",
            "-",
            total_viirs_count as f64 / total_modis_count as f64,
            total_modis_frp,
            total_viirs_frp
        );
    }

    Ok(())
}
