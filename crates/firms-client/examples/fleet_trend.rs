//! Full-fleet (MODIS + VIIRS S-NPP + VIIRS NOAA-20) 2000-2025 trend and level-shift
//! verification on the offline FIRMS country archive (task K7).
//!
//! Usage:
//!   ARCHIVE_CACHE_DIR=.cache/firms-archive \
//!     cargo run --release -p pyrosync-firms --example fleet_trend -- <Country>... [--from 2000] [--to 2025] [--out <csv>]

use anyhow::{Context, Result, bail};
use pyrosync_core::calendar::build_calendar_with;
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::effort::{EffortFactor, estimate_effort};
use pyrosync_core::harmonize::harmonize_with_config;
use pyrosync_core::models::{CalendarCell, HarmonizedHotspot, Sensor};
use pyrosync_firms::archive::{ArchiveClient, ArchiveError};
use pyrosync_firms::parse::retain_wildfires;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing_subscriber::EnvFilter;

const ARCHIVE_FETCH_SENSORS: [Sensor; 3] =
    [Sensor::ModisTerra, Sensor::ViirsSNPP, Sensor::ViirsNOAA20];
const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const DEFAULT_FROM_YEAR: i32 = 2000;
const DEFAULT_TO_YEAR: i32 = 2025;
// +/-10% raw shift threshold: ln(1.10) ~= 0.0953.
const MIN_TESTABLE_ABS_LOG_RAW: f64 = 0.0953;
const NOAA21_NOT_MEASURABLE_MSG: &str =
    "not measurable: NOAA-21 is not in the FIRMS country archive";

struct Args {
    countries: Vec<String>,
    from: i32,
    to: i32,
    out: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct AnnualRow {
    year: i32,
    raw_total: u64,
    norm_total: Option<f64>,
    active_months: usize,
    norm_months: usize,
    missing_norm_months: usize,
    missing_archive_file: bool,
    is_full_year: bool,
    omega_labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrendVerdict {
    Pass,
    Fail,
    Unavailable,
}

impl TrendVerdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
}

#[derive(Debug, Clone)]
struct TrendResult {
    raw_n: usize,
    raw_span: Option<(i32, i32)>,
    rho_raw: Option<f64>,
    norm_n: usize,
    norm_span: Option<(i32, i32)>,
    rho_norm: Option<f64>,
    verdict: TrendVerdict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShiftVerdict {
    Pass,
    Fail,
    RawShiftTooSmall,
    NotMeasured,
    Unavailable,
}

impl ShiftVerdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::RawShiftTooSmall => "RAW_SHIFT_TOO_SMALL",
            Self::NotMeasured => "NOT_MEASURED",
            Self::Unavailable => "UNAVAILABLE",
        }
    }

    fn display_label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::RawShiftTooSmall => "raw shift too small to test",
            Self::NotMeasured => NOAA21_NOT_MEASURABLE_MSG,
            Self::Unavailable => "unavailable (missing data)",
        }
    }
}

#[derive(Debug, Clone)]
struct ShiftResult {
    name: &'static str,
    window_before: &'static str,
    window_after: &'static str,
    n_raw_before: usize,
    n_raw_after: usize,
    n_norm_before: usize,
    n_norm_after: usize,
    raw_before: Option<f64>,
    raw_after: Option<f64>,
    raw_shift: Option<f64>,
    norm_before: Option<f64>,
    norm_after: Option<f64>,
    norm_shift: Option<f64>,
    measured: bool,
    verdict: ShiftVerdict,
    note: &'static str,
}

#[derive(Debug, Clone)]
struct RegionAnalysis {
    region: String,
    effort_factors: Vec<EffortFactor>,
    annual: Vec<AnnualRow>,
    trend: TrendResult,
    shifts: Vec<ShiftResult>,
}

