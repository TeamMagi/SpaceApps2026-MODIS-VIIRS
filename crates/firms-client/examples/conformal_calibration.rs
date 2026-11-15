//! S10 conformal calibration, stage 1: extract held-out-sensor (mu, y) monthly `total_frp` pairs.
//!
//! Era A (`pre_viirs`): mu = MODIS-only pipeline cell, y = full MODIS+VIIRS pipeline cell,
//! 2012-02..2025-12. Era B (`viirs`): SNPP-only vs NOAA-20-only, 2018-05..2025-12, direction fixed
//! by a (country, month) hash parity. One resumable CSV per country.
//!
//! Stage 2 (`fit`) reads only the CSVs: LOYO/LOCO coverage, year-block bootstrap, final table and report.
//!
//! Usage:
//!   ARCHIVE_CACHE_DIR=<archive> conformal_calibration extract --out <dir> \
//!     [--countries A,B,...] [--max-minutes 75] [--threads 4] [--from 2000] [--to 2025]

use anyhow::{Context, Result, bail};
use pyrosync_core::calendar::build_calendar_with;
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::harmonize::harmonize_with_config;
use pyrosync_core::models::{CalendarCell, Hotspot, Sensor};
use pyrosync_firms::archive::{ArchiveClient, ArchiveError};
use pyrosync_firms::parse::retain_wildfires;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use tracing_subscriber::EnvFilter;

const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const CSV_HEADER: &str = "country,era,year,month,mu,y,analytic_lower,analytic_upper,n_mu";
const ERA_A_FIRST: (i32, u32) = (2012, 2);
const ERA_B_FIRST: (i32, u32) = (2018, 5);
const LAST_FULL_YEAR: i32 = 2025;
/// A country whose 2015 MODIS CSV exceeds this is run one at a time to bound memory.
const HEAVY_BYTES: u64 = 15 * 1024 * 1024;

const G12_COUNTRIES: [&str; 10] = [
    "Brazil",
    "Angola",
    "Australia",
    "Indonesia",
    "India",
    "Turkey",
    "United_States",
    "Canada",
    "Russian_Federation",
    "Kazakhstan",
];

const EXTRA_COUNTRIES: &str = "Portugal,Spain,Greece,Italy,Bulgaria,Romania,Ukraine,Poland,Chile,Peru,\
Bolivia,Colombia,Nepal,Thailand,Myanmar,Cambodia,Vietnam,Zambia,Mozambique,Tanzania,Madagascar,\
South_Africa,Mexico,Argentina,Paraguay,Mongolia,Israel,Lebanon,Morocco,Algeria,Tunisia,Cyprus,\
Croatia,Albania,Georgia,Armenia,Azerbaijan,Sweden,Finland,Norway,United_Kingdom,France,Germany,\
Ireland,New_Zealand,Ecuador,Uruguay,Honduras,Guatemala,Nicaragua,Cuba,Panama,Kenya,Uganda,Ghana,\
Nigeria,Serbia,Hungary,Slovakia,Lao_PDR,Sri_Lanka";

struct Args {
    fit: bool,
    pairs: PathBuf,
    json_out: PathBuf,
    doc_out: PathBuf,
    min_countries: usize,
    bootstrap: usize,
    seed: u64,
    out: PathBuf,
    countries: Vec<String>,
    max_minutes: f64,
    threads: usize,
    from: i32,
    to: i32,
}

fn usage() {
    println!(
        "Usage: conformal_calibration extract --out <dir> [--countries A,B,..] [--max-minutes 75] \
         [--threads 4] [--from 2000] [--to 2025]\nArchive: ARCHIVE_CACHE_DIR or {DEFAULT_ARCHIVE_CACHE_DIR}."
    );
}

