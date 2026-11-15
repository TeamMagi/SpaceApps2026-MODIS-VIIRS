//! Task K12: hold-out validation of the adapted Remy & Kaiser (2014) pre-Aqua recovery
//! (`pyrosync_core::pre_aqua`) on real FIRMS MODIS_SP data.
//!
//! Method: harmonize the MODIS_SP window, calibrate the month-of-year factors on every month
//! OUTSIDE the hold-out window, then pretend Aqua did not exist inside the hold-out window (the
//! "Terra-only count" is `terra_visible`, harmonized records containing Terra) and compare
//! `k x terra_visible` with the true `modis_total` (records with any MODIS platform).
//!
//! Live run (spends FIRMS transactions; reruns are served from the area cache at 0 cost):
//!   set -a; . ./.env; set +a
//!   cargo run --release -p pyrosync-firms --example pre_aqua_recovery_check
//! Offline estimate, no network and no key needed:
//!   cargo run --release -p pyrosync-firms --example pre_aqua_recovery_check -- --dry-run
//!
//! Options: --bbox w,s,e,n  --from YYYY-MM-DD  --to YYYY-MM-DD
//!          --holdout-from YYYY-MM  --holdout-to YYYY-MM  --label TEXT  --dry-run  --force
//! The area cache lives in $AREA_CACHE_DIR (default `.cache/firms-area`, relative to the cwd).

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::harmonize::harmonize_with_config;
use pyrosync_core::models::{BBox, Sensor};
use pyrosync_core::pre_aqua::{
    CALIBRATION_END, CALIBRATION_START, MonthTally, RecoveryFactors, factors_from_tallies,
    month_tallies, recover_count,
};
use pyrosync_firms::day_range::{DayRange, date_windows, transaction_cost};
use pyrosync_firms::parse::retain_wildfires;
use pyrosync_firms::source::{FirmsSource, Processing};
use pyrosync_firms::{AreaCache, FirmsClient};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_CACHE_DIR: &str = ".cache/firms-area";
const TRANSACTION_BUDGET: u32 = 4500;

type Ym = (i32, u32);
type Window = (FirmsSource, NaiveDate, DayRange);

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).expect("valid constant date")
}

#[derive(Debug, Clone, PartialEq)]
struct Args {
    bbox: [f64; 4],
    from: NaiveDate,
    to: NaiveDate,
    holdout_from: Ym,
    holdout_to: Ym,
    label: Option<String>,
    dry_run: bool,
    force: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            bbox: [26.0, 36.0, 45.0, 42.0],
            from: d(2003, 1, 1),
            to: d(2008, 12, 31),
            holdout_from: (2003, 1),
            holdout_to: (2004, 8),
            label: None,
            dry_run: false,
            force: false,
        }
    }
}

fn parse_bbox(text: &str) -> Result<[f64; 4]> {
    let parts: Vec<f64> = text
        .split(',')
        .map(|p| p.trim().parse::<f64>().with_context(|| format!("bad bbox number '{p}'")))
        .collect::<Result<_>>()?;
    let [w, s, e, n] = parts[..] else {
        bail!("--bbox needs west,south,east,north");
    };
    if !(w < e && s < n) {
        bail!("--bbox must satisfy west < east and south < north");
    }
    Ok([w, s, e, n])
}

fn parse_ym(text: &str) -> Result<Ym> {
    let (y, m) = text
        .split_once('-')
        .with_context(|| format!("'{text}' must be YYYY-MM"))?;
    let (y, m): (i32, u32) = (y.parse()?, m.parse()?);
    if !(1..=12).contains(&m) {
        bail!("month in '{text}' must be 1..=12");
    }
    Ok((y, m))
}

fn parse_args(raw: &[String]) -> Result<Args> {
    let mut args = Args::default();
    let mut it = raw.iter();
    while let Some(flag) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .cloned()
                .with_context(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--bbox" => args.bbox = parse_bbox(&value("--bbox")?)?,
            "--from" => args.from = value("--from")?.parse().context("--from must be YYYY-MM-DD")?,
            "--to" => args.to = value("--to")?.parse().context("--to must be YYYY-MM-DD")?,
            "--holdout-from" => args.holdout_from = parse_ym(&value("--holdout-from")?)?,
            "--holdout-to" => args.holdout_to = parse_ym(&value("--holdout-to")?)?,
            "--label" => args.label = Some(value("--label")?),
            "--dry-run" => args.dry_run = true,
            "--force" => args.force = true,
            other => bail!("unknown argument '{other}'"),
        }
    }
    validate(&args)?;
    Ok(args)
}

