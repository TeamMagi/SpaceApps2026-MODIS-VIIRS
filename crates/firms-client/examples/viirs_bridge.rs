//! Measure S-NPP <-> NOAA-20 bridge ratios and check Terra/Aqua short-gap VIIRS
//! co-locations on the offline FIRMS country archive.
//!
//! Usage:
//!   ARCHIVE_CACHE_DIR=.cache/firms-archive \
//!     cargo run --release -p pyrosync-firms --example viirs_bridge -- <Country>... [--from 2018] [--to 2025] [--out <csv>]

use anyhow::{Context, Result, bail};
use chrono::{Datelike, TimeDelta};
use pyrosync_core::dedup::{haversine_km, remove_interscan_duplicates};
use pyrosync_core::models::{DayNight, Hotspot, Sensor};
use pyrosync_core::vza::vza_from_scan;
use pyrosync_firms::archive::{ArchiveClient, ArchiveError};
use pyrosync_firms::parse::retain_wildfires;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing_subscriber::EnvFilter;

const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const DEFAULT_FROM_YEAR: i32 = 2018;
const DEFAULT_TO_YEAR: i32 = 2025;

/// NOAA-20 (JPSS-1) FIRMS archive coverage begins in April 2018.
const NOAA20_START_YM: (i32, u32) = (2018, 4);

const MATCH_RADIUS_KM: f64 = 2.0;
const SHORT_GAP_SECS: i64 = 20 * 60;
const LONG_GAP_SECS: i64 = 180 * 60;
const MIN_TERRA_PAIRS_FOR_VZA: usize = 200;
const VZA_BIN_COUNT: usize = 7;
const VZA_BIN_LABELS: [&str; VZA_BIN_COUNT] = [
    "0-10", "10-20", "20-30", "30-40", "40-50", "50-60", "60-70",
];

/// Reference count bridges from the fire-harmony project (relative to fleet mean,
/// not directly comparable to pairwise n_n20 / n_snpp).
const FIRE_HARMONY_REF_SNPP: f64 = 0.990;
const FIRE_HARMONY_REF_NOAA21: f64 = 1.046;

