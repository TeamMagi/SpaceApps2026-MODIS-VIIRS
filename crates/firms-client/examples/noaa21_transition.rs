//! Task K6: measure the 2023 NOAA-21 effort transition on the FIRMS area API (SP sources),
//! replicating the G3 2018 NOAA-20 check one transition later.
//!
//! "Raw" is the monthly harmonized detection count (`CalendarCell::count`, before Omega), the same
//! quantity `fleet_trend` calls raw_total; "normalized" is `count_normalized` (count / Omega).
//! The shift is the same-season ratio of totals, Jul-Dec `--after-year` over Jul-Dec `--before-year`.
//!
//! Live run (spends FIRMS transactions; reruns are served from the area cache at 0 cost):
//!   set -a; . ./.env; set +a
//!   cargo run --release -p pyrosync-firms --example noaa21_transition
//! Offline estimate, no network and no key needed:
//!   cargo run --release -p pyrosync-firms --example noaa21_transition -- --dry-run
//!
//! Options: --bbox w,s,e,n  --from YYYY-MM-DD  --to YYYY-MM-DD  --before-year Y  --after-year Y
//!          --label TEXT  --dry-run  --force
//! The area cache lives in $AREA_CACHE_DIR (default `.cache/firms-area`, relative to the cwd).

use anyhow::{Context, Result, bail};
use chrono::{Datelike, NaiveDate};
use pyrosync_core::calendar::build_calendar;
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::effort::estimate_effort;
use pyrosync_core::harmonize::harmonize_with_config;
use pyrosync_core::models::{BBox, CalendarCell, HarmonizedHotspot, Sensor};
use pyrosync_firms::day_range::{DayRange, date_windows, transaction_cost};
use pyrosync_firms::parse::retain_wildfires;
use pyrosync_firms::source::{FirmsSource, Processing};
use pyrosync_firms::{AreaCache, FirmsClient};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_CACHE_DIR: &str = ".cache/firms-area";
const TRANSACTION_BUDGET: u32 = 4500;
const SEASON_MONTHS: std::ops::RangeInclusive<u32> = 7..=12;

type Window = (FirmsSource, NaiveDate, DayRange);

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).expect("valid constant date")
}

fn noaa21_first_day() -> NaiveDate {
    d(2023, 3, 1)
}

#[derive(Debug, Clone, PartialEq)]
struct Args {
    bbox: [f64; 4],
    from: NaiveDate,
    to: NaiveDate,
    before_year: i32,
    after_year: i32,
    label: Option<String>,
    dry_run: bool,
    force: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            bbox: [26.0, 36.0, 45.0, 42.0],
            from: d(2022, 7, 1),
            to: d(2023, 12, 31),
            before_year: 2022,
            after_year: 2023,
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
            "--before-year" => args.before_year = value("--before-year")?.parse()?,
            "--after-year" => args.after_year = value("--after-year")?.parse()?,
            "--label" => args.label = Some(value("--label")?),
            "--dry-run" => args.dry_run = true,
            "--force" => args.force = true,
            other => bail!("unknown argument '{other}'"),
        }
    }
    if args.from > args.to {
        bail!("--from must not be after --to");
    }
    Ok(args)
}

fn sources() -> [FirmsSource; 4] {
    [
        FirmsSource::new(Sensor::ModisTerra, Processing::Sp),
        FirmsSource::new(Sensor::ViirsSNPP, Processing::Sp),
        FirmsSource::new(Sensor::ViirsNOAA20, Processing::Sp),
        FirmsSource::new(Sensor::ViirsNOAA21, Processing::Sp),
    ]
}

/// First day a source may be requested from; NOAA-21 SP starts 2023-03-01, requesting earlier
/// would only burn transactions on empty windows.
fn source_start(source: FirmsSource, from: NaiveDate) -> NaiveDate {
    if source.sensor == Sensor::ViirsNOAA21 {
        from.max(noaa21_first_day())
    } else {
        from
    }
}