fn ym_of(date: NaiveDate) -> Ym {
    use chrono::Datelike;
    (date.year(), date.month())
}

fn validate(args: &Args) -> Result<()> {
    if args.from > args.to {
        bail!("--from must not be after --to");
    }
    if args.holdout_from > args.holdout_to {
        bail!("--holdout-from must not be after --holdout-to");
    }
    if args.holdout_from < ym_of(args.from) || args.holdout_to > ym_of(args.to) {
        bail!("the hold-out window must lie inside --from..--to");
    }
    if args.holdout_from < CALIBRATION_START || args.holdout_to > CALIBRATION_END {
        bail!(
            "the hold-out window must lie inside the Terra+Aqua era {}-{:02}..{}-{:02}",
            CALIBRATION_START.0,
            CALIBRATION_START.1,
            CALIBRATION_END.0,
            CALIBRATION_END.1
        );
    }
    Ok(())
}

fn source() -> FirmsSource {
    FirmsSource::new(Sensor::ModisTerra, Processing::Sp)
}

fn plan_windows(from: NaiveDate, to: NaiveDate) -> Vec<Window> {
    date_windows(from, to)
        .into_iter()
        .map(|(start, days)| (source(), start, days))
        .collect()
}

fn in_holdout(ym: Ym, from: Ym, to: Ym) -> bool {
    (from..=to).contains(&ym)
}

