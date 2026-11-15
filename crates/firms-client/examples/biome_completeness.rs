//! Task K14: biome-resolved completeness factor `k_c` and power-law parameters, measured on the
//! FIRMS area API (SP sources) with the same method as `calibration_grid_check.rs`.
//!
//! Method (per region and year): Aqua MODIS rows of `MODIS_SP` against S-NPP 375 m VIIRS
//! (`VIIRS_SNPP_SP`), interscan-deduplicated, paired in 1x1 degree UTC cell-days (separately for
//! day and night) when the mean acquisition times differ by at most `--max-dt-min` (20). Per
//! cell-day: numerator = sum of VIIRS FRP, denominator = sum of MODIS FRP x R_rel(theta) (the
//! pipeline's `vza_factor`), so `k = sum(num) / sum(den)` is directly the factor applied to
//! VZA-corrected MODIS totals. Each cell-day gets the `dominant_biome` of its detections.
//!
//! Live run (spends FIRMS transactions; reruns are served from the area cache at 0 cost and only
//! the windows still missing are requested):
//!   set -a; . ./.env; set +a
//!   cargo run --release -p pyrosync-firms --example biome_completeness -- --report docs/BIYOM_TAMLIK.md
//! Offline estimate, no network and no key needed (a lower bound: real cost was observed to be higher):
//!   cargo run --release -p pyrosync-firms --example biome_completeness -- --dry-run
//!
//! Options: --regions a,b  --list-regions  --fit-year Y  --holdout-year Y  --min-n N  --xmin MW
//!          --max-dt-min M  --dry-run  --report PATH  --emit-rust  --emit-toml
//!          --batch-size N (40)  --start-max N (3500)  --retries N (3)  --max-wait-min M (15)
//! The area cache lives in $AREA_CACHE_DIR (default `.cache/firms-area`, relative to the cwd).
//!
//! Pacing: FIRMS charges more than one transaction per day of a window for these boxes, so the
//! tool does not trust an estimate. Before every batch of at most `--batch-size` uncached windows
//! it reads FIRMS's own counter (`mapkey_status`, which costs nothing) and waits until
//! `current_transactions <= --start-max` and the batch (at the observed cost per window) fits under
//! the limit. After each batch it reads the counter again to learn the real cost per window. A
//! window refused with "Exceeding allowed transaction limit" is not a data failure: it is retried
//! after the counter recovers (at most `--retries` times, waiting at most `--max-wait-min` per
//! round) and only then recorded as failed.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{Datelike, NaiveDate, Timelike};
use pyrosync_core::biome::{Biome, biome_at, dominant_biome};
use pyrosync_core::calibration::calibration_for;
use pyrosync_core::config::CalibrationConfig;
use pyrosync_core::dedup::remove_interscan_duplicates;
use pyrosync_core::models::{BBox, DayNight, Hotspot, Sensor};
use pyrosync_core::vza::vza_factor;
use pyrosync_firms::day_range::{DayRange, date_windows};
use pyrosync_firms::parse::retain_wildfires;
use pyrosync_firms::source::{FirmsSource, Processing};
use pyrosync_firms::{AreaCache, FirmsClient};

const DEFAULT_CACHE_DIR: &str = ".cache/firms-area";
const STATUS_URL: &str = "https://firms.modaps.eosdis.nasa.gov/mapserver/mapkey_status/";
const DEFAULT_TRANSACTION_LIMIT: u32 = 5000;
const POLL_INTERVAL: Duration = Duration::from_secs(30);
const UNKNOWN_STATUS_WAIT: Duration = Duration::from_secs(120);
/// Longest wait for headroom before a first attempt; the limit-error retry still protects after it.
const MAX_PRE_BATCH_WAIT: Duration = Duration::from_secs(20 * 60);
/// Cost per window assumed until one is observed; the 1-per-day estimate was seen to be far too low.
const PRIOR_COST_PER_WINDOW: f64 = 10.0;
const LIMIT_ERROR_TEXT: &str = "exceeding allowed transaction limit";
/// MODIS day nadir detection limit, `D_p = 4.44 * 1.0 km^2 + 0.52` (Sofiev & Kouznetsov Eq. 5).
const DEFAULT_XMIN_MW: f64 = 4.96;
const MIN_TAIL_N: usize = 50;
const LITERATURE_ALPHA: f64 = 1.31;

type Window = (FirmsSource, NaiveDate, DayRange);

#[derive(Debug, Clone, Copy, PartialEq)]
struct Region {
    name: &'static str,
    label: &'static str,
    /// west, south, east, north
    bbox: [f64; 4],
}

/// Fixed before any result was seen; boxes stay within about 20 x 10 degrees.
const REGIONS: [Region; 12] = [
    Region { name: "cerrado", label: "Brezilya Cerrado", bbox: [-55.0, -17.0, -43.0, -7.0] },
    Region { name: "s_africa_savanna", label: "Guney Afrika savani (Angola/Zambiya)", bbox: [14.0, -17.0, 30.0, -8.0] },
    Region { name: "n_africa_savanna", label: "Kuzey Afrika savani (Sahel/OAC)", bbox: [10.0, 5.0, 30.0, 13.0] },
    Region { name: "n_australia", label: "Kuzey Avustralya", bbox: [126.0, -18.0, 142.0, -11.0] },
    Region { name: "se_asia", label: "Anakara Guneydogu Asya", bbox: [97.0, 14.0, 107.0, 22.0] },
    Region { name: "punjab", label: "Hint-Gang ovasi (Pencap)", bbox: [73.0, 28.5, 77.5, 32.5] },
    Region { name: "ukraine_russia", label: "Ukrayna-Guney Rusya tarimi", bbox: [30.0, 46.0, 45.0, 52.0] },
    Region { name: "kazakh_steppe", label: "Kazak bozkiri", bbox: [60.0, 47.0, 78.0, 53.0] },
    Region { name: "siberia_boreal", label: "Orta Sibirya boreal orman", bbox: [90.0, 55.0, 110.0, 62.0] },
    Region { name: "turkiye_e_med", label: "Turkiye / Dogu Akdeniz", bbox: [26.0, 36.0, 45.0, 42.0] },
    Region { name: "western_usa", label: "Bati ABD", bbox: [-124.0, 34.0, -110.0, 44.0] },
    Region { name: "amazon_arc", label: "Amazon ormansizlasma yayi", bbox: [-62.0, -12.0, -48.0, -5.0] },
];

#[derive(Debug, Clone, PartialEq)]
struct Args {
    regions: Vec<&'static str>,
    list_regions: bool,
    fit_year: i32,
    holdout_year: i32,
    min_n: usize,
    xmin: f64,
    max_dt_min: f64,
    dry_run: bool,
    report: Option<PathBuf>,
    emit_rust: bool,
    emit_toml: bool,
    batch_size: usize,
    start_max: u32,
    retries: u32,
    max_wait_min: u64,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            regions: REGIONS.iter().map(|r| r.name).collect(),
            list_regions: false,
            fit_year: 2019,
            holdout_year: 2020,
            min_n: 30,
            xmin: DEFAULT_XMIN_MW,
            max_dt_min: 20.0,
            dry_run: false,
            report: None,
            emit_rust: false,
            emit_toml: false,
            batch_size: 40,
            start_max: 3500,
            retries: 3,
            max_wait_min: 15,
        }
    }
}

fn parse_regions(text: &str) -> Result<Vec<&'static str>> {
    let mut out: Vec<&'static str> = Vec::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let region = REGIONS
            .iter()
            .find(|r| r.name == part)
            .with_context(|| format!("unknown region '{part}' (try --list-regions)"))?;
        if !out.contains(&region.name) {
            out.push(region.name);
        }
    }
    if out.is_empty() {
        bail!("--regions needs at least one region name");
    }
    Ok(out)
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
            "--regions" => args.regions = parse_regions(&value("--regions")?)?,
            "--list-regions" => args.list_regions = true,
            "--fit-year" => args.fit_year = value("--fit-year")?.parse().context("bad --fit-year")?,
            "--holdout-year" => {
                args.holdout_year = value("--holdout-year")?.parse().context("bad --holdout-year")?
            }
            "--min-n" => args.min_n = value("--min-n")?.parse().context("bad --min-n")?,
            "--xmin" => args.xmin = value("--xmin")?.parse().context("bad --xmin")?,
            "--max-dt-min" => args.max_dt_min = value("--max-dt-min")?.parse().context("bad --max-dt-min")?,
            "--dry-run" => args.dry_run = true,
            "--report" => args.report = Some(PathBuf::from(value("--report")?)),
            "--emit-rust" => args.emit_rust = true,
            "--emit-toml" => args.emit_toml = true,
            "--batch-size" => args.batch_size = value("--batch-size")?.parse().context("bad --batch-size")?,
            "--start-max" => args.start_max = value("--start-max")?.parse().context("bad --start-max")?,
            "--retries" => args.retries = value("--retries")?.parse().context("bad --retries")?,
            "--max-wait-min" => {
                args.max_wait_min = value("--max-wait-min")?.parse().context("bad --max-wait-min")?
            }
            other => bail!("unknown argument '{other}'"),
        }
    }
    if args.fit_year == args.holdout_year {
        bail!("--fit-year and --holdout-year must differ");
    }
    if args.batch_size == 0 {
        bail!("--batch-size must be at least 1");
    }
    if args.min_n == 0 {
        bail!("--min-n must be at least 1");
    }
    if !args.xmin.is_finite() || args.xmin <= 0.0 {
        bail!("--xmin must be positive and finite");
    }
    if !args.max_dt_min.is_finite() || args.max_dt_min < 0.0 {
        bail!("--max-dt-min must be finite and >= 0");
    }
    Ok(args)
}