fn print_usage() {
    println!(
        "\
Usage: fleet_trend <Country>... [--from {DEFAULT_FROM_YEAR}] [--to {DEFAULT_TO_YEAR}] [--out <csv>]

Evaluate full-fleet (MODIS + S-NPP + NOAA-20) trend and level-shift criteria on the offline FIRMS country archive.
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
            other => {
                if !countries.iter().any(|c| c == other) {
                    countries.push(other.to_string());
                }
            }
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

fn average_ranks(values: &[f64]) -> Vec<f64> {
    let n = values.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| values[a].total_cmp(&values[b]));

    let mut ranks = vec![0.0_f64; n];
    let mut start = 0usize;
    while start < n {
        let mut end = start + 1;
        while end < n && values[order[end]].total_cmp(&values[order[start]]).is_eq() {
            end += 1;
        }
        let avg_rank = (start + 1 + end) as f64 / 2.0;
        for &idx in &order[start..end] {
            ranks[idx] = avg_rank;
        }
        start = end;
    }
    ranks
}

fn spearman_rho(xs: &[f64], ys: &[f64]) -> Option<f64> {
    if xs.len() != ys.len() || xs.len() < 3 {
        return None;
    }
    if xs.iter().any(|v| !v.is_finite()) || ys.iter().any(|v| !v.is_finite()) {
        return None;
    }

    let rx = average_ranks(xs);
    let ry = average_ranks(ys);
    let n = rx.len() as f64;
    let mean_x = rx.iter().sum::<f64>() / n;
    let mean_y = ry.iter().sum::<f64>() / n;

    let mut num = 0.0_f64;
    let mut den_x = 0.0_f64;
    let mut den_y = 0.0_f64;
    for (x, y) in rx.iter().zip(ry.iter()) {
        let dx = x - mean_x;
        let dy = y - mean_y;
        num += dx * dy;
        den_x += dx * dx;
        den_y += dy * dy;
    }

    let denom = (den_x * den_y).sqrt();
    if denom <= 0.0 || !denom.is_finite() {
        return None;
    }

    Some((num / denom).clamp(-1.0, 1.0))
}

fn rollup_calendar_by_year(
    cells: &[CalendarCell],
    missing_file_years: &BTreeSet<i32>,
) -> Vec<AnnualRow> {
    let mut by_year: BTreeMap<i32, Vec<&CalendarCell>> = BTreeMap::new();
    let mut all_years = missing_file_years.clone();
    for cell in cells {
        by_year.entry(cell.year).or_default().push(cell);
        all_years.insert(cell.year);
    }

    all_years
        .into_iter()
        .map(|year| {
            let mut year_cells = by_year.remove(&year).unwrap_or_default();
            year_cells.sort_by_key(|c| c.month);
            let valid_months: BTreeSet<u32> = year_cells
                .iter()
                .map(|c| c.month)
                .filter(|m| (1..=12).contains(m))
                .collect();
            let active_months = valid_months.len();
            let missing_archive_file = missing_file_years.contains(&year);
            // 2000 only has Nov-Dec MODIS coverage, so it cannot be treated as a full annual total.
            let is_full_year = year != 2000 && !missing_archive_file && active_months > 0;

            let mut raw_total = 0_u64;
            let mut norm_sum = 0.0_f64;
            let mut norm_months = 0usize;
            let mut missing_norm_months = 0usize;
            let mut omega_labels = Vec::new();

            for cell in &year_cells {
                raw_total = raw_total.saturating_add(cell.count);
                if let Some(norm) = cell.count_normalized
                    && norm.is_finite()
                {
                    norm_sum += norm;
                    norm_months += 1;
                } else {
                    missing_norm_months += 1;
                }
                let omega_label = match cell.effort_omega {
                    Some(w) if w.is_finite() => format!("{w:.3}"),
                    _ => "-".to_string(),
                };
                if !omega_labels.contains(&omega_label) {
                    omega_labels.push(omega_label);
                }
            }

            // Exclude any year where one or more active months lack a normalized count
            // rather than silently summing a partial normalized year (e.g., pre-Aqua H1 2002).
            let norm_total =
                (is_full_year && active_months > 0 && missing_norm_months == 0).then_some(norm_sum);

            AnnualRow {
                year,
                raw_total,
                norm_total,
                active_months,
                norm_months,
                missing_norm_months,
                missing_archive_file,
                is_full_year,
                omega_labels,
            }
        })
        .collect()
}

fn compute_trend(annual: &[AnnualRow]) -> TrendResult {
    let full_rows: Vec<&AnnualRow> = annual
        .iter()
        .filter(|r| r.is_full_year && (2001..=2025).contains(&r.year))
        .collect();

    let raw_years: Vec<f64> = full_rows.iter().map(|r| r.year as f64).collect();
    let raw_vals: Vec<f64> = full_rows.iter().map(|r| r.raw_total as f64).collect();
    let raw_span = full_rows
        .first()
        .zip(full_rows.last())
        .map(|(a, b)| (a.year, b.year));
    let rho_raw = spearman_rho(&raw_years, &raw_vals);

    let norm_rows: Vec<&AnnualRow> = full_rows
        .iter()
        .copied()
        .filter(|r| r.norm_total.is_some())
        .collect();
    let norm_years: Vec<f64> = norm_rows.iter().map(|r| r.year as f64).collect();
    let norm_vals: Vec<f64> = norm_rows.iter().filter_map(|r| r.norm_total).collect();
    let norm_span = norm_rows
        .first()
        .zip(norm_rows.last())
        .map(|(a, b)| (a.year, b.year));
    let rho_norm = spearman_rho(&norm_years, &norm_vals);

    let verdict = match (rho_raw, rho_norm) {
        (Some(rr), Some(nr)) => {
            if nr.abs() < rr.abs() * 0.5 {
                TrendVerdict::Pass
            } else {
                TrendVerdict::Fail
            }
        }
        _ => TrendVerdict::Unavailable,
    };

    TrendResult {
        raw_n: full_rows.len(),
        raw_span,
        rho_raw,
        norm_n: norm_rows.len(),
        norm_span,
        rho_norm,
        verdict,
    }
}

fn evaluate_shift(raw_shift: Option<f64>, norm_shift: Option<f64>) -> ShiftVerdict {
    let Some(rs) = raw_shift.filter(|v| v.is_finite() && *v > 0.0) else {
        return ShiftVerdict::Unavailable;
    };
    let abs_log_raw = rs.ln().abs();
    if abs_log_raw < MIN_TESTABLE_ABS_LOG_RAW {
        return ShiftVerdict::RawShiftTooSmall;
    }
    let Some(ns) = norm_shift.filter(|v| v.is_finite() && *v > 0.0) else {
        return ShiftVerdict::Unavailable;
    };
    if ns.ln().abs() < abs_log_raw * 0.5 {
        ShiftVerdict::Pass
    } else {
        ShiftVerdict::Fail
    }
}

fn ratio_opt(before: Option<f64>, after: Option<f64>) -> Option<f64> {
    match (before, after) {
        (Some(b), Some(a)) if b > 0.0 && a > 0.0 && b.is_finite() && a.is_finite() => Some(a / b),
        _ => None,
    }
}

struct WindowSummary {
    raw_mean: Option<f64>,
    n_raw: usize,
    norm_mean: Option<f64>,
    n_norm: usize,
}

fn window_block_means(annual: &[AnnualRow], years: RangeInclusive<i32>) -> WindowSummary {
    let mut raw_sum = 0.0_f64;
    let mut n_raw = 0usize;
    let mut norm_sum = 0.0_f64;
    let mut n_norm = 0usize;

    for row in annual {
        if !years.contains(&row.year) || !row.is_full_year {
            continue;
        }
        raw_sum += row.raw_total as f64;
        n_raw += 1;
        if let Some(norm) = row.norm_total {
            norm_sum += norm;
            n_norm += 1;
        }
    }

    WindowSummary {
        raw_mean: (n_raw > 0).then(|| raw_sum / n_raw as f64),
        n_raw,
        norm_mean: (n_norm > 0).then(|| norm_sum / n_norm as f64),
        n_norm,
    }
}

fn may_dec_totals(
    cells: &[CalendarCell],
    year: i32,
    missing_file_years: &BTreeSet<i32>,
) -> WindowSummary {
    if missing_file_years.contains(&year) {
        return WindowSummary {
            raw_mean: None,
            n_raw: 0,
            norm_mean: None,
            n_norm: 0,
        };
    }

    let season_cells: Vec<&CalendarCell> = cells
        .iter()
        .filter(|c| c.year == year && (5..=12).contains(&c.month))
        .collect();
    if season_cells.is_empty() {
        return WindowSummary {
            raw_mean: None,
            n_raw: 0,
            norm_mean: None,
            n_norm: 0,
        };
    }

    let raw_sum: u64 = season_cells.iter().map(|c| c.count).sum();
    let mut norm_sum = 0.0_f64;
    let mut norm_months = 0usize;
    for cell in &season_cells {
        if let Some(norm) = cell.count_normalized
            && norm.is_finite()
        {
            norm_sum += norm;
            norm_months += 1;
        }
    }
    let norm_total = (norm_months == season_cells.len()).then_some(norm_sum);

    WindowSummary {
        raw_mean: Some(raw_sum as f64),
        n_raw: season_cells.len(),
        norm_mean: norm_total,
        n_norm: if norm_total.is_some() { norm_months } else { 0 },
    }
}

fn build_shift_from_summaries(
    name: &'static str,
    window_before: &'static str,
    window_after: &'static str,
    before: WindowSummary,
    after: WindowSummary,
) -> ShiftResult {
    let raw_shift = ratio_opt(before.raw_mean, after.raw_mean);
    let norm_shift = ratio_opt(before.norm_mean, after.norm_mean);
    let verdict = evaluate_shift(raw_shift, norm_shift);
    let note = match verdict {
        ShiftVerdict::RawShiftTooSmall => "raw shift too small to test",
        ShiftVerdict::Unavailable => "missing raw or normalized data in window",
        _ => "",
    };

    ShiftResult {
        name,
        window_before,
        window_after,
        n_raw_before: before.n_raw,
        n_raw_after: after.n_raw,
        n_norm_before: before.n_norm,
        n_norm_after: after.n_norm,
        raw_before: before.raw_mean,
        raw_after: after.raw_mean,
        raw_shift,
        norm_before: before.norm_mean,
        norm_after: after.norm_mean,
        norm_shift,
        measured: true,
        verdict,
        note,
    }
}

fn compute_shifts(
    annual: &[AnnualRow],
    cells: &[CalendarCell],
    missing_file_years: &BTreeSet<i32>,
) -> Vec<ShiftResult> {
    // 2012 (S-NPP): 5 full years before (2007-2011) vs 5 full years after (2013-2017); 2012 excluded.
    let shift_2012 = build_shift_from_summaries(
        "2012_snpp",
        "2007-2011",
        "2013-2017",
        window_block_means(annual, 2007..=2011),
        window_block_means(annual, 2013..=2017),
    );

    // 2018 (NOAA-20 from 2018-04): May-Dec 2017 vs May-Dec 2018 same-season comparison.
    let shift_2018_season = build_shift_from_summaries(
        "2018_noaa20_may_dec",
        "2017-05..12",
        "2018-05..12",
        may_dec_totals(cells, 2017, missing_file_years),
        may_dec_totals(cells, 2018, missing_file_years),
    );

    // 2018 (NOAA-20): 3-year full-year block comparison (2015-2017 vs 2019-2021); 2018 excluded.
    let shift_2018_block = build_shift_from_summaries(
        "2018_noaa20_3yr",
        "2015-2017",
        "2019-2021",
        window_block_means(annual, 2015..=2017),
        window_block_means(annual, 2019..=2021),
    );

    // 2023 (NOAA-21): explicitly unmeasurable on the FIRMS country archive.
    let shift_2023 = ShiftResult {
        name: "2023_noaa21",
        window_before: "-",
        window_after: "-",
        n_raw_before: 0,
        n_raw_after: 0,
        n_norm_before: 0,
        n_norm_after: 0,
        raw_before: None,
        raw_after: None,
        raw_shift: None,
        norm_before: None,
        norm_after: None,
        norm_shift: None,
        measured: false,
        verdict: ShiftVerdict::NotMeasured,
        note: NOAA21_NOT_MEASURABLE_MSG,
    };

    vec![
        shift_2012,
        shift_2018_season,
        shift_2018_block,
        shift_2023,
    ]
}

fn analyze_harmonized(
    region: String,
    harmonized: &[HarmonizedHotspot],
    missing_file_years: &BTreeSet<i32>,
    config: &PipelineConfig,
) -> RegionAnalysis {
    let effort_factors = estimate_effort(harmonized);
    let cells = build_calendar_with(harmonized, &config.uncertainty, &config.calibration);
    let annual = rollup_calendar_by_year(&cells, missing_file_years);
    let trend = compute_trend(&annual);
    let shifts = compute_shifts(&annual, &cells, missing_file_years);

    RegionAnalysis {
        region,
        effort_factors,
        annual,
        trend,
        shifts,
    }
}

async fn load_and_harmonize_country(
    archive: &ArchiveClient,
    config: &PipelineConfig,
    country: &str,
    from: i32,
    to: i32,
) -> Result<(Vec<HarmonizedHotspot>, BTreeSet<i32>)> {
    let mut country_harmonized = Vec::new();
    let mut missing_file_years = BTreeSet::new();
    let mut found_any_file = false;

    for year in from..=to {
        let mut modis_year = Vec::new();
        let mut viirs_year = Vec::new();
        let mut year_missing = false;

        for sensor in ARCHIVE_FETCH_SENSORS {
            if !archive.covers(sensor, year) {
                continue;
            }
            match archive.fetch_country_year(sensor, country, year).await {
                Ok(mut hotspots) => {
                    found_any_file = true;
                    retain_wildfires(&mut hotspots);
                    if sensor.is_modis() {
                        modis_year.append(&mut hotspots);
                    } else {
                        viirs_year.append(&mut hotspots);
                    }
                }
                Err(ArchiveError::NotCached { .. }) if archive.sensor_year_complete(sensor, year) => {
                    // Completed archive year without a country CSV means zero detections.
                }
                Err(ArchiveError::NotCached { path }) => {
                    eprintln!(
                        "warning: missing archive file for {country} {sensor:?} {year} ({}); skipping",
                        path.display()
                    );
                    year_missing = true;
                }
                Err(ArchiveError::NotFound { .. }) => {
                    eprintln!(
                        "warning: archive file not found for {country} {sensor:?} {year}; skipping"
                    );
                    year_missing = true;
                }
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!("failed to load {sensor:?} {country} {year} from archive")
                    });
                }
            }
        }

        if year_missing {
            missing_file_years.insert(year);
        }
        if !modis_year.is_empty() || !viirs_year.is_empty() {
            let mut harmonized_year = harmonize_with_config(modis_year, viirs_year, config);
            country_harmonized.append(&mut harmonized_year);
        }
    }

    if !found_any_file {
        bail!(
            "no archive CSV files found for '{country}' ({from}..={to}) in {}",
            archive.cache_dir().display()
        );
    }

    Ok((country_harmonized, missing_file_years))
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