/// Tallies of every month outside the hold-out window: the only data the factors may see.
fn calibration_tallies(
    all: &BTreeMap<Ym, MonthTally>,
    holdout_from: Ym,
    holdout_to: Ym,
) -> BTreeMap<Ym, MonthTally> {
    all.iter()
        .filter(|&(&ym, _)| !in_holdout(ym, holdout_from, holdout_to))
        .map(|(&ym, &t)| (ym, t))
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
struct Row {
    ym: Ym,
    terra_visible: u64,
    modis_total: u64,
    k: f64,
    recovered: f64,
    lower: f64,
    upper: f64,
}

impl Row {
    fn in_band(&self) -> bool {
        (self.lower..=self.upper).contains(&(self.modis_total as f64))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Summary {
    months: usize,
    months_without_factor: usize,
    sum_terra: f64,
    sum_total: f64,
    sum_recovered: f64,
    bias_before: Option<f64>,
    bias_after: Option<f64>,
    fraction_removed: Option<f64>,
    covered: usize,
}

fn evaluate(
    all: &BTreeMap<Ym, MonthTally>,
    factors: &RecoveryFactors,
    holdout_from: Ym,
    holdout_to: Ym,
) -> (Vec<Row>, Summary) {
    let mut rows = Vec::new();
    let mut without_factor = 0;
    for (&ym, tally) in all.iter().filter(|&(&ym, _)| in_holdout(ym, holdout_from, holdout_to)) {
        let Some(r) = recover_count(factors, ym.1, tally.terra_visible) else {
            without_factor += 1;
            continue;
        };
        rows.push(Row {
            ym,
            terra_visible: tally.terra_visible,
            modis_total: tally.modis_total,
            k: factors[ym.1 as usize - 1].map_or(f64::NAN, |f| f.k),
            recovered: r.count,
            lower: r.lower,
            upper: r.upper,
        });
    }
    let sum_terra: f64 = rows.iter().map(|r| r.terra_visible as f64).sum();
    let sum_total: f64 = rows.iter().map(|r| r.modis_total as f64).sum();
    let sum_recovered: f64 = rows.iter().map(|r| r.recovered).sum();
    let bias = |estimate: f64| (sum_total > 0.0).then(|| estimate / sum_total - 1.0);
    let bias_before = bias(sum_terra);
    let bias_after = bias(sum_recovered);
    let fraction_removed = bias_before
        .zip(bias_after)
        .and_then(|(b, a)| (b != 0.0).then(|| 1.0 - a.abs() / b.abs()));
    let summary = Summary {
        months: rows.len(),
        months_without_factor: without_factor,
        sum_terra,
        sum_total,
        sum_recovered,
        bias_before,
        bias_after,
        fraction_removed,
        covered: rows.iter().filter(|r| r.in_band()).count(),
    };
    (rows, summary)
}

fn cache_dir() -> PathBuf {
    match std::env::var_os("AREA_CACHE_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from(DEFAULT_CACHE_DIR),
    }
}

/// Loads FIRMS_API_KEY from the repo-root `.env` when the environment does not already have it.
fn load_dotenv() {
    if std::env::var_os("FIRMS_API_KEY").is_some() {
        return;
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.env");
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        if let Some(value) = line.trim().strip_prefix("FIRMS_API_KEY=") {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                // SAFETY: called at the start of main before any other thread exists.
                unsafe { std::env::set_var("FIRMS_API_KEY", value) };
            }
        }
    }
}

async fn count_cached(cache: &AreaCache, bbox_str: &str, windows: &[Window]) -> BTreeSet<usize> {
    let mut cached = BTreeSet::new();
    for (i, &(source, start, days)) in windows.iter().enumerate() {
        if cache.load(source, bbox_str, start, days).await.is_some() {
            cached.insert(i);
        }
    }
    cached
}

fn print_plan(windows: &[Window], cached: &BTreeSet<usize>) -> u32 {
    let cost: u32 = windows
        .iter()
        .enumerate()
        .filter(|(i, _)| !cached.contains(i))
        .map(|(_, w)| w.2.as_u32())
        .sum();
    let full = transaction_cost(&windows.iter().map(|&(_, s, d)| (s, d)).collect::<Vec<_>>());
    println!(
        "{}: {} windows, {} cached, estimated transactions {} (uncached; {} without cache)",
        source().as_str(),
        windows.len(),
        cached.len(),
        cost,
        full
    );
    cost
}

fn pct(v: Option<f64>) -> String {
    v.map_or("n/a".to_string(), |v| format!("{:+.1}%", v * 100.0))
}

fn pct_plain(v: Option<f64>) -> String {
    v.map_or("n/a".to_string(), |v| format!("{:.0}%", v * 100.0))
}

fn md_decimal(text: String) -> String {
    text.replace('.', ",")
}

#[tokio::main]
async fn main() -> Result<()> {
    load_dotenv();
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw_args)?;
    let [west, south, east, north] = args.bbox;
    let bbox = BBox { west, south, east, north };
    let bbox_str = bbox.to_firms_string();
    let (hf, ht) = (args.holdout_from, args.holdout_to);

    let label = args.label.clone().unwrap_or_else(|| "pre-Aqua recovery hold-out".to_string());
    println!(
        "K12 {label}: bbox {bbox_str}, {}..{}, hold-out {}-{:02}..{}-{:02}",
        args.from, args.to, hf.0, hf.1, ht.0, ht.1
    );

    let windows = plan_windows(args.from, args.to);
    let dir = cache_dir();
    let cache = AreaCache::new(dir.clone(), Duration::ZERO);
    let cached = count_cached(&cache, &bbox_str, &windows).await;
    println!("Area cache: {}", dir.display());
    let uncached_cost = print_plan(&windows, &cached);

    if args.dry_run {
        println!("--dry-run: no network requests made.");
        return Ok(());
    }
    if uncached_cost > TRANSACTION_BUDGET && !args.force {
        bail!(
            "estimate {uncached_cost} transactions exceeds the {TRANSACTION_BUDGET} guard \
             (FIRMS allows 5000 per 10 minutes); rerun with --force to override"
        );
    }

    let key = std::env::var("FIRMS_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .context("FIRMS_API_KEY is not set (run: set -a; . ./.env; set +a)")?;
    let client = FirmsClient::new(key)?.with_area_cache(AreaCache::new(dir, Duration::ZERO));

    let (mut modis, viirs, report) = client.fetch_source_windows(&bbox, &windows).await;
    for (source, n) in &report.per_source {
        println!("fetched {source}: {n} rows");
    }
    println!(
        "transactions_spent={}, cached windows={}/{}",
        report.transactions_spent,
        cached.len(),
        windows.len()
    );
    if !report.failures.is_empty() {
        eprintln!(
            "FAILED windows ({}): result withheld, a missing window biases the ratios",
            report.failures.len()
        );
        for (source, err) in &report.failures {
            eprintln!("  {source}: {err}");
        }
        std::process::exit(2);
    }

    retain_wildfires(&mut modis);
    let input_rows = modis.len();
    let harmonized = harmonize_with_config(modis, viirs, &PipelineConfig::default());
    println!("input rows after wildfire filter: {input_rows}, harmonized detections: {}", harmonized.len());

    let all = month_tallies(&harmonized);
    let calibration = calibration_tallies(&all, hf, ht);
    let factors = factors_from_tallies(&calibration);
    let (rows, s) = evaluate(&all, &factors, hf, ht);

    println!("\nFactors (calibrated outside the hold-out; month: k [p10..p90], pool, months kept, Terra N):");
    for (i, f) in factors.iter().enumerate() {
        match f {
            Some(f) => println!(
                "  {:>2}: k={:.3} [{:.3}..{:.3}] pool={} months={} N={}",
                i + 1,
                f.k,
                f.lower,
                f.upper,
                f.pool.label(),
                f.months_used,
                f.terra_used
            ),
            None => println!("  {:>2}: no factor (pooled Terra-visible < 400)", i + 1),
        }
    }

    println!("\nmonth    terra_visible  modis_total      k  recovered  [lower .. upper]  in_band");
    for r in &rows {
        println!(
            "{}-{:02} {:>14} {:>12} {:>6.3} {:>10.1}  [{:.1} .. {:.1}]  {}",
            r.ym.0,
            r.ym.1,
            r.terra_visible,
            r.modis_total,
            r.k,
            r.recovered,
            r.lower,
            r.upper,
            if r.in_band() { "yes" } else { "no" }
        );
    }
    if s.months_without_factor > 0 {
        println!("({} hold-out months skipped: no factor available)", s.months_without_factor);
    }

    println!("\nHold-out totals over {} months:", s.months);
    println!("  sum terra_visible (no recovery) {:.0}", s.sum_terra);
    println!("  sum recovered                   {:.1}", s.sum_recovered);
    println!("  sum modis_total (truth)         {:.0}", s.sum_total);
    println!("  bias before recovery: {}", pct(s.bias_before));
    println!("  bias after recovery:  {}", pct(s.bias_after));
    println!("  fraction of bias removed: {}", pct_plain(s.fraction_removed));
    println!(
        "  band coverage (truth inside [k_p10, k_p90] x terra_visible): {}/{} months",
        s.covered, s.months
    );
    println!(
        "Caveat: 'Aqua absent' is simulated through `platforms`. A merged Terra+Aqua record contains \
         Terra and counts once as terra_visible; fleet consolidation never puts two detections of \
         the same platform in one group (fleet.rs), so terra_visible equals the number of Terra \
         detections that survive dedup, i.e. the true Terra-only count, and fleet merging does not \
         understate it. Residual differences: the static mask and interscan dedup run on the joint \
         series, and the hold-out shares the instrument and climate with the calibration years \
         (no independent pre-Aqua truth exists)."
    );

    println!("\nMarkdown (GOREVLER.md K12 table format):");
    println!("| Metrik | Değer |");
    println!("|--------|-------|");
    println!("| Hold-out ayı | {} ({}-{:02}..{}-{:02}) |", s.months, hf.0, hf.1, ht.0, ht.1);
    println!("| Σ terra_visible (düzeltmesiz) | {:.0} |", s.sum_terra);
    println!("| Σ recovered | {} |", md_decimal(format!("{:.1}", s.sum_recovered)));
    println!("| Σ modis_total (gerçek) | {:.0} |", s.sum_total);
    println!("| Sapma, düzeltmeden önce | {} |", md_decimal(pct(s.bias_before)));
    println!("| Sapma, düzeltmeden sonra | **{}** |", md_decimal(pct(s.bias_after)));
    println!("| Giderilen sapma payı | **{}** |", pct_plain(s.fraction_removed));
    println!("| Bant kapsaması | {}/{} ay |", s.covered, s.months);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn tally(terra: u64, total: u64) -> MonthTally {
        MonthTally { terra_visible: terra, modis_total: total }
    }

    #[test]
    fn defaults_are_a_valid_twenty_month_holdout() {
        let a = parse_args(&[]).unwrap();
        assert_eq!(a.holdout_from, (2003, 1));
        assert_eq!(a.holdout_to, (2004, 8));
        let months = (a.holdout_from.0 * 12 + a.holdout_from.1 as i32)
            ..=(a.holdout_to.0 * 12 + a.holdout_to.1 as i32);
        assert_eq!(months.count(), 20);
    }

    #[test]
    fn parses_flags() {
        let a = parse_args(&strs(&[
            "--bbox", "1,2,3,4", "--from", "2004-01-01", "--to", "2009-12-31",
            "--holdout-from", "2005-03", "--holdout-to", "2005-10", "--dry-run", "--force",
        ]))
        .unwrap();
        assert_eq!(a.bbox, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(a.holdout_from, (2005, 3));
        assert_eq!(a.holdout_to, (2005, 10));
        assert!(a.dry_run && a.force);
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(parse_args(&strs(&["--nope"])).is_err());
        assert!(parse_args(&strs(&["--bbox", "5,0,1,1"])).is_err());
        assert!(parse_args(&strs(&["--holdout-from", "2003-13"])).is_err());
        assert!(parse_args(&strs(&["--holdout-from", "2005-01", "--holdout-to", "2004-01"])).is_err());
        // hold-out outside the fetched range
        assert!(parse_args(&strs(&["--holdout-from", "2002-01", "--holdout-to", "2002-12"])).is_err());
        assert!(parse_args(&strs(&["--holdout-to", "2009-01"])).is_err());
    }

    #[test]
    fn plan_covers_the_range_with_modis_sp_only() {
        let w = plan_windows(d(2003, 1, 1), d(2003, 1, 12));
        assert_eq!(w.len(), 3);
        assert!(w.iter().all(|&(s, _, _)| s == source()));
        assert_eq!(w.iter().map(|w| w.2.as_u32()).sum::<u32>(), 12);
    }

    #[test]
    fn calibration_excludes_exactly_the_holdout_months() {
        let all: BTreeMap<Ym, MonthTally> =
            [(2003, 1), (2003, 2), (2003, 3), (2004, 1)].iter().map(|&ym| (ym, tally(1, 2))).collect();
        let cal = calibration_tallies(&all, (2003, 2), (2003, 3));
        assert_eq!(cal.keys().copied().collect::<Vec<_>>(), vec![(2003, 1), (2004, 1)]);
    }

    #[test]
    fn evaluate_removes_a_constant_ratio_bias() {
        let mut all: BTreeMap<Ym, MonthTally> = BTreeMap::new();
        for y in 2003..=2008 {
            for m in 1..=12 {
                all.insert((y, m), tally(100, 150));
            }
        }
        let cal = calibration_tallies(&all, (2003, 1), (2003, 6));
        let factors = factors_from_tallies(&cal);
        let (rows, s) = evaluate(&all, &factors, (2003, 1), (2003, 6));
        assert_eq!(rows.len(), 6);
        assert!((s.bias_before.unwrap() - (-1.0 / 3.0)).abs() < 1e-12);
        assert!(s.bias_after.unwrap().abs() < 1e-12);
        assert!((s.fraction_removed.unwrap() - 1.0).abs() < 1e-12);
        assert_eq!(s.covered, 6);
    }

    #[test]
    fn evaluate_counts_months_without_factor_and_misses_outside_band() {
        let mut all: BTreeMap<Ym, MonthTally> = BTreeMap::new();
        for y in 2003..=2008 {
            for m in 1..=12 {
                all.insert((y, m), tally(100, 150));
            }
        }
        all.insert((2003, 1), tally(100, 400));
        let cal = calibration_tallies(&all, (2003, 1), (2003, 1));
        let factors = factors_from_tallies(&cal);
        let (rows, s) = evaluate(&all, &factors, (2003, 1), (2003, 1));
        assert_eq!(rows.len(), 1);
        assert_eq!(s.covered, 0);
        assert!(s.bias_after.unwrap() < 0.0);

        let (rows, s) = evaluate(&all, &[None; 12], (2003, 1), (2003, 1));
        assert!(rows.is_empty());
        assert_eq!(s.months_without_factor, 1);
        assert_eq!(s.bias_before, None);
        assert_eq!(s.fraction_removed, None);
    }

    #[test]
    fn formatting_helpers() {
        assert_eq!(pct(Some(-0.123)), "-12.3%");
        assert_eq!(pct(None), "n/a");
        assert_eq!(pct_plain(Some(0.874)), "87%");
        assert_eq!(md_decimal("1.5".into()), "1,5");
    }
}