fn region_by_name(name: &str) -> Region {
    *REGIONS
        .iter()
        .find(|r| r.name == name)
        .expect("region names are validated by parse_regions")
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
}

fn sources() -> [FirmsSource; 2] {
    [
        FirmsSource::new(Sensor::ModisAqua, Processing::Sp),
        FirmsSource::new(Sensor::ViirsSNPP, Processing::Sp),
    ]
}

fn plan_year(year: i32) -> Vec<Window> {
    let mut out = Vec::new();
    for source in sources() {
        for (start, days) in date_windows(d(year, 1, 1), d(year, 12, 31)) {
            out.push((source, start, days));
        }
    }
    out
}

fn plan_windows(args: &Args) -> Vec<Window> {
    let mut out = plan_year(args.fit_year);
    out.extend(plan_year(args.holdout_year));
    out
}

// ---------------------------------------------------------------- pairing

#[derive(Debug, Clone, PartialEq)]
struct CellDay {
    month: u32,
    lat_center: f64,
    day: bool,
    biome: Biome,
    /// Sum of VIIRS FRP.
    num: f64,
    /// Sum of MODIS FRP x R_rel(theta).
    den: f64,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct PairStats {
    aqua_cell_days: usize,
    viirs_cell_days: usize,
    overlap: usize,
    rejected_dt: usize,
    nonpositive: usize,
    no_biome: usize,
    paired: usize,
}

#[derive(Default)]
struct Acc {
    modis_raw: f64,
    modis_vza: f64,
    modis_n: u32,
    modis_sec: f64,
    viirs_sum: f64,
    viirs_n: u32,
    viirs_sec: f64,
    points: Vec<(f64, f64)>,
}

type CellKey = (NaiveDate, i32, i32, bool);

fn modis_factor(h: &Hotspot) -> f64 {
    vza_factor(h).filter(|f| f.is_finite() && *f > 0.0).unwrap_or(1.0)
}

fn sec_of_day(h: &Hotspot) -> f64 {
    f64::from(h.acquired.num_seconds_from_midnight()) + f64::from(h.acquired.nanosecond()) * 1e-9
}

fn cell_key(h: &Hotspot) -> CellKey {
    (
        h.acquired.date_naive(),
        h.lat.floor() as i32,
        h.lon.floor() as i32,
        h.daynight == DayNight::Day,
    )
}

fn pair_cell_days(aqua: &[Hotspot], viirs: &[Hotspot], max_dt_min: f64) -> (Vec<CellDay>, PairStats) {
    let mut cells: HashMap<CellKey, Acc> = HashMap::new();
    let mut aqua_keys = 0usize;
    let mut viirs_keys = 0usize;
    for h in aqua {
        let acc = cells.entry(cell_key(h)).or_default();
        if acc.modis_n == 0 {
            aqua_keys += 1;
        }
        acc.modis_raw += h.frp;
        acc.modis_vza += h.frp * modis_factor(h);
        acc.modis_n += 1;
        acc.modis_sec += sec_of_day(h);
        acc.points.push((h.lat, h.lon));
    }
    for h in viirs {
        let acc = cells.entry(cell_key(h)).or_default();
        if acc.viirs_n == 0 {
            viirs_keys += 1;
        }
        acc.viirs_sum += h.frp;
        acc.viirs_n += 1;
        acc.viirs_sec += sec_of_day(h);
        acc.points.push((h.lat, h.lon));
    }

    let mut stats = PairStats {
        aqua_cell_days: aqua_keys,
        viirs_cell_days: viirs_keys,
        ..PairStats::default()
    };
    let mut out = Vec::new();
    for (key, acc) in &cells {
        if acc.modis_n == 0 || acc.viirs_n == 0 {
            continue;
        }
        stats.overlap += 1;
        let dt = (acc.modis_sec / f64::from(acc.modis_n) - acc.viirs_sec / f64::from(acc.viirs_n)).abs();
        if dt > max_dt_min * 60.0 {
            stats.rejected_dt += 1;
            continue;
        }
        if acc.modis_raw <= 0.0 || acc.viirs_sum <= 0.0 || acc.modis_vza <= 0.0 {
            stats.nonpositive += 1;
            continue;
        }
        let biome = match dominant_biome(acc.points.iter().copied()) {
            Some(b) if b != Biome::NonBurnable => b,
            _ => {
                stats.no_biome += 1;
                continue;
            }
        };
        stats.paired += 1;
        out.push(CellDay {
            month: key.0.month(),
            lat_center: (f64::from(key.1) + 0.5).clamp(-90.0, 90.0),
            day: key.3,
            biome,
            num: acc.viirs_sum,
            den: acc.modis_vza,
        });
    }
    // HashMap order is arbitrary; sort so reports and tests are reproducible.
    out.sort_by(|a, b| {
        (a.month, a.biome as u8, a.day, a.lat_center.to_bits(), a.num.to_bits())
            .cmp(&(b.month, b.biome as u8, b.day, b.lat_center.to_bits(), b.num.to_bits()))
    });
    (out, stats)
}

// ------------------------------------------------------------ aggregation

#[derive(Debug, Clone, PartialEq)]
struct Row {
    biome: Biome,
    month: Option<u32>,
    day: Option<bool>,
    ratio: f64,
    /// Standard error of the ratio of sums over cell-days (delta method).
    sigma: f64,
    /// Unweighted standard deviation of the per-cell-day ratio, for context only.
    cell_sd: f64,
    n: usize,
}

fn daynight_of(day: bool) -> DayNight {
    if day { DayNight::Day } else { DayNight::Night }
}

fn summarize(group: &[&CellDay]) -> Option<(f64, f64, f64)> {
    let n = group.len();
    let sum_num: f64 = group.iter().map(|c| c.num).sum();
    let sum_den: f64 = group.iter().map(|c| c.den).sum();
    if n == 0 || sum_den <= 0.0 || sum_num <= 0.0 {
        return None;
    }
    let k = sum_num / sum_den;
    let sse: f64 = group.iter().map(|c| (c.num - k * c.den).powi(2)).sum();
    let se = if n > 1 {
        (sse * n as f64 / (n as f64 - 1.0)).sqrt() / sum_den
    } else {
        0.0
    };
    let mean_r: f64 = group.iter().map(|c| c.num / c.den).sum::<f64>() / n as f64;
    let sd = if n > 1 {
        (group.iter().map(|c| (c.num / c.den - mean_r).powi(2)).sum::<f64>() / (n as f64 - 1.0)).sqrt()
    } else {
        0.0
    };
    Some((k, se, sd))
}

/// Pools cell-days at three levels: (biome, month, day/night), (biome, day/night), (biome).
/// Groups smaller than `min_n` are not emitted so a consumer falls back to the next level.
fn aggregate(cell_days: &[CellDay], min_n: usize) -> Vec<Row> {
    type Key = (u8, Option<u32>, Option<bool>);
    let mut groups: BTreeMap<Key, Vec<&CellDay>> = BTreeMap::new();
    for c in cell_days {
        let b = c.biome as u8;
        groups.entry((b, Some(c.month), Some(c.day))).or_default().push(c);
        groups.entry((b, None, Some(c.day))).or_default().push(c);
        groups.entry((b, None, None)).or_default().push(c);
    }
    let mut rows = Vec::new();
    for ((b, month, day), group) in groups {
        if group.len() < min_n {
            continue;
        }
        if let Some((ratio, sigma, cell_sd)) = summarize(&group) {
            rows.push(Row {
                biome: Biome::ALL[b as usize],
                month,
                day,
                ratio,
                sigma,
                cell_sd,
                n: group.len(),
            });
        }
    }
    rows
}

fn find_row(rows: &[Row], biome: Biome, month: Option<u32>, day: Option<bool>) -> Option<&Row> {
    rows.iter()
        .find(|r| r.biome == biome && r.month == month && r.day == day)
}

/// Most specific available level: (biome, month, day/night) then (biome, day/night) then (biome).
fn lookup_row(rows: &[Row], biome: Biome, month: u32, day: bool) -> Option<&Row> {
    find_row(rows, biome, Some(month), Some(day))
        .or_else(|| find_row(rows, biome, None, Some(day)))
        .or_else(|| find_row(rows, biome, None, None))
}

// ------------------------------------------------------------- power law

#[derive(Debug, Clone, Copy, PartialEq)]
struct Hill {
    alpha: f64,
    se: f64,
    n: usize,
}

/// Maximum-likelihood exponent of `p(x) ~ x^-alpha` for `x >= xmin` (Hill estimator).
fn hill_estimate(values: &[f64], xmin: f64) -> Option<Hill> {
    let tail: Vec<f64> = values.iter().copied().filter(|v| v.is_finite() && *v >= xmin).collect();
    let n = tail.len();
    if n < 2 || xmin <= 0.0 {
        return None;
    }
    let log_sum: f64 = tail.iter().map(|v| (v / xmin).ln()).sum();
    if log_sum <= 0.0 {
        return None;
    }
    let alpha = 1.0 + n as f64 / log_sum;
    Some(Hill {
        alpha,
        se: (alpha - 1.0) / (n as f64).sqrt(),
        n,
    })
}

/// Linear-interpolated quantile of an ascending slice.
fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    if sorted.len() == 1 {
        return Some(sorted[0]);
    }
    let pos = p.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    Some(sorted[lo] + (pos - lo as f64) * (sorted[hi] - sorted[lo]))
}