fn format_platforms(platforms: &[Sensor]) -> String {
    if platforms.is_empty() {
        "-".to_string()
    } else {
        platforms
            .iter()
            .map(|s| format!("{s:?}"))
            .collect::<Vec<_>>()
            .join("+")
    }
}

fn print_effort_table(factors: &[EffortFactor]) {
    println!("\n[Effort factors (Omega) from estimate_effort]");
    if factors.is_empty() {
        println!("  (no effort factors)");
        return;
    }
    println!(
        "{:<44} {:>6} {:>10} {:>10} {:>10}",
        "platforms", "months", "total", "modis_obs", "omega"
    );
    println!("{}", "-".repeat(84));
    for f in factors {
        println!(
            "{:<44} {:>6} {:>10} {:>10} {:>10}",
            format_platforms(&f.platforms),
            f.months,
            f.total,
            f.modis_observable,
            fmt_opt(f.omega, 4),
        );
    }
}

fn annual_status_label(row: &AnnualRow) -> String {
    if row.year == 2000 {
        format!("partial (Nov-Dec only, {} mo)", row.active_months)
    } else if row.missing_archive_file {
        "incomplete (missing archive file)".to_string()
    } else if row.active_months == 0 {
        "empty (0 active months)".to_string()
    } else if row.missing_norm_months > 0 {
        format!(
            "incomplete ({}/{} months normalized)",
            row.norm_months, row.active_months
        )
    } else {
        "full".to_string()
    }
}

