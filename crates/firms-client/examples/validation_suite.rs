//! Internal validation suite for PyroSync harmonization:
//! (A) Placebo step test across sensor seams (2002 Aqua, 2012 SNPP, 2018 NOAA-20),
//! (B) Leave-One-Year-Out (LOYO) effort-normalization accuracy vs. baselines,
//! (C) Year-blocked bootstrap "Model vs Baseline" gate (90% CI).
//!
//! Usage:
//!   ARCHIVE_CACHE_DIR=../../.cache/firms-archive \
//!     cargo run --release -p pyrosync-firms --example validation_suite -- \
//!     <Country>... [--from 2000] [--to 2025] [--out <csv>] [--gate-json <path>] [--bootstrap 1000] [--seed 42]

use anyhow::{Context, Result, bail};
use chrono::{Datelike, SecondsFormat, Utc};
use pyrosync_core::calendar::build_calendar_with;
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::effort::FIRST_TERRA_AQUA_MONTH;
use pyrosync_core::harmonize::harmonize_with_config;
use pyrosync_core::models::{CalendarCell, HarmonizedHotspot, Sensor};
use pyrosync_firms::archive::{ArchiveClient, ArchiveError};
use pyrosync_firms::parse::retain_wildfires;
use std::collections::{BTreeMap, BTreeSet, HashMap};
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
const DEFAULT_BOOTSTRAP: usize = 1000;
const DEFAULT_SEED: u64 = 42;
const MIN_MONTHS_PER_CONFIG: usize = 3;
const SEAM_YEARS: [i32; 3] = [2002, 2012, 2018];