fn sorted_f64(values: &[f32]) -> Vec<f64> {
    let mut v: Vec<f64> = values.iter().map(|&x| f64::from(x)).collect();
    v.sort_by(f64::total_cmp);
    v
}

#[derive(Debug, Clone, PartialEq)]
struct PowerLaw {
    biome: Biome,
    hill: Hill,
    p_max_mw: f64,
    total: usize,
}

fn power_law_for(biome: Biome, frp: &[f32], xmin: f64) -> Option<PowerLaw> {
    let sorted = sorted_f64(frp);
    let hill = hill_estimate(&sorted, xmin)?;
    if hill.n < MIN_TAIL_N {
        return None;
    }
    Some(PowerLaw {
        biome,
        hill,
        p_max_mw: percentile(&sorted, 0.999)?,
        total: sorted.len(),
    })
}

// --------------------------------------------------------------- hold-out

#[derive(Debug, Default, Clone, PartialEq)]
struct Score {
    n: usize,
    sum_num: f64,
    sum_pred: f64,
    abs_logs: Vec<f64>,
}

impl Score {
    fn add(&mut self, num: f64, pred: f64) {
        self.n += 1;
        self.sum_num += num;
        self.sum_pred += pred;
        self.abs_logs.push((pred / num).ln().abs());
    }

    /// Aggregate bias of the predicted VIIRS total relative to the measured one.
    fn bias(&self) -> Option<f64> {
        (self.sum_num > 0.0).then(|| self.sum_pred / self.sum_num - 1.0)
    }

    fn median_abs_log(&self) -> Option<f64> {
        let mut v = self.abs_logs.clone();
        v.sort_by(f64::total_cmp);
        percentile(&v, 0.5)
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
struct Predictors {
    lat: Score,
    biome: Score,
    identity: Score,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct HoldOut {
    overall: Predictors,
    per_biome: BTreeMap<u8, Predictors>,
    /// Hold-out cell-days with no biome-table row at any level (scored with the latitude band).
    biome_fallback_lat: usize,
}

fn evaluate(cell_days: &[CellDay], rows: &[Row], cfg: &CalibrationConfig) -> HoldOut {
    let mut out = HoldOut::default();
    for c in cell_days {
        let lat_k = calibration_for(c.lat_center, cfg).ratio;
        let biome_k = match lookup_row(rows, c.biome, c.month, c.day) {
            Some(r) => r.ratio,
            None => {
                out.biome_fallback_lat += 1;
                lat_k
            }
        };
        for p in [&mut out.overall, out.per_biome.entry(c.biome as u8).or_default()] {
            p.lat.add(c.num, c.den * lat_k);
            p.biome.add(c.num, c.den * biome_k);
            p.identity.add(c.num, c.den);
        }
    }
    out
}

fn verdict(p: &Predictors) -> &'static str {
    let (Some(lat_e), Some(bio_e)) = (p.lat.median_abs_log(), p.biome.median_abs_log()) else {
        return "belirsiz (hold-out ornegi yok)";
    };
    let (Some(lat_b), Some(bio_b)) = (p.lat.bias(), p.biome.bias()) else {
        return "belirsiz (hold-out ornegi yok)";
    };
    match (bio_e < lat_e, bio_b.abs() < lat_b.abs()) {
        (true, true) => "BIYOM TABLOSU ENLEM BANDLARINDAN IYI (hem medyan |log hata| hem toplam sapma)",
        (false, false) => "BIYOM TABLOSU ENLEM BANDLARINDAN IYI DEGIL (ne medyan |log hata| ne toplam sapma)",
        (true, false) => "KARISIK: biyom medyan |log hata|da iyi, toplam sapmada degil",
        (false, true) => "KARISIK: biyom toplam sapmada iyi, medyan |log hata|da degil",
    }
}

// ---------------------------------------------------------------- emitting

fn emit_rust_row(r: &Row) -> String {
    let month = r.month.map_or("None".to_string(), |m| format!("Some({m})"));
    let day = r
        .day
        .map_or("None".to_string(), |d| format!("Some(DayNight::{:?})", daynight_of(d)));
    format!(
        "BiomeCompleteness {{ biome: Biome::{:?}, month: {month}, daynight: {day}, ratio: {:.3}, ratio_sigma: {:.3}, n: {} }},",
        r.biome, r.ratio, r.sigma, r.n
    )
}

fn emit_rust_power_law(p: &PowerLaw) -> String {
    format!(
        "PowerLawBudget {{ biome: Biome::{:?}, alpha: {:.2}, p_max_mw: {:.1} }},",
        p.biome, p.hill.alpha, p.p_max_mw
    )
}

fn emit_toml_row(r: &Row) -> String {
    let mut s = String::from("[[calibration.biome_table]]\n");
    let _ = writeln!(s, "biome = \"{}\"", r.biome.as_str());
    if let Some(m) = r.month {
        let _ = writeln!(s, "month = {m}");
    }
    if let Some(d) = r.day {
        let _ = writeln!(s, "daynight = \"{:?}\"", daynight_of(d));
    }
    let _ = writeln!(s, "ratio = {:.3}", r.ratio);
    let _ = writeln!(s, "ratio_sigma = {:.3}", r.sigma);
    let _ = writeln!(s, "n = {}", r.n);
    s
}

fn emit_toml_power_law(p: &PowerLaw) -> String {
    format!(
        "[[calibration.power_law]]\nbiome = \"{}\"\nalpha = {:.2}\np_max_mw = {:.1}\n",
        p.biome.as_str(),
        p.hill.alpha,
        p.p_max_mw
    )
}

fn emit_rust(rows: &[Row], laws: &[PowerLaw]) -> String {
    let mut s = String::from("// BiomeCompleteness rows\n");
    for r in rows {
        let _ = writeln!(s, "{}", emit_rust_row(r));
    }
    s.push_str("// PowerLawBudget rows\n");
    for p in laws {
        let _ = writeln!(s, "{}", emit_rust_power_law(p));
    }
    s
}

fn emit_toml(rows: &[Row], laws: &[PowerLaw]) -> String {
    let mut s = String::new();
    for r in rows {
        s.push_str(&emit_toml_row(r));
        s.push('\n');
    }
    for p in laws {
        s.push_str(&emit_toml_power_law(p));
        s.push('\n');
    }
    s
}

// ------------------------------------------------------------------ report

fn level_label(r: &Row) -> &'static str {
    match (r.month, r.day) {
        (Some(_), Some(_)) => "biyom-ay-gun/gece",
        (None, Some(_)) => "biyom-gun/gece",
        _ => "biyom",
    }
}

fn dn_label(day: Option<bool>) -> &'static str {
    match day {
        Some(true) => "gunduz",
        Some(false) => "gece",
        None => "hepsi",
    }
}

struct RegionSummary {
    name: &'static str,
    bbox: [f64; 4],
    stats_fit: PairStats,
    stats_hold: PairStats,
    aqua_rows: usize,
    viirs_rows: usize,
}

