//! Full-fleet (MODIS + VIIRS S-NPP + VIIRS NOAA-20 [+ VIIRS NOAA-21]) 2000-2025 trend and
//! level-shift verification on the offline FIRMS country archive and optional area API (task K7).
//!
//! Usage:
//!   ARCHIVE_CACHE_DIR=.cache/firms-archive \
//!     cargo run --release -p pyrosync-firms --example fleet_trend -- <Country>... [--from 2000] [--to 2025] [--out <csv>] [--yearly-omega] [--noaa21] [--noaa21-union] [--regions-geo <path>]

use anyhow::{Context, Result, bail};
use chrono::{Datelike, NaiveDate};
use pyrosync_core::calendar::build_calendar_with;
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::effort::{EffortFactor, estimate_effort, pre_aqua_excluded_months};
use pyrosync_core::harmonize::harmonize_with_config;
use pyrosync_core::models::{BBox, CalendarCell, HarmonizedHotspot, Hotspot, Sensor};
use pyrosync_firms::archive::{ArchiveClient, ArchiveError};
use pyrosync_firms::parse::retain_wildfires;
use pyrosync_firms::{AreaCache, FirmsClient};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracing_subscriber::EnvFilter;

const ARCHIVE_FETCH_SENSORS: [Sensor; 3] =
    [Sensor::ModisTerra, Sensor::ViirsSNPP, Sensor::ViirsNOAA20];
const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const DEFAULT_AREA_CACHE_DIR: &str = ".cache/firms-area";
const DEFAULT_AREA_CACHE_NRT_TTL_SECS: u64 = 600;
const DEFAULT_REGIONS_GEO: &str = "web/public/geo/regions.json";
const DEFAULT_FROM_YEAR: i32 = 2000;
const DEFAULT_TO_YEAR: i32 = 2025;
// +/-10% raw shift threshold: ln(1.10) ~= 0.0953.
const MIN_TESTABLE_ABS_LOG_RAW: f64 = 0.0953;
const NOAA21_NOT_MEASURABLE_MSG: &str =
    "not measurable on archive alone; area API flag --noaa21 (needs FIRMS_API_KEY) measures it";
const NOAA21_INCOMPLETE_MSG: &str = "unavailable (NOAA-21 fetch incomplete)";

struct Args {
    countries: Vec<String>,
    from: i32,
    to: i32,
    out: Option<PathBuf>,
    yearly_omega: bool,
    noaa21: bool,
    noaa21_union: bool,
    regions_geo: PathBuf,
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
    UnavailableNoaa21Incomplete,
}

impl ShiftVerdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::RawShiftTooSmall => "RAW_SHIFT_TOO_SMALL",
            Self::NotMeasured => "NOT_MEASURED",
            Self::Unavailable | Self::UnavailableNoaa21Incomplete => "UNAVAILABLE",
        }
    }

    fn display_label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::RawShiftTooSmall => "raw shift too small to test",
            Self::NotMeasured => NOAA21_NOT_MEASURABLE_MSG,
            Self::Unavailable => "unavailable (missing data)",
            Self::UnavailableNoaa21Incomplete => NOAA21_INCOMPLETE_MSG,
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
    ref_before: Option<f64>,
    ref_after: Option<f64>,
    ref_shift: Option<f64>,
    raw_adj: Option<f64>,
    norm_adj: Option<f64>,
    measured: bool,
    verdict: ShiftVerdict,
    adj_verdict: ShiftVerdict,
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

type PoolingKey = (bool, Vec<Sensor>);

#[derive(Debug, Clone)]
struct YearlyConfigRow {
    year: i32,
    has_modis: bool,
    viirs_platforms: Vec<Sensor>,
    months: usize,
    total: u64,
    modis_observable: u64,
    yearly_ratio: Option<f64>,
    pooled_omega: Option<f64>,
}

#[derive(Debug, Clone)]
struct YearlyPlatformRow {
    year: i32,
    total_records: u64,
    terra_records: u64,
    aqua_records: u64,
    viirs_records: u64,
    modis_observable_records: u64,
    modis_observable_share: Option<f64>,
}

#[derive(Debug, Clone)]
struct YearlyModisSnppNoaa20Summary {
    n_years: usize,
    rho_2018_2025: Option<f64>,
    n_2018_2020: usize,
    mean_2018_2020: Option<f64>,
    n_2022_2025: usize,
    mean_2022_2025: Option<f64>,
    pooled_omega: Option<f64>,
}

#[derive(Debug, Clone)]
struct YearlyOmegaAnalysis {
    region: String,
    config_rows: Vec<YearlyConfigRow>,
    platform_rows: Vec<YearlyPlatformRow>,
    summary_2018_2025: YearlyModisSnppNoaa20Summary,
}

fn print_usage() {
    println!(
        "\
Usage: fleet_trend <Country>... [--from {DEFAULT_FROM_YEAR}] [--to {DEFAULT_TO_YEAR}] [--out <csv>] [--yearly-omega] [--noaa21] [--noaa21-union] [--regions-geo <path>]

Evaluate full-fleet (MODIS + S-NPP + NOAA-20 [+ NOAA-21]) trend and level-shift criteria on the offline FIRMS country archive.
Archive directory: ARCHIVE_CACHE_DIR env var, or {DEFAULT_ARCHIVE_CACHE_DIR}.
Options:
  --noaa21               Fetch VIIRS_NOAA21_SP (2023-03-01..=2025-12-31) from the FIRMS area API (requires FIRMS_API_KEY; cached in AREA_CACHE_DIR or {DEFAULT_AREA_CACHE_DIR}).
  --noaa21-union         Fetch NOAA-21 once over the union bbox of all requested countries and clip per country (implies --noaa21).
  --regions-geo <path>   Geo regions JSON for country bboxes and polygons (default: {DEFAULT_REGIONS_GEO}).
  --yearly-omega         Print and export yearly Omega diagnostics."
    );
}

fn parse_args() -> Result<Option<Args>> {
    let mut raw_args = std::env::args().skip(1);
    let mut countries = Vec::new();
    let mut from = DEFAULT_FROM_YEAR;
    let mut to = DEFAULT_TO_YEAR;
    let mut out: Option<PathBuf> = None;
    let mut yearly_omega = false;
    let mut noaa21 = false;
    let mut noaa21_union = false;
    let mut regions_geo = PathBuf::from(DEFAULT_REGIONS_GEO);

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
            "--yearly-omega" => {
                yearly_omega = true;
            }
            "--noaa21" => {
                noaa21 = true;
            }
            "--noaa21-union" => {
                noaa21 = true;
                noaa21_union = true;
            }
            "--regions-geo" => {
                let val = raw_args.next().context("missing value for --regions-geo")?;
                if val.trim().is_empty() {
                    bail!("empty value for --regions-geo");
                }
                regions_geo = PathBuf::from(val);
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
        yearly_omega,
        noaa21,
        noaa21_union,
        regions_geo,
    }))
}

fn archive_cache_dir() -> PathBuf {
    match std::env::var_os("ARCHIVE_CACHE_DIR") {
        Some(val) if !val.is_empty() => PathBuf::from(val),
        _ => PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR),
    }
}

fn parse_area_cache_dir(raw: Option<&str>) -> Option<PathBuf> {
    match raw.map(str::trim) {
        None => Some(PathBuf::from(DEFAULT_AREA_CACHE_DIR)),
        Some("") => None,
        Some(dir) => Some(PathBuf::from(dir)),
    }
}

#[derive(Deserialize)]
struct RegionsGeoFile {
    features: Vec<RegionGeoFeature>,
}

#[derive(Deserialize)]
struct RegionGeoFeature {
    #[serde(rename = "regionId")]
    region_id: Option<String>,
    bbox: [f64; 4],
    polygons: Vec<Vec<Vec<[f64; 2]>>>,
}

#[derive(Debug, Clone)]
struct CountryGeo {
    bbox: BBox,
    polygons: Vec<Vec<Vec<[f64; 2]>>>,
}

fn union_bbox(bboxes: &[BBox]) -> Option<BBox> {
    let first = bboxes.first()?;
    let mut west = first.west;
    let mut south = first.south;
    let mut east = first.east;
    let mut north = first.north;
    for b in &bboxes[1..] {
        west = west.min(b.west);
        south = south.min(b.south);
        east = east.max(b.east);
        north = north.max(b.north);
    }
    Some(BBox {
        west,
        south,
        east,
        north,
    })
}

fn load_country_geos(path: &Path, countries: &[String]) -> Result<BTreeMap<String, CountryGeo>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read regions geo file {}", path.display()))?;
    let parsed: RegionsGeoFile = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse regions geo JSON {}", path.display()))?;

    let mut by_region: BTreeMap<String, CountryGeo> = BTreeMap::new();
    for feature in parsed.features {
        let Some(region_id) = feature.region_id else {
            continue;
        };
        let bbox = BBox {
            west: feature.bbox[0],
            south: feature.bbox[1],
            east: feature.bbox[2],
            north: feature.bbox[3],
        };
        match by_region.get_mut(&region_id) {
            Some(existing) => {
                existing.bbox = union_bbox(&[existing.bbox.clone(), bbox])
                    .expect("two bboxes always yield a union");
                existing.polygons.extend(feature.polygons);
            }
            None => {
                by_region.insert(
                    region_id,
                    CountryGeo {
                        bbox,
                        polygons: feature.polygons,
                    },
                );
            }
        }
    }

    let mut selected = BTreeMap::new();
    for country in countries {
        let Some(geo) = by_region.get(country).cloned() else {
            bail!(
                "country '{country}' not found (by regionId) in {}",
                path.display()
            );
        };
        selected.insert(country.clone(), geo);
    }
    Ok(selected)
}

