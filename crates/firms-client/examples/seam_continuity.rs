//! Measure continuity of raw vs harmonized calendar metrics across MODIS/VIIRS sensor seams
//! (2002-07 Aqua, 2012-01-20 SNPP, 2018-04 NOAA-20) on the offline FIRMS country archive.
//!
//! Usage:
//!   ARCHIVE_CACHE_DIR=../../.cache/firms-archive \
//!     cargo run --release -p pyrosync-firms --example seam_continuity -- <Country>... [--from 2000] [--to 2025] [--out <csv>]

use anyhow::{Context, Result, bail};
use pyrosync_core::calendar::build_calendar_with;
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::effort::{EffortFactor, estimate_effort};
use pyrosync_core::harmonize::harmonize_with_config;
use pyrosync_core::models::{CalendarCell, Sensor};
use pyrosync_firms::archive::{ArchiveClient, ArchiveError};
use pyrosync_firms::parse::retain_wildfires;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;
use tracing_subscriber::EnvFilter;

const ARCHIVE_FETCH_SENSORS: [Sensor; 3] =
    [Sensor::ModisTerra, Sensor::ViirsSNPP, Sensor::ViirsNOAA20];
const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const DEFAULT_FROM_YEAR: i32 = 2000;
const DEFAULT_TO_YEAR: i32 = 2025;
const NOISE_COUNT_THRESHOLD: u64 = 100;
const SEAM_YEARS: [i32; 3] = [2002, 2012, 2018];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Metric {
    Raw,
    Count,
    CountNormalized,
    TotalFrp,
    FYear,
    MeanFrpRaw,
}

impl Metric {
    const ALL: [Self; 6] = [
        Self::Raw,
        Self::Count,
        Self::CountNormalized,
        Self::TotalFrp,
        Self::FYear,
        Self::MeanFrpRaw,
    ];

    fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Count => "count",
            Self::CountNormalized => "count_normalized",
            Self::TotalFrp => "total_frp",
            Self::FYear => "F_year",
            Self::MeanFrpRaw => "mean_frp_raw",
        }
    }

    fn extract(self, summary: &AnnualSummary) -> Option<f64> {
        match self {
            Self::Raw => (summary.raw > 0).then_some(summary.raw as f64),
            Self::Count => (summary.count > 0).then_some(summary.count as f64),
            Self::CountNormalized => summary.count_normalized.filter(|v| v.is_finite() && *v > 0.0),
            Self::TotalFrp => summary.total_frp.filter(|v| v.is_finite() && *v > 0.0),
            Self::FYear => summary.f_year.filter(|v| v.is_finite() && *v > 0.0),
            Self::MeanFrpRaw => summary.mean_frp_raw.filter(|v| v.is_finite() && *v > 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SeamSpec {
    name: &'static str,
    before: i32,
    after: i32,
}

const SEAMS: [SeamSpec; 4] = [
    SeamSpec {
        name: "aqua_2002",
        before: 2001,
        after: 2003,
    },
    SeamSpec {
        name: "snpp_2012_1y",
        before: 2011,
        after: 2012,
    },
    SeamSpec {
        name: "snpp_2012_2y",
        before: 2011,
        after: 2013,
    },
    SeamSpec {
        name: "noaa20_2018",
        before: 2017,
        after: 2019,
    },
];

struct Args {
    countries: Vec<String>,
    from: i32,
    to: i32,
    out: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct AnnualSummary {
    year: i32,
    months: usize,
    raw: u64,
    count: u64,
    count_normalized: Option<f64>,
    total_frp: Option<f64>,
    f_year: Option<f64>,
    mean_frp_raw: Option<f64>,
    is_noise: bool,
    tier: &'static str,
}

#[derive(Debug, Clone)]
struct NoiseRef {
    n_pairs: usize,
    median: Option<f64>,
    p90: Option<f64>,
    samples_sorted: Vec<f64>,
}

#[derive(Debug, Clone)]
struct SeamRow {
    country: String,
    seam: &'static str,
    before_year: i32,
    after_year: i32,
    delta_years: i32,
    metric: Metric,
    before_val: Option<f64>,
    after_val: Option<f64>,
    log_step: Option<f64>,
    abs_log_step: Option<f64>,
    ratio: Option<f64>,
    before_count: u64,
    after_count: u64,
    noise_flagged: bool,
    ref_n_pairs: usize,
    ref_median: Option<f64>,
    ref_p90: Option<f64>,
    z_score: Option<f64>,
    pct_rank: Option<f64>,
}

fn print_usage() {
    println!(
        "\
Usage: seam_continuity <Country>... [--from {DEFAULT_FROM_YEAR}] [--to {DEFAULT_TO_YEAR}] [--out <csv>]

Measure raw vs harmonized metric continuity across Aqua (2002), SNPP (2012), and NOAA-20 (2018) seams.
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

fn volume_tier(count: u64) -> &'static str {
    if count < 10 {
        "<10"
    } else if count < NOISE_COUNT_THRESHOLD {
        "<100"
    } else if count < 1000 {
        "100-999"
    } else {
        ">=1000"
    }
}

fn summarize_years(
    from: i32,
    to: i32,
    raw_by_year: &BTreeMap<i32, u64>,
    cells: &[CalendarCell],
) -> BTreeMap<i32, AnnualSummary> {
    let mut cells_by_year: BTreeMap<i32, Vec<&CalendarCell>> = BTreeMap::new();
    for cell in cells {
        cells_by_year.entry(cell.year).or_default().push(cell);
    }

    let mut out = BTreeMap::new();
    for year in from..=to {
        let raw = raw_by_year.get(&year).copied().unwrap_or(0);
        let year_cells = cells_by_year.get(&year).map(Vec::as_slice).unwrap_or(&[]);
        let months = year_cells.len();
        let count: u64 = year_cells.iter().map(|c| c.count).sum();

        // Keep `None` when no month in the year has an effort-normalized count (e.g. pre-Aqua 2000–2001).
        let mut norm_sum = 0.0_f64;
        let mut has_norm = false;
        for c in year_cells {
            if let Some(v) = c.count_normalized && v.is_finite() {
                norm_sum += v;
                has_norm = true;
            }
        }
        let count_normalized = has_norm.then_some(norm_sum);

        let total_frp = (count > 0).then(|| year_cells.iter().map(|c| c.total_frp).sum::<f64>());
        let f_year = match (total_frp, count_normalized) {
            (Some(frp), Some(norm)) if norm > 0.0 => Some(frp / norm),
            _ => None,
        };
        let mean_frp_raw = (count > 0).then(|| {
            year_cells
                .iter()
                .map(|c| c.mean_frp_raw * c.count as f64)
                .sum::<f64>()
                / count as f64
        });

        out.insert(
            year,
            AnnualSummary {
                year,
                months,
                raw,
                count,
                count_normalized,
                total_frp,
                f_year,
                mean_frp_raw,
                is_noise: count < NOISE_COUNT_THRESHOLD,
                tier: volume_tier(count),
            },
        );
    }
    out
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

fn pair_crosses_seam(y1: i32, y2: i32) -> bool {
    SEAM_YEARS.iter().any(|&s| (y1..=y2).contains(&s))
}

fn build_noise_reference(
    years: &BTreeMap<i32, AnnualSummary>,
    metric: Metric,
    delta: i32,
) -> NoiseRef {
    let mut samples = Vec::new();
    for (&y1, s1) in years {
        let y2 = y1 + delta;
        if pair_crosses_seam(y1, y2) {
            continue;
        }
        let Some(s2) = years.get(&y2) else {
            continue;
        };
        if let (Some(v1), Some(v2)) = (metric.extract(s1), metric.extract(s2)) {
            let abs_log = (v2 / v1).ln().abs();
            if abs_log.is_finite() {
                samples.push(abs_log);
            }
        }
    }
    samples.sort_by(f64::total_cmp);
    let median = quantile(&samples, 0.50);
    let p90 = quantile(&samples, 0.90);
    NoiseRef {
        n_pairs: samples.len(),
        median,
        p90,
        samples_sorted: samples,
    }
}

fn compute_seam_rows(
    country: &str,
    years: &BTreeMap<i32, AnnualSummary>,
    noise_refs: &HashMap<(i32, &'static str), NoiseRef>,
) -> Vec<SeamRow> {
    let mut rows = Vec::with_capacity(SEAMS.len() * Metric::ALL.len());

    for seam in SEAMS {
        let delta = seam.after - seam.before;
        let s_before = years.get(&seam.before);
        let s_after = years.get(&seam.after);
        let before_count = s_before.map_or(0, |s| s.count);
        let after_count = s_after.map_or(0, |s| s.count);
        let noise_flagged = s_before.is_none_or(|s| s.is_noise) || s_after.is_none_or(|s| s.is_noise);

        for metric in Metric::ALL {
            let before_val = s_before.and_then(|s| metric.extract(s));
            let after_val = s_after.and_then(|s| metric.extract(s));
            let (log_step, abs_log_step, ratio) = match (before_val, after_val) {
                (Some(b), Some(a)) if b > 0.0 && a > 0.0 => {
                    let r = a / b;
                    let l = r.ln();
                    (Some(l), Some(l.abs()), Some(r))
                }
                _ => (None, None, None),
            };

            let nref = &noise_refs[&(delta, metric.as_str())];
            let z_score = match (abs_log_step, nref.median, nref.p90) {
                (Some(step), Some(med), Some(p90)) if (p90 - med).abs() > 1e-12 => {
                    Some((step - med) / (p90 - med))
                }
                _ => None,
            };
            let pct_rank = abs_log_step.and_then(|step| {
                (!nref.samples_sorted.is_empty()).then(|| {
                    let le = nref.samples_sorted.iter().filter(|&&x| x <= step).count();
                    le as f64 / nref.samples_sorted.len() as f64
                })
            });

            rows.push(SeamRow {
                country: country.to_string(),
                seam: seam.name,
                before_year: seam.before,
                after_year: seam.after,
                delta_years: delta,
                metric,
                before_val,
                after_val,
                log_step,
                abs_log_step,
                ratio,
                before_count,
                after_count,
                noise_flagged,
                ref_n_pairs: nref.n_pairs,
                ref_median: nref.median,
                ref_p90: nref.p90,
                z_score,
                pct_rank,
            });
        }
    }

    rows
}

fn fmt_opt(val: Option<f64>, decimals: usize) -> String {
    match val {
        Some(v) if v.is_finite() => format!("{v:.decimals$}"),
        _ => "NA".to_string(),
    }
}

fn fmt_signed_opt(val: Option<f64>, decimals: usize) -> String {
    match val {
        Some(v) if v.is_finite() => format!("{v:+.decimals$}"),
        _ => "NA".to_string(),
    }
}

fn print_effort_table(factors: &[EffortFactor]) {
    println!("\n[Effort factors (Omega)]");
    println!("{:<42} {:>6} {:>10} {:>10} {:>10}", "platforms", "months", "total", "modis_obs", "omega");
    println!("{}", "-".repeat(82));
    for f in factors {
        let platforms = if f.platforms.is_empty() {
            "-".to_string()
        } else {
            f.platforms
                .iter()
                .map(|s| format!("{s:?}"))
                .collect::<Vec<_>>()
                .join("+")
        };
        println!(
            "{:<42} {:>6} {:>10} {:>10} {:>10}",
            platforms,
            f.months,
            f.total,
            f.modis_observable,
            fmt_opt(f.omega, 4),
        );
    }
}

fn print_annual_table(years: &BTreeMap<i32, AnnualSummary>) {
    println!("\n[Annual totals]");
    println!(
        "{:>4} {:>3} {:>10} {:>10} {:>11} {:>13} {:>9} {:>10} {:>8}",
        "year", "mo", "raw", "count", "count_norm", "total_frp", "F_year", "mean_raw", "tier"
    );
    println!("{}", "-".repeat(96));
    for s in years.values() {
        println!(
            "{:>4} {:>3} {:>10} {:>10} {:>11} {:>13} {:>9} {:>10} {:>8}",
            s.year,
            s.months,
            s.raw,
            s.count,
            fmt_opt(s.count_normalized, 1),
            fmt_opt(s.total_frp, 1),
            fmt_opt(s.f_year, 2),
            fmt_opt(s.mean_frp_raw, 2),
            s.tier,
        );
    }
}

fn print_noise_table(noise_refs: &HashMap<(i32, &'static str), NoiseRef>) {
    println!("\n[Non-seam noise reference (|ln(after/before)|)]");
    println!(
        "{:>5} {:<18} {:>7} {:>10} {:>10}",
        "delta", "metric", "n_pairs", "median", "p90"
    );
    println!("{}", "-".repeat(55));
    for delta in [1, 2] {
        for metric in Metric::ALL {
            let r = &noise_refs[&(delta, metric.as_str())];
            println!(
                "{:>5} {:<18} {:>7} {:>10} {:>10}",
                delta,
                metric.as_str(),
                r.n_pairs,
                fmt_opt(r.median, 4),
                fmt_opt(r.p90, 4),
            );
        }
    }
}

fn print_seam_table(rows: &[SeamRow]) {
    println!("\n[Seam steps]");
    println!(
        "{:<13} {:<9} {:<17} {:>11} {:>11} {:>9} {:>8} {:>8} {:>7} {:>6} flags",
        "seam", "years", "metric", "before", "after", "log_step", "ref_med", "ref_p90", "z_like", "pct"
    );
    println!("{}", "-".repeat(122));
    for r in rows {
        let mut flags = Vec::new();
        if r.noise_flagged {
            flags.push("NOISE(<100)");
        }
        if let (Some(abs_s), Some(p90)) = (r.abs_log_step, r.ref_p90)
            && abs_s > p90
        {
            flags.push(">P90");
        }
        let flag_str = if flags.is_empty() {
            "-".to_string()
        } else {
            flags.join(",")
        };
        println!(
            "{:<13} {:>4}->{:<4} {:<17} {:>11} {:>11} {:>9} {:>8} {:>8} {:>7} {:>6} {}",
            r.seam,
            r.before_year,
            r.after_year,
            r.metric.as_str(),
            fmt_opt(r.before_val, 2),
            fmt_opt(r.after_val, 2),
            fmt_signed_opt(r.log_step, 4),
            fmt_opt(r.ref_median, 4),
            fmt_opt(r.ref_p90, 4),
            fmt_signed_opt(r.z_score, 2),
            fmt_opt(r.pct_rank.map(|p| p * 100.0), 0),
            flag_str,
        );
    }
}

fn print_cross_country_summary(all_rows: &[SeamRow]) {
    println!("\n================================================================================");
    println!("ACROSS-COUNTRY MEDIAN |log_step| (non-noise rows; all rows in parentheses)");
    println!("================================================================================");
    println!(
        "{:<13} {:<17} {:>6} {:>12} {:>12} {:>10}",
        "seam", "metric", "n_ok", "med_abs_log", "all_med_abs", "gt_p90"
    );
    println!("{}", "-".repeat(76));
    for seam in SEAMS {
        for metric in Metric::ALL {
            let matching: Vec<&SeamRow> = all_rows
                .iter()
                .filter(|r| r.seam == seam.name && r.metric == metric)
                .collect();
            let mut clean: Vec<f64> = matching
                .iter()
                .filter(|r| !r.noise_flagged)
                .filter_map(|r| r.abs_log_step)
                .collect();
            let mut all_abs: Vec<f64> = matching.iter().filter_map(|r| r.abs_log_step).collect();
            clean.sort_by(f64::total_cmp);
            all_abs.sort_by(f64::total_cmp);
            let gt_p90 = matching
                .iter()
                .filter(|r| match (r.abs_log_step, r.ref_p90) {
                    (Some(a), Some(p)) => a > p,
                    _ => false,
                })
                .count();
            println!(
                "{:<13} {:<17} {:>6} {:>12} {:>12} {:>4}/{}",
                seam.name,
                metric.as_str(),
                clean.len(),
                fmt_opt(quantile(&clean, 0.50), 4),
                fmt_opt(quantile(&all_abs, 0.50), 4),
                gt_p90,
                all_abs.len(),
            );
        }
    }
}

fn write_csv(path: &PathBuf, rows: &[SeamRow]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut w = BufWriter::new(file);
    writeln!(
        w,
        "country,seam,before_year,after_year,delta_years,metric,before_val,after_val,log_step,abs_log_step,ratio,before_count,after_count,noise_flagged,ref_n_pairs,ref_median,ref_p90,z_score,pct_rank"
    )?;
    for r in rows {
        writeln!(
            w,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            r.country,
            r.seam,
            r.before_year,
            r.after_year,
            r.delta_years,
            r.metric.as_str(),
            fmt_opt(r.before_val, 6),
            fmt_opt(r.after_val, 6),
            fmt_opt(r.log_step, 6),
            fmt_opt(r.abs_log_step, 6),
            fmt_opt(r.ratio, 6),
            r.before_count,
            r.after_count,
            r.noise_flagged,
            r.ref_n_pairs,
            fmt_opt(r.ref_median, 6),
            fmt_opt(r.ref_p90, 6),
            fmt_opt(r.z_score, 4),
            fmt_opt(r.pct_rank, 4),
        )?;
    }
    w.flush()?;
    Ok(())
}

async fn analyze_country(
    archive: &ArchiveClient,
    config: &PipelineConfig,
    country: &str,
    from: i32,
    to: i32,
) -> Result<Vec<SeamRow>> {
    let total_t0 = Instant::now();
    let load_t0 = Instant::now();

    let mut modis = Vec::new();
    let mut viirs = Vec::new();
    let mut raw_by_year: BTreeMap<i32, u64> = BTreeMap::new();
    let mut found_any_file = false;

    for year in from..=to {
        for sensor in ARCHIVE_FETCH_SENSORS {
            if !archive.covers(sensor, year) {
                continue;
            }
            match archive.fetch_country_year(sensor, country, year).await {
                Ok(mut hotspots) => {
                    found_any_file = true;
                    retain_wildfires(&mut hotspots);
                    *raw_by_year.entry(year).or_insert(0) += hotspots.len() as u64;
                    if sensor.is_modis() {
                        modis.append(&mut hotspots);
                    } else {
                        viirs.append(&mut hotspots);
                    }
                }
                Err(ArchiveError::NotCached { .. }) if archive.sensor_year_complete(sensor, year) => {
                    // Completed archive year without a country CSV means zero detections for that sensor-year.
                }
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!("failed to load {sensor:?} {country} {year} from archive")
                    });
                }
            }
        }
    }

    if !found_any_file {
        bail!(
            "no archive CSV files found for '{country}' ({from}..={to}) in {}",
            archive.cache_dir().display()
        );
    }

    let load_secs = load_t0.elapsed().as_secs_f64();
    let raw_modis = modis.len();
    let raw_viirs = viirs.len();

    let harm_t0 = Instant::now();
    let harmonized = harmonize_with_config(modis, viirs, config);
    let harm_secs = harm_t0.elapsed().as_secs_f64();
    let harm_count = harmonized.len();

    let cal_t0 = Instant::now();
    let effort_factors = estimate_effort(&harmonized);
    let cells = build_calendar_with(&harmonized, &config.uncertainty, &config.calibration);
    let cal_secs = cal_t0.elapsed().as_secs_f64();
    drop(harmonized);

    let years = summarize_years(from, to, &raw_by_year, &cells);
    let mut noise_refs = HashMap::new();
    for delta in [1, 2] {
        for metric in Metric::ALL {
            noise_refs.insert(
                (delta, metric.as_str()),
                build_noise_reference(&years, metric, delta),
            );
        }
    }
    let rows = compute_seam_rows(country, &years, &noise_refs);

    println!("\n================================================================================");
    println!(
        "COUNTRY: {country} ({from}..={to}) | raw_modis={raw_modis} raw_viirs={raw_viirs} harmonized={harm_count}"
    );
    println!(
        "TIMING: load={load_secs:.2}s harmonize={harm_secs:.2}s calendar={cal_secs:.2}s total={:.2}s",
        total_t0.elapsed().as_secs_f64()
    );
    print_effort_table(&effort_factors);
    print_annual_table(&years);
    print_noise_table(&noise_refs);
    print_seam_table(&rows);

    Ok(rows)
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
    let config = PipelineConfig::default();

    println!("Archive cache: {}", cache_dir.display());
    println!(
        "Countries ({}): {}",
        args.countries.len(),
        args.countries.join(", ")
    );

    let overall_t0 = Instant::now();
    let mut all_rows = Vec::new();
    for country in &args.countries {
        let rows = analyze_country(&archive, &config, country, args.from, args.to).await?;
        all_rows.extend(rows);
    }

    if args.countries.len() > 1 {
        print_cross_country_summary(&all_rows);
    }

    if let Some(ref out_path) = args.out {
        write_csv(out_path, &all_rows)?;
        println!("\nCSV written: {} ({} rows)", out_path.display(), all_rows.len());
    }

    println!(
        "\nCompleted {} countries in {:.2}s",
        args.countries.len(),
        overall_t0.elapsed().as_secs_f64()
    );
    Ok(())
}