fn plan_windows(from: NaiveDate, to: NaiveDate) -> Vec<Window> {
    let mut out = Vec::new();
    for source in sources() {
        let start = source_start(source, from);
        for (win_start, days) in date_windows(start, to) {
            out.push((source, win_start, days));
        }
    }
    out
}

/// `ln(after / before)` is undefined for non-positive totals.
fn shift_ratio(before: f64, after: f64) -> Option<f64> {
    (before.is_finite() && after.is_finite() && before > 0.0 && after > 0.0).then(|| after / before)
}

fn abs_log(ratio: f64) -> f64 {
    ratio.ln().abs()
}

/// K6 "Bitti sayilir": normalization must remove more than half of the raw log shift.
fn passes_criterion(raw_ratio: f64, norm_ratio: f64) -> bool {
    abs_log(norm_ratio) < abs_log(raw_ratio) / 2.0
}

fn season_total<F: Fn(&CalendarCell) -> Option<f64>>(
    cells: &[CalendarCell],
    year: i32,
    value: F,
) -> Option<f64> {
    let mut total = 0.0;
    for month in SEASON_MONTHS {
        let cell = cells.iter().find(|c| c.year == year && c.month == month)?;
        total += value(cell)?;
    }
    Some(total)
}

type MonthCounts = BTreeMap<(i32, u32), u64>;

/// A season month with no rows counts as zero detections.
fn season_count(counts: &MonthCounts, year: i32) -> f64 {
    SEASON_MONTHS
        .map(|m| counts.get(&(year, m)).copied().unwrap_or(0) as f64)
        .sum()
}

/// Shift relative to the MODIS reference channel (whose effort did not change).
fn excess(shift: Option<f64>, reference: Option<f64>) -> Option<f64> {
    shift_ratio(1.0, shift?).zip(reference).and_then(|(s, r)| shift_ratio(r, s))
}

fn passes_reference_criterion(raw: f64, norm: f64, reference: f64) -> bool {
    passes_criterion(raw / reference, norm / reference)
}

fn platform_label(platforms: &BTreeSet<Sensor>) -> String {
    let mut parts = Vec::new();
    if platforms.iter().any(|s| s.is_modis()) {
        parts.push("MODIS");
    }
    for s in platforms.iter().filter(|s| s.is_viirs()) {
        parts.push(match s {
            Sensor::ViirsSNPP => "SNPP",
            Sensor::ViirsNOAA20 => "N20",
            Sensor::ViirsNOAA21 => "N21",
            _ => "VIIRS",
        });
    }
    parts.join("+")
}

fn month_platforms(hotspots: &[HarmonizedHotspot]) -> BTreeMap<(i32, u32), BTreeSet<Sensor>> {
    let mut map: BTreeMap<(i32, u32), BTreeSet<Sensor>> = BTreeMap::new();
    for h in hotspots {
        let date = h.acquired.date_naive();
        map.entry((date.year(), date.month()))
            .or_default()
            .extend(h.platforms.iter().copied());
    }
    map
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
    println!("Source              windows  cached  transactions(uncached)");
    let mut uncached_total = 0;
    for source in sources() {
        let idx: Vec<usize> = (0..windows.len()).filter(|&i| windows[i].0 == source).collect();
        let hit = idx.iter().filter(|i| cached.contains(i)).count();
        let cost: u32 = idx
            .iter()
            .filter(|i| !cached.contains(i))
            .map(|&i| windows[i].2.as_u32())
            .sum();
        uncached_total += cost;
        println!("{:<19} {:>7} {:>7} {:>10}", source.as_str(), idx.len(), hit, cost);
    }
    let full = transaction_cost(&windows.iter().map(|&(_, s, d)| (s, d)).collect::<Vec<_>>());
    println!(
        "Total: {} windows, {} cached, estimated transactions {} (uncached; {} without cache)",
        windows.len(),
        cached.len(),
        uncached_total,
        full
    );
    uncached_total
}