fn point_in_ring(lon: f64, lat: f64, ring: &[[f64; 2]]) -> bool {
    if !lon.is_finite() || !lat.is_finite() || ring.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut prev = ring[ring.len() - 1];
    for &curr in ring {
        let x1 = prev[0];
        let y1 = prev[1];
        let x2 = curr[0];
        let y2 = curr[1];
        if (y1 > lat) != (y2 > lat) {
            let x_intersect = x1 + (lat - y1) * (x2 - x1) / (y2 - y1);
            if lon < x_intersect {
                inside = !inside;
            }
        }
        prev = curr;
    }
    inside
}

fn point_in_polygon(lon: f64, lat: f64, rings: &[Vec<[f64; 2]>]) -> bool {
    let Some((outer, holes)) = rings.split_first() else {
        return false;
    };
    if !point_in_ring(lon, lat, outer) {
        return false;
    }
    !holes.iter().any(|hole| point_in_ring(lon, lat, hole))
}

fn point_in_multipolygon(lon: f64, lat: f64, polygons: &[Vec<Vec<[f64; 2]>>]) -> bool {
    polygons.iter().any(|poly| point_in_polygon(lon, lat, poly))
}

fn clip_hotspots_to_country(hotspots: &[Hotspot], geo: &CountryGeo) -> (Vec<Hotspot>, usize) {
    let mut kept = Vec::new();
    let mut dropped = 0usize;
    for h in hotspots {
        if point_in_multipolygon(h.lon, h.lat, &geo.polygons) {
            kept.push(h.clone());
        } else {
            dropped += 1;
        }
    }
    (kept, dropped)
}

fn noaa21_fetch_range(from_year: i32, to_year: i32) -> Option<(NaiveDate, NaiveDate)> {
    let avail_start = NaiveDate::from_ymd_opt(2023, 3, 1)?;
    let avail_end = NaiveDate::from_ymd_opt(2025, 12, 31)?;
    let req_start = NaiveDate::from_ymd_opt(from_year, 1, 1)?;
    let req_end = NaiveDate::from_ymd_opt(to_year, 12, 31)?;
    let start = req_start.max(avail_start);
    let end = req_end.min(avail_end);
    (start <= end).then_some((start, end))
}

struct Noaa21Fetch {
    hotspots: Vec<Hotspot>,
    incomplete: bool,
}