fn print_annual_table(annual: &[AnnualRow]) {
    println!("\n[Annual rollup]");
    println!(
        "{:>4} {:>6} {:>7} {:>11} {:>12} {:<16} status",
        "year", "mo", "norm_mo", "raw_count", "norm_count", "omega"
    );
    println!("{}", "-".repeat(92));
    let mut incomplete_norm_years = Vec::new();
    for r in annual {
        let omega_str = if r.omega_labels.is_empty() {
            "-".to_string()
        } else {
            r.omega_labels.join(",")
        };
        if r.is_full_year && r.norm_total.is_none() {
            incomplete_norm_years.push(format!(
                "{} ({}/{})",
                r.year, r.norm_months, r.active_months
            ));
        }
        println!(
            "{:>4} {:>6} {:>7} {:>11} {:>12} {:<16} {}",
            r.year,
            r.active_months,
            r.norm_months,
            r.raw_total,
            fmt_opt(r.norm_total, 1),
            omega_str,
            annual_status_label(r),
        );
    }
    if !incomplete_norm_years.is_empty() {
        println!(
            "Note: years with incomplete normalization were excluded from normalized statistics: {}",
            incomplete_norm_years.join(", ")
        );
    }
}

fn print_region_detail(analysis: &RegionAnalysis) {
    println!("\n================================================================================");
    println!("REGION: {}", analysis.region);
    println!("================================================================================");
    print_effort_table(&analysis.effort_factors);
    print_annual_table(&analysis.annual);

    let t = &analysis.trend;
    let raw_span = t
        .raw_span
        .map_or_else(|| "-".to_string(), |(a, b)| format!("{a}..={b}"));
    let norm_span = t
        .norm_span
        .map_or_else(|| "-".to_string(), |(a, b)| format!("{a}..={b}"));
    println!("\n[Trend (Spearman rho, full years 2001..=2025)]");
    println!(
        "  raw ({}, n={}): rho = {} | norm ({}, n={}): rho = {} | |rho_norm| < |rho_raw|/2 -> {}",
        raw_span,
        t.raw_n,
        fmt_signed_opt(t.rho_raw, 4),
        norm_span,
        t.norm_n,
        fmt_signed_opt(t.rho_norm, 4),
        t.verdict.as_str(),
    );

    println!("\n[Level shifts]");
    for s in &analysis.shifts {
        if !s.measured {
            println!("  {}: {}", s.name, NOAA21_NOT_MEASURABLE_MSG);
            continue;
        }
        let abs_log_raw = s.raw_shift.map(|v| v.ln().abs());
        let abs_log_norm = s.norm_shift.map(|v| v.ln().abs());
        println!(
            "  {} ({} vs {}): raw={}->{} (x{}, |log|={}) | norm={}->{} (x{}, |log|={}) | {}",
            s.name,
            s.window_before,
            s.window_after,
            fmt_opt(s.raw_before, 1),
            fmt_opt(s.raw_after, 1),
            fmt_opt(s.raw_shift, 3),
            fmt_opt(abs_log_raw, 4),
            fmt_opt(s.norm_before, 1),
            fmt_opt(s.norm_after, 1),
            fmt_opt(s.norm_shift, 3),
            fmt_opt(abs_log_norm, 4),
            s.verdict.display_label(),
        );
    }
}