struct Args {
    countries: Vec<String>,
    from: i32,
    to: i32,
    out: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DayNightFilter {
    All,
    Day,
    Night,
}

impl DayNightFilter {
    const ALL: [Self; 3] = [Self::All, Self::Day, Self::Night];

    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Day => "day",
            Self::Night => "night",
        }
    }

    fn matches(self, daynight: DayNight) -> bool {
        match self {
            Self::All => true,
            Self::Day => daynight == DayNight::Day,
            Self::Night => daynight == DayNight::Night,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct BridgeTotals {
    n_snpp: u64,
    n_n20: u64,
    sum_frp_snpp: f64,
    sum_frp_n20: f64,
}

impl BridgeTotals {
    fn add_stats(&mut self, stats: &BridgeStats) {
        self.n_snpp += stats.n_snpp;
        self.n_n20 += stats.n_n20;
        self.sum_frp_snpp += stats.sum_frp_snpp;
        self.sum_frp_n20 += stats.sum_frp_n20;
    }

    fn to_stats(&self) -> BridgeStats {
        let count_bridge = safe_ratio(self.n_n20 as f64, self.n_snpp as f64);
        let frp_bridge = safe_ratio(self.sum_frp_n20, self.sum_frp_snpp);
        let mean_frp_snpp = safe_ratio(self.sum_frp_snpp, self.n_snpp as f64);
        let mean_frp_n20 = safe_ratio(self.sum_frp_n20, self.n_n20 as f64);
        let mean_frp_ratio = match (mean_frp_n20, mean_frp_snpp) {
            (Some(n20), Some(snpp)) => safe_ratio(n20, snpp),
            _ => None,
        };
        BridgeStats {
            n_snpp: self.n_snpp,
            n_n20: self.n_n20,
            count_bridge,
            sum_frp_snpp: self.sum_frp_snpp,
            sum_frp_n20: self.sum_frp_n20,
            frp_bridge,
            mean_frp_snpp,
            mean_frp_n20,
            mean_frp_ratio,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct BridgeStats {
    n_snpp: u64,
    n_n20: u64,
    count_bridge: Option<f64>,
    sum_frp_snpp: f64,
    sum_frp_n20: f64,
    frp_bridge: Option<f64>,
    mean_frp_snpp: Option<f64>,
    mean_frp_n20: Option<f64>,
    mean_frp_ratio: Option<f64>,
}

#[derive(Debug, Clone)]
struct BridgeRow {
    country: String,
    year: String,
    overlap_months: usize,
    daynight: DayNightFilter,
    stats: BridgeStats,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ColocatedPair {
    dt_abs_secs: i64,
    modis_frp: f64,
    viirs_frp: f64,
    modis_vza_deg: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct DtSummary {
    n_short: usize,
    n_long: usize,
    min_dt_min: Option<f64>,
    median_dt_min: Option<f64>,
}

#[derive(Debug, Clone)]
struct YearGapSummary {
    year: i32,
    terra: DtSummary,
    aqua: DtSummary,
}

#[derive(Debug, Clone, Default)]
struct VzaBinStats {
    n_pairs: usize,
    sum_modis_frp: f64,
    sum_viirs_frp: f64,
    ratio_of_sums: Option<f64>,
    median_pair_ratio: Option<f64>,
}

struct CountryOutcome {
    bridge_rows: Vec<BridgeRow>,
    pooled_months: usize,
    pooled_totals: [BridgeTotals; 3],
    terra_pairs_180m: Vec<ColocatedPair>,
    aqua_pairs_180m: Vec<ColocatedPair>,
}

fn print_usage() {
    println!(
        "\
Usage: viirs_bridge <Country>... [--from {DEFAULT_FROM_YEAR}] [--to {DEFAULT_TO_YEAR}] [--out <csv>]

Measure S-NPP <-> NOAA-20 bridge ratios and check Terra/Aqua short-gap VIIRS co-locations.
Archive directory: ARCHIVE_CACHE_DIR env var, or {DEFAULT_ARCHIVE_CACHE_DIR}."
    );
}

fn parse_args() -> Result<Option<Args>> {
    let mut raw_args = std::env::args().skip(1);
    let mut countries = Vec::new();
    let mut from = DEFAULT_FROM_YEAR;
    let mut to = DEFAULT_TO_YEAR;
    let mut out: Option<PathBuf> = None;

    while let Some(arg) = raw_args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return Ok(None);
            }
            "--from" => {
                let val = raw_args.next().context("missing value for --from")?;
                from = val
                    .parse()
                    .with_context(|| format!("invalid --from year: {val}"))?;
            }
            "--to" => {
                let val = raw_args.next().context("missing value for --to")?;
                to = val
                    .parse()
                    .with_context(|| format!("invalid --to year: {val}"))?;
            }
            "--out" => {
                let val = raw_args.next().context("missing value for --out")?;
                out = Some(PathBuf::from(val));
            }
            other if other.starts_with('-') => {
                bail!("unknown flag: {other} (try --help)");
            }
            other => countries.push(other.to_string()),
        }
    }

    if countries.is_empty() {
        bail!("expected at least one <Country> argument (try --help)");
    }
    if from > to {
        bail!("--from ({from}) must be <= --to ({to})");
    }

    Ok(Some(Args {
        countries,
        from,
        to,
        out,
    }))
}

fn archive_cache_dir() -> PathBuf {
    match std::env::var_os("ARCHIVE_CACHE_DIR") {
        Some(val) if !val.is_empty() => PathBuf::from(val),
        _ => PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR),
    }
}

fn safe_ratio(num: f64, den: f64) -> Option<f64> {
    if num.is_finite() && den.is_finite() && den > 0.0 {
        let r = num / den;
        r.is_finite().then_some(r)
    } else {
        None
    }
}

fn quantile(sorted: &[f64], q: f64) -> Option<f64> {
    let n = sorted.len();
    if n == 0 {
        return None;
    }
    if n == 1 {
        return Some(sorted[0]);
    }
    let pos = q.clamp(0.0, 1.0) * (n - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    Some(sorted[lo] * (1.0 - frac) + sorted[hi] * frac)
}

fn fmt_opt(val: Option<f64>, decimals: usize) -> String {
    match val {
        Some(v) if v.is_finite() => format!("{v:.decimals$}"),
        _ => "NA".to_string(),
    }
}

fn overlap_months(snpp: &[Hotspot], n20: &[Hotspot]) -> BTreeSet<(i32, u32)> {
    let collect_months = |hotspots: &[Hotspot]| -> BTreeSet<(i32, u32)> {
        hotspots
            .iter()
            .filter_map(|h| {
                let d = h.acquired.date_naive();
                let ym = (d.year(), d.month());
                (ym >= NOAA20_START_YM).then_some(ym)
            })
            .collect()
    };
    let snpp_months = collect_months(snpp);
    let n20_months = collect_months(n20);
    snpp_months.intersection(&n20_months).copied().collect()
}

fn restrict_to_overlap_months(
    mut snpp: Vec<Hotspot>,
    mut n20: Vec<Hotspot>,
) -> (Vec<Hotspot>, Vec<Hotspot>, BTreeSet<(i32, u32)>) {
    let months = overlap_months(&snpp, &n20);
    snpp.retain(|h| {
        let d = h.acquired.date_naive();
        months.contains(&(d.year(), d.month()))
    });
    n20.retain(|h| {
        let d = h.acquired.date_naive();
        months.contains(&(d.year(), d.month()))
    });
    (snpp, n20, months)
}

fn compute_bridge_stats(
    snpp: &[Hotspot],
    n20: &[Hotspot],
    filter: DayNightFilter,
) -> BridgeStats {
    let mut totals = BridgeTotals::default();
    for h in snpp {
        if filter.matches(h.daynight) {
            totals.n_snpp += 1;
            totals.sum_frp_snpp += h.frp;
        }
    }
    for h in n20 {
        if filter.matches(h.daynight) {
            totals.n_n20 += 1;
            totals.sum_frp_n20 += h.frp;
        }
    }
    totals.to_stats()
}

/// Find all colocated `(modis, viirs)` pairs within `max_dist_km` and `|dt| <= max_dt_secs`
/// using a time-sorted two-pointer sweep.
fn find_colocated_pairs(
    modis: &[Hotspot],
    viirs: &[Hotspot],
    max_dist_km: f64,
    max_dt_secs: i64,
) -> Vec<ColocatedPair> {
    if modis.is_empty() || viirs.is_empty() || max_dist_km < 0.0 || max_dt_secs < 0 {
        return Vec::new();
    }

    let mut m_order: Vec<usize> = (0..modis.len()).collect();
    m_order.sort_by_key(|&i| modis[i].acquired);

    let mut v_order: Vec<usize> = (0..viirs.len()).collect();
    v_order.sort_by_key(|&i| viirs[i].acquired);

    let max_dt = TimeDelta::seconds(max_dt_secs);
    let max_dlat_deg = (max_dist_km / 6371.0).to_degrees() + 1e-9;

    let mut pairs = Vec::new();
    let mut v_start = 0usize;
    let mut v_end = 0usize;

    for &mi in &m_order {
        let m = &modis[mi];
        while v_start < v_order.len()
            && (m.acquired - viirs[v_order[v_start]].acquired) > max_dt
        {
            v_start += 1;
        }
        if v_end < v_start {
            v_end = v_start;
        }
        while v_end < v_order.len()
            && (viirs[v_order[v_end]].acquired - m.acquired) <= max_dt
        {
            v_end += 1;
        }

        for &vi in &v_order[v_start..v_end] {
            let v = &viirs[vi];
            if (v.lat - m.lat).abs() > max_dlat_deg {
                continue;
            }
            let dt = (v.acquired - m.acquired).abs();
            if dt > max_dt {
                continue;
            }
            let dist_km = haversine_km(m.lat, m.lon, v.lat, v.lon);
            if dist_km <= max_dist_km {
                pairs.push(ColocatedPair {
                    dt_abs_secs: dt.num_seconds(),
                    modis_frp: m.frp,
                    viirs_frp: v.frp,
                    modis_vza_deg: m.vza.or_else(|| vza_from_scan(m.scan)),
                });
            }
        }
    }

    pairs
}

fn summarize_dt(pairs_180m: &[ColocatedPair]) -> DtSummary {
    let n_long = pairs_180m.len();
    let n_short = pairs_180m
        .iter()
        .filter(|p| p.dt_abs_secs <= SHORT_GAP_SECS)
        .count();
    let mut dt_mins: Vec<f64> = pairs_180m
        .iter()
        .map(|p| p.dt_abs_secs as f64 / 60.0)
        .collect();
    dt_mins.sort_by(f64::total_cmp);
    DtSummary {
        n_short,
        n_long,
        min_dt_min: dt_mins.first().copied(),
        median_dt_min: quantile(&dt_mins, 0.50),
    }
}

fn vza_bin_index(vza_deg: f64) -> Option<usize> {
    if !vza_deg.is_finite() || vza_deg < 0.0 {
        return None;
    }
    Some(((vza_deg / 10.0).floor() as usize).min(VZA_BIN_COUNT - 1))
}

fn bin_pairs_by_vza(pairs: &[ColocatedPair]) -> [VzaBinStats; VZA_BIN_COUNT] {
    let mut bins: [VzaBinStats; VZA_BIN_COUNT] = Default::default();
    let mut ratios_per_bin: [Vec<f64>; VZA_BIN_COUNT] = Default::default();

    for p in pairs {
        let Some(vza) = p.modis_vza_deg else {
            continue;
        };
        let Some(idx) = vza_bin_index(vza) else {
            continue;
        };
        bins[idx].n_pairs += 1;
        bins[idx].sum_modis_frp += p.modis_frp;
        bins[idx].sum_viirs_frp += p.viirs_frp;
        if let Some(r) = safe_ratio(p.viirs_frp, p.modis_frp) {
            ratios_per_bin[idx].push(r);
        }
    }

    for (bin, ratios) in bins.iter_mut().zip(ratios_per_bin.iter_mut()) {
        bin.ratio_of_sums = safe_ratio(bin.sum_viirs_frp, bin.sum_modis_frp);
        ratios.sort_by(f64::total_cmp);
        bin.median_pair_ratio = quantile(ratios, 0.50);
    }

    bins
}

async fn load_sensor_year(
    archive: &ArchiveClient,
    sensor: Sensor,
    country: &str,
    year: i32,
) -> Result<Vec<Hotspot>> {
    if !archive.covers(sensor, year) {
        return Ok(Vec::new());
    }
    match archive.fetch_country_year(sensor, country, year).await {
        Ok(mut hotspots) => {
            retain_wildfires(&mut hotspots);
            Ok(hotspots)
        }
        Err(ArchiveError::NotCached { path }) => {
            tracing::warn!(
                country,
                year,
                sensor = ?sensor,
                path = %path.display(),
                "missing archive file, skipping"
            );
            Ok(Vec::new())
        }
        Err(ArchiveError::NotFound { .. }) => {
            tracing::warn!(
                country,
                year,
                sensor = ?sensor,
                "archive file not found, skipping"
            );
            Ok(Vec::new())
        }
        Err(err) => Err(err).with_context(|| {
            format!("failed to load {sensor:?} {country} {year} from archive")
        }),
    }
}

fn print_reference_header() {
    println!(
        "Note: fire-harmony reference count bridges are relative to its fleet mean \
         (S-NPP = {FIRE_HARMONY_REF_SNPP:.3}, NOAA-21 = {FIRE_HARMONY_REF_NOAA21:.3}) \
         and are not directly the same quantity as pairwise n_n20 / n_snpp."
    );
}

fn print_bridge_table(rows: &[BridgeRow]) {
    println!("\n[Part 1: S-NPP vs NOAA-20 bridge (overlap months only)]");
    println!(
        "{:<6} {:>3} {:<5} {:>9} {:>9} {:>10} {:>12} {:>12} {:>10} {:>9} {:>9} {:>10}",
        "year",
        "mo",
        "dn",
        "n_snpp",
        "n_n20",
        "n20/snpp",
        "frp_snpp",
        "frp_n20",
        "frp_ratio",
        "mean_snpp",
        "mean_n20",
        "mean_ratio"
    );
    println!("{}", "-".repeat(120));
    for r in rows {
        if r.year == "pooled" && r.daynight == DayNightFilter::All {
            println!("{}", "-".repeat(120));
        }
        let s = &r.stats;
        println!(
            "{:<6} {:>3} {:<5} {:>9} {:>9} {:>10} {:>12.1} {:>12.1} {:>10} {:>9} {:>9} {:>10}",
            r.year,
            r.overlap_months,
            r.daynight.as_str(),
            s.n_snpp,
            s.n_n20,
            fmt_opt(s.count_bridge, 4),
            s.sum_frp_snpp,
            s.sum_frp_n20,
            fmt_opt(s.frp_bridge, 4),
            fmt_opt(s.mean_frp_snpp, 2),
            fmt_opt(s.mean_frp_n20, 2),
            fmt_opt(s.mean_frp_ratio, 4),
        );
    }
    if let Some(pooled_all) = rows
        .iter()
        .find(|r| r.year == "pooled" && r.daynight == DayNightFilter::All)
    {
        println!(
            "Measured pooled n_n20/n_snpp = {}; fire-harmony fleet-mean refs: S-NPP = {FIRE_HARMONY_REF_SNPP:.3}, NOAA-21 = {FIRE_HARMONY_REF_NOAA21:.3}",
            fmt_opt(pooled_all.stats.count_bridge, 4),
        );
    }
}

fn print_gap_table(
    country: &str,
    yearly: &[YearGapSummary],
    terra_pooled: &[ColocatedPair],
    aqua_pooled: &[ColocatedPair],
) {
    println!("\n[Part 2: Terra vs Aqua short-gap check (<= {MATCH_RADIUS_KM:.0} km)]");
    println!(
        "{:<6} {:>10} {:>11} {:>11} {:>11} {:>10} {:>11} {:>11} {:>11}",
        "year",
        "terra_<=20",
        "terra_<=180",
        "t_min_dt_m",
        "t_med_dt_m",
        "aqua_<=20",
        "aqua_<=180",
        "a_min_dt_m",
        "a_med_dt_m"
    );
    println!("{}", "-".repeat(102));
    for y in yearly {
        println!(
            "{:<6} {:>10} {:>11} {:>11} {:>11} {:>10} {:>11} {:>11} {:>11}",
            y.year,
            y.terra.n_short,
            y.terra.n_long,
            fmt_opt(y.terra.min_dt_min, 1),
            fmt_opt(y.terra.median_dt_min, 1),
            y.aqua.n_short,
            y.aqua.n_long,
            fmt_opt(y.aqua.min_dt_min, 1),
            fmt_opt(y.aqua.median_dt_min, 1),
        );
    }

    let t_sum = summarize_dt(terra_pooled);
    let a_sum = summarize_dt(aqua_pooled);
    println!("{}", "-".repeat(102));
    println!(
        "{:<6} {:>10} {:>11} {:>11} {:>11} {:>10} {:>11} {:>11} {:>11}",
        "pooled",
        t_sum.n_short,
        t_sum.n_long,
        fmt_opt(t_sum.min_dt_min, 1),
        fmt_opt(t_sum.median_dt_min, 1),
        a_sum.n_short,
        a_sum.n_long,
        fmt_opt(a_sum.min_dt_min, 1),
        fmt_opt(a_sum.median_dt_min, 1),
    );

    print_vza_check(country, terra_pooled, aqua_pooled);
}

fn print_vza_check(
    label: &str,
    terra_180m: &[ColocatedPair],
    aqua_180m: &[ColocatedPair],
) {
    let terra_short: Vec<ColocatedPair> = terra_180m
        .iter()
        .copied()
        .filter(|p| p.dt_abs_secs <= SHORT_GAP_SECS)
        .collect();
    let aqua_short: Vec<ColocatedPair> = aqua_180m
        .iter()
        .copied()
        .filter(|p| p.dt_abs_secs <= SHORT_GAP_SECS)
        .collect();

    if terra_short.len() < MIN_TERRA_PAIRS_FOR_VZA {
        let t_sum = summarize_dt(terra_180m);
        let a_sum = summarize_dt(aqua_180m);
        println!(
            "VZA check for {label}: CANNOT BE PERFORMED with this data ({} Terra-VIIRS pairs with |dt| <= 20 min and dist <= {MATCH_RADIUS_KM:.0} km; minimum {MIN_TERRA_PAIRS_FOR_VZA} required). \
             Why: across {} co-located Terra-VIIRS pairs within 180 min, min |dt| = {} min and median |dt| = {} min \
             (morning Terra ~10:30 LT vs afternoon S-NPP/NOAA-20 ~13:30 LT; cf. Aqua: {} pairs <= 20 min, min |dt| = {} min, median |dt| = {} min over {} pairs <= 180 min).",
            terra_short.len(),
            t_sum.n_long,
            fmt_opt(t_sum.min_dt_min, 1),
            fmt_opt(t_sum.median_dt_min, 1),
            a_sum.n_short,
            fmt_opt(a_sum.min_dt_min, 1),
            fmt_opt(a_sum.median_dt_min, 1),
            a_sum.n_long,
        );
        return;
    }

    let terra_bins = bin_pairs_by_vza(&terra_short);
    let aqua_bins = bin_pairs_by_vza(&aqua_short);
    println!(
        "\n[VZA-binned VIIRS_FRP / MODIS_FRP (|dt| <= 20 min, <= {MATCH_RADIUS_KM:.0} km) for {label}]"
    );
    println!(
        "{:<8} {:>8} {:>12} {:>12} {:>8} {:>12} {:>12}",
        "vza_deg",
        "terra_n",
        "terra_sum_r",
        "terra_med_r",
        "aqua_n",
        "aqua_sum_r",
        "aqua_med_r"
    );
    println!("{}", "-".repeat(78));
    for ((label_bin, tb), ab) in VZA_BIN_LABELS
        .iter()
        .zip(terra_bins.iter())
        .zip(aqua_bins.iter())
    {
        println!(
            "{:<8} {:>8} {:>12} {:>12} {:>8} {:>12} {:>12}",
            label_bin,
            tb.n_pairs,
            fmt_opt(tb.ratio_of_sums, 4),
            fmt_opt(tb.median_pair_ratio, 4),
            ab.n_pairs,
            fmt_opt(ab.ratio_of_sums, 4),
            fmt_opt(ab.median_pair_ratio, 4),
        );
    }
}

fn csv_escape(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn write_csv(path: &Path, rows: &[BridgeRow]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut w = BufWriter::new(file);
    writeln!(
        w,
        "country,year,overlap_months,daynight,n_snpp,n_n20,count_bridge,sum_frp_snpp,sum_frp_n20,frp_bridge,mean_frp_snpp,mean_frp_n20,mean_frp_ratio"
    )?;
    for r in rows {
        let s = &r.stats;
        writeln!(
            w,
            "{},{},{},{},{},{},{},{:.6},{:.6},{},{},{},{}",
            csv_escape(&r.country),
            r.year,
            r.overlap_months,
            r.daynight.as_str(),
            s.n_snpp,
            s.n_n20,
            fmt_opt(s.count_bridge, 6),
            s.sum_frp_snpp,
            s.sum_frp_n20,
            fmt_opt(s.frp_bridge, 6),
            fmt_opt(s.mean_frp_snpp, 6),
            fmt_opt(s.mean_frp_n20, 6),
            fmt_opt(s.mean_frp_ratio, 6),
        )?;
    }
    w.flush()?;
    Ok(())
}

async fn analyze_country(
    archive: &ArchiveClient,
    country: &str,
    from: i32,
    to: i32,
) -> Result<CountryOutcome> {
    let t0 = Instant::now();
    let mut bridge_rows = Vec::new();
    let mut pooled_months = 0usize;
    let mut pooled_totals: [BridgeTotals; 3] = Default::default();
    let mut yearly_gaps = Vec::new();
    let mut terra_pairs_180m = Vec::new();
    let mut aqua_pairs_180m = Vec::new();

    for year in from..=to {
        let mut snpp = load_sensor_year(archive, Sensor::ViirsSNPP, country, year).await?;
        let mut n20 = load_sensor_year(archive, Sensor::ViirsNOAA20, country, year).await?;
        let modis = load_sensor_year(archive, Sensor::ModisTerra, country, year).await?;

        remove_interscan_duplicates(&mut snpp);
        remove_interscan_duplicates(&mut n20);

        let (mut terra, mut aqua): (Vec<Hotspot>, Vec<Hotspot>) = modis
            .into_iter()
            .partition(|h| h.sensor == Sensor::ModisTerra);
        aqua.retain(|h| h.sensor == Sensor::ModisAqua);
        remove_interscan_duplicates(&mut terra);
        remove_interscan_duplicates(&mut aqua);

        // Part 2: pair Terra and Aqua against all deduplicated VIIRS (S-NPP + NOAA-20) in this year.
        if (!terra.is_empty() || !aqua.is_empty()) && (!snpp.is_empty() || !n20.is_empty()) {
            let mut viirs_all = Vec::with_capacity(snpp.len() + n20.len());
            viirs_all.extend_from_slice(&snpp);
            viirs_all.extend_from_slice(&n20);

            let t_pairs = find_colocated_pairs(&terra, &viirs_all, MATCH_RADIUS_KM, LONG_GAP_SECS);
            let a_pairs = find_colocated_pairs(&aqua, &viirs_all, MATCH_RADIUS_KM, LONG_GAP_SECS);
            yearly_gaps.push(YearGapSummary {
                year,
                terra: summarize_dt(&t_pairs),
                aqua: summarize_dt(&a_pairs),
            });
            terra_pairs_180m.extend(t_pairs);
            aqua_pairs_180m.extend(a_pairs);
        } else {
            yearly_gaps.push(YearGapSummary {
                year,
                terra: DtSummary::default(),
                aqua: DtSummary::default(),
            });
        }

        // Part 1: restrict S-NPP and NOAA-20 to months where both have data (2018-04 onward).
        if archive.covers(Sensor::ViirsSNPP, year) && archive.covers(Sensor::ViirsNOAA20, year) {
            let (snpp_ov, n20_ov, months) = restrict_to_overlap_months(snpp, n20);
            pooled_months += months.len();
            for (idx, filter) in DayNightFilter::ALL.into_iter().enumerate() {
                let stats = compute_bridge_stats(&snpp_ov, &n20_ov, filter);
                pooled_totals[idx].add_stats(&stats);
                bridge_rows.push(BridgeRow {
                    country: country.to_string(),
                    year: year.to_string(),
                    overlap_months: months.len(),
                    daynight: filter,
                    stats,
                });
            }
        }
    }

    for (idx, filter) in DayNightFilter::ALL.into_iter().enumerate() {
        bridge_rows.push(BridgeRow {
            country: country.to_string(),
            year: "pooled".to_string(),
            overlap_months: pooled_months,
            daynight: filter,
            stats: pooled_totals[idx].to_stats(),
        });
    }

    println!("\n================================================================================");
    println!(
        "COUNTRY: {country} ({from}..={to}) | elapsed={:.2}s",
        t0.elapsed().as_secs_f64()
    );
    print_bridge_table(&bridge_rows);
    print_gap_table(country, &yearly_gaps, &terra_pairs_180m, &aqua_pairs_180m);

    Ok(CountryOutcome {
        bridge_rows,
        pooled_months,
        pooled_totals,
        terra_pairs_180m,
        aqua_pairs_180m,
    })
}

fn print_cross_country_summary(outcomes: &[(&str, &CountryOutcome)]) {
    println!("\n================================================================================");
    println!("ACROSS-COUNTRY POOLED SUMMARY");
    println!("================================================================================");
    print_reference_header();

    let mut total_months = 0usize;
    let mut total_pooled: [BridgeTotals; 3] = Default::default();
    let mut all_terra_180m = Vec::new();
    let mut all_aqua_180m = Vec::new();

    println!("\n[Part 1 pooled per country]");
    println!(
        "{:<18} {:>4} {:<5} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "country", "mo", "dn", "n_snpp", "n_n20", "n20/snpp", "frp_ratio", "mean_ratio"
    );
    println!("{}", "-".repeat(86));
    for &(country, outcome) in outcomes {
        total_months += outcome.pooled_months;
        all_terra_180m.extend_from_slice(&outcome.terra_pairs_180m);
        all_aqua_180m.extend_from_slice(&outcome.aqua_pairs_180m);
        for (idx, filter) in DayNightFilter::ALL.into_iter().enumerate() {
            let s = outcome.pooled_totals[idx].to_stats();
            total_pooled[idx].add_stats(&s);
            println!(
                "{:<18} {:>4} {:<5} {:>10} {:>10} {:>10} {:>10} {:>10}",
                country,
                outcome.pooled_months,
                filter.as_str(),
                s.n_snpp,
                s.n_n20,
                fmt_opt(s.count_bridge, 4),
                fmt_opt(s.frp_bridge, 4),
                fmt_opt(s.mean_frp_ratio, 4),
            );
        }
    }
    println!("{}", "-".repeat(86));
    for (idx, filter) in DayNightFilter::ALL.into_iter().enumerate() {
        let s = total_pooled[idx].to_stats();
        println!(
            "{:<18} {:>4} {:<5} {:>10} {:>10} {:>10} {:>10} {:>10}",
            "ALL_POOLED",
            total_months,
            filter.as_str(),
            s.n_snpp,
            s.n_n20,
            fmt_opt(s.count_bridge, 4),
            fmt_opt(s.frp_bridge, 4),
            fmt_opt(s.mean_frp_ratio, 4),
        );
    }

    println!("\n[Part 2 pooled across all countries]");
    print_vza_check("ALL_POOLED", &all_terra_180m, &all_aqua_180m);
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

    println!("Archive cache: {}", cache_dir.display());
    println!(
        "Countries ({}): {} | range={}..={}",
        args.countries.len(),
        args.countries.join(", "),
        args.from,
        args.to,
    );
    print_reference_header();

    let overall_t0 = Instant::now();
    let mut outcomes = Vec::with_capacity(args.countries.len());
    for country in &args.countries {
        let outcome = analyze_country(&archive, country, args.from, args.to).await?;
        outcomes.push((country.as_str(), outcome));
    }

    if outcomes.len() > 1 {
        let refs: Vec<(&str, &CountryOutcome)> =
            outcomes.iter().map(|(c, o)| (*c, o)).collect();
        print_cross_country_summary(&refs);
    }

    if let Some(ref out_path) = args.out {
        let all_rows: Vec<BridgeRow> = outcomes
            .iter()
            .flat_map(|(_, o)| o.bridge_rows.iter().cloned())
            .collect();
        write_csv(out_path, &all_rows)?;
        println!(
            "\nCSV written: {} ({} rows)",
            out_path.display(),
            all_rows.len()
        );
    }

    println!(
        "\nCompleted {} countries in {:.2}s",
        args.countries.len(),
        overall_t0.elapsed().as_secs_f64()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use pyrosync_core::models::{Confidence, FireType};

    fn make_hotspot(
        sensor: Sensor,
        lat: f64,
        lon: f64,
        frp: f64,
        daynight: DayNight,
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        min: u32,
    ) -> Hotspot {
        Hotspot {
            lat,
            lon,
            brightness: 320.0,
            frp,
            confidence: Confidence::Nominal,
            sensor,
            acquired: Utc
                .with_ymd_and_hms(year, month, day, hour, min, 0)
                .unwrap(),
            scan: 1.0,
            track: 1.0,
            satellite: match sensor {
                Sensor::ModisTerra => "Terra".to_string(),
                Sensor::ModisAqua => "Aqua".to_string(),
                Sensor::ViirsSNPP => "N".to_string(),
                Sensor::ViirsNOAA20 => "N20".to_string(),
                Sensor::ViirsNOAA21 => "N21".to_string(),
            },
            daynight,
            vza: if sensor.is_modis() { Some(15.0) } else { None },
            fire_type: FireType::VegetationFire,
        }
    }

    #[test]
    fn ratio_computation_handles_zero_denominators() {
        assert_eq!(safe_ratio(10.0, 0.0), None);
        assert_eq!(safe_ratio(0.0, 0.0), None);
        assert_eq!(safe_ratio(10.0, f64::NAN), None);
        assert!(safe_ratio(0.0, 4.0).unwrap().abs() < 1e-12);
        assert!((safe_ratio(3.0, 2.0).unwrap() - 1.5).abs() < 1e-12);

        let n20_only = [make_hotspot(
            Sensor::ViirsNOAA20,
            38.0,
            28.0,
            25.0,
            DayNight::Day,
            2020,
            7,
            1,
            12,
            0,
        )];
        let empty_snpp: [Hotspot; 0] = [];
        let stats = compute_bridge_stats(&empty_snpp, &n20_only, DayNightFilter::All);
        assert_eq!(stats.n_snpp, 0);
        assert_eq!(stats.n_n20, 1);
        assert_eq!(stats.count_bridge, None);
        assert_eq!(stats.frp_bridge, None);
        assert_eq!(stats.mean_frp_snpp, None);
        assert!((stats.mean_frp_n20.unwrap() - 25.0).abs() < 1e-12);
        assert_eq!(stats.mean_frp_ratio, None);

        let zero_frp_snpp = [make_hotspot(
            Sensor::ViirsSNPP,
            38.0,
            28.0,
            0.0,
            DayNight::Night,
            2020,
            7,
            1,
            1,
            0,
        )];
        let stats_zero_frp =
            compute_bridge_stats(&zero_frp_snpp, &n20_only, DayNightFilter::All);
        assert!((stats_zero_frp.count_bridge.unwrap() - 1.0).abs() < 1e-12);
        assert_eq!(stats_zero_frp.frp_bridge, None);
        assert!(stats_zero_frp.mean_frp_snpp.unwrap().abs() < 1e-12);
        assert_eq!(stats_zero_frp.mean_frp_ratio, None);

        let night_only = compute_bridge_stats(&zero_frp_snpp, &n20_only, DayNightFilter::Night);
        assert_eq!(night_only.n_snpp, 1);
        assert_eq!(night_only.n_n20, 0);
        assert!(night_only.count_bridge.unwrap().abs() < 1e-12);
        assert_eq!(night_only.mean_frp_n20, None);
        assert_eq!(night_only.mean_frp_ratio, None);
    }

    #[test]
    fn overlap_month_restriction_skips_pre_noaa20_and_single_sensor_months() {
        let snpp = vec![
            make_hotspot(Sensor::ViirsSNPP, 38.0, 28.0, 10.0, DayNight::Day, 2018, 1, 10, 12, 0),
            make_hotspot(Sensor::ViirsSNPP, 38.0, 28.0, 10.0, DayNight::Day, 2018, 3, 10, 12, 0),
            make_hotspot(Sensor::ViirsSNPP, 38.0, 28.0, 12.0, DayNight::Day, 2018, 4, 10, 12, 0),
            make_hotspot(Sensor::ViirsSNPP, 38.0, 28.0, 15.0, DayNight::Night, 2018, 5, 10, 1, 0),
            make_hotspot(Sensor::ViirsSNPP, 38.0, 28.0, 20.0, DayNight::Day, 2018, 6, 10, 12, 0),
        ];
        let n20 = vec![
            // Guard against any pre-2018-04 commissioning detections even if present.
            make_hotspot(Sensor::ViirsNOAA20, 38.0, 28.0, 9.0, DayNight::Day, 2018, 3, 15, 12, 0),
            make_hotspot(Sensor::ViirsNOAA20, 38.0, 28.0, 14.0, DayNight::Day, 2018, 4, 12, 12, 0),
            make_hotspot(Sensor::ViirsNOAA20, 38.0, 28.0, 18.0, DayNight::Night, 2018, 5, 12, 1, 0),
            make_hotspot(Sensor::ViirsNOAA20, 38.0, 28.0, 22.0, DayNight::Day, 2018, 7, 12, 12, 0),
        ];

        let (snpp_kept, n20_kept, months) = restrict_to_overlap_months(snpp, n20);
        assert_eq!(months, BTreeSet::from([(2018, 4), (2018, 5)]));
        assert_eq!(snpp_kept.len(), 2);
        assert_eq!(n20_kept.len(), 2);
        assert_eq!(
            snpp_kept
                .iter()
                .map(|h| h.acquired.date_naive().month())
                .collect::<Vec<_>>(),
            vec![4, 5]
        );
        assert_eq!(
            n20_kept
                .iter()
                .map(|h| h.acquired.date_naive().month())
                .collect::<Vec<_>>(),
            vec![4, 5]
        );
    }

    #[test]
    fn time_sweep_pair_finder_respects_time_and_distance_boundaries() {
        // 1 deg latitude on spherical Earth (R = 6371 km) is 111.19492664455873 km.
        let deg_per_km = 1.0 / haversine_km(0.0, 0.0, 1.0, 0.0);

        // Intentionally out of time order to verify the sweep sorts internally; distinct FRPs tag each point.
        let modis = vec![
            // frp=50: 12:30 at lat = 0.0 (will match viirs frp=55 at 10 min / ~2.0 km boundary)
            make_hotspot(Sensor::ModisTerra, 0.0, 30.0, 50.0, DayNight::Day, 2021, 8, 1, 12, 30),
            // frp=40: 12:00 at lat = 10.0 (will match viirs frp=45 at 10 min / 0 km, exclude 21 min and 2.1 km)
            make_hotspot(Sensor::ModisTerra, 10.0, 30.0, 40.0, DayNight::Day, 2021, 8, 1, 12, 0),
        ];
        let viirs = vec![
            // frp=45: 12:10 at lat = 10.0 -> 10 min, 0.0 km from modis(40) (included)
            make_hotspot(Sensor::ViirsSNPP, 10.0, 30.0, 45.0, DayNight::Day, 2021, 8, 1, 12, 10),
            // frp=55: 12:20 at ~1.9999 km from modis(50) -> 10 min, <= 2.0 km boundary (included)
            make_hotspot(
                Sensor::ViirsNOAA20,
                1.9999 * deg_per_km,
                30.0,
                55.0,
                DayNight::Day,
                2021,
                8,
                1,
                12,
                20,
            ),
            // frp=30: 12:21 at lat = 10.0 -> 21 min from modis(40) (excluded)
            make_hotspot(Sensor::ViirsSNPP, 10.0, 30.0, 30.0, DayNight::Day, 2021, 8, 1, 12, 21),
            // frp=35: 12:05 at 2.1 km from modis(40) -> 5 min, 2.1 km (excluded)
            make_hotspot(
                Sensor::ViirsSNPP,
                10.0 + 2.1 * deg_per_km,
                30.0,
                35.0,
                DayNight::Day,
                2021,
                8,
                1,
                12,
                5,
            ),
        ];

        let pairs = find_colocated_pairs(&modis, &viirs, MATCH_RADIUS_KM, SHORT_GAP_SECS);
        let tags: Vec<(u64, u64, i64)> = pairs
            .iter()
            .map(|p| {
                (
                    p.modis_frp.round() as u64,
                    p.viirs_frp.round() as u64,
                    p.dt_abs_secs,
                )
            })
            .collect();
        assert_eq!(tags, vec![(40, 45, 600), (50, 55, 600)]);
    }

    #[test]
    fn dt_summary_and_vza_binning_compute_exact_aggregates() {
        let empty = summarize_dt(&[]);
        assert_eq!(empty.n_short, 0);
        assert_eq!(empty.n_long, 0);
        assert_eq!(empty.min_dt_min, None);
        assert_eq!(empty.median_dt_min, None);

        let pairs = vec![
            ColocatedPair {
                dt_abs_secs: 600,
                modis_frp: 20.0,
                viirs_frp: 30.0,
                modis_vza_deg: Some(5.0),
            },
            ColocatedPair {
                dt_abs_secs: 1800,
                modis_frp: 40.0,
                viirs_frp: 60.0,
                modis_vza_deg: Some(5.0),
            },
            ColocatedPair {
                dt_abs_secs: 5400,
                modis_frp: 50.0,
                viirs_frp: 100.0,
                modis_vza_deg: Some(45.0),
            },
        ];

        let summary = summarize_dt(&pairs);
        assert_eq!(summary.n_short, 1);
        assert_eq!(summary.n_long, 3);
        assert!((summary.min_dt_min.unwrap() - 10.0).abs() < 1e-12);
        assert!((summary.median_dt_min.unwrap() - 30.0).abs() < 1e-12);

        let bins = bin_pairs_by_vza(&pairs);
        assert_eq!(bins[0].n_pairs, 2);
        assert!((bins[0].ratio_of_sums.unwrap() - 1.5).abs() < 1e-12);
        assert!((bins[0].median_pair_ratio.unwrap() - 1.5).abs() < 1e-12);
        assert_eq!(bins[4].n_pairs, 1);
        assert!((bins[4].ratio_of_sums.unwrap() - 2.0).abs() < 1e-12);
        assert_eq!(bins[1].n_pairs, 0);
        assert_eq!(bins[1].ratio_of_sums, None);
    }
}