struct Results {
    args: Args,
    used: Vec<RegionSummary>,
    excluded: Vec<(&'static str, String)>,
    rows: Vec<Row>,
    laws: Vec<PowerLaw>,
    cerrado: Option<Hill>,
    holdout: HoldOut,
    holdout_n: usize,
    cached_windows: usize,
    uncached_windows: usize,
    transactions_spent: u32,
    observed_transactions: u64,
    observed_rates: Vec<String>,
}

fn fmt_opt(v: Option<f64>, digits: usize) -> String {
    v.map_or("n/a".to_string(), |x| format!("{x:.digits$}"))
}

fn fmt_pct(v: Option<f64>) -> String {
    v.map_or("n/a".to_string(), |x| format!("{:+.1}%", x * 100.0))
}

fn render_report(res: &Results) -> String {
    let a = &res.args;
    let mut s = String::new();
    let _ = writeln!(s, "# Biyom bazli tamlik katsayisi k_c (K14 olcumu)\n");
    let _ = writeln!(
        s,
        "Aqua MODIS (`MODIS_SP`, yalniz Aqua satirlari) ve S-NPP 375 m VIIRS (`VIIRS_SNPP_SP`), 1x1 derece UTC hucre-gun, |dt| <= {} dk, gunduz ve gece ayri. \
         k = toplam(VIIRS FRP) / toplam(MODIS FRP x R_rel(theta)). Uydurma yil {}, hold-out yil {}. Asgari ornek (hucre-gun) {}.\n",
        a.max_dt_min, a.fit_year, a.holdout_year, a.min_n
    );
    let _ = writeln!(
        s,
        "Onbellek: {} pencere onbellekten, {} pencere agdan; harcanan islem {}.\n",
        res.cached_windows, res.uncached_windows, res.transactions_spent
    );
    let _ = writeln!(
        s,
        "FIRMS sayacindan gozlenen gercek islem: {} (yukaridaki rakam pencere basina gun sayisina dayanan tahmindir ve gercek maliyetin altindadir).",
        res.observed_transactions
    );
    for line in &res.observed_rates {
        let _ = writeln!(s, "- {line}");
    }
    s.push('\n');

    let _ = writeln!(s, "## Bolgeler\n");
    let _ = writeln!(s, "| Bolge | bbox (w,s,e,n) | Aqua satir | VIIRS satir | cift hucre-gun {0} | cift hucre-gun {1} |", a.fit_year, a.holdout_year);
    let _ = writeln!(s, "|---|---|---:|---:|---:|---:|");
    for r in &res.used {
        let _ = writeln!(
            s,
            "| {} | {},{},{},{} | {} | {} | {} | {} |",
            r.name, r.bbox[0], r.bbox[1], r.bbox[2], r.bbox[3], r.aqua_rows, r.viirs_rows, r.stats_fit.paired, r.stats_hold.paired
        );
    }
    if res.excluded.is_empty() {
        let _ = writeln!(s, "\nDisarida birakilan bolge yok.\n");
    } else {
        let _ = writeln!(s, "\nDISARIDA BIRAKILAN BOLGELER (basarisiz pencere; sonuca girmedi):\n");
        for (name, why) in &res.excluded {
            let _ = writeln!(s, "- {name}: {why}");
        }
        s.push('\n');
    }

    let _ = writeln!(s, "## k_c tablosu (uydurma yili {})\n", a.fit_year);
    let _ = writeln!(s, "sigma = hucre-gunler uzerinden oran-toplam kestiriminin standart hatasi (delta yontemi); `sd_hg` = hucre-gun orani standart sapmasi (yalniz bilgi).\n");
    let _ = writeln!(s, "| Biyom | Seviye | Ay | Gun/gece | k_c | sigma | sd_hg | N |");
    let _ = writeln!(s, "|---|---|---:|---|---:|---:|---:|---:|");
    for r in &res.rows {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {:.3} | {:.3} | {:.3} | {} |",
            r.biome.as_str(),
            level_label(r),
            r.month.map_or("-".to_string(), |m| m.to_string()),
            dn_label(r.day),
            r.ratio,
            r.sigma,
            r.cell_sd,
            r.n
        );
    }

    let _ = writeln!(s, "\n## Guc yasasi (VIIRS FRP, uydurma yili)\n");
    let _ = writeln!(
        s,
        "alpha: p(x) ~ x^-alpha icin Hill en-olabilirlik kestirimi, x >= {} MW; SE = (alpha-1)/sqrt(N). `p_max` = tum VIIRS FRP degerlerinin 99.9. yuzdeligi. Literatur (Cerrado, [15, Sek. 6]) {}.\n",
        a.xmin, LITERATURE_ALPHA
    );
    let _ = writeln!(s, "| Biyom | alpha | SE | N (kuyruk) | N (tum) | p_max (MW) |");
    let _ = writeln!(s, "|---|---:|---:|---:|---:|---:|");
    for p in &res.laws {
        let _ = writeln!(
            s,
            "| {} | {:.3} | {:.3} | {} | {} | {:.1} |",
            p.biome.as_str(),
            p.hill.alpha,
            p.hill.se,
            p.hill.n,
            p.total,
            p.p_max_mw
        );
    }
    match res.cerrado {
        Some(h) => {
            let _ = writeln!(
                s,
                "\nCerrado bolgesi tek basina: alpha = {:.3} +- {:.3} (N = {}); literatur {}.",
                h.alpha, h.se, h.n, LITERATURE_ALPHA
            );
        }
        None => {
            let _ = writeln!(s, "\nCerrado bolgesi alpha kestirimi icin yeterli veri/bolge yok (literatur {LITERATURE_ALPHA}).");
        }
    }

    let _ = writeln!(s, "\n## Hold-out ({}), {} hucre-gun\n", a.holdout_year, res.holdout_n);
    let _ = writeln!(
        s,
        "Sapma = toplam(MODIS x R_rel x k) / toplam(VIIRS) - 1. MedAE = medyan |ln(tahmin/olculen)| (hucre-gun basina). Biyom tablosunda hicbir seviyede satir yoksa enlem bandi kullanildi ({} hucre-gun).\n",
        res.holdout.biome_fallback_lat
    );
    let _ = writeln!(s, "| Kapsam | N | sapma enlem | sapma biyom | sapma k=1 | MedAE enlem | MedAE biyom | MedAE k=1 |");
    let _ = writeln!(s, "|---|---:|---:|---:|---:|---:|---:|---:|");
    let line = |label: &str, p: &Predictors| {
        format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            label,
            p.lat.n,
            fmt_pct(p.lat.bias()),
            fmt_pct(p.biome.bias()),
            fmt_pct(p.identity.bias()),
            fmt_opt(p.lat.median_abs_log(), 3),
            fmt_opt(p.biome.median_abs_log(), 3),
            fmt_opt(p.identity.median_abs_log(), 3)
        )
    };
    let _ = writeln!(s, "{}", line("Hepsi", &res.holdout.overall));
    for (b, p) in &res.holdout.per_biome {
        let _ = writeln!(s, "{}", line(Biome::ALL[*b as usize].as_str(), p));
    }
    let _ = writeln!(s, "\nSonuc: {}.", verdict(&res.holdout.overall));
    s
}

// --------------------------------------------------------------- plumbing

fn cache_dir() -> PathBuf {
    match std::env::var_os("AREA_CACHE_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from(DEFAULT_CACHE_DIR),
    }
}

/// Loads FIRMS_API_KEY from the repo-root `.env` when the environment does not already have it,
/// without ever printing it.
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

async fn uncached_indices(cache: &AreaCache, bbox_str: &str, windows: &[Window]) -> Vec<bool> {
    let mut cached = Vec::with_capacity(windows.len());
    for &(source, start, days) in windows {
        cached.push(cache.load(source, bbox_str, start, days).await.is_some());
    }
    cached
}

fn window_cost(windows: &[Window], cached: &[bool], year: i32, source: FirmsSource) -> (usize, usize, u32) {
    let mut total = 0;
    let mut hit = 0;
    let mut cost = 0;
    for (i, w) in windows.iter().enumerate() {
        if w.0 == source && w.1.year() == year {
            total += 1;
            if cached[i] {
                hit += 1;
            } else {
                cost += w.2.as_u32();
            }
        }
    }
    (total, hit, cost)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuotaStatus {
    limit: u32,
    current: u32,
}

/// Reads `{ "transaction_limit": 5000, "current_transactions": 4980, ... }`; field order and
/// whitespace are irrelevant. A missing/invalid `current_transactions` means "unknown".
fn parse_quota_status(text: &str) -> Option<QuotaStatus> {
    let v: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    let num = |key: &str| {
        let x = v.get(key)?;
        let n = x.as_u64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))?;
        u32::try_from(n).ok()
    };
    let current = num("current_transactions")?;
    let limit = num("transaction_limit").filter(|l| *l > 0).unwrap_or(DEFAULT_TRANSACTION_LIMIT);
    Some(QuotaStatus { limit, current })
}

/// Never prints the key or the URL: a reqwest error would embed it, so failures only yield `None`.
struct StatusReader {
    http: reqwest::Client,
    url: String,
}