fn print_summary_table(title: &str, analyses: &[&RegionAnalysis]) {
    println!("\n================================================================================");
    println!("{title}");
    println!("================================================================================");
    println!(
        "{:<18} {:<22} {:>9} {:>9} {:>10} {:>10} {:>8} verdict",
        "region", "check", "raw_stat", "norm_stat", "|raw|", "|norm|", "measured"
    );
    println!("{}", "-".repeat(114));

    for analysis in analyses {
        let t = &analysis.trend;
        println!(
            "{:<18} {:<22} {:>9} {:>9} {:>10} {:>10} {:>8} {}",
            analysis.region,
            "trend_spearman_01_25",
            fmt_signed_opt(t.rho_raw, 4),
            fmt_signed_opt(t.rho_norm, 4),
            fmt_opt(t.rho_raw.map(f64::abs), 4),
            fmt_opt(t.rho_norm.map(f64::abs), 4),
            "yes",
            t.verdict.as_str(),
        );

        for s in &analysis.shifts {
            if !s.measured {
                println!(
                    "{:<18} {:<22} {:>9} {:>9} {:>10} {:>10} {:>8} {}",
                    analysis.region,
                    s.name,
                    "NA",
                    "NA",
                    "NA",
                    "NA",
                    "no",
                    NOAA21_NOT_MEASURABLE_MSG,
                );
                continue;
            }
            let log_raw = s.raw_shift.map(f64::ln);
            let log_norm = s.norm_shift.map(f64::ln);
            println!(
                "{:<18} {:<22} {:>9} {:>9} {:>10} {:>10} {:>8} {}",
                analysis.region,
                s.name,
                fmt_signed_opt(log_raw, 4),
                fmt_signed_opt(log_norm, 4),
                fmt_opt(log_raw.map(f64::abs), 4),
                fmt_opt(log_norm.map(f64::abs), 4),
                "yes",
                s.verdict.display_label(),
            );
        }
    }
}