fn fmt_ratio(r: Option<f64>) -> String {
    r.map_or("n/a".to_string(), |v| format!("{v:.3}"))
}

fn fmt_log(r: Option<f64>) -> String {
    r.map_or("n/a".to_string(), |v| format!("{:.3}", abs_log(v)))
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

    let label = args.label.clone().unwrap_or_else(|| "NOAA-21 2023".to_string());
    println!(
        "K6 {label}: bbox {bbox_str}, {}..{}, same-season Jul-Dec {} vs {}",
        args.from, args.to, args.before_year, args.after_year
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

    let (mut modis, mut viirs, report) = client.fetch_source_windows(&bbox, &windows).await;

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
        eprintln!("FAILED windows ({}): result withheld, a failed source corrupts Omega", report.failures.len());
        for (source, err) in &report.failures {
            eprintln!("  {source}: {err}");
        }
        std::process::exit(2);
    }

    retain_wildfires(&mut modis);
    retain_wildfires(&mut viirs);
    let input_rows = modis.len() + viirs.len();
    let mut modis_input: MonthCounts = BTreeMap::new();
    for h in &modis {
        let date = h.acquired.date_naive();
        *modis_input.entry((date.year(), date.month())).or_default() += 1;
    }
    let mut modis_observable: MonthCounts = BTreeMap::new();
    let harmonized = harmonize_with_config(modis, viirs, &PipelineConfig::default());
    let factors = estimate_effort(&harmonized);
    let cells = build_calendar(&harmonized);
    let platforms = month_platforms(&harmonized);
    for h in harmonized.iter().filter(|h| h.platforms.iter().any(|s| s.is_modis())) {
        let date = h.acquired.date_naive();
        *modis_observable.entry((date.year(), date.month())).or_default() += 1;
    }
    println!("input rows after wildfire filter: {input_rows}, harmonized detections: {}", harmonized.len());

    println!("\nmonth    raw   harmonized  modis_obs modis_in  config               omega   count_normalized");
    for cell in &cells {
        let cfg = platforms
            .get(&(cell.year, cell.month))
            .map(platform_label)
            .unwrap_or_default();
        println!(
            "{}-{:02} {:>6} {:>11} {:>10} {:>8}  {:<19} {:>7} {:>18}",
            cell.year,
            cell.month,
            cell.count,
            cell.count,
            modis_observable.get(&(cell.year, cell.month)).copied().unwrap_or(0),
            modis_input.get(&(cell.year, cell.month)).copied().unwrap_or(0),
            cfg,
            cell.effort_omega.map_or("n/a".into(), |o| format!("{o:.3}")),
            cell.count_normalized.map_or("n/a".into(), |n| format!("{n:.1}")),
        );
    }
    println!("(raw = harmonized = CalendarCell::count; modis_obs = harmonized records with a MODIS platform; modis_in = MODIS_SP input rows after the wildfire filter)");

    println!("\nOmega per configuration:");
    for f in &factors {
        println!(
            "  {:<19} months={:>2} total={:>7} omega={}",
            platform_label(&f.platforms.iter().copied().collect()),
            f.months,
            f.total,
            f.omega.map_or("n/a".into(), |o| format!("{o:.3}")),
        );
    }
    let n21_start = platforms
        .iter()
        .find(|(_, set)| set.contains(&Sensor::ViirsNOAA21))
        .map(|((y, m), _)| format!("{y}-{m:02}"));
    println!("NOAA-21 configuration starts: {}", n21_start.as_deref().unwrap_or("not observed"));

    let raw_before = season_total(&cells, args.before_year, |c| Some(c.count as f64));
    let raw_after = season_total(&cells, args.after_year, |c| Some(c.count as f64));
    let norm_before = season_total(&cells, args.before_year, |c| c.count_normalized);
    let norm_after = season_total(&cells, args.after_year, |c| c.count_normalized);
    let raw_shift = raw_before.zip(raw_after).and_then(|(b, a)| shift_ratio(b, a));
    let norm_shift = norm_before.zip(norm_after).and_then(|(b, a)| shift_ratio(b, a));

    println!("\nSame-season Jul-Dec {} vs {}:", args.before_year, args.after_year);
    println!("  raw total  {} -> {}", fmt_ratio(raw_before), fmt_ratio(raw_after));
    println!("  norm total {} -> {}", fmt_ratio(norm_before), fmt_ratio(norm_after));
    println!("  raw shift x{} (|log| {})", fmt_ratio(raw_shift), fmt_log(raw_shift));
    println!("  normalized shift x{} (|log| {})", fmt_ratio(norm_shift), fmt_log(norm_shift));
    let verdict = match raw_shift.zip(norm_shift) {
        Some((r, n)) => {
            if passes_criterion(r, n) { "PASS" } else { "FAIL" }
        }
        None => "UNAVAILABLE (a season month is missing or has no Omega)",
    };
    println!("  task's literal criterion |log(norm)| < |log(raw)|/2: {verdict}");

    let ref_obs_before = season_count(&modis_observable, args.before_year);
    let ref_obs_after = season_count(&modis_observable, args.after_year);
    let ref_in_before = season_count(&modis_input, args.before_year);
    let ref_in_after = season_count(&modis_input, args.after_year);
    let ref_obs = shift_ratio(ref_obs_before, ref_obs_after);
    let ref_in = shift_ratio(ref_in_before, ref_in_after);
    println!("
MODIS reference (effort unchanged across the transition):");
    println!("  modis_observable {ref_obs_before:.0} -> {ref_obs_after:.0}: shift x{} (|log| {})", fmt_ratio(ref_obs), fmt_log(ref_obs));
    println!("  MODIS input rows {ref_in_before:.0} -> {ref_in_after:.0}: shift x{} (|log| {})", fmt_ratio(ref_in), fmt_log(ref_in));

    let raw_excess = excess(raw_shift, ref_obs);
    let norm_excess = excess(norm_shift, ref_obs);
    println!("Excess over the modis_observable reference:");
    println!("  raw/ref x{} (|log| {})", fmt_ratio(raw_excess), fmt_log(raw_excess));
    println!("  normalized/ref x{} (|log| {})", fmt_ratio(norm_excess), fmt_log(norm_excess));
    let ref_verdict = match (raw_shift, norm_shift, ref_obs) {
        (Some(r), Some(n), Some(f)) => {
            if passes_reference_criterion(r, n, f) { "PASS" } else { "FAIL" }
        }
        _ => "UNAVAILABLE",
    };
    println!("  reference criterion |log(norm/ref)| < |log(raw/ref)|/2: {ref_verdict}");
    let omega_step = raw_shift.zip(norm_shift).and_then(|(r, n)| shift_ratio(n, r));
    println!(
        "  artificial step implied by the fleet change (season Omega after/before = raw shift / normalized shift): x{}; observed raw/ref excess: x{}",
        fmt_ratio(omega_step),
        fmt_ratio(raw_excess)
    );

    println!("
Markdown (GOREVLER.md G3 table format):");
    println!("| | Ham | Normalize | MODIS referansı |");
    println!("|---|-----|-----------|-----------------|");
    println!(
        "| Kayma | ×{} | **×{}** | ×{} |",
        md_decimal(fmt_ratio(raw_shift)),
        md_decimal(fmt_ratio(norm_shift)),
        md_decimal(fmt_ratio(ref_obs))
    );
    println!(
        r"| \|log(kayma)\| | {} | **{}** | {} |",
        md_decimal(fmt_log(raw_shift)),
        md_decimal(fmt_log(norm_shift)),
        md_decimal(fmt_log(ref_obs))
    );
    println!(
        "| Referansa göre fazlalık | ×{} | **×{}** | |",
        md_decimal(fmt_ratio(raw_excess)),
        md_decimal(fmt_ratio(norm_excess))
    );
    println!(
        r"| \|log(fazlalık)\| | {} | **{}** | |",
        md_decimal(fmt_log(raw_excess)),
        md_decimal(fmt_log(norm_excess))
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn shift_and_log() {
        assert_eq!(shift_ratio(100.0, 150.0), Some(1.5));
        assert_eq!(shift_ratio(0.0, 5.0), None);
        assert_eq!(shift_ratio(f64::NAN, 5.0), None);
        assert!((abs_log(0.5) - abs_log(2.0)).abs() < 1e-12);
    }

    #[test]
    fn criterion_matches_g3_2018_numbers() {
        assert!(passes_criterion(1.317, 0.912));
        assert!(!passes_criterion(1.317, 1.25));
        assert!(!passes_criterion(1.0, 1.0));
    }

    #[test]
    fn excess_divides_shift_by_reference() {
        assert!((excess(Some(2.46), Some(1.2)).unwrap() - 2.05).abs() < 1e-12);
        assert_eq!(excess(None, Some(1.2)), None);
        assert_eq!(excess(Some(2.0), None), None);
    }

    #[test]
    fn reference_criterion_uses_excess_over_reference() {
        // fails literally (|log 2.0| < |log 2.4|/2 is false) but passes against a 2.0 reference
        assert!(!passes_criterion(2.4, 2.0));
        assert!(passes_reference_criterion(2.4, 2.0, 1.9));
        assert!(!passes_reference_criterion(2.4, 2.4, 1.0));
    }

    #[test]
    fn season_count_sums_jul_dec_and_treats_gaps_as_zero() {
        let mut m = MonthCounts::new();
        m.insert((2022, 6), 99);
        m.insert((2022, 7), 10);
        m.insert((2022, 12), 5);
        m.insert((2023, 7), 7);
        assert_eq!(season_count(&m, 2022), 15.0);
        assert_eq!(season_count(&m, 2023), 7.0);
        assert_eq!(season_count(&m, 2021), 0.0);
    }

    #[test]
    fn noaa21_windows_start_at_first_day() {
        let windows = plan_windows(d(2022, 7, 1), d(2023, 12, 31));
        let n21: Vec<_> = windows
            .iter()
            .filter(|w| w.0.sensor == Sensor::ViirsNOAA21)
            .collect();
        assert_eq!(n21.first().unwrap().1, noaa21_first_day());
        assert!(windows
            .iter()
            .filter(|w| w.0.sensor == Sensor::ViirsNOAA20)
            .any(|w| w.1 == d(2022, 7, 1)));
        // 2023-03-01..2023-12-31 is 306 days
        let days: u32 = n21.iter().map(|w| w.2.as_u32()).sum();
        assert_eq!(days, 306);
    }

    #[test]
    fn noaa21_range_entirely_before_start_yields_no_windows() {
        let windows = plan_windows(d(2022, 7, 1), d(2023, 2, 28));
        assert!(windows.iter().all(|w| w.0.sensor != Sensor::ViirsNOAA21));
    }

    #[test]
    fn parses_defaults_and_overrides() {
        assert_eq!(parse_args(&[]).unwrap(), Args::default());
        let a = parse_args(&strs(&[
            "--bbox", "1,2,3,4", "--from", "2022-09-01", "--to", "2023-09-30", "--label", "x",
            "--dry-run", "--force",
        ]))
        .unwrap();
        assert_eq!(a.bbox, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(a.from, d(2022, 9, 1));
        assert_eq!(a.to, d(2023, 9, 30));
        assert_eq!(a.label.as_deref(), Some("x"));
        assert!(a.dry_run && a.force);
    }

    #[test]
    fn rejects_bad_args() {
        assert!(parse_args(&strs(&["--bbox", "1,2,3"])).is_err());
        assert!(parse_args(&strs(&["--bbox", "5,2,3,4"])).is_err());
        assert!(parse_args(&strs(&["--from", "2024-01-01", "--to", "2023-01-01"])).is_err());
        assert!(parse_args(&strs(&["--nope"])).is_err());
        assert!(parse_args(&strs(&["--from"])).is_err());
    }
}