type PoolingKey = (bool, Vec<Sensor>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Metric {
    Raw,
    CountNormalized,
    TotalFrp,
    FYear,
}

impl Metric {
    const ALL: [Self; 4] = [
        Self::Raw,
        Self::CountNormalized,
        Self::TotalFrp,
        Self::FYear,
    ];

    fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::CountNormalized => "count_normalized",
            Self::TotalFrp => "total_frp",
            Self::FYear => "F_year",
        }
    }

    fn extract(self, summary: &AnnualSummary) -> Option<f64> {
        match self {
            Self::Raw => (summary.raw > 0).then_some(summary.raw as f64),
            Self::CountNormalized => summary
                .count_normalized
                .filter(|v| v.is_finite() && *v > 0.0),
            Self::TotalFrp => summary.total_frp.filter(|v| v.is_finite() && *v > 0.0),
            Self::FYear => summary.f_year.filter(|v| v.is_finite() && *v > 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SeamSpec {
    name: &'static str,
    before: i32,
    after: i32,
}

const SEAMS: [SeamSpec; 3] = [
    SeamSpec {
        name: "aqua_2002",
        before: 2001,
        after: 2003,
    },
    SeamSpec {
        name: "snpp_2012",
        before: 2011,
        after: 2012,
    },
    SeamSpec {
        name: "noaa20_2018",
        before: 2017,
        after: 2019,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BaselineKind {
    Unnormalized,
    Persistence,
}

impl BaselineKind {
    const ALL: [Self; 2] = [Self::Unnormalized, Self::Persistence];

    fn as_str(self) -> &'static str {
        match self {
            Self::Unnormalized => "b0_unnormalized",
            Self::Persistence => "b1_persistence",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateVerdict {
    Pass,
    Uncertain,
    Fail,
}

impl GateVerdict {
    fn as_json_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Uncertain => "inconclusive",
            Self::Fail => "fail",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "GEÇTİ",
            Self::Uncertain => "BELİRSİZ",
            Self::Fail => "KALDI",
        }
    }
}

struct Args {
    countries: Vec<String>,
    from: i32,
    to: i32,
    out: Option<PathBuf>,
    gate_json: Option<PathBuf>,
    bootstrap: usize,
    seed: u64,
}

#[derive(Debug, Clone)]
struct AnnualSummary {
    raw: u64,
    count_normalized: Option<f64>,
    total_frp: Option<f64>,
    f_year: Option<f64>,
}

#[derive(Debug, Clone)]
struct MonthBucket {
    platforms: BTreeSet<Sensor>,
    total: u64,
    modis_observable: u64,
}

#[derive(Debug, Clone)]
struct PlaceboRow {
    country: String,
    seam: &'static str,
    before_year: i32,
    after_year: i32,
    metric: Metric,
    real_log_step: Option<f64>,
    real_abs_log_step: Option<f64>,
    n_placebo: usize,
    placebo_median: Option<f64>,
    placebo_p90: Option<f64>,
    empirical_p: Option<f64>,
    pct_rank: Option<f64>,
}

#[derive(Debug, Clone)]
struct PooledPlaceboRow {
    seam: &'static str,
    metric: Metric,
    n_countries: usize,
    mean_pct_rank: Option<f64>,
    median_pct_rank: Option<f64>,
    n_above_half: usize,
    n_below_half: usize,
    sign_p: Option<f64>,
    wilcoxon_w_plus: Option<f64>,
    wilcoxon_p: Option<f64>,
}

#[derive(Debug, Clone)]
struct LoyoYearRow {
    country: String,
    year: i32,
    total_obs: u64,
    modis_observable: u64,
    skipped_months: usize,
    pred_loyo: Option<f64>,
    pred_b0: Option<f64>,
    pred_b1: Option<f64>,
    err_loyo: Option<f64>,
    err_b0: Option<f64>,
    err_b1: Option<f64>,
}

#[derive(Debug, Clone)]
struct GateRow {
    country: String,
    baseline: BaselineKind,
    n_years: usize,
    mean_err_model: Option<f64>,
    mean_err_baseline: Option<f64>,
    mean_diff: Option<f64>,
    ci90_lo: Option<f64>,
    ci90_hi: Option<f64>,
    verdict: Option<GateVerdict>,
}

fn print_usage() {
    println!(
        "\
Usage: validation_suite <Country>... [--from {DEFAULT_FROM_YEAR}] [--to {DEFAULT_TO_YEAR}] [--out <csv>] [--gate-json <path>] [--bootstrap {DEFAULT_BOOTSTRAP}] [--seed {DEFAULT_SEED}]

Run internal validation tests (Placebo step test, LOYO normalization, Model vs Baseline gate).
Archive directory: ARCHIVE_CACHE_DIR env var, or {DEFAULT_ARCHIVE_CACHE_DIR}."
    );
}

fn parse_args() -> Result<Option<Args>> {
    let mut raw_args = std::env::args().skip(1);
    let mut countries = Vec::new();
    let mut from = DEFAULT_FROM_YEAR;
    let mut to = DEFAULT_TO_YEAR;
    let mut out: Option<PathBuf> = None;
    let mut gate_json = None;
    let mut bootstrap = DEFAULT_BOOTSTRAP;
    let mut seed = DEFAULT_SEED;

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
            "--gate-json" => {
                let val = raw_args.next().context("missing value for --gate-json")?;
                gate_json = Some(PathBuf::from(val));
            }
            "--bootstrap" => {
                let val = raw_args.next().context("missing value for --bootstrap")?;
                bootstrap = val
                    .parse()
                    .with_context(|| format!("invalid --bootstrap count: {val}"))?;
            }
            "--seed" => {
                let val = raw_args.next().context("missing value for --seed")?;
                seed = val
                    .parse()
                    .with_context(|| format!("invalid --seed value: {val}"))?;
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
    if bootstrap == 0 {
        bail!("--bootstrap must be >= 1");
    }

    Ok(Some(Args {
        countries,
        from,
        to,
        out,
        gate_json,
        bootstrap,
        seed,
    }))
}

fn archive_cache_dir() -> PathBuf {
    match std::env::var_os("ARCHIVE_CACHE_DIR") {
        Some(val) if !val.is_empty() => PathBuf::from(val),
        _ => PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR),
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
        let count: u64 = year_cells.iter().map(|c| c.count).sum();

        let mut norm_sum = 0.0_f64;
        let mut has_norm = false;
        for c in year_cells {
            if let Some(v) = c.count_normalized
                && v.is_finite()
            {
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

        out.insert(
            year,
            AnnualSummary {
                raw,
                count_normalized,
                total_frp,
                f_year,
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

fn build_placebo_samples(
    years: &BTreeMap<i32, AnnualSummary>,
    metric: Metric,
    delta: i32,
) -> Vec<f64> {
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
    samples
}

fn compute_placebo_rows(country: &str, years: &BTreeMap<i32, AnnualSummary>) -> Vec<PlaceboRow> {
    let mut rows = Vec::with_capacity(SEAMS.len() * Metric::ALL.len());
    let mut cache: HashMap<(i32, &'static str), Vec<f64>> = HashMap::new();

    for seam in SEAMS {
        let delta = seam.after - seam.before;
        let s_before = years.get(&seam.before);
        let s_after = years.get(&seam.after);

        for metric in Metric::ALL {
            let samples = cache
                .entry((delta, metric.as_str()))
                .or_insert_with(|| build_placebo_samples(years, metric, delta));
            let before_val = s_before.and_then(|s| metric.extract(s));
            let after_val = s_after.and_then(|s| metric.extract(s));

            let (real_log_step, real_abs_log_step) = match (before_val, after_val) {
                (Some(b), Some(a)) if b > 0.0 && a > 0.0 => {
                    let l = (a / b).ln();
                    (Some(l), Some(l.abs()))
                }
                _ => (None, None),
            };

            let n_placebo = samples.len();
            let placebo_median = quantile(samples, 0.50);
            let placebo_p90 = quantile(samples, 0.90);

            let (empirical_p, pct_rank) = match real_abs_log_step {
                Some(abs_step) if n_placebo > 0 => {
                    let k_ge = samples.iter().filter(|&&x| x >= abs_step).count();
                    let p = (k_ge + 1) as f64 / (n_placebo + 1) as f64;
                    // Mid-rank centers the null expectation at 0.50 even if exact ties occur.
                    let lt = samples.iter().filter(|&&x| x < abs_step).count();
                    let eq = samples.iter().filter(|&&x| x == abs_step).count();
                    let rank = (lt as f64 + 0.5 * eq as f64) / n_placebo as f64;
                    (Some(p), Some(rank))
                }
                _ => (None, None),
            };

            rows.push(PlaceboRow {
                country: country.to_string(),
                seam: seam.name,
                before_year: seam.before,
                after_year: seam.after,
                metric,
                real_log_step,
                real_abs_log_step,
                n_placebo,
                placebo_median,
                placebo_p90,
                empirical_p,
                pct_rank,
            });
        }
    }

    rows
}

fn exact_two_sided_sign_p(k_pos: usize, k_neg: usize) -> Option<f64> {
    let m = k_pos + k_neg;
    if m == 0 {
        return None;
    }
    let tail = k_pos.min(k_neg);
    let mut sum = 0.0_f64;
    let mut binom = 1.0_f64;
    for j in 0..=tail {
        if j > 0 {
            binom *= (m - j + 1) as f64 / j as f64;
        }
        sum += binom;
    }
    let p = 2.0 * sum * 0.5_f64.powi(m as i32);
    Some(p.min(1.0))
}

fn exact_two_sided_wilcoxon(diffs: &[f64]) -> (Option<f64>, Option<f64>) {
    let nonzero: Vec<f64> = diffs
        .iter()
        .copied()
        .filter(|d| d.abs() > 1e-12 && d.is_finite())
        .collect();
    let m = nonzero.len();
    if m == 0 {
        return (None, None);
    }

    let mut indexed: Vec<(usize, f64, bool)> = nonzero
        .iter()
        .enumerate()
        .map(|(i, &d)| (i, d.abs(), d > 0.0))
        .collect();
    indexed.sort_by(|a, b| a.1.total_cmp(&b.1));

    let mut ranks = vec![0.0_f64; m];
    let mut pos = 0;
    while pos < m {
        let mut end = pos + 1;
        while end < m && (indexed[end].1 - indexed[pos].1).abs() <= 1e-12 {
            end += 1;
        }
        let avg_rank = 0.5 * ((pos + 1 + end) as f64);
        ranks[pos..end].fill(avg_rank);
        pos = end;
    }

    let w_plus: f64 = indexed
        .iter()
        .enumerate()
        .filter(|(_, item)| item.2)
        .map(|(k, _)| ranks[k])
        .sum();
    let total_rank_sum: f64 = ranks.iter().sum();

    if m > 20 {
        // Normal fallback only if called on >20 countries; standard path for <=20 is exact below.
        let mean_w = 0.5 * total_rank_sum;
        let var_w = (m * (m + 1) * (2 * m + 1)) as f64 / 24.0;
        let z = ((w_plus - mean_w).abs() - 0.5).max(0.0) / var_w.sqrt();
        // Abramowitz & Stegun 7.1.26 erfc approximation for two-sided normal p-value.
        let t = 1.0 / (1.0 + 0.2316419 * z);
        let poly = t
            * (0.319381530
                + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
        let pdf = (-0.5 * z * z).exp() / (2.0 * std::f64::consts::PI).sqrt();
        return (Some(w_plus), Some((2.0 * pdf * poly).clamp(0.0, 1.0)));
    }

    let target_dev = (w_plus - 0.5 * total_rank_sum).abs() - 1e-9;
    let total_masks = 1_usize << m;
    let mut extreme = 0_usize;
    for mask in 0..total_masks {
        let mut w = 0.0_f64;
        for (k, &r) in ranks.iter().enumerate() {
            if (mask >> k) & 1 == 1 {
                w += r;
            }
        }
        if (w - 0.5 * total_rank_sum).abs() >= target_dev {
            extreme += 1;
        }
    }
    let p = (extreme as f64 / total_masks as f64).min(1.0);
    (Some(w_plus), Some(p))
}

fn compute_pooled_placebo_rows(all_rows: &[PlaceboRow]) -> Vec<PooledPlaceboRow> {
    let mut out = Vec::with_capacity(SEAMS.len() * Metric::ALL.len());
    for seam in SEAMS {
        for metric in Metric::ALL {
            let mut ranks: Vec<f64> = all_rows
                .iter()
                .filter(|r| r.seam == seam.name && r.metric == metric)
                .filter_map(|r| r.pct_rank)
                .collect();
            ranks.sort_by(f64::total_cmp);
            let n_countries = ranks.len();
            let mean_pct_rank =
                (n_countries > 0).then(|| ranks.iter().sum::<f64>() / n_countries as f64);
            let median_pct_rank = quantile(&ranks, 0.50);

            let n_above_half = ranks.iter().filter(|&&r| r > 0.5 + 1e-12).count();
            let n_below_half = ranks.iter().filter(|&&r| r < 0.5 - 1e-12).count();
            let sign_p = exact_two_sided_sign_p(n_above_half, n_below_half);
            let diffs: Vec<f64> = ranks.iter().map(|&r| r - 0.5).collect();
            let (wilcoxon_w_plus, wilcoxon_p) = exact_two_sided_wilcoxon(&diffs);

            out.push(PooledPlaceboRow {
                seam: seam.name,
                metric,
                n_countries,
                mean_pct_rank,
                median_pct_rank,
                n_above_half,
                n_below_half,
                sign_p,
                wilcoxon_w_plus,
                wilcoxon_p,
            });
        }
    }
    out
}

fn aggregate_months(hotspots: &[HarmonizedHotspot]) -> HashMap<(i32, u32), MonthBucket> {
    let mut months: HashMap<(i32, u32), MonthBucket> = HashMap::new();
    for h in hotspots {
        let date = h.acquired.date_naive();
        let bucket = months
            .entry((date.year(), date.month()))
            .or_insert_with(|| MonthBucket {
                platforms: BTreeSet::new(),
                total: 0,
                modis_observable: 0,
            });
        bucket.total += 1;
        if h.platforms.iter().any(|s| s.is_modis()) {
            bucket.modis_observable += 1;
        }
        bucket.platforms.extend(h.platforms.iter().copied());
    }
    months
}

fn pooling_key(platforms: &BTreeSet<Sensor>) -> PoolingKey {
    let has_modis = platforms.iter().any(|s| s.is_modis());
    let viirs_platforms = platforms.iter().copied().filter(|s| s.is_viirs()).collect();
    (has_modis, viirs_platforms)
}

fn take_pre_aqua_months(months: &mut HashMap<(i32, u32), MonthBucket>) {
    if !months.keys().any(|&ym| ym >= FIRST_TERRA_AQUA_MONTH) {
        return;
    }
    months.retain(|&ym, _| ym >= FIRST_TERRA_AQUA_MONTH);
}

fn compute_loyo_rows(
    country: &str,
    from: i32,
    to: i32,
    raw_months: &HashMap<(i32, u32), MonthBucket>,
) -> Vec<LoyoYearRow> {
    let mut effort_months = raw_months.clone();
    take_pre_aqua_months(&mut effort_months);

    let mut year_stats: BTreeMap<i32, (u64, u64, bool)> = BTreeMap::new();
    for (&(yr, _mo), bucket) in raw_months {
        let entry = year_stats.entry(yr).or_insert((0, 0, false));
        entry.0 += bucket.total;
        entry.1 += bucket.modis_observable;
        if bucket.platforms.iter().any(|s| s.is_viirs()) {
            entry.2 = true;
        }
    }

    let mut rows = Vec::new();
    for year in from..=to {
        let Some(&(total_obs, modis_observable, has_viirs)) = year_stats.get(&year) else {
            continue;
        };
        if !has_viirs {
            continue;
        }

        // Pool all effort-comparable months outside `year` by (has_modis, viirs_platforms).
        let mut pools: BTreeMap<PoolingKey, (usize, u64, u64)> = BTreeMap::new();
        for (&(yr, _mo), bucket) in &effort_months {
            if yr == year {
                continue;
            }
            let key = pooling_key(&bucket.platforms);
            let acc = pools.entry(key).or_insert((0, 0, 0));
            acc.0 += 1;
            acc.1 += bucket.total;
            acc.2 += bucket.modis_observable;
        }

        let mut pred_sum = 0.0_f64;
        let mut has_valid_month = false;
        let mut skipped_months = 0_usize;

        for (&(yr, _mo), bucket) in &effort_months {
            if yr != year {
                continue;
            }
            let key = pooling_key(&bucket.platforms);
            let omega_loo = pools.get(&key).and_then(|&(months, tot, mob)| {
                (months >= MIN_MONTHS_PER_CONFIG && mob > 0).then(|| tot as f64 / mob as f64)
            });
            match omega_loo {
                Some(omega) if omega.is_finite() && omega > 0.0 => {
                    pred_sum += bucket.total as f64 / omega;
                    has_valid_month = true;
                }
                _ => {
                    skipped_months += 1;
                }
            }
        }

        let pred_loyo = (has_valid_month && pred_sum > 0.0).then_some(pred_sum);
        let pred_b0 = (total_obs > 0).then_some(total_obs as f64);
        let pred_b1 = year_stats
            .get(&(year - 1))
            .and_then(|&(_, prev_mob, _)| (prev_mob > 0).then_some(prev_mob as f64));

        let abs_log_err = |pred: Option<f64>| -> Option<f64> {
            match pred {
                Some(p) if p > 0.0 && modis_observable > 0 => {
                    Some((p / modis_observable as f64).ln().abs())
                }
                _ => None,
            }
        };

        rows.push(LoyoYearRow {
            country: country.to_string(),
            year,
            total_obs,
            modis_observable,
            skipped_months,
            pred_loyo,
            pred_b0,
            pred_b1,
            err_loyo: abs_log_err(pred_loyo),
            err_b0: abs_log_err(pred_b0),
            err_b1: abs_log_err(pred_b1),
        });
    }

    rows
}

struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    fn from_seed(seed: u64, stream: u64) -> Self {
        // Mix seed and stream so each gate is deterministic and invariant to country order.
        let mut z = seed.wrapping_add(stream.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        if z == 0 {
            z = 0x9E37_79B9_7F4A_7C15;
        }
        Self { state: z }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn gen_index(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

fn gate_verdict(lo: f64, hi: f64) -> GateVerdict {
    if lo > 0.0 {
        GateVerdict::Pass
    } else if hi < 0.0 {
        GateVerdict::Fail
    } else {
        GateVerdict::Uncertain
    }
}

fn compute_gate_row(
    label: &str,
    baseline: BaselineKind,
    loyo_rows: &[LoyoYearRow],
    bootstrap: usize,
    seed: u64,
    stream_id: u64,
) -> GateRow {
    // Group (err_model, err_base) by year so bootstrap resamples whole year-blocks across countries.
    let mut by_year: BTreeMap<i32, Vec<(f64, f64)>> = BTreeMap::new();
    for r in loyo_rows {
        let err_base = match baseline {
            BaselineKind::Unnormalized => r.err_b0,
            BaselineKind::Persistence => r.err_b1,
        };
        if let (Some(em), Some(eb)) = (r.err_loyo, err_base) {
            by_year.entry(r.year).or_default().push((em, eb));
        }
    }

    let blocks: Vec<Vec<f64>> = by_year
        .values()
        .map(|pairs| pairs.iter().map(|&(em, eb)| eb - em).collect())
        .collect();
    let n_obs: usize = by_year.values().map(Vec::len).sum();
    let n_years = blocks.len();

    if n_years == 0 || n_obs == 0 {
        return GateRow {
            country: label.to_string(),
            baseline,
            n_years: 0,
            mean_err_model: None,
            mean_err_baseline: None,
            mean_diff: None,
            ci90_lo: None,
            ci90_hi: None,
            verdict: None,
        };
    }

    let sum_em: f64 = by_year.values().flatten().map(|&(em, _)| em).sum();
    let sum_eb: f64 = by_year.values().flatten().map(|&(_, eb)| eb).sum();
    let mean_err_model = Some(sum_em / n_obs as f64);
    let mean_err_baseline = Some(sum_eb / n_obs as f64);
    let mean_diff = Some((sum_eb - sum_em) / n_obs as f64);

    let mut rng = Xorshift64::from_seed(seed, stream_id);
    let mut boot_means = Vec::with_capacity(bootstrap);
    for _ in 0..bootstrap {
        let mut sum = 0.0_f64;
        let mut count = 0_usize;
        for _ in 0..n_years {
            let block = &blocks[rng.gen_index(n_years)];
            sum += block.iter().sum::<f64>();
            count += block.len();
        }
        boot_means.push(sum / count as f64);
    }
    boot_means.sort_by(f64::total_cmp);

    let ci90_lo = quantile(&boot_means, 0.05);
    let ci90_hi = quantile(&boot_means, 0.95);
    let verdict = match (ci90_lo, ci90_hi) {
        (Some(lo), Some(hi)) => Some(gate_verdict(lo, hi)),
        _ => None,
    };

    GateRow {
        country: label.to_string(),
        baseline,
        n_years,
        mean_err_model,
        mean_err_baseline,
        mean_diff,
        ci90_lo,
        ci90_hi,
        verdict,
    }
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

fn print_placebo_table(rows: &[PlaceboRow]) {
    println!("\n[A. Placebo Step Test]");
    println!(
        "{:<13} {:<9} {:<17} {:>9} {:>9} {:>5} {:>8} {:>8} {:>8} {:>7}",
        "seam",
        "years",
        "metric",
        "log_step",
        "abs_step",
        "n_pl",
        "pl_med",
        "pl_p90",
        "emp_p",
        "pct_rk"
    );
    println!("{}", "-".repeat(103));
    for r in rows {
        println!(
            "{:<13} {:>4}->{:<4} {:<17} {:>9} {:>9} {:>5} {:>8} {:>8} {:>8} {:>7}",
            r.seam,
            r.before_year,
            r.after_year,
            r.metric.as_str(),
            fmt_signed_opt(r.real_log_step, 4),
            fmt_opt(r.real_abs_log_step, 4),
            r.n_placebo,
            fmt_opt(r.placebo_median, 4),
            fmt_opt(r.placebo_p90, 4),
            fmt_opt(r.empirical_p, 4),
            fmt_opt(r.pct_rank.map(|p| p * 100.0), 1),
        );
    }
}

fn print_loyo_table(rows: &[LoyoYearRow]) {
    println!("\n[B. LOYO Normalization Test (VIIRS era)]");
    println!(
        "{:>4} {:>10} {:>10} {:>4} {:>11} {:>11} {:>11} {:>8} {:>8} {:>8}",
        "year",
        "total",
        "modis_obs",
        "skip",
        "pred_loyo",
        "pred_b0",
        "pred_b1",
        "err_mod",
        "err_b0",
        "err_b1"
    );
    println!("{}", "-".repeat(106));
    for r in rows {
        println!(
            "{:>4} {:>10} {:>10} {:>4} {:>11} {:>11} {:>11} {:>8} {:>8} {:>8}",
            r.year,
            r.total_obs,
            r.modis_observable,
            r.skipped_months,
            fmt_opt(r.pred_loyo, 1),
            fmt_opt(r.pred_b0, 1),
            fmt_opt(r.pred_b1, 1),
            fmt_opt(r.err_loyo, 4),
            fmt_opt(r.err_b0, 4),
            fmt_opt(r.err_b1, 4),
        );
    }
    let mean_of = |f: fn(&LoyoYearRow) -> Option<f64>| -> Option<f64> {
        let vals: Vec<f64> = rows.iter().filter_map(f).collect();
        (!vals.is_empty()).then(|| vals.iter().sum::<f64>() / vals.len() as f64)
    };
    println!("{}", "-".repeat(106));
    println!(
        "{:<4} {:>10} {:>10} {:>4} {:>11} {:>11} {:>11} {:>8} {:>8} {:>8}",
        "MEAN",
        "-",
        "-",
        rows.iter().map(|r| r.skipped_months).sum::<usize>(),
        "-",
        "-",
        "-",
        fmt_opt(mean_of(|r| r.err_loyo), 4),
        fmt_opt(mean_of(|r| r.err_b0), 4),
        fmt_opt(mean_of(|r| r.err_b1), 4),
    );
}

fn print_gate_table(title: &str, rows: &[GateRow]) {
    println!("\n[{title}]");
    println!(
        "{:<20} {:<16} {:>4} {:>9} {:>9} {:>10} {:>10} {:>10} {:<10}",
        "scope",
        "baseline",
        "n_yr",
        "err_model",
        "err_base",
        "diff(B-M)",
        "ci90_lo",
        "ci90_hi",
        "verdict"
    );
    println!("{}", "-".repeat(108));
    for r in rows {
        println!(
            "{:<20} {:<16} {:>4} {:>9} {:>9} {:>10} {:>10} {:>10} {:<10}",
            r.country,
            r.baseline.as_str(),
            r.n_years,
            fmt_opt(r.mean_err_model, 4),
            fmt_opt(r.mean_err_baseline, 4),
            fmt_signed_opt(r.mean_diff, 4),
            fmt_signed_opt(r.ci90_lo, 4),
            fmt_signed_opt(r.ci90_hi, 4),
            r.verdict.map_or("NA", GateVerdict::as_str),
        );
    }
}

fn print_pooled_placebo_summary(rows: &[PooledPlaceboRow]) {
    println!("\n================================================================================");
    println!("POOLED ACROSS-COUNTRY PLACEBO RANK TEST (H0: mean percentile rank == 0.50)");
    println!("================================================================================");
    println!(
        "{:<13} {:<17} {:>3} {:>9} {:>9} {:>6} {:>6} {:>8} {:>7} {:>8}",
        "seam", "metric", "n", "mean_rk", "med_rk", ">0.5", "<0.5", "sign_p", "W_plus", "wilc_p"
    );
    println!("{}", "-".repeat(96));
    for r in rows {
        println!(
            "{:<13} {:<17} {:>3} {:>9} {:>9} {:>6} {:>6} {:>8} {:>7} {:>8}",
            r.seam,
            r.metric.as_str(),
            r.n_countries,
            fmt_opt(r.mean_pct_rank, 4),
            fmt_opt(r.median_pct_rank, 4),
            r.n_above_half,
            r.n_below_half,
            fmt_opt(r.sign_p, 4),
            fmt_opt(r.wilcoxon_w_plus, 1),
            fmt_opt(r.wilcoxon_p, 4),
        );
    }
}

fn print_cross_country_loyo_summary(loyo_rows: &[LoyoYearRow]) {
    println!("\n================================================================================");
    println!("ACROSS-COUNTRY LOYO ERROR SUMMARY (|ln(pred/actual)|)");
    println!("================================================================================");
    println!(
        "{:<20} {:>5} {:>10} {:>10} {:>10}",
        "country", "n_yr", "err_model", "err_b0", "err_b1"
    );
    println!("{}", "-".repeat(60));

    let mut countries_order = Vec::new();
    let mut by_country: HashMap<&str, Vec<&LoyoYearRow>> = HashMap::new();
    for r in loyo_rows {
        if !by_country.contains_key(r.country.as_str()) {
            countries_order.push(r.country.as_str());
        }
        by_country.entry(r.country.as_str()).or_default().push(r);
    }

    let mean_slice = |rows: &[&LoyoYearRow], f: fn(&LoyoYearRow) -> Option<f64>| -> Option<f64> {
        let vals: Vec<f64> = rows.iter().filter_map(|r| f(r)).collect();
        (!vals.is_empty()).then(|| vals.iter().sum::<f64>() / vals.len() as f64)
    };

    for c in countries_order {
        let rows = &by_country[c];
        println!(
            "{:<20} {:>5} {:>10} {:>10} {:>10}",
            c,
            rows.iter().filter(|r| r.err_loyo.is_some()).count(),
            fmt_opt(mean_slice(rows, |r| r.err_loyo), 4),
            fmt_opt(mean_slice(rows, |r| r.err_b0), 4),
            fmt_opt(mean_slice(rows, |r| r.err_b1), 4),
        );
    }

    let all_refs: Vec<&LoyoYearRow> = loyo_rows.iter().collect();
    println!("{}", "-".repeat(60));
    println!(
        "{:<20} {:>5} {:>10} {:>10} {:>10}",
        "ALL_POOLED",
        all_refs.iter().filter(|r| r.err_loyo.is_some()).count(),
        fmt_opt(mean_slice(&all_refs, |r| r.err_loyo), 4),
        fmt_opt(mean_slice(&all_refs, |r| r.err_b0), 4),
        fmt_opt(mean_slice(&all_refs, |r| r.err_b1), 4),
    );
}

fn write_csv(
    path: &PathBuf,
    placebo_rows: &[PlaceboRow],
    pooled_placebo_rows: &[PooledPlaceboRow],
    loyo_rows: &[LoyoYearRow],
    gate_rows: &[GateRow],
) -> Result<()> {
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
        "section,country,seam,metric,before_year,after_year,year,baseline,n,real_log_step,real_abs_log_step,placebo_median,placebo_p90,empirical_p,pct_rank,mean_pct_rank,sign_p,wilcoxon_p,total_obs,modis_observable,pred_model,pred_baseline,err_model,err_baseline,mean_diff,ci90_lo,ci90_hi,verdict"
    )?;

    for r in placebo_rows {
        writeln!(
            w,
            "placebo,{},{},{},{},{},,,{},{},{},{},{},{},{},,,,,,,,,,,,,",
            r.country,
            r.seam,
            r.metric.as_str(),
            r.before_year,
            r.after_year,
            r.n_placebo,
            fmt_opt(r.real_log_step, 6),
            fmt_opt(r.real_abs_log_step, 6),
            fmt_opt(r.placebo_median, 6),
            fmt_opt(r.placebo_p90, 6),
            fmt_opt(r.empirical_p, 6),
            fmt_opt(r.pct_rank, 6),
        )?;
    }

    for r in pooled_placebo_rows {
        writeln!(
            w,
            "placebo_pooled,ALL,{},{},,,,,{},,,,,,,{},{},{},,,,,,,,,,",
            r.seam,
            r.metric.as_str(),
            r.n_countries,
            fmt_opt(r.mean_pct_rank, 6),
            fmt_opt(r.sign_p, 6),
            fmt_opt(r.wilcoxon_p, 6),
        )?;
    }

    for r in loyo_rows {
        for baseline in BaselineKind::ALL {
            let (pred_b, err_b) = match baseline {
                BaselineKind::Unnormalized => (r.pred_b0, r.err_b0),
                BaselineKind::Persistence => (r.pred_b1, r.err_b1),
            };
            writeln!(
                w,
                "loyo,{},,,,,{},{},{},,,,,,,,,,,{},{},{},{},{},{},,,,",
                r.country,
                r.year,
                baseline.as_str(),
                r.skipped_months,
                r.total_obs,
                r.modis_observable,
                fmt_opt(r.pred_loyo, 4),
                fmt_opt(pred_b, 4),
                fmt_opt(r.err_loyo, 6),
                fmt_opt(err_b, 6),
            )?;
        }
    }

    for r in gate_rows {
        writeln!(
            w,
            "gate,{},,,,,,{},{},,,,,,,,,,,,,,{},{},{},{},{},{}",
            r.country,
            r.baseline.as_str(),
            r.n_years,
            fmt_opt(r.mean_err_model, 6),
            fmt_opt(r.mean_err_baseline, 6),
            fmt_opt(r.mean_diff, 6),
            fmt_opt(r.ci90_lo, 6),
            fmt_opt(r.ci90_hi, 6),
            r.verdict.map_or("NA", GateVerdict::as_str),
        )?;
    }

    w.flush()?;
    Ok(())
}

fn gate_json_document(
    countries: &[String],
    rows: &[GateRow],
    bootstrap: usize,
    seed: u64,
) -> Result<serde_json::Value> {
    let mut regions = BTreeMap::new();
    for country in countries {
        let mut baselines = Vec::new();
        let mut decision = GateVerdict::Pass;
        let mut model_error = None;
        for baseline in BaselineKind::ALL {
            let row = rows
                .iter()
                .find(|r| r.country == *country && r.baseline == baseline)
                .with_context(|| format!("missing gate for {country} {}", baseline.as_str()))?;
            let (Some(em), Some(eb), Some(lo), Some(hi), Some(verdict)) = (
                row.mean_err_model,
                row.mean_err_baseline,
                row.ci90_lo,
                row.ci90_hi,
                row.verdict,
            ) else {
                bail!("gate unavailable for {country} {}", baseline.as_str());
            };
            if ![em, eb, lo, hi].iter().all(|v| v.is_finite()) {
                bail!(
                    "non-finite gate statistics for {country} {}",
                    baseline.as_str()
                );
            }
            model_error.get_or_insert(em);
            decision = match (decision, verdict) {
                (GateVerdict::Fail, _) | (_, GateVerdict::Fail) => GateVerdict::Fail,
                (GateVerdict::Uncertain, _) | (_, GateVerdict::Uncertain) => GateVerdict::Uncertain,
                _ => GateVerdict::Pass,
            };
            baselines.push(serde_json::json!({
                "name": baseline.as_str(),
                "baseline_error": eb,
                "ci90": [lo, hi],
                "decision": verdict.as_json_str(),
            }));
        }
        regions.insert(
            country,
            serde_json::json!({
                "model_error": model_error,
                "baselines": baselines,
                "decision": decision.as_json_str(),
            }),
        );
    }
    Ok(serde_json::json!({
        "generated": Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        "method": format!("year-blocked LOYO, bootstrap 90% CI, B={bootstrap}, seed={seed}"),
        "regions": regions,
    }))
}

fn write_gate_json(path: &PathBuf, args: &Args, rows: &[GateRow]) -> Result<()> {
    let document = gate_json_document(&args.countries, rows, args.bootstrap, args.seed)?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &document)?;
    writeln!(writer)?;
    writer.flush()?;
    Ok(())
}

struct CountryValidation {
    placebo_rows: Vec<PlaceboRow>,
    loyo_rows: Vec<LoyoYearRow>,
    gate_rows: Vec<GateRow>,
}

async fn analyze_country(
    archive: &ArchiveClient,
    config: &PipelineConfig,
    country: &str,
    args: &Args,
    country_idx: usize,
) -> Result<CountryValidation> {
    let total_t0 = Instant::now();
    let load_t0 = Instant::now();

    let mut modis = Vec::new();
    let mut viirs = Vec::new();
    let mut raw_by_year: BTreeMap<i32, u64> = BTreeMap::new();
    let mut found_any_file = false;

    for year in args.from..=args.to {
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
                Err(ArchiveError::NotCached { .. })
                    if archive.sensor_year_complete(sensor, year) => {}
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
            "no archive CSV files found for '{country}' ({}..={}) in {}",
            args.from,
            args.to,
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
    let cells = build_calendar_with(&harmonized, &config.uncertainty, &config.calibration);
    let raw_months = aggregate_months(&harmonized);
    let cal_secs = cal_t0.elapsed().as_secs_f64();
    drop(harmonized);

    let years = summarize_years(args.from, args.to, &raw_by_year, &cells);
    let placebo_rows = compute_placebo_rows(country, &years);
    let loyo_rows = compute_loyo_rows(country, args.from, args.to, &raw_months);

    let mut gate_rows = Vec::with_capacity(BaselineKind::ALL.len());
    for (b_idx, baseline) in BaselineKind::ALL.into_iter().enumerate() {
        let stream_id = (country_idx as u64 + 1) * 16 + b_idx as u64;
        gate_rows.push(compute_gate_row(
            country,
            baseline,
            &loyo_rows,
            args.bootstrap,
            args.seed,
            stream_id,
        ));
    }

    println!("\n================================================================================");
    println!(
        "COUNTRY: {country} ({}..={}) | raw_modis={raw_modis} raw_viirs={raw_viirs} harmonized={harm_count}",
        args.from, args.to
    );
    println!(
        "TIMING: load={load_secs:.2}s harmonize={harm_secs:.2}s calendar={cal_secs:.2}s total={:.2}s",
        total_t0.elapsed().as_secs_f64()
    );
    print_placebo_table(&placebo_rows);
    print_loyo_table(&loyo_rows);
    print_gate_table(
        "C. Model vs Baseline Gate (90% year-blocked bootstrap CI)",
        &gate_rows,
    );

    Ok(CountryValidation {
        placebo_rows,
        loyo_rows,
        gate_rows,
    })
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
        "Countries ({}): {} | bootstrap={} seed={}",
        args.countries.len(),
        args.countries.join(", "),
        args.bootstrap,
        args.seed
    );

    let overall_t0 = Instant::now();
    let mut all_placebo = Vec::new();
    let mut all_loyo = Vec::new();
    let mut all_gate = Vec::new();

    for (idx, country) in args.countries.iter().enumerate() {
        let res = analyze_country(&archive, &config, country, &args, idx).await?;
        all_placebo.extend(res.placebo_rows);
        all_loyo.extend(res.loyo_rows);
        all_gate.extend(res.gate_rows);
    }

    let pooled_placebo = compute_pooled_placebo_rows(&all_placebo);
    if args.countries.len() > 1 {
        print_pooled_placebo_summary(&pooled_placebo);
        print_cross_country_loyo_summary(&all_loyo);
        for (b_idx, baseline) in BaselineKind::ALL.into_iter().enumerate() {
            let stream_id = 10_000 + b_idx as u64;
            all_gate.push(compute_gate_row(
                "ALL_POOLED",
                baseline,
                &all_loyo,
                args.bootstrap,
                args.seed,
                stream_id,
            ));
        }
        print_gate_table(
            "POOLED MODEL vs BASELINE GATE (year-blocked 90% CI)",
            &all_gate,
        );
    }

    if let Some(ref out_path) = args.out {
        write_csv(
            out_path,
            &all_placebo,
            &pooled_placebo,
            &all_loyo,
            &all_gate,
        )?;
        println!("\nCSV written: {}", out_path.display());
    }
    if let Some(ref path) = args.gate_json {
        write_gate_json(path, &args, &all_gate)?;
        println!("\nGate JSON written: {}", path.display());
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

    #[test]
    fn injected_seam_step_tops_placebo_distribution_but_control_does_not() {
        let series = |step: f64| {
            (2000..=2025)
                .map(|year| {
                    let phase = if year == 2012 { 2011 } else { year };
                    let log_value =
                        0.1 * (phase as f64).sin() + if year >= 2012 { step } else { 0.0 };
                    let value = 1000.0 * log_value.exp();
                    (
                        year,
                        AnnualSummary {
                            raw: value.round() as u64,
                            count_normalized: Some(value),
                            total_frp: Some(value),
                            f_year: Some(value),
                        },
                    )
                })
                .collect()
        };
        for (step, expected_rank) in [(2.0, 1.0), (0.0, 0.0)] {
            let rows = compute_placebo_rows("Synthetic", &series(step));
            for row in rows.iter().filter(|r| r.seam == "snpp_2012") {
                assert_eq!(row.n_placebo, 19);
                assert_eq!(row.pct_rank, Some(expected_rank));
                assert!((row.real_log_step.unwrap() - step).abs() < 0.002);
                let expected_p = if step > 0.0 { 0.05 } else { 1.0 };
                assert_eq!(row.empirical_p, Some(expected_p));
            }
        }
    }

    fn synthetic_months() -> HashMap<(i32, u32), MonthBucket> {
        let mut months = HashMap::new();
        for year in 2012..=2019 {
            for month in 1..=12 {
                let mut platforms = BTreeSet::from([Sensor::ModisTerra, Sensor::ViirsSNPP]);
                let omega = if year < 2016 {
                    2
                } else {
                    platforms.insert(Sensor::ViirsNOAA20);
                    4
                };
                let modis_observable = (year - 2010) as u64 * 10 + u64::from(month);
                months.insert(
                    (year, month),
                    MonthBucket {
                        platforms,
                        total: modis_observable * omega,
                        modis_observable,
                    },
                );
            }
        }
        months
    }

    #[test]
    fn loyo_recovers_known_omega_for_two_configurations() {
        let rows = compute_loyo_rows("Synthetic", 2012, 2019, &synthetic_months());
        assert_eq!(rows.len(), 8);
        for row in rows {
            let omega: f64 = if row.year < 2016 { 2.0 } else { 4.0 };
            assert_eq!(row.skipped_months, 0);
            assert!((row.pred_loyo.unwrap() - row.modis_observable as f64).abs() < 1e-10);
            assert!((row.total_obs as f64 / row.pred_loyo.unwrap() - omega).abs() < 1e-12);
            assert!(row.err_loyo.unwrap() < 1e-12);
            assert!((row.err_b0.unwrap() - omega.ln()).abs() < 1e-12);
            assert!(row.err_loyo.unwrap() < row.err_b0.unwrap() * 0.01);
        }
    }

    #[test]
    fn loyo_excludes_every_month_of_the_held_out_year() {
        let mut months = synthetic_months();
        for (&(year, _), bucket) in &mut months {
            if year == 2015 {
                bucket.total *= 3;
            }
        }
        let rows = compute_loyo_rows("Synthetic", 2012, 2019, &months);
        let held_out = rows.iter().find(|r| r.year == 2015).unwrap();
        assert!(
            (held_out.pred_loyo.unwrap() - 3.0 * held_out.modis_observable as f64).abs() < 1e-10
        );
        assert!((held_out.err_loyo.unwrap() - 3.0_f64.ln()).abs() < 1e-12);
    }

    fn error_rows(model: f64, baseline: f64) -> Vec<LoyoYearRow> {
        (2012..=2025)
            .map(|year| LoyoYearRow {
                country: "Synthetic".to_string(),
                year,
                total_obs: 100,
                modis_observable: 100,
                skipped_months: 0,
                pred_loyo: None,
                pred_b0: None,
                pred_b1: None,
                err_loyo: Some(model + f64::from(year - 2012) * 0.01),
                err_b0: Some(baseline + f64::from(year - 2012) * 0.02),
                err_b1: Some(baseline + f64::from(year - 2012) * 0.02),
            })
            .collect()
    }

    #[test]
    fn bootstrap_gate_passes_fails_and_is_inconclusive_for_equal_errors() {
        for (model, baseline, expected) in [
            (0.1, 1.0, GateVerdict::Pass),
            (1.0, 0.1, GateVerdict::Fail),
            (0.5, 0.5, GateVerdict::Uncertain),
        ] {
            let mut rows = error_rows(model, baseline);
            if expected == GateVerdict::Uncertain {
                for row in &mut rows {
                    row.err_b0 = row.err_loyo;
                    row.err_b1 = row.err_loyo;
                }
            }
            for kind in BaselineKind::ALL {
                let gate = compute_gate_row("Synthetic", kind, &rows, 1000, 42, 16);
                let repeat = compute_gate_row("Synthetic", kind, &rows, 1000, 42, 16);
                assert_eq!(gate.n_years, 14);
                assert_eq!(gate.verdict, Some(expected));
                assert_eq!(gate.ci90_lo, repeat.ci90_lo);
                assert_eq!(gate.ci90_hi, repeat.ci90_hi);
                match expected {
                    GateVerdict::Pass => assert!(gate.ci90_lo.unwrap() > 0.0),
                    GateVerdict::Fail => assert!(gate.ci90_hi.unwrap() < 0.0),
                    GateVerdict::Uncertain => {
                        assert_eq!(gate.ci90_lo, Some(0.0));
                        assert_eq!(gate.ci90_hi, Some(0.0));
                    }
                }
            }
        }
    }

    #[test]
    fn gate_json_uses_worst_baseline_and_excludes_pooled_rows() {
        let countries = vec!["Synthetic".to_string()];
        let rows = error_rows(0.1, 1.0);
        let mut gates: Vec<_> = BaselineKind::ALL
            .into_iter()
            .map(|baseline| compute_gate_row("Synthetic", baseline, &rows, 1000, 42, 16))
            .collect();
        gates.push(compute_gate_row(
            "ALL_POOLED",
            BaselineKind::Unnormalized,
            &rows,
            1000,
            42,
            17,
        ));
        for (verdict, expected) in [
            (GateVerdict::Pass, "pass"),
            (GateVerdict::Uncertain, "inconclusive"),
            (GateVerdict::Fail, "fail"),
        ] {
            gates[1].verdict = Some(verdict);
            let document = gate_json_document(&countries, &gates, 1000, 42).unwrap();
            assert_eq!(document["regions"].as_object().unwrap().len(), 1);
            let region = &document["regions"]["Synthetic"];
            assert_eq!(region["decision"], expected);
            assert_eq!(region["baselines"][1]["decision"], expected);
            assert_eq!(region["baselines"][0]["name"], "b0_unnormalized");
            assert_eq!(region["baselines"][1]["name"], "b1_persistence");
            assert_eq!(region["model_error"], gates[0].mean_err_model.unwrap());
            assert_eq!(region["baselines"][0]["ci90"][0], gates[0].ci90_lo.unwrap());
            assert_eq!(region["baselines"][0]["ci90"][1], gates[0].ci90_hi.unwrap());
            assert_eq!(
                document["method"],
                "year-blocked LOYO, bootstrap 90% CI, B=1000, seed=42"
            );
            let generated = document["generated"].as_str().unwrap();
            assert!(generated.ends_with('Z'));
            assert!(chrono::DateTime::parse_from_rfc3339(generated).is_ok());
        }
        gates[0].ci90_lo = None;
        assert!(gate_json_document(&countries, &gates, 1000, 42).is_err());
    }
}