fn csv_escape(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn write_csv(path: &Path, analyses: &[RegionAnalysis]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut w = BufWriter::new(file);
    writeln!(
        w,
        "region,check,window_before,window_after,n_raw_before,n_raw_after,n_norm_before,n_norm_after,raw_before,raw_after,raw_shift,raw_stat,abs_raw_stat,norm_before,norm_after,norm_shift,norm_stat,abs_norm_stat,half_abs_raw,measured,verdict,note"
    )?;

    for a in analyses {
        let t = &a.trend;
        let abs_raw_rho = t.rho_raw.map(f64::abs);
        let abs_norm_rho = t.rho_norm.map(f64::abs);
        let half_rho = abs_raw_rho.map(|v| v * 0.5);
        writeln!(
            w,
            "{},trend_spearman,2001-2025,-,0,{},0,{},NA,NA,NA,{},{},NA,NA,NA,{},{},{},true,{},",
            csv_escape(&a.region),
            t.raw_n,
            t.norm_n,
            fmt_opt(t.rho_raw, 6),
            fmt_opt(abs_raw_rho, 6),
            fmt_opt(t.rho_norm, 6),
            fmt_opt(abs_norm_rho, 6),
            fmt_opt(half_rho, 6),
            t.verdict.as_str(),
        )?;

        for s in &a.shifts {
            let log_raw = s.raw_shift.map(f64::ln);
            let log_norm = s.norm_shift.map(f64::ln);
            let abs_raw = log_raw.map(f64::abs);
            let abs_norm = log_norm.map(f64::abs);
            let half_raw = abs_raw.map(|v| v * 0.5);
            writeln!(
                w,
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                csv_escape(&a.region),
                s.name,
                s.window_before,
                s.window_after,
                s.n_raw_before,
                s.n_raw_after,
                s.n_norm_before,
                s.n_norm_after,
                fmt_opt(s.raw_before, 2),
                fmt_opt(s.raw_after, 2),
                fmt_opt(s.raw_shift, 6),
                fmt_opt(log_raw, 6),
                fmt_opt(abs_raw, 6),
                fmt_opt(s.norm_before, 2),
                fmt_opt(s.norm_after, 2),
                fmt_opt(s.norm_shift, 6),
                fmt_opt(log_norm, 6),
                fmt_opt(abs_norm, 6),
                fmt_opt(half_raw, 6),
                s.measured,
                s.verdict.as_str(),
                csv_escape(s.note),
            )?;
        }
    }

    w.flush()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let Some(args) = parse_args()? else {
        return Ok(());
    };

    let cache_dir = archive_cache_dir();
    if !cache_dir.is_dir() {
        bail!(
            "archive cache directory {} does not exist (set ARCHIVE_CACHE_DIR)",
            cache_dir.display()
        );
    }

    let archive = ArchiveClient::offline(cache_dir.clone());
    let config = PipelineConfig::default();

    println!("Archive cache (offline): {}", cache_dir.display());
    println!("Years: {}..={}", args.from, args.to);
    println!(
        "Countries ({}): {}",
        args.countries.len(),
        args.countries.join(", ")
    );
    println!("Archive sensors: MODIS (Terra+Aqua), VIIRS S-NPP, VIIRS NOAA-20");
    println!("2023 (NOAA-21): {NOAA21_NOT_MEASURABLE_MSG}");

    let overall_t0 = Instant::now();
    let pool_multiple = args.countries.len() > 1;
    let mut pooled_harmonized = Vec::new();
    let mut pooled_missing_years = BTreeSet::new();
    let mut per_country_analyses = Vec::with_capacity(args.countries.len());

    for country in &args.countries {
        let t0 = Instant::now();
        let (mut harmonized, missing_file_years) =
            load_and_harmonize_country(&archive, &config, country, args.from, args.to).await?;
        let analysis = analyze_harmonized(
            country.clone(),
            &harmonized,
            &missing_file_years,
            &config,
        );
        println!(
            "\nProcessed {country}: {} harmonized hotspots in {:.2}s",
            harmonized.len(),
            t0.elapsed().as_secs_f64()
        );
        print_region_detail(&analysis);
        per_country_analyses.push(analysis);

        if pool_multiple {
            pooled_harmonized.append(&mut harmonized);
            pooled_missing_years.extend(missing_file_years);
        }
    }

    let pooled_analysis = if pool_multiple {
        let t0 = Instant::now();
        let analysis = analyze_harmonized(
            "POOLED".to_string(),
            &pooled_harmonized,
            &pooled_missing_years,
            &config,
        );
        println!(
            "\nProcessed POOLED ({} countries): {} harmonized hotspots in {:.2}s",
            args.countries.len(),
            pooled_harmonized.len(),
            t0.elapsed().as_secs_f64()
        );
        print_region_detail(&analysis);
        Some(analysis)
    } else {
        None
    };

    let country_refs: Vec<&RegionAnalysis> = per_country_analyses.iter().collect();
    print_summary_table("PER-COUNTRY SUMMARY", &country_refs);

    if let Some(ref pooled) = pooled_analysis {
        print_summary_table("POOLED SUMMARY", &[pooled]);
    } else {
        println!(
            "\nPOOLED SUMMARY: single country specified ({}); identical to per-country table above.",
            args.countries[0]
        );
    }

    println!(
        "\nTransition coverage: 2012 (S-NPP) = measured | 2018 (NOAA-20) = measured | 2023 (NOAA-21) = NOT measured ({NOAA21_NOT_MEASURABLE_MSG})"
    );

    let mut all_analyses = per_country_analyses;
    if let Some(pooled) = pooled_analysis {
        all_analyses.push(pooled);
    }

    if let Some(ref out_path) = args.out {
        write_csv(out_path, &all_analyses)?;
        println!("CSV written: {}", out_path.display());
    }

    println!(
        "Completed in {:.2}s",
        overall_t0.elapsed().as_secs_f64()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cell(year: i32, month: u32, count: u64, count_normalized: Option<f64>) -> CalendarCell {
        CalendarCell {
            year,
            month,
            count,
            count_normalized,
            effort_omega: count_normalized.map(|n| if n > 0.0 { count as f64 / n } else { 1.0 }),
            mean_frp: 20.0,
            mean_frp_raw: 20.0,
            mean_frp_lower: None,
            mean_frp_upper: None,
            total_frp: 20.0 * count as f64,
            total_frp_lower: None,
            total_frp_upper: None,
            frp_per_detection_eq: None,
            anomaly: None,
            sfai: None,
            quality_flags: Vec::new(),
        }
    }

    fn make_full_year_cells(year: i32, monthly_raw: u64, monthly_norm: Option<f64>) -> Vec<CalendarCell> {
        (1..=12)
            .map(|m| make_cell(year, m, monthly_raw, monthly_norm))
            .collect()
    }

    fn make_annual_row(year: i32, raw_total: u64, norm_total: Option<f64>) -> AnnualRow {
        AnnualRow {
            year,
            raw_total,
            norm_total,
            active_months: 12,
            norm_months: if norm_total.is_some() { 12 } else { 0 },
            missing_norm_months: if norm_total.is_some() { 0 } else { 12 },
            missing_archive_file: false,
            is_full_year: true,
            omega_labels: vec!["1.000".to_string()],
        }
    }

    #[test]
    fn spearman_increasing_and_constant_series() {
        let xs = [2001.0, 2002.0, 2003.0, 2004.0, 2005.0];
        let inc = [10.0, 25.0, 40.0, 120.0, 500.0];
        let rho_inc = spearman_rho(&xs, &inc).expect("increasing series has valid rho");
        assert!((rho_inc - 1.0).abs() < 1e-12, "got {rho_inc}");

        let dec = [500.0, 120.0, 40.0, 25.0, 10.0];
        let rho_dec = spearman_rho(&xs, &dec).expect("decreasing series has valid rho");
        assert!((rho_dec + 1.0).abs() < 1e-12, "got {rho_dec}");

        let constant = [42.0, 42.0, 42.0, 42.0, 42.0];
        assert_eq!(spearman_rho(&xs, &constant), None);
    }

    #[test]
    fn window_2012_excludes_2012_and_handles_missing_year() {
        // Put an extreme spike in 2012 and omit 2009 completely.
        let annual = vec![
            make_annual_row(2007, 100, Some(100.0)),
            make_annual_row(2008, 100, Some(100.0)),
            // 2009 missing
            make_annual_row(2010, 100, Some(100.0)),
            make_annual_row(2011, 100, Some(100.0)),
            make_annual_row(2012, 999_999, Some(999_999.0)),
            make_annual_row(2013, 500, Some(100.0)),
            make_annual_row(2014, 500, Some(100.0)),
            make_annual_row(2015, 500, Some(100.0)),
            make_annual_row(2016, 500, Some(100.0)),
            make_annual_row(2017, 500, Some(100.0)),
        ];

        let shifts = compute_shifts(&annual, &[], &BTreeSet::new());
        let s2012 = &shifts[0];
        assert_eq!(s2012.name, "2012_snpp");
        assert_eq!(s2012.n_raw_before, 4);
        assert_eq!(s2012.n_raw_after, 5);
        assert_eq!(s2012.n_norm_before, 4);
        assert_eq!(s2012.n_norm_after, 5);
        assert!((s2012.raw_before.unwrap() - 100.0).abs() < 1e-12);
        assert!((s2012.raw_after.unwrap() - 500.0).abs() < 1e-12);
        assert!((s2012.raw_shift.unwrap() - 5.0).abs() < 1e-12);
        assert!((s2012.norm_shift.unwrap() - 1.0).abs() < 1e-12);
        assert_eq!(s2012.verdict, ShiftVerdict::Pass);
    }

    #[test]
    fn shift_criterion_and_too_small_guard() {
        // Within +/-10% (|ln(1.05)| ~= 0.0488 < 0.0953): must report RawShiftTooSmall even if norm is worse.
        assert_eq!(
            evaluate_shift(Some(1.05), Some(1.20)),
            ShiftVerdict::RawShiftTooSmall
        );
        assert_eq!(
            evaluate_shift(Some(0.95), Some(0.80)),
            ShiftVerdict::RawShiftTooSmall
        );

        // Raw shift large enough: |ln(2.0)| ~= 0.6931 >= 0.0953.
        assert_eq!(evaluate_shift(Some(2.0), Some(1.10)), ShiftVerdict::Pass);
        assert_eq!(evaluate_shift(Some(2.0), Some(1.50)), ShiftVerdict::Fail);
        assert_eq!(evaluate_shift(Some(2.0), None), ShiftVerdict::Unavailable);
        assert_eq!(evaluate_shift(None, Some(1.0)), ShiftVerdict::Unavailable);
    }

    #[test]
    fn incomplete_years_are_excluded_rather_than_silently_summed() {
        // 2000: partial year (Nov-Dec only).
        let mut cells = vec![
            make_cell(2000, 11, 50, None),
            make_cell(2000, 12, 50, None),
        ];
        // 2001: full raw year, no normalized months.
        cells.extend(make_full_year_cells(2001, 10, None));
        // 2002: pre-Aqua Jan-Jun have None, Jul-Dec have Some(10.0) -> must NOT sum to 60.0.
        for m in 1..=6 {
            cells.push(make_cell(2002, m, 10, None));
        }
        for m in 7..=12 {
            cells.push(make_cell(2002, m, 10, Some(10.0)));
        }
        // 2003..=2005: complete years with increasing raw and flat normalized counts.
        cells.extend(make_full_year_cells(2003, 10, Some(10.0)));
        cells.extend(make_full_year_cells(2004, 20, Some(10.0)));
        cells.extend(make_full_year_cells(2005, 30, Some(10.0)));
        // 2006: 11 normalized months and 1 month lacking norm -> must be excluded from norm stats.
        for m in 1..=11 {
            cells.push(make_cell(2006, m, 40, Some(10.0)));
        }
        cells.push(make_cell(2006, 12, 40, None));

        let rows = rollup_calendar_by_year(&cells, &BTreeSet::new());
        let by_year: BTreeMap<i32, &AnnualRow> = rows.iter().map(|r| (r.year, r)).collect();

        assert!(!by_year[&2000].is_full_year);
        assert_eq!(by_year[&2000].norm_total, None);

        assert!(by_year[&2001].is_full_year);
        assert_eq!(by_year[&2001].raw_total, 120);
        assert_eq!(by_year[&2001].norm_total, None);

        assert!(by_year[&2002].is_full_year);
        assert_eq!(by_year[&2002].raw_total, 120);
        assert_eq!(by_year[&2002].norm_months, 6);
        assert_eq!(by_year[&2002].missing_norm_months, 6);
        assert_eq!(by_year[&2002].norm_total, None);

        assert_eq!(by_year[&2003].norm_total, Some(120.0));
        assert_eq!(by_year[&2006].missing_norm_months, 1);
        assert_eq!(by_year[&2006].norm_total, None);

        let trend = compute_trend(&rows);
        // Raw full years: 2001..=2006 (6 years), Norm complete years: 2003..=2005 (3 years).
        assert_eq!(trend.raw_n, 6);
        assert_eq!(trend.norm_n, 3);
        assert_eq!(trend.norm_span, Some((2003, 2005)));
    }
}