async fn fetch_noaa21_range(
    client: &FirmsClient,
    label: &str,
    bbox: &BBox,
    start: NaiveDate,
    end: NaiveDate,
) -> Noaa21Fetch {
    let mut hotspots = Vec::new();
    let mut transactions_spent = 0u32;
    let mut failures = Vec::new();

    for year in start.year()..=end.year() {
        let yr_start = NaiveDate::from_ymd_opt(year, 1, 1)
            .expect("valid Jan 1")
            .max(start);
        let yr_end = NaiveDate::from_ymd_opt(year, 12, 31)
            .expect("valid Dec 31")
            .min(end);
        let (mut yr_hotspots, report) = client
            .fetch_range(Sensor::ViirsNOAA21, bbox, yr_start, yr_end)
            .await;
        transactions_spent += report.transactions_spent;
        println!(
            "  NOAA-21 [{label}] {yr_start}..={yr_end}: rows={}, tx_spent={}, failures={}",
            yr_hotspots.len(),
            report.transactions_spent,
            report.failures.len()
        );
        if !report.uncovered.is_empty() {
            let ranges: Vec<String> = report
                .uncovered
                .iter()
                .map(|(a, b)| format!("{a}..={b}"))
                .collect();
            eprintln!(
                "WARNING: NOAA-21 [{label}] {year} has uncovered intervals: {}",
                ranges.join(", ")
            );
        }
        for failure in report.failures {
            failures.push(failure);
        }
        hotspots.append(&mut yr_hotspots);
    }

    println!(
        "  NOAA-21 [{label}] total: rows={}, tx_spent={}, failures={}",
        hotspots.len(),
        transactions_spent,
        failures.len()
    );
    let incomplete = !failures.is_empty();
    if incomplete {
        eprintln!(
            "WARNING: NOAA-21 fetch for {label} had {} failure(s); data is INCOMPLETE and 2023 checks will be marked unavailable:",
            failures.len()
        );
        for (src, msg) in &failures {
            eprintln!("  NOAA-21 failure [{label}] {src}: {msg}");
        }
    }

    Noaa21Fetch {
        hotspots,
        incomplete,
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

/// Evaluates a level-shift transition relative to the stable MODIS reference channel.
///
/// WHY: A single-season comparison mixes real year-to-year fire activity with the
/// instrument step (for example, 2018 was a quiet fire year, and MODIS itself dropped).
/// Dividing by the stable MODIS reference channel (`ref_shift = ref_after / ref_before`)
/// removes the shared activity change and leaves only the instrument artifact.
fn evaluate_adjusted_shift(
    raw_shift: Option<f64>,
    norm_shift: Option<f64>,
    ref_shift: Option<f64>,
) -> (Option<f64>, Option<f64>, ShiftVerdict) {
    let Some(ref_s) = ref_shift.filter(|v| v.is_finite() && *v > 0.0) else {
        return (None, None, ShiftVerdict::Unavailable);
    };
    let raw_adj = raw_shift
        .filter(|v| v.is_finite() && *v > 0.0)
        .map(|rs| rs / ref_s)
        .filter(|v| v.is_finite() && *v > 0.0);
    let norm_adj = norm_shift
        .filter(|v| v.is_finite() && *v > 0.0)
        .map(|ns| ns / ref_s)
        .filter(|v| v.is_finite() && *v > 0.0);
    let verdict = evaluate_shift(raw_adj, norm_adj);
    (raw_adj, norm_adj, verdict)
}

fn monthly_modis_observable(hotspots: &[HarmonizedHotspot]) -> BTreeMap<(i32, u32), u64> {
    let excluded: BTreeSet<(i32, u32)> = pre_aqua_excluded_months(hotspots).into_iter().collect();
    let mut counts: BTreeMap<(i32, u32), u64> = BTreeMap::new();
    for h in hotspots {
        let date = h.acquired.date_naive();
        let ym = (date.year(), date.month());
        if excluded.contains(&ym) {
            continue;
        }
        if h.platforms.iter().any(|s| s.is_modis()) {
            *counts.entry(ym).or_default() += 1;
        }
    }
    counts
}

struct WindowSummary {
    raw_mean: Option<f64>,
    n_raw: usize,
    norm_mean: Option<f64>,
    n_norm: usize,
    ref_mean: Option<f64>,
}

fn window_block_means(
    annual: &[AnnualRow],
    years: RangeInclusive<i32>,
    modis_obs: &BTreeMap<(i32, u32), u64>,
) -> WindowSummary {
    let mut raw_sum = 0.0_f64;
    let mut n_raw = 0usize;
    let mut norm_sum = 0.0_f64;
    let mut n_norm = 0usize;
    let mut ref_sum = 0_u64;

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
        for m in 1..=12 {
            ref_sum += modis_obs.get(&(row.year, m)).copied().unwrap_or(0);
        }
    }

    WindowSummary {
        raw_mean: (n_raw > 0).then(|| raw_sum / n_raw as f64),
        n_raw,
        norm_mean: (n_norm > 0).then(|| norm_sum / n_norm as f64),
        n_norm,
        ref_mean: (n_raw > 0).then(|| ref_sum as f64 / n_raw as f64),
    }
}

fn may_dec_totals(
    cells: &[CalendarCell],
    year: i32,
    missing_file_years: &BTreeSet<i32>,
    modis_obs: &BTreeMap<(i32, u32), u64>,
) -> WindowSummary {
    if missing_file_years.contains(&year) {
        return WindowSummary {
            raw_mean: None,
            n_raw: 0,
            norm_mean: None,
            n_norm: 0,
            ref_mean: None,
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
            ref_mean: None,
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
    let ref_sum: u64 = (5..=12)
        .map(|m| modis_obs.get(&(year, m)).copied().unwrap_or(0))
        .sum();

    WindowSummary {
        raw_mean: Some(raw_sum as f64),
        n_raw: season_cells.len(),
        norm_mean: norm_total,
        n_norm: if norm_total.is_some() { norm_months } else { 0 },
        ref_mean: Some(ref_sum as f64),
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
    let ref_shift = ratio_opt(before.ref_mean, after.ref_mean);
    let verdict = evaluate_shift(raw_shift, norm_shift);
    let (raw_adj, norm_adj, adj_verdict) =
        evaluate_adjusted_shift(raw_shift, norm_shift, ref_shift);
    let note = match (verdict, adj_verdict) {
        (ShiftVerdict::Unavailable, _) => "missing raw or normalized data in window",
        (_, ShiftVerdict::Unavailable) => "missing MODIS reference data in window",
        (ShiftVerdict::RawShiftTooSmall, _) => "raw shift too small to test",
        (_, ShiftVerdict::RawShiftTooSmall) => "adjusted raw shift too small to test",
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
        ref_before: before.ref_mean,
        ref_after: after.ref_mean,
        ref_shift,
        raw_adj,
        norm_adj,
        measured: true,
        verdict,
        adj_verdict,
        note,
    }
}

fn unavailable_noaa21_shift(
    name: &'static str,
    window_before: &'static str,
    window_after: &'static str,
) -> ShiftResult {
    ShiftResult {
        name,
        window_before,
        window_after,
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
        ref_before: None,
        ref_after: None,
        ref_shift: None,
        raw_adj: None,
        norm_adj: None,
        measured: false,
        verdict: ShiftVerdict::UnavailableNoaa21Incomplete,
        adj_verdict: ShiftVerdict::UnavailableNoaa21Incomplete,
        note: NOAA21_INCOMPLETE_MSG,
    }
}

fn compute_shifts_with_ref(
    annual: &[AnnualRow],
    cells: &[CalendarCell],
    missing_file_years: &BTreeSet<i32>,
    modis_obs: &BTreeMap<(i32, u32), u64>,
    noaa21_enabled: bool,
    noaa21_incomplete: bool,
) -> Vec<ShiftResult> {
    // 2012 (S-NPP): 5 full years before (2007-2011) vs 5 full years after (2013-2017); 2012 excluded.
    let shift_2012 = build_shift_from_summaries(
        "2012_snpp",
        "2007-2011",
        "2013-2017",
        window_block_means(annual, 2007..=2011, modis_obs),
        window_block_means(annual, 2013..=2017, modis_obs),
    );

    // 2018 (NOAA-20 from 2018-04): May-Dec 2017 vs May-Dec 2018 same-season comparison.
    let shift_2018_season = build_shift_from_summaries(
        "2018_noaa20_may_dec",
        "2017-05..12",
        "2018-05..12",
        may_dec_totals(cells, 2017, missing_file_years, modis_obs),
        may_dec_totals(cells, 2018, missing_file_years, modis_obs),
    );

    // 2018 (NOAA-20): 3-year full-year block comparison (2015-2017 vs 2019-2021); 2018 excluded.
    let shift_2018_block = build_shift_from_summaries(
        "2018_noaa20_3yr",
        "2015-2017",
        "2019-2021",
        window_block_means(annual, 2015..=2017, modis_obs),
        window_block_means(annual, 2019..=2021, modis_obs),
    );

    if !noaa21_enabled {
        // 2023 (NOAA-21): unmeasurable without --noaa21 (country archive has no NOAA-21).
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
            ref_before: None,
            ref_after: None,
            ref_shift: None,
            raw_adj: None,
            norm_adj: None,
            measured: false,
            verdict: ShiftVerdict::NotMeasured,
            adj_verdict: ShiftVerdict::NotMeasured,
            note: NOAA21_NOT_MEASURABLE_MSG,
        };
        return vec![shift_2012, shift_2018_season, shift_2018_block, shift_2023];
    }

    let (shift_2023_season, shift_2023_block) = if noaa21_incomplete {
        (
            unavailable_noaa21_shift("2023_noaa21_may_dec", "2022-05..12", "2023-05..12"),
            unavailable_noaa21_shift("2023_noaa21_block", "2020-2022", "2024-2025"),
        )
    } else {
        // 2023 (NOAA-21 from 2023-03): May-Dec 2022 vs May-Dec 2023 same-season comparison.
        let s_season = build_shift_from_summaries(
            "2023_noaa21_may_dec",
            "2022-05..12",
            "2023-05..12",
            may_dec_totals(cells, 2022, missing_file_years, modis_obs),
            may_dec_totals(cells, 2023, missing_file_years, modis_obs),
        );
        // 2023 (NOAA-21): full years 2020-2022 vs 2024-2025; 2023 excluded, 2026 not in archive.
        let s_block = build_shift_from_summaries(
            "2023_noaa21_block",
            "2020-2022",
            "2024-2025",
            window_block_means(annual, 2020..=2022, modis_obs),
            window_block_means(annual, 2024..=2025, modis_obs),
        );
        (s_season, s_block)
    };

    vec![
        shift_2012,
        shift_2018_season,
        shift_2018_block,
        shift_2023_season,
        shift_2023_block,
    ]
}

#[cfg(test)]
fn compute_shifts(
    annual: &[AnnualRow],
    cells: &[CalendarCell],
    missing_file_years: &BTreeSet<i32>,
) -> Vec<ShiftResult> {
    compute_shifts_with_ref(
        annual,
        cells,
        missing_file_years,
        &BTreeMap::new(),
        false,
        false,
    )
}

fn analyze_harmonized(
    region: String,
    harmonized: &[HarmonizedHotspot],
    missing_file_years: &BTreeSet<i32>,
    config: &PipelineConfig,
    noaa21_enabled: bool,
    noaa21_incomplete: bool,
) -> RegionAnalysis {
    let effort_factors = estimate_effort(harmonized);
    let cells = build_calendar_with(harmonized, &config.uncertainty, &config.calibration);
    let annual = rollup_calendar_by_year(&cells, missing_file_years);
    let trend = compute_trend(&annual);
    let modis_obs = monthly_modis_observable(harmonized);
    let shifts = compute_shifts_with_ref(
        &annual,
        &cells,
        missing_file_years,
        &modis_obs,
        noaa21_enabled,
        noaa21_incomplete,
    );

    RegionAnalysis {
        region,
        effort_factors,
        annual,
        trend,
        shifts,
    }
}

fn find_shift<'a>(analysis: &'a RegionAnalysis, name: &str) -> Option<&'a ShiftResult> {
    analysis.shifts.iter().find(|s| s.name == name)
}

// This aggregation is the reporting rule documented in docs/TAM_FILO_TREND.md:
// MET requires trend PASS and, for each transition, the block/3yr adjusted
// verdict PASS and the same-season adjusted verdict not FAIL.
fn k7_criterion_met(analysis: &RegionAnalysis) -> bool {
    if analysis.trend.verdict != TrendVerdict::Pass {
        return false;
    }
    let Some(s2012) = find_shift(analysis, "2012_snpp") else {
        return false;
    };
    let Some(s2018_season) = find_shift(analysis, "2018_noaa20_may_dec") else {
        return false;
    };
    let Some(s2018_block) = find_shift(analysis, "2018_noaa20_3yr") else {
        return false;
    };
    let Some(s2023_season) = find_shift(analysis, "2023_noaa21_may_dec") else {
        return false;
    };
    let Some(s2023_block) = find_shift(analysis, "2023_noaa21_block") else {
        return false;
    };

    // Missing data must not count as a pass; only a raw step too small to test is excused.
    let season_ok =
        |v: ShiftVerdict| matches!(v, ShiftVerdict::Pass | ShiftVerdict::RawShiftTooSmall);
    s2012.adj_verdict == ShiftVerdict::Pass
        && s2018_block.adj_verdict == ShiftVerdict::Pass
        && season_ok(s2018_season.adj_verdict)
        && s2023_block.adj_verdict == ShiftVerdict::Pass
        && season_ok(s2023_season.adj_verdict)
}

fn format_k7_line(analysis: &RegionAnalysis) -> String {
    let v2012 = find_shift(analysis, "2012_snpp")
        .map(|s| s.adj_verdict.as_str())
        .unwrap_or("UNAVAILABLE");
    let v2018_season = find_shift(analysis, "2018_noaa20_may_dec")
        .map(|s| s.adj_verdict.as_str())
        .unwrap_or("UNAVAILABLE");
    let v2018_block = find_shift(analysis, "2018_noaa20_3yr")
        .map(|s| s.adj_verdict.as_str())
        .unwrap_or("UNAVAILABLE");
    let (v2023_season, v2023_block) = match (
        find_shift(analysis, "2023_noaa21_may_dec"),
        find_shift(analysis, "2023_noaa21_block"),
    ) {
        (Some(ss), Some(sb)) => (ss.adj_verdict.as_str(), sb.adj_verdict.as_str()),
        _ => match find_shift(analysis, "2023_noaa21") {
            Some(s) => (s.adj_verdict.as_str(), s.adj_verdict.as_str()),
            None => ("UNAVAILABLE", "UNAVAILABLE"),
        },
    };
    let status = if k7_criterion_met(analysis) {
        "MET"
    } else {
        "NOT MET"
    };

    format!(
        "{}: K7 criterion: trend={} 2012={} 2018={}/{} 2023={}/{} -> {}",
        analysis.region,
        analysis.trend.verdict.as_str(),
        v2012,
        v2018_season,
        v2018_block,
        v2023_season,
        v2023_block,
        status
    )
}

fn pooling_key(platforms: &BTreeSet<Sensor>) -> PoolingKey {
    let has_modis = platforms.iter().any(|s| s.is_modis());
    let viirs_platforms = platforms.iter().copied().filter(|s| s.is_viirs()).collect();
    (has_modis, viirs_platforms)
}

fn block_mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn compute_yearly_omega(
    region: String,
    hotspots: &[HarmonizedHotspot],
    effort_factors: &[EffortFactor],
) -> YearlyOmegaAnalysis {
    struct MonthStats {
        platforms: BTreeSet<Sensor>,
        total: u64,
        modis_observable: u64,
    }

    #[derive(Default)]
    struct YearlyConfigAccum {
        months: usize,
        total: u64,
        modis_observable: u64,
    }

    #[derive(Default)]
    struct YearlyPlatformAccum {
        total_records: u64,
        terra_records: u64,
        aqua_records: u64,
        viirs_records: u64,
        modis_observable_records: u64,
    }

    let mut months: BTreeMap<(i32, u32), MonthStats> = BTreeMap::new();
    let mut platform_by_year: BTreeMap<i32, YearlyPlatformAccum> = BTreeMap::new();

    for h in hotspots {
        let date = h.acquired.date_naive();
        let year = date.year();
        let month = date.month();

        let p_entry = platform_by_year.entry(year).or_default();
        p_entry.total_records += 1;
        if h.platforms.contains(&Sensor::ModisTerra) {
            p_entry.terra_records += 1;
        }
        if h.platforms.contains(&Sensor::ModisAqua) {
            p_entry.aqua_records += 1;
        }
        if h.platforms.iter().any(|s| s.is_viirs()) {
            p_entry.viirs_records += 1;
        }
        let has_modis = h.platforms.iter().any(|s| s.is_modis());
        if has_modis {
            p_entry.modis_observable_records += 1;
        }

        let bucket = months.entry((year, month)).or_insert_with(|| MonthStats {
            platforms: BTreeSet::new(),
            total: 0,
            modis_observable: 0,
        });
        bucket.total += 1;
        if has_modis {
            bucket.modis_observable += 1;
        }
        bucket.platforms.extend(h.platforms.iter().copied());
    }

    // Match effort.rs: exclude pre-Aqua months when the series reaches 2002-07 or later.
    let excluded: BTreeSet<(i32, u32)> = pre_aqua_excluded_months(hotspots).into_iter().collect();
    for ym in excluded {
        months.remove(&ym);
    }

    let mut by_year_config: BTreeMap<(i32, PoolingKey), YearlyConfigAccum> = BTreeMap::new();
    for ((year, _month), bucket) in months {
        let key = pooling_key(&bucket.platforms);
        let entry = by_year_config.entry((year, key)).or_default();
        entry.months += 1;
        entry.total += bucket.total;
        entry.modis_observable += bucket.modis_observable;
    }

    let config_rows: Vec<YearlyConfigRow> = by_year_config
        .into_iter()
        .map(|((year, (has_modis, viirs_platforms)), acc)| {
            let yearly_ratio = if acc.modis_observable > 0 {
                let r = acc.total as f64 / acc.modis_observable as f64;
                r.is_finite().then_some(r)
            } else {
                None
            };
            let pooled_omega = effort_factors
                .iter()
                .find(|f| f.has_modis == has_modis && f.viirs_platforms == viirs_platforms)
                .and_then(|f| f.omega);
            YearlyConfigRow {
                year,
                has_modis,
                viirs_platforms,
                months: acc.months,
                total: acc.total,
                modis_observable: acc.modis_observable,
                yearly_ratio,
                pooled_omega,
            }
        })
        .collect();

    let platform_rows: Vec<YearlyPlatformRow> = platform_by_year
        .into_iter()
        .map(|(year, acc)| {
            let modis_observable_share = if acc.total_records > 0 {
                let share = acc.modis_observable_records as f64 / acc.total_records as f64;
                share.is_finite().then_some(share)
            } else {
                None
            };
            YearlyPlatformRow {
                year,
                total_records: acc.total_records,
                terra_records: acc.terra_records,
                aqua_records: acc.aqua_records,
                viirs_records: acc.viirs_records,
                modis_observable_records: acc.modis_observable_records,
                modis_observable_share,
            }
        })
        .collect();

    let target_viirs = [Sensor::ViirsSNPP, Sensor::ViirsNOAA20];
    let mut years_18_25 = Vec::new();
    let mut ratios_18_25 = Vec::new();
    let mut ratios_18_20 = Vec::new();
    let mut ratios_22_25 = Vec::new();

    for row in &config_rows {
        if !row.has_modis || row.viirs_platforms.as_slice() != target_viirs {
            continue;
        }
        let Some(ratio) = row.yearly_ratio else {
            continue;
        };
        if (2018..=2025).contains(&row.year) {
            years_18_25.push(row.year as f64);
            ratios_18_25.push(ratio);
        }
        if (2018..=2020).contains(&row.year) {
            ratios_18_20.push(ratio);
        }
        if (2022..=2025).contains(&row.year) {
            ratios_22_25.push(ratio);
        }
    }

    let pooled_omega = effort_factors
        .iter()
        .find(|f| f.has_modis && f.viirs_platforms.as_slice() == target_viirs)
        .and_then(|f| f.omega);

    let summary_2018_2025 = YearlyModisSnppNoaa20Summary {
        n_years: years_18_25.len(),
        rho_2018_2025: spearman_rho(&years_18_25, &ratios_18_25),
        n_2018_2020: ratios_18_20.len(),
        mean_2018_2020: block_mean(&ratios_18_20),
        n_2022_2025: ratios_22_25.len(),
        mean_2022_2025: block_mean(&ratios_22_25),
        pooled_omega,
    };

    YearlyOmegaAnalysis {
        region,
        config_rows,
        platform_rows,
        summary_2018_2025,
    }
}

async fn load_and_harmonize_country(
    archive: &ArchiveClient,
    config: &PipelineConfig,
    country: &str,
    from: i32,
    to: i32,
    mut noaa21_by_year: BTreeMap<i32, Vec<Hotspot>>,
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
                Err(ArchiveError::NotCached { .. })
                    if archive.sensor_year_complete(sensor, year) =>
                {
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

        if let Some(mut extra_viirs) = noaa21_by_year.remove(&year) {
            viirs_year.append(&mut extra_viirs);
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

fn format_pooling_key(has_modis: bool, viirs_platforms: &[Sensor]) -> String {
    let mut parts = Vec::new();
    if has_modis {
        parts.push("MODIS".to_string());
    }
    for s in viirs_platforms {
        parts.push(format!("{s:?}"));
    }
    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join("+")
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
            println!(
                "  {} ({} vs {}): {}",
                s.name,
                s.window_before,
                s.window_after,
                s.verdict.display_label()
            );
            continue;
        }
        let abs_log_raw = s.raw_shift.map(|v| v.ln().abs());
        let abs_log_norm = s.norm_shift.map(|v| v.ln().abs());
        let abs_log_raw_adj = s.raw_adj.map(|v| v.ln().abs());
        let abs_log_norm_adj = s.norm_adj.map(|v| v.ln().abs());
        println!(
            "  {} ({} vs {}): raw={}->{} (x{}, |log|={}) | norm={}->{} (x{}, |log|={}) | verdict={} | ref={}->{} (x{}) | adj_raw=|log|={}, adj_norm=|log|={} | adj_verdict(decision)={}",
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
            fmt_opt(s.ref_before, 1),
            fmt_opt(s.ref_after, 1),
            fmt_opt(s.ref_shift, 3),
            fmt_opt(abs_log_raw_adj, 4),
            fmt_opt(abs_log_norm_adj, 4),
            s.adj_verdict.display_label(),
        );
    }
}

fn print_summary_table(title: &str, analyses: &[&RegionAnalysis]) {
    println!("\n================================================================================");
    println!("{title}");
    println!("================================================================================");
    println!(
        "{:<18} {:<22} {:>9} {:>9} {:>10} {:>10} {:>10} {:>10} {:>9} {:>12} {:>13} {:>8} {:<28} adj_verdict(decision)",
        "region",
        "check",
        "raw_stat",
        "norm_stat",
        "|raw|",
        "|norm|",
        "ref_before",
        "ref_after",
        "ref_shift",
        "raw_adj_stat",
        "norm_adj_stat",
        "measured",
        "verdict",
    );
    println!("{}", "-".repeat(196));

    for analysis in analyses {
        let t = &analysis.trend;
        println!(
            "{:<18} {:<22} {:>9} {:>9} {:>10} {:>10} {:>10} {:>10} {:>9} {:>12} {:>13} {:>8} {:<28} {}",
            analysis.region,
            "trend_spearman_01_25",
            fmt_signed_opt(t.rho_raw, 4),
            fmt_signed_opt(t.rho_norm, 4),
            fmt_opt(t.rho_raw.map(f64::abs), 4),
            fmt_opt(t.rho_norm.map(f64::abs), 4),
            "NA",
            "NA",
            "NA",
            "NA",
            "NA",
            "yes",
            t.verdict.as_str(),
            t.verdict.as_str(),
        );

        for s in &analysis.shifts {
            if !s.measured {
                println!(
                    "{:<18} {:<22} {:>9} {:>9} {:>10} {:>10} {:>10} {:>10} {:>9} {:>12} {:>13} {:>8} {:<28} {}",
                    analysis.region,
                    s.name,
                    "NA",
                    "NA",
                    "NA",
                    "NA",
                    "NA",
                    "NA",
                    "NA",
                    "NA",
                    "NA",
                    "no",
                    s.verdict.display_label(),
                    s.adj_verdict.display_label(),
                );
                continue;
            }
            let log_raw = s.raw_shift.map(f64::ln);
            let log_norm = s.norm_shift.map(f64::ln);
            let log_raw_adj = s.raw_adj.map(f64::ln);
            let log_norm_adj = s.norm_adj.map(f64::ln);
            println!(
                "{:<18} {:<22} {:>9} {:>9} {:>10} {:>10} {:>10} {:>10} {:>9} {:>12} {:>13} {:>8} {:<28} {}",
                analysis.region,
                s.name,
                fmt_signed_opt(log_raw, 4),
                fmt_signed_opt(log_norm, 4),
                fmt_opt(log_raw.map(f64::abs), 4),
                fmt_opt(log_norm.map(f64::abs), 4),
                fmt_opt(s.ref_before, 1),
                fmt_opt(s.ref_after, 1),
                fmt_opt(s.ref_shift, 3),
                fmt_signed_opt(log_raw_adj, 4),
                fmt_signed_opt(log_norm_adj, 4),
                "yes",
                s.verdict.display_label(),
                s.adj_verdict.display_label(),
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
    let file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut w = BufWriter::new(file);
    writeln!(
        w,
        "region,check,window_before,window_after,n_raw_before,n_raw_after,n_norm_before,n_norm_after,raw_before,raw_after,raw_shift,raw_stat,abs_raw_stat,norm_before,norm_after,norm_shift,norm_stat,abs_norm_stat,half_abs_raw,ref_before,ref_after,ref_shift,raw_adj_stat,norm_adj_stat,measured,verdict,adj_verdict,note"
    )?;

    for a in analyses {
        let t = &a.trend;
        let abs_raw_rho = t.rho_raw.map(f64::abs);
        let abs_norm_rho = t.rho_norm.map(f64::abs);
        let half_rho = abs_raw_rho.map(|v| v * 0.5);
        writeln!(
            w,
            "{},trend_spearman,2001-2025,-,0,{},0,{},NA,NA,NA,{},{},NA,NA,NA,{},{},{},NA,NA,NA,NA,NA,true,{},{},",
            csv_escape(&a.region),
            t.raw_n,
            t.norm_n,
            fmt_opt(t.rho_raw, 6),
            fmt_opt(abs_raw_rho, 6),
            fmt_opt(t.rho_norm, 6),
            fmt_opt(abs_norm_rho, 6),
            fmt_opt(half_rho, 6),
            t.verdict.as_str(),
            t.verdict.as_str(),
        )?;

        for s in &a.shifts {
            let log_raw = s.raw_shift.map(f64::ln);
            let log_norm = s.norm_shift.map(f64::ln);
            let abs_raw = log_raw.map(f64::abs);
            let abs_norm = log_norm.map(f64::abs);
            let half_raw = abs_raw.map(|v| v * 0.5);
            let log_raw_adj = s.raw_adj.map(f64::ln);
            let log_norm_adj = s.norm_adj.map(f64::ln);
            writeln!(
                w,
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
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
                fmt_opt(s.ref_before, 2),
                fmt_opt(s.ref_after, 2),
                fmt_opt(s.ref_shift, 6),
                fmt_opt(log_raw_adj, 6),
                fmt_opt(log_norm_adj, 6),
                s.measured,
                s.verdict.as_str(),
                s.adj_verdict.as_str(),
                csv_escape(s.note),
            )?;
        }
    }

    w.flush()?;
    Ok(())
}

fn yearly_omega_csv_path(out_path: &Path) -> PathBuf {
    let stem = out_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "fleet_trend".to_string());
    let filename = format!("{stem}_yearly_omega.csv");
    match out_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(filename),
        _ => PathBuf::from(filename),
    }
}

fn print_yearly_omega_detail(yearly: &YearlyOmegaAnalysis) {
    println!("\n[Yearly Omega by platform configuration (pre-Aqua excluded)]");
    if yearly.config_rows.is_empty() {
        println!("  (no effort configurations)");
    } else {
        println!(
            "{:>4} {:<38} {:>6} {:>10} {:>10} {:>10} {:>12}",
            "year", "config", "months", "total", "modis_obs", "yr_ratio", "pooled_omega"
        );
        println!("{}", "-".repeat(96));
        for r in &yearly.config_rows {
            println!(
                "{:>4} {:<38} {:>6} {:>10} {:>10} {:>10} {:>12}",
                r.year,
                format_pooling_key(r.has_modis, &r.viirs_platforms),
                r.months,
                r.total,
                r.modis_observable,
                fmt_opt(r.yearly_ratio, 4),
                fmt_opt(r.pooled_omega, 4),
            );
        }
    }

    println!("\n[Yearly platform composition]");
    if yearly.platform_rows.is_empty() {
        println!("  (no harmonized records)");
    } else {
        println!(
            "{:>4} {:>10} {:>10} {:>10} {:>10} {:>10} {:>11}",
            "year", "records", "terra", "aqua", "viirs", "modis_obs", "modis_share"
        );
        println!("{}", "-".repeat(71));
        for r in &yearly.platform_rows {
            println!(
                "{:>4} {:>10} {:>10} {:>10} {:>10} {:>10} {:>11}",
                r.year,
                r.total_records,
                r.terra_records,
                r.aqua_records,
                r.viirs_records,
                r.modis_observable_records,
                fmt_opt(r.modis_observable_share, 4),
            );
        }
    }
}

fn print_yearly_omega_summary(analyses: &[YearlyOmegaAnalysis]) {
    println!("\n================================================================================");
    println!("YEARLY OMEGA SUMMARY (MODIS+ViirsSNPP+ViirsNOAA20)");
    println!("================================================================================");
    for a in analyses {
        let s = &a.summary_2018_2025;
        let block_ratio = ratio_opt(s.mean_2018_2020, s.mean_2022_2025);
        println!(
            "{:<18} n_18_25={} rho={} | mean_18_20(n={})={} vs mean_22_25(n={})={} (x{}) | pooled_omega={}",
            a.region,
            s.n_years,
            fmt_signed_opt(s.rho_2018_2025, 4),
            s.n_2018_2020,
            fmt_opt(s.mean_2018_2020, 4),
            s.n_2022_2025,
            fmt_opt(s.mean_2022_2025, 4),
            fmt_opt(block_ratio, 3),
            fmt_opt(s.pooled_omega, 4),
        );
    }
}

fn write_yearly_omega_csv(path: &Path, analyses: &[YearlyOmegaAnalysis]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut w = BufWriter::new(file);
    writeln!(
        w,
        "region,year,has_modis,viirs_platforms,config,months,total,modis_observable,yearly_ratio,pooled_omega,year_records,terra_records,aqua_records,viirs_records,modis_observable_records,modis_observable_share"
    )?;

    for a in analyses {
        let platforms_by_year: BTreeMap<i32, &YearlyPlatformRow> =
            a.platform_rows.iter().map(|p| (p.year, p)).collect();
        for c in &a.config_rows {
            let p = platforms_by_year.get(&c.year).copied();
            writeln!(
                w,
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                csv_escape(&a.region),
                c.year,
                c.has_modis,
                csv_escape(&format_platforms(&c.viirs_platforms)),
                csv_escape(&format_pooling_key(c.has_modis, &c.viirs_platforms)),
                c.months,
                c.total,
                c.modis_observable,
                fmt_opt(c.yearly_ratio, 6),
                fmt_opt(c.pooled_omega, 6),
                p.map_or(0, |r| r.total_records),
                p.map_or(0, |r| r.terra_records),
                p.map_or(0, |r| r.aqua_records),
                p.map_or(0, |r| r.viirs_records),
                p.map_or(0, |r| r.modis_observable_records),
                fmt_opt(p.and_then(|r| r.modis_observable_share), 6),
            )?;
        }
    }

    w.flush()?;
    Ok(())
}

type Noaa21CountryData = (BTreeMap<i32, Vec<Hotspot>>, bool);

fn group_wildfires_by_year(mut hotspots: Vec<Hotspot>) -> BTreeMap<i32, Vec<Hotspot>> {
    retain_wildfires(&mut hotspots);
    let mut by_year: BTreeMap<i32, Vec<Hotspot>> = BTreeMap::new();
    for h in hotspots {
        by_year
            .entry(h.acquired.date_naive().year())
            .or_default()
            .push(h);
    }
    by_year
}

async fn load_noaa21_by_country(args: &Args) -> Result<BTreeMap<String, Noaa21CountryData>> {
    let api_key = std::env::var("FIRMS_API_KEY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .context("FIRMS_API_KEY environment variable is required when --noaa21 is set")?;
    let area_cache_raw = std::env::var("AREA_CACHE_DIR").ok();
    let area_cache_dir = parse_area_cache_dir(area_cache_raw.as_deref());
    let mut client = FirmsClient::new(api_key)?;
    if let Some(ref dir) = area_cache_dir {
        println!("NOAA-21 area cache: {}", dir.display());
        let cache = AreaCache::new(
            dir.clone(),
            Duration::from_secs(DEFAULT_AREA_CACHE_NRT_TTL_SECS),
        );
        client = client.with_area_cache(cache);
    } else {
        println!("NOAA-21 area cache: disabled (AREA_CACHE_DIR is empty)");
    }

    let country_geos = load_country_geos(&args.regions_geo, &args.countries)?;
    let Some((start, end)) = noaa21_fetch_range(args.from, args.to) else {
        println!(
            "NOAA-21: requested years {}..={} do not overlap 2023-03-01..=2025-12-31; skipping area fetch",
            args.from, args.to
        );
        return Ok(args
            .countries
            .iter()
            .map(|c| (c.clone(), (BTreeMap::new(), false)))
            .collect());
    };

    println!("NOAA-21 fetch window: {start}..={end}");
    let mut result = BTreeMap::new();

    if args.noaa21_union {
        let bboxes: Vec<BBox> = args
            .countries
            .iter()
            .map(|c| country_geos[c].bbox.clone())
            .collect();
        let union =
            union_bbox(&bboxes).context("at least one country is required for --noaa21-union")?;
        println!(
            "NOAA-21 union bbox: [{:.3}, {:.3}, {:.3}, {:.3}]",
            union.west, union.south, union.east, union.north
        );
        let fetched = fetch_noaa21_range(&client, "UNION", &union, start, end).await;
        for country in &args.countries {
            let (kept, dropped) =
                clip_hotspots_to_country(&fetched.hotspots, &country_geos[country]);
            println!(
                "  NOAA-21 clip [{country}]: fetched={}, kept={}, dropped={dropped}",
                fetched.hotspots.len(),
                kept.len()
            );
            result.insert(
                country.clone(),
                (group_wildfires_by_year(kept), fetched.incomplete),
            );
        }
    } else {
        for country in &args.countries {
            let geo = &country_geos[country];
            let fetched = fetch_noaa21_range(&client, country, &geo.bbox, start, end).await;
            let (kept, dropped) = clip_hotspots_to_country(&fetched.hotspots, geo);
            println!(
                "  NOAA-21 clip [{country}]: fetched={}, kept={}, dropped={dropped}",
                fetched.hotspots.len(),
                kept.len()
            );
            result.insert(
                country.clone(),
                (group_wildfires_by_year(kept), fetched.incomplete),
            );
        }
    }

    Ok(result)
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
    if args.noaa21 {
        let mode = if args.noaa21_union {
            "union bbox"
        } else {
            "per-country bbox"
        };
        println!(
            "Sensors: MODIS (Terra+Aqua), VIIRS S-NPP, VIIRS NOAA-20, VIIRS NOAA-21 (area API SP)"
        );
        println!(
            "2023 (NOAA-21): enabled via VIIRS_NOAA21_SP ({mode}, regions: {})",
            args.regions_geo.display()
        );
    } else {
        println!("Archive sensors: MODIS (Terra+Aqua), VIIRS S-NPP, VIIRS NOAA-20");
        println!("2023 (NOAA-21): {NOAA21_NOT_MEASURABLE_MSG}");
    }

    let mut noaa21_by_country = if args.noaa21 {
        load_noaa21_by_country(&args).await?
    } else {
        BTreeMap::new()
    };

    let overall_t0 = Instant::now();
    let pool_multiple = args.countries.len() > 1;
    let mut pooled_harmonized = Vec::new();
    let mut pooled_missing_years = BTreeSet::new();
    let mut pooled_noaa21_incomplete = false;
    let mut any_noaa21_incomplete = false;
    let mut per_country_analyses = Vec::with_capacity(args.countries.len());
    let mut yearly_analyses = Vec::new();

    for country in &args.countries {
        let t0 = Instant::now();
        let (noaa21_years, noaa21_incomplete) =
            noaa21_by_country.remove(country).unwrap_or_default();
        any_noaa21_incomplete |= noaa21_incomplete;

        let (mut harmonized, missing_file_years) = load_and_harmonize_country(
            &archive,
            &config,
            country,
            args.from,
            args.to,
            noaa21_years,
        )
        .await?;
        let analysis = analyze_harmonized(
            country.clone(),
            &harmonized,
            &missing_file_years,
            &config,
            args.noaa21,
            noaa21_incomplete,
        );
        println!(
            "\nProcessed {country}: {} harmonized hotspots in {:.2}s",
            harmonized.len(),
            t0.elapsed().as_secs_f64()
        );
        print_region_detail(&analysis);
        if args.yearly_omega {
            let yearly = compute_yearly_omega(
                analysis.region.clone(),
                &harmonized,
                &analysis.effort_factors,
            );
            print_yearly_omega_detail(&yearly);
            yearly_analyses.push(yearly);
        }
        per_country_analyses.push(analysis);

        if pool_multiple {
            pooled_harmonized.append(&mut harmonized);
            pooled_missing_years.extend(missing_file_years);
            pooled_noaa21_incomplete |= noaa21_incomplete;
        }
    }

    let pooled_analysis = if pool_multiple {
        let t0 = Instant::now();
        let analysis = analyze_harmonized(
            "POOLED".to_string(),
            &pooled_harmonized,
            &pooled_missing_years,
            &config,
            args.noaa21,
            pooled_noaa21_incomplete,
        );
        println!(
            "\nProcessed POOLED ({} countries): {} harmonized hotspots in {:.2}s",
            args.countries.len(),
            pooled_harmonized.len(),
            t0.elapsed().as_secs_f64()
        );
        print_region_detail(&analysis);
        if args.yearly_omega {
            let yearly = compute_yearly_omega(
                analysis.region.clone(),
                &pooled_harmonized,
                &analysis.effort_factors,
            );
            print_yearly_omega_detail(&yearly);
            yearly_analyses.push(yearly);
        }
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

    if !args.noaa21 {
        println!(
            "\nTransition coverage: 2012 (S-NPP) = measured | 2018 (NOAA-20) = measured | 2023 (NOAA-21) = NOT measured ({NOAA21_NOT_MEASURABLE_MSG})"
        );
    } else if any_noaa21_incomplete {
        println!(
            "\nTransition coverage: 2012 (S-NPP) = measured | 2018 (NOAA-20) = measured | 2023 (NOAA-21) = PARTIAL ({NOAA21_INCOMPLETE_MSG} for one or more regions)"
        );
    } else {
        println!(
            "\nTransition coverage: 2012 (S-NPP) = measured | 2018 (NOAA-20) = measured | 2023 (NOAA-21) = measured (VIIRS_NOAA21_SP)"
        );
    }

    if args.yearly_omega {
        print_yearly_omega_summary(&yearly_analyses);
    }

    let mut all_analyses = per_country_analyses;
    if let Some(pooled) = pooled_analysis {
        all_analyses.push(pooled);
    }

    if let Some(ref out_path) = args.out {
        write_csv(out_path, &all_analyses)?;
        println!("CSV written: {}", out_path.display());
        if args.yearly_omega {
            let yearly_path = yearly_omega_csv_path(out_path);
            write_yearly_omega_csv(&yearly_path, &yearly_analyses)?;
            println!("Yearly Omega CSV written: {}", yearly_path.display());
        }
    }

    println!();
    for analysis in &all_analyses {
        println!("{}", format_k7_line(analysis));
    }

    println!("Completed in {:.2}s", overall_t0.elapsed().as_secs_f64());
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
            conformal: None,
            count_recovered: None,
            count_recovered_lower: None,
            count_recovered_upper: None,
        }
    }

    fn make_full_year_cells(
        year: i32,
        monthly_raw: u64,
        monthly_norm: Option<f64>,
    ) -> Vec<CalendarCell> {
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
        let mut cells = vec![make_cell(2000, 11, 50, None), make_cell(2000, 12, 50, None)];
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

    fn make_hotspot(year: i32, month: u32, platforms: Vec<Sensor>) -> HarmonizedHotspot {
        use chrono::{TimeZone, Utc};
        use pyrosync_core::models::{Confidence, DayNight, HotspotSource};

        let has_modis = platforms.iter().any(|s| s.is_modis());
        let has_viirs = platforms.iter().any(|s| s.is_viirs());
        let source = match (has_modis, has_viirs) {
            (true, true) => HotspotSource::Matched,
            (true, false) => HotspotSource::ModisOnly,
            (false, true) | (false, false) => HotspotSource::ViirsOnly,
        };
        HarmonizedHotspot {
            lat: 39.0,
            lon: 32.0,
            frp: 30.0,
            frp_corrected: 30.0,
            frp_lower: 27.6,
            frp_upper: 32.4,
            vza_factor: None,
            n_obs: platforms.len() as u16,
            platforms,
            confidence: Confidence::Nominal,
            source,
            acquired: Utc.with_ymd_and_hms(year, month, 15, 12, 0, 0).unwrap(),
            daynight: DayNight::Day,
        }
    }

    #[test]
    fn yearly_omega_ratio_and_modis_less_month_pooling_match_effort() {
        let hotspots = vec![
            // Month 1: Terra + Aqua + 3 S-NPP -> (true, [ViirsSNPP]), total=5, modis=2
            make_hotspot(2018, 1, vec![Sensor::ModisTerra]),
            make_hotspot(2018, 1, vec![Sensor::ModisAqua]),
            make_hotspot(2018, 1, vec![Sensor::ViirsSNPP]),
            make_hotspot(2018, 1, vec![Sensor::ViirsSNPP]),
            make_hotspot(2018, 1, vec![Sensor::ViirsSNPP]),
            // Month 2: MODIS-less month (only S-NPP) -> (false, [ViirsSNPP]), total=2, modis=0
            make_hotspot(2018, 2, vec![Sensor::ViirsSNPP]),
            make_hotspot(2018, 2, vec![Sensor::ViirsSNPP]),
            // Month 3: matched Terra+SNPP + 2 S-NPP -> (true, [ViirsSNPP]), total=3, modis=1
            make_hotspot(2018, 3, vec![Sensor::ModisTerra, Sensor::ViirsSNPP]),
            make_hotspot(2018, 3, vec![Sensor::ViirsSNPP]),
            make_hotspot(2018, 3, vec![Sensor::ViirsSNPP]),
            // Month 4: Aqua + S-NPP + NOAA-20 -> (true, [ViirsSNPP, ViirsNOAA20]), total=6, modis=1
            make_hotspot(2018, 4, vec![Sensor::ModisAqua]),
            make_hotspot(2018, 4, vec![Sensor::ViirsSNPP]),
            make_hotspot(2018, 4, vec![Sensor::ViirsSNPP]),
            make_hotspot(2018, 4, vec![Sensor::ViirsSNPP]),
            make_hotspot(2018, 4, vec![Sensor::ViirsNOAA20]),
            make_hotspot(2018, 4, vec![Sensor::ViirsNOAA20]),
        ];

        let effort_factors = estimate_effort(&hotspots);
        let yearly = compute_yearly_omega("TEST".to_string(), &hotspots, &effort_factors);

        let effort_keys: Vec<(bool, Vec<Sensor>)> = effort_factors
            .iter()
            .map(|f| (f.has_modis, f.viirs_platforms.clone()))
            .collect();
        let yearly_keys: Vec<(bool, Vec<Sensor>)> = yearly
            .config_rows
            .iter()
            .map(|r| (r.has_modis, r.viirs_platforms.clone()))
            .collect();
        assert_eq!(yearly_keys, effort_keys);

        let modis_less = &yearly.config_rows[0];
        assert!(!modis_less.has_modis);
        assert_eq!(modis_less.viirs_platforms, vec![Sensor::ViirsSNPP]);
        assert_eq!(modis_less.months, 1);
        assert_eq!(modis_less.total, 2);
        assert_eq!(modis_less.modis_observable, 0);
        assert_eq!(modis_less.yearly_ratio, None);

        let modis_snpp = &yearly.config_rows[1];
        assert!(modis_snpp.has_modis);
        assert_eq!(modis_snpp.viirs_platforms, vec![Sensor::ViirsSNPP]);
        assert_eq!(modis_snpp.months, 2);
        assert_eq!(modis_snpp.total, 8);
        assert_eq!(modis_snpp.modis_observable, 3);
        assert!((modis_snpp.yearly_ratio.unwrap() - 8.0 / 3.0).abs() < 1e-12);

        let modis_both_viirs = &yearly.config_rows[2];
        assert!(modis_both_viirs.has_modis);
        assert_eq!(
            modis_both_viirs.viirs_platforms,
            vec![Sensor::ViirsSNPP, Sensor::ViirsNOAA20]
        );
        assert_eq!(modis_both_viirs.months, 1);
        assert_eq!(modis_both_viirs.total, 6);
        assert_eq!(modis_both_viirs.modis_observable, 1);
        assert!((modis_both_viirs.yearly_ratio.unwrap() - 6.0).abs() < 1e-12);

        assert_eq!(yearly.platform_rows.len(), 1);
        let plat = &yearly.platform_rows[0];
        assert_eq!(plat.year, 2018);
        assert_eq!(plat.total_records, 16);
        assert_eq!(plat.terra_records, 2);
        assert_eq!(plat.aqua_records, 2);
        assert_eq!(plat.viirs_records, 13);
        assert_eq!(plat.modis_observable_records, 4);
        assert!((plat.modis_observable_share.unwrap() - 0.25).abs() < 1e-12);
    }

    #[test]
    fn yearly_omega_summary_and_csv_path() {
        // Pre-Aqua month excluded from effort pooling when series crosses 2002-07.
        let mut hotspots = vec![
            make_hotspot(2002, 5, vec![Sensor::ModisTerra]),
            make_hotspot(2002, 7, vec![Sensor::ModisTerra, Sensor::ModisAqua]),
        ];

        for (offset, year) in (2018..=2025).enumerate() {
            let viirs_count = 3 + offset;
            hotspots.push(make_hotspot(year, 6, vec![Sensor::ModisAqua]));
            for _ in 0..viirs_count {
                hotspots.push(make_hotspot(year, 6, vec![Sensor::ViirsSNPP]));
            }
            hotspots.push(make_hotspot(year, 6, vec![Sensor::ViirsNOAA20]));
        }

        let effort_factors = estimate_effort(&hotspots);
        let yearly = compute_yearly_omega("SYRIA".to_string(), &hotspots, &effort_factors);

        // 2002-05 was excluded, so 2002 config_rows only counts 2002-07 (1 month).
        let row_2002 = yearly.config_rows.iter().find(|r| r.year == 2002).unwrap();
        assert_eq!(row_2002.months, 1);

        // Ratios for 2018..=2025 are (1 + 3 + offset + 1) / 1 = 5..=12.
        let s = &yearly.summary_2018_2025;
        assert_eq!(s.n_years, 8);
        assert!((s.rho_2018_2025.unwrap() - 1.0).abs() < 1e-12);
        assert_eq!(s.n_2018_2020, 3);
        assert!((s.mean_2018_2020.unwrap() - 6.0).abs() < 1e-12);
        assert_eq!(s.n_2022_2025, 4);
        assert!((s.mean_2022_2025.unwrap() - 10.5).abs() < 1e-12);

        assert_eq!(
            yearly_omega_csv_path(Path::new("out.csv")),
            PathBuf::from("out_yearly_omega.csv")
        );
        assert_eq!(
            yearly_omega_csv_path(Path::new("reports/k7.csv")),
            PathBuf::from("reports/k7_yearly_omega.csv")
        );
    }

    #[test]
    fn point_in_polygon_square_with_hole_and_multipolygon() {
        let outer = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
            [0.0, 0.0],
        ];
        let hole = vec![[3.0, 3.0], [7.0, 3.0], [7.0, 7.0], [3.0, 7.0], [3.0, 3.0]];
        let poly_with_hole = vec![outer, hole];

        assert!(point_in_polygon(1.5, 5.0, &poly_with_hole));
        assert!(!point_in_polygon(5.0, 5.0, &poly_with_hole));
        assert!(!point_in_polygon(12.0, 5.0, &poly_with_hole));

        let second_poly = vec![vec![
            [20.0, 20.0],
            [25.0, 20.0],
            [25.0, 25.0],
            [20.0, 25.0],
            [20.0, 20.0],
        ]];
        let multipoly = vec![poly_with_hole, second_poly];

        assert!(point_in_multipolygon(1.5, 5.0, &multipoly));
        assert!(!point_in_multipolygon(5.0, 5.0, &multipoly));
        assert!(point_in_multipolygon(22.0, 22.0, &multipoly));
        assert!(!point_in_multipolygon(15.0, 15.0, &multipoly));
    }

    #[test]
    fn adjusted_criterion_recovers_quiet_year_and_applies_guard() {
        // Confounded unadjusted 2018-like shift: raw x1.32, norm x0.80, MODIS ref x0.71.
        assert_eq!(evaluate_shift(Some(1.32), Some(0.80)), ShiftVerdict::Fail);

        let (raw_adj, norm_adj, verdict) =
            evaluate_adjusted_shift(Some(1.32), Some(0.80), Some(0.71));
        assert!((raw_adj.unwrap() - 1.32 / 0.71).abs() < 1e-12);
        assert!((norm_adj.unwrap() - 0.80 / 0.71).abs() < 1e-12);
        assert_eq!(verdict, ShiftVerdict::Pass);

        // Guard applied to |ln(raw_adj)|: 1.40 / 1.38 = 1.0145 (|ln| < 0.0953).
        let (_, _, guard_verdict) = evaluate_adjusted_shift(Some(1.40), Some(1.60), Some(1.38));
        assert_eq!(guard_verdict, ShiftVerdict::RawShiftTooSmall);

        // Missing any part yields Unavailable.
        assert_eq!(
            evaluate_adjusted_shift(Some(1.32), Some(0.80), None),
            (None, None, ShiftVerdict::Unavailable)
        );
    }

    #[test]
    fn windows_2023_noaa21_selection_and_incomplete_handling() {
        let annual = vec![
            make_annual_row(2020, 100, Some(100.0)),
            make_annual_row(2021, 100, Some(100.0)),
            make_annual_row(2022, 100, Some(100.0)),
            make_annual_row(2023, 999_999, Some(999_999.0)),
            make_annual_row(2024, 300, Some(100.0)),
            make_annual_row(2025, 300, Some(100.0)),
        ];

        // Include non-season Jan-Apr cells that must NOT enter May-Dec totals.
        let mut cells = vec![
            make_cell(2022, 2, 500, Some(500.0)),
            make_cell(2023, 2, 10, Some(10.0)),
        ];
        for m in 5..=12 {
            cells.push(make_cell(2022, m, 25, Some(25.0)));
            cells.push(make_cell(2023, m, 75, Some(25.0)));
        }

        let mut modis_obs = BTreeMap::new();
        for year in [2020, 2021, 2022, 2024, 2025] {
            modis_obs.insert((year, 6), 50);
        }
        // Feb 2022 counts for the full-year block (2022 = 20 + 8 * 10 = 100) but not for May-Dec.
        modis_obs.insert((2022, 2), 20);
        for m in 5..=12 {
            modis_obs.insert((2022, m), 10);
            modis_obs.insert((2023, m), 10);
        }

        // Complete NOAA-21 fetch produces measured 2023_noaa21_may_dec and 2023_noaa21_block.
        let shifts =
            compute_shifts_with_ref(&annual, &cells, &BTreeSet::new(), &modis_obs, true, false);
        assert_eq!(shifts.len(), 5);

        let season = shifts
            .iter()
            .find(|s| s.name == "2023_noaa21_may_dec")
            .unwrap();
        assert_eq!(season.window_before, "2022-05..12");
        assert_eq!(season.window_after, "2023-05..12");
        assert!((season.raw_before.unwrap() - 200.0).abs() < 1e-12);
        assert!((season.raw_after.unwrap() - 600.0).abs() < 1e-12);
        assert!((season.ref_before.unwrap() - 80.0).abs() < 1e-12);
        assert!((season.ref_after.unwrap() - 80.0).abs() < 1e-12);
        assert_eq!(season.verdict, ShiftVerdict::Pass);
        assert_eq!(season.adj_verdict, ShiftVerdict::Pass);

        let block = shifts
            .iter()
            .find(|s| s.name == "2023_noaa21_block")
            .unwrap();
        assert_eq!(block.window_before, "2020-2022");
        assert_eq!(block.window_after, "2024-2025");
        assert_eq!(block.n_raw_before, 3);
        assert_eq!(block.n_raw_after, 2);
        assert!((block.raw_before.unwrap() - 100.0).abs() < 1e-12);
        assert!((block.raw_after.unwrap() - 300.0).abs() < 1e-12);
        assert!((block.norm_shift.unwrap() - 1.0).abs() < 1e-12);
        assert!((block.ref_shift.unwrap() - 50.0 / (200.0 / 3.0)).abs() < 1e-12);
        assert_eq!(block.verdict, ShiftVerdict::Pass);
        assert_eq!(block.adj_verdict, ShiftVerdict::Pass);

        // Incomplete fetch marks both 2023 checks unavailable instead of emitting a verdict.
        let incomplete =
            compute_shifts_with_ref(&annual, &cells, &BTreeSet::new(), &modis_obs, true, true);
        let inc_season = incomplete
            .iter()
            .find(|s| s.name == "2023_noaa21_may_dec")
            .unwrap();
        let inc_block = incomplete
            .iter()
            .find(|s| s.name == "2023_noaa21_block")
            .unwrap();
        assert!(!inc_season.measured);
        assert!(!inc_block.measured);
        assert_eq!(
            inc_season.verdict,
            ShiftVerdict::UnavailableNoaa21Incomplete
        );
        assert_eq!(
            inc_block.adj_verdict,
            ShiftVerdict::UnavailableNoaa21Incomplete
        );
        assert_eq!(inc_season.verdict.display_label(), NOAA21_INCOMPLETE_MSG);
    }

    #[test]
    fn union_bbox_covers_all_country_bboxes() {
        assert!(union_bbox(&[]).is_none());

        let bboxes = vec![
            BBox {
                west: 25.6,
                south: 35.8,
                east: 44.8,
                north: 42.1,
            },
            BBox {
                west: 19.3,
                south: 34.8,
                east: 28.2,
                north: 41.7,
            },
            BBox {
                west: 35.6,
                south: 32.3,
                east: 42.4,
                north: 37.3,
            },
        ];
        let u = union_bbox(&bboxes).unwrap();
        for b in &bboxes {
            assert!(u.west <= b.west);
            assert!(u.south <= b.south);
            assert!(u.east >= b.east);
            assert!(u.north >= b.north);
        }
        assert!((u.west - 19.3).abs() < 1e-12);
        assert!((u.south - 32.3).abs() < 1e-12);
        assert!((u.east - 44.8).abs() < 1e-12);
        assert!((u.north - 42.1).abs() < 1e-12);
    }

    fn make_raw_hotspot(lon: f64, lat: f64) -> Hotspot {
        use chrono::{TimeZone, Utc};
        use pyrosync_core::models::{Confidence, DayNight, FireType};

        Hotspot {
            lat,
            lon,
            brightness: 320.0,
            frp: 25.0,
            confidence: Confidence::Nominal,
            sensor: Sensor::ViirsNOAA21,
            acquired: Utc.with_ymd_and_hms(2023, 7, 15, 12, 0, 0).unwrap(),
            scan: 0.4,
            track: 0.4,
            satellite: "N21".to_string(),
            daynight: DayNight::Day,
            vza: None,
            fire_type: FireType::VegetationFire,
        }
    }

    #[test]
    fn noaa21_clipping_keeps_only_in_polygon_hotspots() {
        let geo = CountryGeo {
            bbox: BBox {
                west: 26.0,
                south: 36.0,
                east: 36.0,
                north: 42.0,
            },
            polygons: vec![vec![
                vec![
                    [26.0, 36.0],
                    [36.0, 36.0],
                    [36.0, 42.0],
                    [26.0, 42.0],
                    [26.0, 36.0],
                ],
                vec![
                    [30.0, 38.0],
                    [32.0, 38.0],
                    [32.0, 40.0],
                    [30.0, 40.0],
                    [30.0, 38.0],
                ],
            ]],
        };
        let rows = vec![
            make_raw_hotspot(28.0, 39.0), // inside
            make_raw_hotspot(31.0, 39.0), // in hole
            make_raw_hotspot(40.0, 39.0), // outside
        ];

        let (kept, dropped) = clip_hotspots_to_country(&rows, &geo);
        assert_eq!(kept.len(), 1);
        assert_eq!(dropped, 2);
        assert!((kept[0].lon - 28.0).abs() < 1e-12);
    }

    fn make_shift(name: &'static str, adj_verdict: ShiftVerdict) -> ShiftResult {
        ShiftResult {
            name,
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
            ref_before: None,
            ref_after: None,
            ref_shift: None,
            raw_adj: None,
            norm_adj: None,
            measured: true,
            verdict: adj_verdict,
            adj_verdict,
            note: "",
        }
    }

    #[test]
    fn k7_criterion_aggregation_and_summary_line() {
        let mut analysis = RegionAnalysis {
            region: "Turkey".to_string(),
            effort_factors: Vec::new(),
            annual: Vec::new(),
            trend: TrendResult {
                raw_n: 20,
                raw_span: Some((2001, 2025)),
                rho_raw: Some(0.8),
                norm_n: 20,
                norm_span: Some((2001, 2025)),
                rho_norm: Some(0.1),
                verdict: TrendVerdict::Pass,
            },
            shifts: vec![
                make_shift("2012_snpp", ShiftVerdict::Pass),
                make_shift("2018_noaa20_may_dec", ShiftVerdict::RawShiftTooSmall),
                make_shift("2018_noaa20_3yr", ShiftVerdict::Pass),
                make_shift("2023_noaa21_may_dec", ShiftVerdict::Pass),
                make_shift("2023_noaa21_block", ShiftVerdict::Pass),
            ],
        };
        assert!(k7_criterion_met(&analysis));
        assert_eq!(
            format_k7_line(&analysis),
            "Turkey: K7 criterion: trend=PASS 2012=PASS 2018=RAW_SHIFT_TOO_SMALL/PASS 2023=PASS/PASS -> MET"
        );

        // Same-season FAIL must fail K7 even when 3yr block passes.
        analysis.shifts[1].adj_verdict = ShiftVerdict::Fail;
        assert!(!k7_criterion_met(&analysis));
        assert!(format_k7_line(&analysis).ends_with("-> NOT MET"));

        // Unmeasured 2023 cannot satisfy K7.
        analysis.shifts[1].adj_verdict = ShiftVerdict::Pass;
        analysis.shifts.truncate(3);
        analysis
            .shifts
            .push(make_shift("2023_noaa21", ShiftVerdict::NotMeasured));
        assert!(!k7_criterion_met(&analysis));
        assert_eq!(
            format_k7_line(&analysis),
            "Turkey: K7 criterion: trend=PASS 2012=PASS 2018=PASS/PASS 2023=NOT_MEASURED/NOT_MEASURED -> NOT MET"
        );
    }
}