impl StatusReader {
    fn new(key: &str) -> Result<Self> {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(20)).build()?;
        Ok(Self { http, url: format!("{STATUS_URL}?MAP_KEY={key}") })
    }

    async fn read(&self) -> Option<QuotaStatus> {
        let resp = self.http.get(&self.url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        parse_quota_status(&resp.text().await.ok()?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Headroom {
    Go,
    Wait(Duration),
}

/// Start only when the counter is at most `start_max` and `est_cost` more fit under the limit.
fn headroom_decision(status: Option<QuotaStatus>, est_cost: u32, start_max: u32) -> Headroom {
    match status {
        None => Headroom::Wait(UNKNOWN_STATUS_WAIT),
        Some(s) if s.current <= start_max && s.current.saturating_add(est_cost) <= s.limit => Headroom::Go,
        Some(_) => Headroom::Wait(POLL_INTERVAL),
    }
}

/// Waits (one status line per wait) until `headroom_decision` says go, or `max_wait` is spent.
/// An unknown counter costs one fixed wait and then proceeds, so a dead status endpoint cannot hang a run.
/// Returns the last counter read.
async fn wait_for_headroom(
    reader: &StatusReader,
    label: &str,
    est_cost: u32,
    start_max: u32,
    max_wait: Duration,
) -> Option<QuotaStatus> {
    let mut waited = Duration::ZERO;
    loop {
        let status = reader.read().await;
        match headroom_decision(status, est_cost, start_max) {
            Headroom::Go => return status,
            Headroom::Wait(d) => {
                if waited >= max_wait {
                    println!("{label}: waited {}s for quota headroom, proceeding anyway", waited.as_secs());
                    return status;
                }
                match status {
                    Some(s) => println!(
                        "{label}: quota {}/{} (need ~{est_cost}, start at <= {start_max}); waiting {}s",
                        s.current,
                        s.limit,
                        d.as_secs()
                    ),
                    None => println!("{label}: quota status unavailable; waiting {}s once, then proceeding", d.as_secs()),
                }
                tokio::time::sleep(d).await;
                waited += d;
                status?;
            }
        }
    }
}

/// Real transactions per window, learned from counter deltas.
#[derive(Default)]
struct Observed {
    by_region_source: HashMap<(String, String), (u64, u64)>,
    by_source: HashMap<String, (u64, u64)>,
}

impl Observed {
    fn record(&mut self, region: &str, source: &str, transactions: u32, windows: usize) {
        if windows == 0 || transactions == 0 {
            return;
        }
        for slot in [
            self.by_region_source.entry((region.to_string(), source.to_string())).or_default(),
            self.by_source.entry(source.to_string()).or_default(),
        ] {
            slot.0 += u64::from(transactions);
            slot.1 += windows as u64;
        }
    }

    /// Region+source observation first, then the source over all regions, then the prior.
    fn rate(&self, region: &str, source: &str) -> f64 {
        let ratio = |&(t, w): &(u64, u64)| t as f64 / w as f64;
        self.by_region_source
            .get(&(region.to_string(), source.to_string()))
            .or_else(|| self.by_source.get(source))
            .map_or(PRIOR_COST_PER_WINDOW, ratio)
    }
}

fn cost_estimate(rate: f64, windows: usize) -> u32 {
    (rate * windows as f64).ceil() as u32
}

fn is_limit_error(message: &str) -> bool {
    message.to_ascii_lowercase().contains(LIMIT_ERROR_TEXT)
}

/// Maps the client's per-window failures back to windows: limit errors are retryable, every other
/// failure (and any failure that cannot be matched to a window) is fatal for that window.
fn classify_failures(batch: &[Window], failures: &[(String, String)]) -> (Vec<Window>, Vec<String>) {
    let mut retry = Vec::new();
    let mut fatal = Vec::new();
    for (src, msg) in failures {
        let window = batch.iter().find(|w| {
            w.0.as_str() == src.as_str()
                && msg.starts_with(&format!("{} ({}d):", w.1.format("%Y-%m-%d"), w.2.get()))
        });
        match window {
            Some(w) if is_limit_error(msg) => retry.push(*w),
            _ => fatal.push(format!("{src}: {msg}")),
        }
    }
    (retry, fatal)
}

/// Consecutive windows of one source, at most `size` per batch.
fn split_batches(windows: &[Window], size: usize) -> Vec<Vec<Window>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < windows.len() {
        let mut j = i;
        while j < windows.len() && j - i < size.max(1) && windows[j].0 == windows[i].0 {
            j += 1;
        }
        out.push(windows[i..j].to_vec());
        i = j;
    }
    out
}

#[derive(Default)]
struct BatchResult {
    modis: Vec<Hotspot>,
    viirs: Vec<Hotspot>,
    failures: Vec<String>,
    estimated_spent: u32,
    observed_spent: u64,
}

/// Fetches one batch; windows refused for the transaction limit are re-requested alone (the
/// windows that succeeded are already in the area cache and their rows are kept here).
async fn fetch_batch(
    client: &FirmsClient,
    reader: &StatusReader,
    args: &Args,
    region: &str,
    bbox: &BBox,
    batch: &[Window],
    observed: &mut Observed,
) -> BatchResult {
    let source = batch[0].0.as_str();
    let mut out = BatchResult::default();
    let mut pending = batch.to_vec();
    let label = format!("{region} {source}");
    for attempt in 0..=args.retries {
        let est = cost_estimate(observed.rate(region, source), pending.len());
        let cap = if attempt == 0 { MAX_PRE_BATCH_WAIT } else { Duration::from_secs(args.max_wait_min * 60) };
        let before = wait_for_headroom(reader, &label, est, args.start_max, cap).await;
        let (m, v, rep) = client.fetch_source_windows(bbox, &pending).await;
        out.modis.extend(m);
        out.viirs.extend(v);
        out.estimated_spent += rep.transactions_spent;
        let (retry, fatal) = classify_failures(&pending, &rep.failures);
        out.failures.extend(fatal);
        if let (Some(b), Some(a)) = (before, reader.read().await) {
            let ok_windows = pending.len().saturating_sub(rep.failures.len());
            let delta = a.current.saturating_sub(b.current);
            out.observed_spent += u64::from(delta);
            if ok_windows > 0 && delta > 0 {
                observed.record(region, source, delta, ok_windows);
                println!(
                    "{label}: {ok_windows} window(s) took {delta} transactions ({:.1} per window), counter {}/{}",
                    f64::from(delta) / ok_windows as f64,
                    a.current,
                    a.limit
                );
            }
        }
        if retry.is_empty() {
            return out;
        }
        if attempt == args.retries {
            out.failures.extend(retry.iter().map(|w| {
                format!(
                    "{}: {} ({}d): transaction limit still exceeded after {} retries",
                    w.0.as_str(),
                    w.1,
                    w.2.get(),
                    args.retries
                )
            }));
            return out;
        }
        println!("{label}: {} window(s) refused for the transaction limit; retrying after recovery", retry.len());
        pending = retry;
    }
    out
}

fn filter_rows(mut rows: Vec<Hotspot>, sensor: Sensor) -> Vec<Hotspot> {
    retain_wildfires(&mut rows);
    rows.retain(|h| {
        h.sensor == sensor && h.lat.is_finite() && h.lon.is_finite() && h.frp.is_finite() && h.frp >= 0.0
    });
    remove_interscan_duplicates(&mut rows);
    rows
}

fn split_year(rows: Vec<Hotspot>, year: i32) -> Vec<Hotspot> {
    rows.into_iter().filter(|h| h.acquired.year() == year).collect()
}

fn print_plan_for(region: &Region, windows: &[Window], cached: &[bool], args: &Args) -> u32 {
    let mut region_cost = 0;
    for year in [args.fit_year, args.holdout_year] {
        for source in sources() {
            let (total, hit, cost) = window_cost(windows, cached, year, source);
            region_cost += cost;
            println!(
                "{:<18} {} {:<15} {:>4} windows {:>4} cached {:>5} transactions",
                region.name,
                year,
                source.as_str(),
                total,
                hit,
                cost
            );
        }
    }
    region_cost
}

struct RegionData {
    summary: RegionSummary,
    fit: Vec<CellDay>,
    hold: Vec<CellDay>,
    viirs_fit: Vec<(Biome, f32)>,
}

fn process_region(region: &Region, aqua_all: Vec<Hotspot>, viirs_all: Vec<Hotspot>, args: &Args) -> RegionData {
    let aqua_all = filter_rows(aqua_all, Sensor::ModisAqua);
    let viirs_all = filter_rows(viirs_all, Sensor::ViirsSNPP);
    let aqua_fit = split_year(aqua_all.clone(), args.fit_year);
    let viirs_fit = split_year(viirs_all.clone(), args.fit_year);
    let aqua_hold = split_year(aqua_all, args.holdout_year);
    let viirs_hold = split_year(viirs_all, args.holdout_year);
    let (fit, stats_fit) = pair_cell_days(&aqua_fit, &viirs_fit, args.max_dt_min);
    let (hold, stats_hold) = pair_cell_days(&aqua_hold, &viirs_hold, args.max_dt_min);
    let viirs_frp: Vec<(Biome, f32)> = viirs_fit
        .iter()
        .filter_map(|h| match biome_at(h.lat, h.lon) {
            Some(b) if b != Biome::NonBurnable => Some((b, h.frp as f32)),
            _ => None,
        })
        .collect();
    RegionData {
        summary: RegionSummary {
            name: region.name,
            bbox: region.bbox,
            stats_fit,
            stats_hold,
            aqua_rows: aqua_fit.len() + aqua_hold.len(),
            viirs_rows: viirs_fit.len() + viirs_hold.len(),
        },
        fit,
        hold,
        viirs_fit: viirs_frp,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    load_dotenv();
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw)?;

    if args.list_regions {
        for r in &REGIONS {
            println!(
                "{:<18} {:>8},{:>6},{:>8},{:>6}  {}",
                r.name, r.bbox[0], r.bbox[1], r.bbox[2], r.bbox[3], r.label
            );
        }
        return Ok(());
    }

    let dir = cache_dir();
    let cache = AreaCache::new(dir.clone(), Duration::ZERO);
    let windows = plan_windows(&args);
    println!(
        "K14 biome completeness: fit {}, hold-out {}, {} region(s), area cache {}",
        args.fit_year,
        args.holdout_year,
        args.regions.len(),
        dir.display()
    );

    if args.dry_run {
        let mut total = 0u32;
        for name in &args.regions {
            let region = region_by_name(name);
            let [w, s, e, n] = region.bbox;
            let bbox_str = BBox { west: w, south: s, east: e, north: n }.to_firms_string();
            let cached = uncached_indices(&cache, &bbox_str, &windows).await;
            let cost = print_plan_for(&region, &windows, &cached, &args);
            let hit = cached.iter().filter(|c| **c).count();
            println!(
                "{:<18} {hit} cached / {} uncached of {} windows; subtotal {cost} estimated transactions (lower bound)",
                region.name,
                windows.len() - hit,
                windows.len()
            );
            total += cost;
        }
        println!(
            "Total estimated transactions: {total} (uncached). This is a LOWER BOUND (1 per day of each window); \
             the real cost was observed to be higher, so the live run reads FIRMS's own counter and paces on that."
        );
        println!("--dry-run: no network requests made.");
        return Ok(());
    }

    let key = std::env::var("FIRMS_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .context("FIRMS_API_KEY is not set (run: set -a; . ./.env; set +a)")?;
    let reader = StatusReader::new(&key)?;
    let client = FirmsClient::new(key)?.with_area_cache(AreaCache::new(dir, Duration::ZERO));

    let mut observed = Observed::default();
    let mut used = Vec::new();
    let mut excluded: Vec<(&'static str, String)> = Vec::new();
    let mut fit_all: Vec<CellDay> = Vec::new();
    let mut hold_all: Vec<CellDay> = Vec::new();
    let mut viirs_by_biome: BTreeMap<u8, Vec<f32>> = BTreeMap::new();
    let mut cerrado_frp: Vec<f32> = Vec::new();
    let (mut cached_windows, mut uncached_windows, mut spent_total) = (0usize, 0usize, 0u32);
    let mut observed_total = 0u64;

    for name in &args.regions {
        let region = region_by_name(name);
        let [w, s, e, n] = region.bbox;
        let bbox = BBox { west: w, south: s, east: e, north: n };
        let bbox_str = bbox.to_firms_string();
        let cached = uncached_indices(&cache, &bbox_str, &windows).await;
        print_plan_for(&region, &windows, &cached, &args);
        let hit = cached.iter().filter(|c| **c).count();
        cached_windows += hit;
        uncached_windows += windows.len() - hit;

        let (cached_w, missing_w): (Vec<Window>, Vec<Window>) = {
            let mut c = Vec::new();
            let mut m = Vec::new();
            for (w, is_cached) in windows.iter().zip(&cached) {
                if *is_cached { c.push(*w) } else { m.push(*w) }
            }
            (c, m)
        };

        let mut modis = Vec::new();
        let mut viirs = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        if !cached_w.is_empty() {
            let (m, v, rep) = client.fetch_source_windows(&bbox, &cached_w).await;
            modis.extend(m);
            viirs.extend(v);
            failures.extend(rep.failures.iter().map(|(src, err)| format!("{src}: {err}")));
        }
        let batches = split_batches(&missing_w, args.batch_size);
        println!("{}: {} uncached window(s) in {} batch(es)", region.name, missing_w.len(), batches.len());
        for batch in &batches {
            let r = fetch_batch(&client, &reader, &args, region.name, &bbox, batch, &mut observed).await;
            spent_total += r.estimated_spent;
            observed_total += r.observed_spent;
            modis.extend(r.modis);
            viirs.extend(r.viirs);
            failures.extend(r.failures);
        }
        println!(
            "{}: {} cached / {} total windows, {} failed",
            region.name,
            hit,
            windows.len(),
            failures.len()
        );
        if !failures.is_empty() {
            let first = failures.iter().take(3).cloned().collect::<Vec<_>>().join("; ");
            eprintln!("{}: {} failed window(s), region excluded: {first}", region.name, failures.len());
            excluded.push((region.name, format!("{} basarisiz pencere ({first})", failures.len())));
            continue;
        }

        let data = process_region(&region, modis, viirs, &args);
        println!(
            "{}: paired cell-days fit={} hold-out={}",
            region.name, data.summary.stats_fit.paired, data.summary.stats_hold.paired
        );
        for (b, frp) in &data.viirs_fit {
            viirs_by_biome.entry(*b as u8).or_default().push(*frp);
            if region.name == "cerrado" {
                cerrado_frp.push(*frp);
            }
        }
        fit_all.extend(data.fit);
        hold_all.extend(data.hold);
        used.push(data.summary);
    }

    let rows = aggregate(&fit_all, args.min_n);
    let laws: Vec<PowerLaw> = viirs_by_biome
        .iter()
        .filter_map(|(b, v)| power_law_for(Biome::ALL[*b as usize], v, args.xmin))
        .collect();
    let cerrado = hill_estimate(&sorted_f64(&cerrado_frp), args.xmin);
    let holdout = evaluate(&hold_all, &rows, &CalibrationConfig::default());
    let results = Results {
        args: args.clone(),
        used,
        excluded,
        rows,
        laws,
        cerrado,
        holdout,
        holdout_n: hold_all.len(),
        cached_windows,
        uncached_windows,
        transactions_spent: spent_total,
        observed_transactions: observed_total,
        observed_rates: {
            let mut v: Vec<_> = observed.by_source.iter().collect();
            v.sort();
            v.iter()
                .map(|(src, (t, w))| format!("{src}: {:.1} islem/pencere ({t} islem, {w} pencere)", *t as f64 / *w as f64))
                .collect()
        },
    };

    let report = render_report(&results);
    println!("\n{report}");
    if let Some(path) = &args.report {
        std::fs::write(path, &report).with_context(|| format!("writing {}", path.display()))?;
        println!("report written to {}", path.display());
    }
    if args.emit_rust {
        println!("\n{}", emit_rust(&results.rows, &results.laws));
    }
    if args.emit_toml {
        println!("\n{}", emit_toml(&results.rows, &results.laws));
    }
    if !results.excluded.is_empty() {
        eprintln!("{} region(s) excluded; rerun to retry them (successful windows are cached)", results.excluded.len());
        std::process::exit(2);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use pyrosync_core::models::{Confidence, FireType};

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn hs(sensor: Sensor, lat: f64, lon: f64, frp: f64, day: bool, h: u32, m: u32) -> Hotspot {
        Hotspot {
            lat,
            lon,
            brightness: 330.0,
            frp,
            confidence: Confidence::Nominal,
            sensor,
            acquired: Utc.with_ymd_and_hms(2019, 8, 10, h, m, 0).unwrap(),
            scan: 1.0,
            track: 1.0,
            satellite: "x".into(),
            daynight: if day { DayNight::Day } else { DayNight::Night },
            vza: Some(0.0),
            fire_type: FireType::VegetationFire,
        }
    }

    // Savanna (Cerrado) location.
    const SAV: (f64, f64) = (-13.2, -47.2);

    #[test]
    fn pairs_within_dt_and_buckets_cell_days() {
        let aqua = vec![
            hs(Sensor::ModisAqua, SAV.0, SAV.1, 10.0, true, 16, 0),
            hs(Sensor::ModisAqua, SAV.0 + 0.1, SAV.1, 20.0, true, 16, 0),
            hs(Sensor::ModisAqua, -5.5, -47.5, 30.0, true, 16, 0),
        ];
        let viirs = vec![
            hs(Sensor::ViirsSNPP, SAV.0, SAV.1, 40.0, true, 16, 15),
            // same 1-degree cell-day but >20 min apart in another cell
            hs(Sensor::ViirsSNPP, -5.4, -47.4, 50.0, true, 17, 0),
        ];
        let (cds, stats) = pair_cell_days(&aqua, &viirs, 20.0);
        assert_eq!(cds.len(), 1);
        assert_eq!(stats.overlap, 2);
        assert_eq!(stats.rejected_dt, 1);
        assert_eq!(stats.paired, 1);
        assert_eq!(cds[0].biome, Biome::Savanna);
        assert!((cds[0].num - 40.0).abs() < 1e-9);
        assert!((cds[0].den - 30.0).abs() < 1e-9);
        assert_eq!(cds[0].month, 8);
        assert!(cds[0].day);
    }

    #[test]
    fn day_and_night_never_pair_and_nonburnable_is_skipped() {
        let aqua = vec![hs(Sensor::ModisAqua, SAV.0, SAV.1, 10.0, true, 16, 0)];
        let viirs = vec![hs(Sensor::ViirsSNPP, SAV.0, SAV.1, 40.0, false, 16, 0)];
        assert!(pair_cell_days(&aqua, &viirs, 20.0).0.is_empty());

        // Sahara
        let aqua = vec![hs(Sensor::ModisAqua, 23.0, 12.0, 10.0, true, 12, 0)];
        let viirs = vec![hs(Sensor::ViirsSNPP, 23.0, 12.0, 40.0, true, 12, 0)];
        let (cds, stats) = pair_cell_days(&aqua, &viirs, 20.0);
        assert!(cds.is_empty());
        assert_eq!(stats.no_biome, 1);
    }

    #[test]
    fn modis_frp_is_scaled_by_the_pipeline_vza_factor() {
        let mut edge = hs(Sensor::ModisAqua, SAV.0, SAV.1, 10.0, true, 16, 0);
        edge.vza = Some(60.0);
        let viirs = vec![hs(Sensor::ViirsSNPP, SAV.0, SAV.1, 40.0, true, 16, 0)];
        let (cds, _) = pair_cell_days(&[edge], &viirs, 20.0);
        let expected = 10.0 * (0.2017 * 60f64.to_radians().powi(2) + 0.0168 * 60f64.to_radians() + 1.0);
        assert!((cds[0].den - expected).abs() < 1e-9);
    }

    fn cd(biome: Biome, month: u32, day: bool, num: f64, den: f64) -> CellDay {
        CellDay { month, lat_center: -13.5, day, biome, num, den }
    }

    #[test]
    fn ratio_sigma_and_pooling_levels_respect_min_n() {
        let mut cds = Vec::new();
        for i in 0..40 {
            cds.push(cd(Biome::Savanna, 8, true, 120.0 + f64::from(i % 2), 100.0));
        }
        for _ in 0..10 {
            cds.push(cd(Biome::Savanna, 9, true, 150.0, 100.0));
        }
        let rows = aggregate(&cds, 30);
        // (Savanna, 8, Day) 40, (Savanna, Day) 50, (Savanna) 50; month 9 has only 10.
        assert_eq!(rows.len(), 3);
        assert!(find_row(&rows, Biome::Savanna, Some(9), Some(true)).is_none());
        let m8 = find_row(&rows, Biome::Savanna, Some(8), Some(true)).unwrap();
        assert_eq!(m8.n, 40);
        assert!((m8.ratio - 1.205).abs() < 1e-9);
        assert!(m8.sigma > 0.0 && m8.sigma < 0.01);
        let pooled = find_row(&rows, Biome::Savanna, None, None).unwrap();
        assert_eq!(pooled.n, 50);
        let expect = (40.0 * 120.5 + 10.0 * 150.0) / 5000.0;
        assert!((pooled.ratio - expect).abs() < 1e-9);
        // fallback: month 9 resolves to the (biome, daynight) row
        assert_eq!(lookup_row(&rows, Biome::Savanna, 9, true).unwrap().month, None);
        assert_eq!(lookup_row(&rows, Biome::Savanna, 8, true).unwrap().month, Some(8));
        // night has no data at any level
        assert!(lookup_row(&rows, Biome::Savanna, 8, false).is_some());
        assert!(lookup_row(&rows, Biome::Forest, 8, true).is_none());
    }

    #[test]
    fn sigma_is_zero_for_identical_cell_days() {
        let cds: Vec<CellDay> = (0..5).map(|_| cd(Biome::Forest, 1, false, 150.0, 100.0)).collect();
        let rows = aggregate(&cds, 5);
        assert!(rows.iter().all(|r| r.sigma.abs() < 1e-12 && (r.ratio - 1.5).abs() < 1e-12));
    }

    #[test]
    fn hill_recovers_pareto_alpha() {
        // Deterministic Pareto quantiles with pdf exponent alpha = 2.2, xmin = 5.
        let (alpha, xmin, n) = (2.2_f64, 5.0_f64, 20000usize);
        let sample: Vec<f64> = (0..n)
            .map(|i| {
                let u = (i as f64 + 0.5) / n as f64;
                xmin * (1.0 - u).powf(-1.0 / (alpha - 1.0))
            })
            .collect();
        let h = hill_estimate(&sample, xmin).unwrap();
        assert!((h.alpha - alpha).abs() < 0.03, "alpha {}", h.alpha);
        assert_eq!(h.n, n);
        assert!((h.se - (h.alpha - 1.0) / (n as f64).sqrt()).abs() < 1e-12);
        // values below xmin are ignored
        let mut with_noise = sample.clone();
        with_noise.extend([0.5, 1.0, 4.9]);
        assert_eq!(hill_estimate(&with_noise, xmin).unwrap().n, n);
        assert!(hill_estimate(&[1.0, 2.0], 5.0).is_none());
    }

    #[test]
    fn percentile_interpolates() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile(&v, 0.5), Some(3.0));
        assert_eq!(percentile(&v, 0.0), Some(1.0));
        assert_eq!(percentile(&v, 1.0), Some(5.0));
        assert!((percentile(&v, 0.125).unwrap() - 1.5).abs() < 1e-12);
        assert_eq!(percentile(&[], 0.5), None);
        assert_eq!(percentile(&[7.0], 0.9), Some(7.0));
    }

    #[test]
    fn power_law_needs_enough_tail() {
        let few: Vec<f32> = (0..10).map(|i| 6.0 + i as f32).collect();
        assert!(power_law_for(Biome::Savanna, &few, 4.96).is_none());
        let many: Vec<f32> = (0..500).map(|i| 5.0 * (1.0 - (i as f32 + 0.5) / 500.0).powf(-1.0 / 1.3)).collect();
        let p = power_law_for(Biome::Savanna, &many, 4.96).unwrap();
        assert!(p.p_max_mw > 100.0 && p.hill.n == 500);
    }

    #[test]
    fn holdout_metrics_and_verdict() {
        let rows = aggregate(
            &(0..40).map(|_| cd(Biome::Savanna, 8, true, 150.0, 100.0)).collect::<Vec<_>>(),
            30,
        );
        // measured ratio 1.5 everywhere: biome table (1.5) beats latitude band (1.11 tropics) and k=1.
        let hold: Vec<CellDay> = (0..10).map(|_| cd(Biome::Savanna, 8, true, 150.0, 100.0)).collect();
        let ho = evaluate(&hold, &rows, &CalibrationConfig::default());
        assert_eq!(ho.overall.biome.n, 10);
        assert!(ho.overall.biome.bias().unwrap().abs() < 1e-12);
        assert!(ho.overall.biome.median_abs_log().unwrap() < 1e-12);
        assert!((ho.overall.identity.bias().unwrap() - (-1.0 / 3.0)).abs() < 1e-12);
        assert!(ho.overall.lat.bias().unwrap() < 0.0);
        assert!(ho.per_biome.contains_key(&(Biome::Savanna as u8)));
        assert_eq!(ho.biome_fallback_lat, 0);
        assert!(verdict(&ho.overall).starts_with("BIYOM TABLOSU ENLEM BANDLARINDAN IYI ("));

        // forest has no row: scored with the latitude band, so the two predictors tie.
        let ho = evaluate(&[cd(Biome::Forest, 8, true, 150.0, 100.0)], &rows, &CalibrationConfig::default());
        assert_eq!(ho.biome_fallback_lat, 1);
        assert!(verdict(&ho.overall).contains("DEGIL"));
    }

    #[test]
    fn score_median_abs_log_is_symmetric() {
        let mut s = Score::default();
        s.add(100.0, 200.0);
        s.add(100.0, 50.0);
        s.add(100.0, 100.0);
        assert!((s.median_abs_log().unwrap() - 2f64.ln()).abs() < 1e-12);
        assert!((s.bias().unwrap() - (350.0 / 300.0 - 1.0)).abs() < 1e-12);
        assert_eq!(Score::default().median_abs_log(), None);
        assert_eq!(Score::default().bias(), None);
    }

    #[test]
    fn parses_regions_and_args() {
        assert_eq!(parse_args(&[]).unwrap(), Args::default());
        assert_eq!(parse_regions("cerrado, punjab,cerrado").unwrap(), vec!["cerrado", "punjab"]);
        assert!(parse_regions("nowhere").is_err());
        assert!(parse_regions(" , ").is_err());
        let a = parse_args(&strs(&[
            "--regions", "cerrado", "--min-n", "10", "--xmin", "3.5", "--fit-year", "2018", "--holdout-year", "2021",
            "--dry-run", "--report", "x.md", "--emit-rust", "--emit-toml", "--max-dt-min", "15",
            "--batch-size", "20", "--start-max", "3000", "--retries", "5", "--max-wait-min", "7",
        ]))
        .unwrap();
        assert_eq!(a.regions, vec!["cerrado"]);
        assert_eq!((a.min_n, a.fit_year, a.holdout_year), (10, 2018, 2021));
        assert_eq!(a.xmin, 3.5);
        assert_eq!(a.max_dt_min, 15.0);
        assert!(a.dry_run && a.emit_rust && a.emit_toml);
        assert_eq!((a.batch_size, a.start_max, a.retries, a.max_wait_min), (20, 3000, 5, 7));
        assert!(parse_args(&strs(&["--batch-size", "0"])).is_err());
        assert_eq!(a.report, Some(PathBuf::from("x.md")));
        assert!(parse_args(&strs(&["--fit-year", "2019", "--holdout-year", "2019"])).is_err());
        assert!(parse_args(&strs(&["--min-n", "0"])).is_err());
        assert!(parse_args(&strs(&["--nope"])).is_err());
        assert!(parse_args(&strs(&["--regions"])).is_err());
    }

    #[test]
    fn region_list_is_fixed_and_moderate() {
        assert_eq!(REGIONS.len(), 12);
        for r in &REGIONS {
            let [w, s, e, n] = r.bbox;
            assert!(w < e && s < n, "{}", r.name);
            assert!(e - w <= 20.0 && n - s <= 10.0, "{} too big", r.name);
        }
        let mut names: Vec<_> = REGIONS.iter().map(|r| r.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 12);
    }

    #[test]
    fn plan_costs_one_transaction_per_day_per_source() {
        let args = Args::default();
        let cost: u32 = plan_windows(&args).iter().map(|w| w.2.as_u32()).sum();
        // 2019 (365) + 2020 (366), two sources each
        assert_eq!(cost, 2 * (365 + 366));
    }

    #[test]
    fn quota_status_json_is_parsed_tolerantly() {
        let q = parse_quota_status(r#"{ "transaction_limit" : 5000, "current_transactions": 4980, "transaction_interval" : "10 minutes" }"#);
        assert_eq!(q, Some(QuotaStatus { limit: 5000, current: 4980 }));
        let reordered = parse_quota_status("\n{\"current_transactions\":12,\n\"transaction_limit\":\"3000\"}\n ");
        assert_eq!(reordered, Some(QuotaStatus { limit: 3000, current: 12 }));
        // missing limit falls back to the documented 5000
        assert_eq!(
            parse_quota_status(r#"{"current_transactions": 7}"#),
            Some(QuotaStatus { limit: 5000, current: 7 })
        );
        assert_eq!(parse_quota_status(r#"{"transaction_limit": 5000}"#), None);
        assert_eq!(parse_quota_status(r#"{"current_transactions": "many"}"#), None);
        assert_eq!(parse_quota_status("<html>Bad key</html>"), None);
        assert_eq!(parse_quota_status(""), None);
    }

    #[test]
    fn headroom_waits_until_counter_and_batch_fit() {
        let st = |current| Some(QuotaStatus { limit: 5000, current });
        assert_eq!(headroom_decision(st(0), 400, 3500), Headroom::Go);
        assert_eq!(headroom_decision(st(3500), 400, 3500), Headroom::Go);
        assert_eq!(headroom_decision(st(3501), 400, 3500), Headroom::Wait(POLL_INTERVAL));
        // under start_max but the batch itself would cross the limit
        assert_eq!(headroom_decision(st(3400), 1700, 3500), Headroom::Wait(POLL_INTERVAL));
        assert_eq!(headroom_decision(st(4980), 0, 3500), Headroom::Wait(POLL_INTERVAL));
        assert_eq!(headroom_decision(None, 400, 3500), Headroom::Wait(UNKNOWN_STATUS_WAIT));
    }

    #[test]
    fn observed_rate_prefers_region_then_source_then_prior() {
        let mut o = Observed::default();
        assert_eq!(o.rate("cerrado", "MODIS_SP"), PRIOR_COST_PER_WINDOW);
        o.record("cerrado", "MODIS_SP", 400, 40);
        o.record("punjab", "MODIS_SP", 120, 40);
        assert!((o.rate("cerrado", "MODIS_SP") - 10.0).abs() < 1e-12);
        assert!((o.rate("punjab", "MODIS_SP") - 3.0).abs() < 1e-12);
        // unseen region uses the source-wide rate (520 / 80)
        assert!((o.rate("amazon_arc", "MODIS_SP") - 6.5).abs() < 1e-12);
        assert_eq!(o.rate("cerrado", "VIIRS_SNPP_SP"), PRIOR_COST_PER_WINDOW);
        o.record("x", "MODIS_SP", 0, 5);
        assert!((o.rate("amazon_arc", "MODIS_SP") - 6.5).abs() < 1e-12);
        assert_eq!(cost_estimate(6.5, 40), 260);
        assert_eq!(cost_estimate(2.1, 3), 7);
    }

    #[test]
    fn only_the_limit_error_is_retryable() {
        let plan = plan_year(2019);
        let batch = &plan[..3];
        let (src, start, days) = batch[0];
        let tag = |i: usize| format!("{} ({}d)", batch[i].1.format("%Y-%m-%d"), batch[i].2.get());
        let limit = format!("{}: FIRMS request to https://x/*** failed with HTTP 400 Bad Request: Exceeding allowed transaction limit.", tag(0));
        let other = format!("{}: FIRMS request to https://x/*** failed with HTTP 400 Bad Request: Invalid MAP_KEY.", tag(1));
        assert!(is_limit_error(&limit));
        assert!(is_limit_error("exceeding ALLOWED transaction limit"));
        assert!(!is_limit_error(&other));
        let failures = vec![
            (src.as_str().to_string(), limit),
            (batch[1].0.as_str().to_string(), other),
            ("task".to_string(), "window task panicked".to_string()),
        ];
        let (retry, fatal) = classify_failures(batch, &failures);
        assert_eq!(retry, vec![(src, start, days)]);
        assert_eq!(fatal.len(), 2);
        assert!(fatal[0].contains("Invalid MAP_KEY") && fatal[1].contains("panicked"));
    }

    #[test]
    fn batches_split_per_source_and_respect_size() {
        let plan = plan_year(2019);
        let per_source = plan.iter().filter(|w| w.0 == plan[0].0).count();
        assert_eq!(plan.len(), 2 * per_source);
        let batches = split_batches(&plan, 40);
        assert!(batches.iter().all(|b| !b.is_empty() && b.len() <= 40));
        assert!(batches.iter().all(|b| b.iter().all(|w| w.0 == b[0].0)));
        assert_eq!(batches.iter().map(Vec::len).sum::<usize>(), plan.len());
        assert_eq!(batches.len(), 2 * per_source.div_ceil(40));
        let flat: Vec<Window> = batches.concat();
        assert_eq!(flat, plan);
        assert!(split_batches(&[], 40).is_empty());
        assert_eq!(split_batches(&plan, 0).len(), plan.len());
    }

    #[test]
    fn emit_formats_match_the_core_struct_shape() {
        let full = Row {
            biome: Biome::Savanna,
            month: Some(8),
            day: Some(true),
            ratio: 1.2344,
            sigma: 0.0561,
            cell_sd: 0.3,
            n: 412,
        };
        assert_eq!(
            emit_rust_row(&full),
            "BiomeCompleteness { biome: Biome::Savanna, month: Some(8), daynight: Some(DayNight::Day), ratio: 1.234, ratio_sigma: 0.056, n: 412 },"
        );
        let pooled = Row { month: None, day: None, biome: Biome::Cropland, ..full.clone() };
        assert_eq!(
            emit_rust_row(&pooled),
            "BiomeCompleteness { biome: Biome::Cropland, month: None, daynight: None, ratio: 1.234, ratio_sigma: 0.056, n: 412 },"
        );
        let night = Row { day: Some(false), ..full.clone() };
        assert!(emit_rust_row(&night).contains("Some(DayNight::Night)"));

        let toml = emit_toml_row(&full);
        assert_eq!(
            toml,
            "[[calibration.biome_table]]\nbiome = \"savanna\"\nmonth = 8\ndaynight = \"Day\"\nratio = 1.234\nratio_sigma = 0.056\nn = 412\n"
        );
        let toml = emit_toml_row(&pooled);
        assert!(!toml.contains("month") && !toml.contains("daynight"));

        let law = PowerLaw {
            biome: Biome::Savanna,
            hill: Hill { alpha: 1.3149, se: 0.01, n: 100 },
            p_max_mw: 850.04,
            total: 200,
        };
        assert_eq!(emit_rust_power_law(&law), "PowerLawBudget { biome: Biome::Savanna, alpha: 1.31, p_max_mw: 850.0 },");
        assert!(emit_toml_power_law(&law).contains("p_max_mw = 850.0"));
        let all = emit_rust(&[full], &[law]);
        assert_eq!(all.lines().filter(|l| l.starts_with("BiomeCompleteness")).count(), 1);
    }

    #[test]
    fn report_mentions_exclusions_and_verdict() {
        let rows = vec![Row {
            biome: Biome::Savanna,
            month: None,
            day: None,
            ratio: 1.3,
            sigma: 0.02,
            cell_sd: 0.4,
            n: 99,
        }];
        let res = Results {
            args: Args::default(),
            used: vec![],
            excluded: vec![("punjab", "2 basarisiz pencere".into())],
            rows,
            laws: vec![],
            cerrado: None,
            holdout: HoldOut::default(),
            holdout_n: 0,
            cached_windows: 1,
            uncached_windows: 2,
            transactions_spent: 10,
            observed_transactions: 42,
            observed_rates: vec!["MODIS_SP: 7.5 islem/pencere".into()],
        };
        let text = render_report(&res);
        assert!(text.contains("DISARIDA BIRAKILAN"));
        assert!(text.contains("punjab"));
        assert!(text.contains("| savanna | biyom |"));
        assert!(text.contains("belirsiz"));
    }
}