fn parse_args() -> Result<Option<Args>> {
    let mut it = std::env::args().skip(1);
    let fit = match it.next().as_deref() {
        Some("extract") => false,
        Some("fit") => true,
        Some("-h" | "--help") | None => {
            usage();
            return Ok(None);
        }
        Some(other) => bail!("unknown subcommand: {other}"),
    };
    let mut a = Args {
        fit,
        pairs: PathBuf::from("pairs"),
        json_out: PathBuf::from("data/conformal.json"),
        doc_out: PathBuf::from("docs/KONFORMAL_KAPSAMA.md"),
        min_countries: 10,
        bootstrap: 1000,
        seed: 42,
        out: PathBuf::from("pairs"),
        countries: Vec::new(),
        max_minutes: f64::INFINITY,
        threads: 4,
        from: 2000,
        to: LAST_FULL_YEAR,
    };
    while let Some(arg) = it.next() {
        let mut val = |name: &str| it.next().with_context(|| format!("missing value for {name}"));
        match arg.as_str() {
            "--out" => a.out = PathBuf::from(val("--out")?),
            "--pairs" => a.pairs = PathBuf::from(val("--pairs")?),
            "--json-out" => a.json_out = PathBuf::from(val("--json-out")?),
            "--doc-out" => a.doc_out = PathBuf::from(val("--doc-out")?),
            "--min-countries" => a.min_countries = val("--min-countries")?.parse()?,
            "--bootstrap" => a.bootstrap = val("--bootstrap")?.parse()?,
            "--seed" => a.seed = val("--seed")?.parse()?,
            "--countries" => {
                a.countries = val("--countries")?
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "--max-minutes" => a.max_minutes = val("--max-minutes")?.parse()?,
            "--threads" => a.threads = val("--threads")?.parse::<usize>()?.clamp(1, 4),
            "--from" => a.from = val("--from")?.parse()?,
            "--to" => a.to = val("--to")?.parse()?,
            other => bail!("unknown flag: {other}"),
        }
    }
    Ok(Some(a))
}

fn archive_cache_dir() -> PathBuf {
    match std::env::var_os("ARCHIVE_CACHE_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR),
    }
}

fn modis_size_2015(cache: &Path, country: &str) -> Option<u64> {
    std::fs::metadata(cache.join("modis/2015").join(format!("modis_2015_{country}.csv")))
        .ok()
        .map(|m| m.len())
}

/// Smallest first so an early stop still yields many countries; large ones run last, one at a time.
fn intended_countries() -> Vec<String> {
    let mut all: Vec<String> = G12_COUNTRIES.iter().map(|s| s.to_string()).collect();
    all.extend(EXTRA_COUNTRIES.split(',').map(str::to_string));
    all
}

fn default_countries(cache: &Path) -> Vec<String> {
    let all = intended_countries();
    let mut sized: Vec<(u64, String)> = all
        .into_iter()
        .filter_map(|c| modis_size_2015(cache, &c).map(|s| (s, c)))
        .collect();
    sized.sort();
    let mut order: Vec<String> = sized.into_iter().map(|(_, c)| c).collect();
    // Turkey goes first as the sanity country.
    if let Some(pos) = order.iter().position(|c| c == "Turkey") {
        let t = order.remove(pos);
        order.insert(0, t);
    }
    order
}

/// FNV-1a over (country, year, month): the parity fixes which VIIRS fleet plays mu.
fn snpp_is_mu(country: &str, year: i32, month: u32) -> bool {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in country
        .bytes()
        .chain(year.to_le_bytes())
        .chain(month.to_le_bytes())
    {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h.count_ones().is_multiple_of(2)
}

fn cells_by_month(cells: Vec<CalendarCell>) -> HashMap<(i32, u32), CalendarCell> {
    cells.into_iter().map(|c| ((c.year, c.month), c)).collect()
}

fn run_pipeline(
    modis: Vec<Hotspot>,
    viirs: Vec<Hotspot>,
    cfg: &PipelineConfig,
) -> HashMap<(i32, u32), CalendarCell> {
    let h = harmonize_with_config(modis, viirs, cfg);
    cells_by_month(build_calendar_with(&h, &cfg.uncertainty, &cfg.calibration))
}

async fn load(
    archive: &ArchiveClient,
    country: &str,
    from: i32,
    to: i32,
) -> Result<(Vec<Hotspot>, Vec<Hotspot>, Vec<Hotspot>)> {
    let (mut modis, mut snpp, mut noaa) = (Vec::new(), Vec::new(), Vec::new());
    let mut found = false;
    for year in from..=to {
        for sensor in [Sensor::ModisTerra, Sensor::ViirsSNPP, Sensor::ViirsNOAA20] {
            if !archive.covers(sensor, year) {
                continue;
            }
            match archive.fetch_country_year(sensor, country, year).await {
                Ok(mut hs) => {
                    found = true;
                    retain_wildfires(&mut hs);
                    match sensor {
                        Sensor::ViirsSNPP => snpp.append(&mut hs),
                        Sensor::ViirsNOAA20 => noaa.append(&mut hs),
                        _ => modis.append(&mut hs),
                    }
                }
                Err(ArchiveError::NotCached { .. }) if archive.sensor_year_complete(sensor, year) => {}
                Err(e) => {
                    return Err(e).with_context(|| format!("load {sensor:?} {country} {year}"));
                }
            }
        }
    }
    if !found {
        bail!("no archive files for {country}");
    }
    Ok((modis, snpp, noaa))
}

fn fmt_opt(v: Option<f64>) -> String {
    v.filter(|x| x.is_finite()).map_or(String::new(), |x| format!("{x:.6}"))
}

fn push_row(
    out: &mut String,
    country: &str,
    era: &str,
    ym: (i32, u32),
    mu: &CalendarCell,
    y: f64,
) {
    let _ = writeln!(
        out,
        "{country},{era},{},{},{:.6},{:.6},{},{},{}",
        ym.0,
        ym.1,
        mu.total_frp,
        y,
        fmt_opt(mu.total_frp_lower),
        fmt_opt(mu.total_frp_upper),
        mu.count
    );
}

fn months(first: (i32, u32), last_year: i32) -> impl Iterator<Item = (i32, u32)> {
    (first.0..=last_year)
        .flat_map(|y| (1..=12).map(move |m| (y, m)))
        .filter(move |&ym| ym >= first)
}

fn eligible(mu: &CalendarCell, y: f64) -> bool {
    mu.total_frp.is_finite() && mu.total_frp > 0.0 && y.is_finite() && y >= 0.0
}

async fn extract_country(
    archive: &ArchiveClient,
    cfg: &PipelineConfig,
    country: &str,
    args: &Args,
) -> Result<(usize, usize)> {
    let (modis, snpp, noaa) = load(archive, country, args.from, args.to).await?;
    let mut csv = String::new();
    let (mut n_a, mut n_b) = (0, 0);

    // Re-run from raw input without VIIRS: harmonized output cannot be stripped (Matched keeps VIIRS FRP).
    let modis_only = run_pipeline(modis.clone(), Vec::new(), cfg);
    let snpp_only = run_pipeline(Vec::new(), snpp.clone(), cfg);
    let noaa_only = run_pipeline(Vec::new(), noaa.clone(), cfg);
    let mut viirs = snpp;
    viirs.extend(noaa);
    let full = run_pipeline(modis, viirs, cfg);

    for ym in months(ERA_A_FIRST, LAST_FULL_YEAR) {
        if let (Some(mu), Some(yc)) = (modis_only.get(&ym), full.get(&ym))
            && eligible(mu, yc.total_frp)
        {
            push_row(&mut csv, country, "pre_viirs", ym, mu, yc.total_frp);
            n_a += 1;
        }
    }
    for ym in months(ERA_B_FIRST, LAST_FULL_YEAR) {
        let (mu_map, y_map) = if snpp_is_mu(country, ym.0, ym.1) {
            (&snpp_only, &noaa_only)
        } else {
            (&noaa_only, &snpp_only)
        };
        let y = y_map.get(&ym).map_or(0.0, |c| c.total_frp);
        if let Some(mu) = mu_map.get(&ym)
            && eligible(mu, y)
        {
            push_row(&mut csv, country, "viirs", ym, mu, y);
            n_b += 1;
        }
    }

    let final_path = args.out.join(format!("{country}.csv"));
    let tmp = args.out.join(format!("{country}.csv.tmp"));
    std::fs::write(&tmp, format!("{CSV_HEADER}\n{csv}"))?;
    std::fs::rename(&tmp, &final_path)?;
    Ok((n_a, n_b))
}

fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    if args.fit {
        return fit::run(&args, &intended_countries());
    }
    std::fs::create_dir_all(&args.out)?;
    let cache = archive_cache_dir();
    let countries = if args.countries.is_empty() {
        default_countries(&cache)
    } else {
        args.countries.clone()
    };
    let todo: Vec<String> = countries
        .into_iter()
        .filter(|c| !args.out.join(format!("{c}.csv")).is_file())
        .collect();
    println!(
        "Archive {} | {} countries to do, {} threads, budget {} min\nOrder: {}",
        cache.display(),
        todo.len(),
        args.threads,
        args.max_minutes,
        todo.join(",")
    );

    let archive = ArchiveClient::offline(cache.clone());
    let cfg = PipelineConfig::default();
    let next = AtomicUsize::new(0);
    let heavy_lock = Mutex::new(());
    let t0 = Instant::now();
    let budget_secs = args.max_minutes * 60.0;

    std::thread::scope(|s| {
        for _ in 0..args.threads {
            s.spawn(|| {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                loop {
                    if t0.elapsed().as_secs_f64() > budget_secs {
                        return;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(country) = todo.get(i) else { return };
                    let heavy = modis_size_2015(&cache, country).unwrap_or(0) > HEAVY_BYTES;
                    let _guard = heavy.then(|| heavy_lock.lock().unwrap_or_else(|e| e.into_inner()));
                    let c0 = Instant::now();
                    match rt.block_on(extract_country(&archive, &cfg, country, &args)) {
                        Ok((a, b)) => println!(
                            "[{:.1} min] {country}: era A {a} rows, era B {b} rows ({:.1}s)",
                            t0.elapsed().as_secs_f64() / 60.0,
                            c0.elapsed().as_secs_f64()
                        ),
                        Err(e) => eprintln!("[skip] {country}: {e:#}"),
                    }
                }
            });
        }
    });
    println!("Done in {:.1} min", t0.elapsed().as_secs_f64() / 60.0);
    Ok(())
}

mod fit {
    use super::{Args, G12_COUNTRIES};
    use anyhow::{Context, Result, bail};
    use pyrosync_core::config::PipelineConfig;
    use pyrosync_core::conformal::{
        ConformalEra, ConformalSample, ConformalTable, calibration_fingerprint, conformal_interval,
        fit_mondrian,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::fmt::Write as _;

    const MIN_ERA_SAMPLES: usize = 200;
    const ERAS: [ConformalEra; 2] = [ConformalEra::PreViirs, ConformalEra::Viirs];

    struct Row {
        country: String,
        year: i32,
        sample: ConformalSample,
        analytic: Option<(f64, f64)>,
    }

    /// One held-out evaluation: which bin served it and whether each band covered `y`.
    struct Eval {
        year: i32,
        bin: usize,
        cov68: bool,
        cov90: bool,
        analytic: Option<bool>,
        rel_width90: f64,
        resid: f64,
    }

    type Checks = Vec<(String, bool)>;

    fn era_name(e: ConformalEra) -> &'static str {
        match e {
            ConformalEra::PreViirs => "pre_viirs",
            ConformalEra::Viirs => "viirs",
        }
    }

    fn read_rows(dir: &std::path::Path) -> Result<(Vec<Row>, Vec<String>)> {
        let mut rows = Vec::new();
        let mut countries = Vec::new();
        for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
            let path = entry?.path();
            if path.extension().is_none_or(|e| e != "csv") {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let mut name = String::new();
            for line in text.lines().skip(1) {
                let f: Vec<&str> = line.split(',').collect();
                if f.len() < 9 {
                    continue;
                }
                let era = match f[1] {
                    "pre_viirs" => ConformalEra::PreViirs,
                    "viirs" => ConformalEra::Viirs,
                    _ => continue,
                };
                let (Ok(year), Ok(mu), Ok(y)) = (f[2].parse(), f[4].parse(), f[5].parse()) else {
                    continue;
                };
                let analytic = match (f[6].parse::<f64>(), f[7].parse::<f64>()) {
                    (Ok(l), Ok(u)) => Some((l, u)),
                    _ => None,
                };
                name = f[0].to_string();
                rows.push(Row {
                    country: name.clone(),
                    year,
                    sample: ConformalSample { era, mu, y },
                    analytic,
                });
            }
            if !name.is_empty() {
                countries.push(name);
            }
        }
        countries.sort();
        Ok((rows, countries))
    }

    fn evaluate(cal: &[ConformalSample], held: &[&Row], era: ConformalEra, out: &mut Vec<Eval>) {
        let table = fit_mondrian(cal);
        for r in held {
            let Some(b) = conformal_interval(&table, era, r.sample.mu) else {
                continue;
            };
            let y = r.sample.y;
            out.push(Eval {
                year: r.year,
                bin: b.bin,
                cov68: b.p68_lower <= y && y <= b.p68_upper,
                cov90: b.p90_lower <= y && y <= b.p90_upper,
                analytic: r.analytic.map(|(l, u)| l <= y && y <= u),
                rel_width90: (b.p90_upper - b.p90_lower) / r.sample.mu,
                resid: (y - r.sample.mu) / r.sample.mu,
            });
        }
    }

    fn folds(rows: &[Row], era: ConformalEra, by_year: bool) -> Vec<Eval> {
        let era_rows: Vec<&Row> = rows.iter().filter(|r| r.sample.era == era).collect();
        let key = |r: &Row| {
            if by_year {
                r.year.to_string()
            } else {
                r.country.clone()
            }
        };
        let mut keys: Vec<String> = era_rows.iter().map(|r| key(r)).collect();
        keys.sort();
        keys.dedup();
        let mut out = Vec::new();
        for k in keys {
            let cal: Vec<ConformalSample> = era_rows
                .iter()
                .filter(|r| key(r) != k)
                .map(|r| r.sample)
                .collect();
            let held: Vec<&Row> = era_rows.iter().filter(|r| key(r) == k).copied().collect();
            evaluate(&cal, &held, era, &mut out);
        }
        out
    }

    fn rate(evals: &[&Eval], f: impl Fn(&Eval) -> Option<bool>) -> Option<f64> {
        let v: Vec<bool> = evals.iter().filter_map(|e| f(e)).collect();
        (!v.is_empty()).then(|| v.iter().filter(|&&b| b).count() as f64 / v.len() as f64)
    }

    fn quantile(sorted: &[f64], q: f64) -> Option<f64> {
        if sorted.is_empty() {
            return None;
        }
        let pos = q * (sorted.len() - 1) as f64;
        let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
        Some(sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64))
    }

    fn median(mut v: Vec<f64>) -> Option<f64> {
        v.sort_by(f64::total_cmp);
        quantile(&v, 0.5)
    }

    struct Rng(u64);
    impl Rng {
        fn new(seed: u64) -> Self {
            let mut z = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xBF58_476D_1CE4_E5B9;
            z = (z ^ (z >> 30)).wrapping_mul(0x94D0_49BB_1331_11EB);
            Self(z | 1)
        }
        fn index(&mut self, n: usize) -> usize {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            (x % n as u64) as usize
        }
    }

    /// Year-block bootstrap of the pooled coverage; 5th/95th percentile per level.
    fn bootstrap(evals: &[Eval], b: usize, seed: u64) -> Value {
        let mut years: BTreeMap<i32, (usize, usize, usize)> = BTreeMap::new();
        for e in evals {
            let t = years.entry(e.year).or_default();
            t.0 += 1;
            t.1 += usize::from(e.cov68);
            t.2 += usize::from(e.cov90);
        }
        let blocks: Vec<(usize, usize, usize)> = years.into_values().collect();
        if blocks.is_empty() {
            return Value::Null;
        }
        let mut rng = Rng::new(seed);
        let (mut c68, mut c90) = (Vec::with_capacity(b), Vec::with_capacity(b));
        for _ in 0..b {
            let (mut n, mut a, mut c) = (0, 0, 0);
            for _ in 0..blocks.len() {
                let t = blocks[rng.index(blocks.len())];
                n += t.0;
                a += t.1;
                c += t.2;
            }
            c68.push(a as f64 / n as f64);
            c90.push(c as f64 / n as f64);
        }
        c68.sort_by(f64::total_cmp);
        c90.sort_by(f64::total_cmp);
        json!({
            "b": b, "seed": seed, "n_year_blocks": blocks.len(),
            "ci90_cov68": [quantile(&c68, 0.05), quantile(&c68, 0.95)],
            "ci90_cov90": [quantile(&c90, 0.05), quantile(&c90, 0.95)],
        })
    }

    fn summarize(evals: &[Eval]) -> Value {
        let all: Vec<&Eval> = evals.iter().collect();
        let mut widths: Vec<f64> = evals.iter().map(|e| e.rel_width90).collect();
        widths.sort_by(f64::total_cmp);
        let mut bins: BTreeMap<usize, Vec<&Eval>> = BTreeMap::new();
        for e in evals {
            bins.entry(e.bin).or_default().push(e);
        }
        let per_bin: Vec<Value> = bins
            .iter()
            .map(|(bin, v)| {
                json!({
                    "bin": bin, "n": v.len(),
                    "cov68": rate(v, |e| Some(e.cov68)),
                    "cov90": rate(v, |e| Some(e.cov90)),
                    "analytic": rate(v, |e| e.analytic),
                })
            })
            .collect();
        json!({
            "n": evals.len(),
            "cov68": rate(&all, |e| Some(e.cov68)),
            "cov90": rate(&all, |e| Some(e.cov90)),
            "analytic": rate(&all, |e| e.analytic),
            "n_analytic": evals.iter().filter(|e| e.analytic.is_some()).count(),
            "median_rel_width90": quantile(&widths, 0.5),
            "per_bin": per_bin,
        })
    }

    fn pct(v: Option<f64>) -> String {
        v.map_or("-".into(), |x| format!("{:.1}%", x * 100.0))
    }

    fn within(v: Option<f64>, lo: f64, hi: f64) -> bool {
        v.is_some_and(|x| x >= lo && x <= hi)
    }

    fn acceptance(loyo: &Value, loco: &Value, boot: &Value) -> (Checks, bool) {
        let mut checks: Checks = Vec::new();
        for (name, s) in [("LOYO", loyo), ("LOCO", loco)] {
            let c68 = s["cov68"].as_f64();
            let c90 = s["cov90"].as_f64();
            checks.push((
                format!("{name} havuz %68 kapsaması {} in [64,0%; 76,0%]", pct(c68)),
                within(c68, 0.64, 0.76),
            ));
            checks.push((
                format!("{name} havuz %90 kapsaması {} in [87,0%; 96,0%]", pct(c90)),
                within(c90, 0.87, 0.96),
            ));
        }
        let lo90 = boot["ci90_cov90"][0].as_f64();
        checks.push((
            format!("bootstrap %90 bandı GA alt ucu {} >= 85,0%", pct(lo90)),
            lo90.is_some_and(|x| x >= 0.85),
        ));
        let lo68 = boot["ci90_cov68"][0].as_f64();
        checks.push((
            format!("bootstrap %68 bandı GA alt ucu {} >= 63,0%", pct(lo68)),
            lo68.is_some_and(|x| x >= 0.63),
        ));
        let mut bad = String::new();
        for b in loyo["per_bin"].as_array().into_iter().flatten() {
            if b["n"].as_u64().unwrap_or(0) >= 100 {
                let ok = b["cov68"].as_f64().is_some_and(|x| x >= 0.60)
                    && b["cov90"].as_f64().is_some_and(|x| x >= 0.82);
                if !ok {
                    let _ = write!(bad, " kutu {}", b["bin"]);
                }
            }
        }
        checks.push((
            format!(
                "n>=100 her kutuda kapsama >= nominal-0,08 (LOYO){}",
                if bad.is_empty() { String::new() } else { format!(", ihlal:{bad}") }
            ),
            bad.is_empty(),
        ));
        let w = loyo["median_rel_width90"].as_f64();
        checks.push((
            format!(
                "%90 bandın medyan göreli genişliği {} < 3",
                w.map_or("-".into(), |x| format!("{x:.2}"))
            ),
            w.is_some_and(|x| x < 3.0),
        ));
        let ok = checks.iter().all(|c| c.1);
        (checks, ok)
    }

    pub fn run(args: &Args, intended: &[String]) -> Result<()> {
        let (rows, countries) = read_rows(&args.pairs)?;
        if countries.len() < args.min_countries {
            bail!(
                "only {} countries in {} (--min-countries {})",
                countries.len(),
                args.pairs.display(),
                args.min_countries
            );
        }
        let missing: Vec<&String> = intended.iter().filter(|c| !countries.contains(c)).collect();
        let g12_missing: Vec<&str> = G12_COUNTRIES
            .iter()
            .copied()
            .filter(|c| !countries.iter().any(|x| x == c))
            .collect();
        println!("{} countries, {} rows", countries.len(), rows.len());

        let all_samples: Vec<ConformalSample> = rows.iter().map(|r| r.sample).collect();
        let mut table: ConformalTable = fit_mondrian(&all_samples);
        table.calibration_fingerprint = calibration_fingerprint(&PipelineConfig::default());

        let mut eras = serde_json::Map::new();
        let mut verdicts: Vec<(ConformalEra, Checks, bool)> = Vec::new();
        for era in ERAS {
            let n = rows.iter().filter(|r| r.sample.era == era).count();
            if n < MIN_ERA_SAMPLES {
                eprintln!("WARNING: era {} has only {n} samples (< {MIN_ERA_SAMPLES})", era_name(era));
            }
            let loyo_e = folds(&rows, era, true);
            let loco_e = folds(&rows, era, false);
            let loyo = summarize(&loyo_e);
            let loco = summarize(&loco_e);
            let boot = bootstrap(&loyo_e, args.bootstrap, args.seed);
            let ratios: Vec<f64> = rows
                .iter()
                .filter(|r| r.sample.era == era)
                .map(|r| r.sample.y / r.sample.mu)
                .collect();
            let resid_med = median(loyo_e.iter().map(|e| e.resid).collect());
            let (checks, ok) = acceptance(&loyo, &loco, &boot);
            println!(
                "era {}: n={n} loyo68={} loyo90={} loco68={} loco90={} pass={ok}",
                era_name(era),
                pct(loyo["cov68"].as_f64()),
                pct(loyo["cov90"].as_f64()),
                pct(loco["cov68"].as_f64()),
                pct(loco["cov90"].as_f64()),
            );
            eras.insert(
                era_name(era).to_string(),
                json!({
                    "n_samples": n,
                    "median_y_over_mu": median(ratios),
                    "median_residual": resid_med,
                    "loyo": loyo, "loco": loco, "bootstrap_loyo": boot,
                    "acceptance_pass": ok,
                }),
            );
            verdicts.push((era, checks, ok));
        }

        table.coverage = Some(json!({
            "countries": countries,
            "n_countries": countries.len(),
            "missing_intended_countries": missing,
            "eras": Value::Object(eras.clone()),
            "scope": "sensor-dışarıda tekrarlanabilirlik bandı; gerçek FRP doğruluğu değildir",
        }));
        table.notes = Some(
            "Mondrian split conformal; era A: MODIS-only emulation vs full pipeline, era B: SNPP vs NOAA-20 (parity direction). \
             Transfer to 2002-2011 / 2012-2017 is an assumption, not measured."
                .into(),
        );
        if let Some(p) = args.json_out.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&args.json_out, serde_json::to_string_pretty(&table)? + "\n")?;

        let doc = render_doc(&table, &countries, &missing, &g12_missing, &verdicts, &eras);
        if let Some(p) = args.doc_out.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&args.doc_out, doc)?;
        println!("wrote {} and {}", args.json_out.display(), args.doc_out.display());
        Ok(())
    }

    fn cov_table(s: &Value) -> String {
        let mut t = String::from(
            "| Kutu | n | %68 kapsama | %90 kapsama | Analitik bant |\n|---|---|---|---|---|\n",
        );
        for b in s["per_bin"].as_array().into_iter().flatten() {
            let _ = writeln!(
                t,
                "| {} | {} | {} | {} | {} |",
                b["bin"],
                b["n"],
                pct(b["cov68"].as_f64()),
                pct(b["cov90"].as_f64()),
                pct(b["analytic"].as_f64())
            );
        }
        let _ = writeln!(
            t,
            "| **Havuz** | {} | {} | {} | {} |",
            s["n"],
            pct(s["cov68"].as_f64()),
            pct(s["cov90"].as_f64()),
            pct(s["analytic"].as_f64())
        );
        t
    }

    fn render_doc(
        table: &ConformalTable,
        countries: &[String],
        missing: &[&String],
        g12_missing: &[&str],
        verdicts: &[(ConformalEra, Checks, bool)],
        eras: &serde_json::Map<String, Value>,
    ) -> String {
        let mut d = String::new();
        let _ = writeln!(d, "# Konformal Kapsama Raporu (S10)\n");
        let _ = writeln!(
            d,
            "Bu belge `data/conformal.json` tablosunun nasıl kalibre edildiğini ve ölçülen kapsamayı verir. \
             Tablo `calibration_fingerprint = {}` taşır. Üretici: `crates/firms-client/examples/conformal_calibration.rs` (`extract` ve `fit`).\n",
            table.calibration_fingerprint
        );
        let _ = writeln!(d, "## 1. Bu bant neyi ölçer\n");
        let _ = writeln!(
            d,
            "Gerçek yayılan enerji (FRP) için bir referans doğru yoktur. Bantlar bir **sensör-dışarıda (held-out-sensor) tekrarlanabilirlik** ölçüsüdür; ölçüm doğruluğu veya doğrulanmış ground-truth kapsaması değildir.\n\n\
             - **Dönem A (`pre_viirs`):** μ = yalnız MODIS girdisiyle yeniden koşulan pipeline'ın (Terra+Aqua, `k_c` düzeltmeli) ülke/ay `total_frp`'si; y = aynı ay için tam MODIS+VIIRS pipeline `total_frp`'si. Aylar 2012-02..2025-12.\n\
             - **Dönem B (`viirs`):** μ ve y, SNPP-only ve NOAA-20-only `total_frp`'dir; yön (ülke, ay) hash paritesiyle sabittir. Aylar 2018-05..2025-12.\n\
             - Kalıntı `e = (y-μ)/μ`; bölme = (dönem, μ büyüklük kutusu); aralık = eşit-kuyruklu ampirik kantil (%68, %90). Kenar ve kantiller yalnız kalibrasyon katında hesaplanır.\n"
        );
        let _ = writeln!(d, "## 2. Kullanılan ülkeler\n");
        let _ = writeln!(d, "{} ülke: {}.\n", countries.len(), countries.join(", "));
        if g12_missing.is_empty() {
            let _ = writeln!(d, "G12'nin 10 ülkesinin tamamı dahildir.\n");
        } else {
            let _ = writeln!(
                d,
                "**Eksik G12 (ağır) ülkeler:** {}. Koşu zaman bütçesiyle kesildi; en büyük μ kutuları bu yüzden zayıf temsil edilmiş olabilir.\n",
                g12_missing.join(", ")
            );
        }
        if !missing.is_empty() {
            let _ = writeln!(
                d,
                "Planlanıp tamamlanmayan ülkeler: {}.\n",
                missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            );
        }
        let _ = writeln!(d, "## 3. Tablo: örnek sayıları ve kutular\n");
        let o = |v: Option<f64>| v.map_or("-".into(), |x| format!("{x:.3}"));
        for era in ERAS {
            let n = eras[era_name(era)]["n_samples"].as_u64().unwrap_or(0);
            let _ = writeln!(d, "### Dönem `{}` (n = {n})\n", era_name(era));
            let _ = writeln!(
                d,
                "| Kutu | μ alt (MW) | μ üst (MW) | n_cal | q68 alt | q68 üst | q90 alt | q90 üst |\n|---|---|---|---|---|---|---|---|"
            );
            for e in table.entries.iter().filter(|e| e.era == era) {
                let _ = writeln!(
                    d,
                    "| {} | {} | {} | {} | {} | {} | {} | {} |",
                    e.bin,
                    o(e.mu_lo),
                    o(e.mu_hi),
                    e.n_cal,
                    o(e.q68_lower),
                    o(e.q68_upper),
                    o(e.q90_lower),
                    o(e.q90_upper)
                );
            }
            let _ = writeln!(d);
        }
        let _ = writeln!(d, "## 4. Ölçülen kapsama\n");
        for era in ERAS {
            let e = &eras[era_name(era)];
            let _ = writeln!(d, "### Dönem `{}`\n", era_name(era));
            let _ = writeln!(
                d,
                "Medyan y/μ = {:.3}; medyan işaretli kalıntı = {}.\n",
                e["median_y_over_mu"].as_f64().unwrap_or(f64::NAN),
                pct(e["median_residual"].as_f64())
            );
            let _ = writeln!(d, "**LOYO (yıl bloğu dışarıda):**\n\n{}", cov_table(&e["loyo"]));
            let _ = writeln!(d, "**LOCO (ülke bloğu dışarıda):**\n\n{}", cov_table(&e["loco"]));
            let b = &e["bootstrap_loyo"];
            let _ = writeln!(
                d,
                "**Yıl-bloklu bootstrap** (B={}, seed={}, {} yıl bloğu), %90 GA: %68 bandı [{}, {}], %90 bandı [{}, {}].\n",
                b["b"],
                b["seed"],
                b["n_year_blocks"],
                pct(b["ci90_cov68"][0].as_f64()),
                pct(b["ci90_cov68"][1].as_f64()),
                pct(b["ci90_cov90"][0].as_f64()),
                pct(b["ci90_cov90"][1].as_f64()),
            );
        }
        let _ = writeln!(d, "## 5. Kabul ölçütleri\n");
        let _ = writeln!(
            d,
            "Ölçütler tasarımdan (§5) alınmıştır; seviyeler kapsama sonucuna bakılarak ayarlanmamıştır.\n"
        );
        for (era, checks, ok) in verdicts {
            let _ = writeln!(
                d,
                "### Dönem `{}`: **{}**\n",
                era_name(*era),
                if *ok { "PASS" } else { "FAIL" }
            );
            for (txt, ok) in checks {
                let _ = writeln!(d, "- {}: {}", if *ok { "PASS" } else { "FAIL" }, txt);
            }
            let _ = writeln!(d);
        }
        let _ = writeln!(d, "## 6. Analitik bant ile karşılaştırma\n");
        let _ = writeln!(
            d,
            "Aynı hücrelerde, aynı `y` için μ hücresinin analitik `total_frp_lower/upper` bandının kapsaması (yan rapor, geçti/kaldı değildir):\n"
        );
        let _ = writeln!(
            d,
            "| Dönem | Analitik bant | Conformal %68 | Conformal %90 |\n|---|---|---|---|"
        );
        for era in ERAS {
            let s = &eras[era_name(era)]["loyo"];
            let _ = writeln!(
                d,
                "| `{}` | {} | {} | {} |",
                era_name(era),
                pct(s["analytic"].as_f64()),
                pct(s["cov68"].as_f64()),
                pct(s["cov90"].as_f64())
            );
        }
        let _ = writeln!(
            d,
            "\nAnalitik bandın nominal seviyesi yaklaşık 1σ'dır; kıyas yön göstericidir. Analitik bant zaten nominalin üstünde çıkarsa S10'un öncülü bizim veride çürümüş demektir.\n"
        );
        let _ = writeln!(d, "## 7. Dürüst sınırlamalar\n");
        let _ = writeln!(
            d,
            "- **Doğruluk değil tekrarlanabilirlik:** bant gerçek FRP'ye karşı doğrulanmamıştır. \"Doğrulanmış kapsama\" denemez; doğrusu \"sensör-dışarıda tekrarlanabilirlik bandı, yıl-dışarıda kapsama\"dır.\n\
             - **Dönem A yanlılığı:** dönem A'da medyan y/μ = {:.2} (Türkiye ön ölçümünde ortalama 1,78). Yalnız MODIS + `k_c` ile kurulan toplam, VIIRS'li tam pipeline'a göre sistematik olarak düşük kalır (VIIRS ek tespit ekler); bu yüzden bant asimetriktir ve üst yöne kaymıştır. İşaretli kantil bunu yakalar ama bant geniştir.\n\
             - **Aktarım varsayımdır:** 2002-07..2011 (dönem A) ve 2012..2017 (tek VIIRS, dönem B) için kapsama ölçülmedi; ölçüm 2012-2025 ve 2018-2025'te yapıldı. Yörünge kayması ve farklı yangın rejimi durağanlığı sınanamaz.\n\
             - **Dönem B tek-filo çiftidir:** kaynaşık (MODIS+2×VIIRS) hücrenin gerçek hatasını ölçmez; tek-filo kopyası kabaca √2 geniş olduğundan muhafazakârdır.\n\
             - **Marjinal garanti:** değiştirilebilirlik varsayımı yıl şokları ve komşu ülke korelasyonuyla bozulur; LOYO ve LOCO bunu ölçmek içindir.\n\
             - **`k_c` kısmen in-sample:** 2019 fit / 2020 hold-out nedeniyle bu yıllar dönem A için tam bağımsız değildir.\n\
             - **Ülke örneklemi:** küçük ve orta ülkeler küçük-μ kutularını, G12 ülkeleri büyük kutuları besler; eksik ağır ülkeler üst kutuyu zayıflatır.\n\
             - **Kapsam:** yalnız ülke toplamı `total_frp` içindir; bbox veya `min_confidence` süzgeçli sorgulara uygulanmaz.\n\
             - **Merkez değişimi:** R10/K11 gibi `total_frp` merkezini değiştiren değişiklikler `calibration_fingerprint`'i bozar; tablo yeniden üretilmelidir.\n",
            eras["pre_viirs"]["median_y_over_mu"].as_f64().unwrap_or(f64::NAN)
        );
        d
    }
}
