//! Y8 case studies from the local FIRMS country archive (read-only): Amazon 2019-2021 anomaly and
//! India city smoke exposure, each as raw FIRMS rows vs the PyroSync harmonised series.
//!
//! Amazon: Brazil, Legal Amazon bbox and its forest-biome cells, 2012-2021, anomaly vs the
//! 2012-2018 mean on raw counts, `count_normalized` and `total_frp` (same calendar calls as C11).
//! India: 20x20 km boxes around GeoNames cities (pop > 100k), 2019-2023, raw vs harmonised counts,
//! FRP, VIIRS/MODIS ratio and the share of < 1 MW detections.
//!
//! Usage: cargo run --release -p pyrosync-firms --example amazon_india_case [amazon|india|all] [out-dir]
//! (default out-dir `docs/vaka`; archive from ARCHIVE_CACHE_DIR or `.cache/firms-archive`;
//! cities from `.cache/geonames/cities15000.txt`, GeoNames dump, public).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use chrono::Datelike;
use pyrosync_core::biome::{Biome, biome_at};
use pyrosync_core::calendar::{build_calendar_with, compute_anomalies};
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::harmonize::harmonize_with_links_config;
use pyrosync_core::models::{CalendarCell, HarmonizedHotspot, Hotspot, HotspotSource, Sensor};
use pyrosync_firms::parse::parse_csv;
use pyrosync_firms::{FirmsSource, Processing};

const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const CITIES_FILE: &str = ".cache/geonames/cities15000.txt";

const SENSOR_FILES: [(&str, Sensor); 3] = [
    ("modis", Sensor::ModisTerra),
    ("viirs-snpp", Sensor::ViirsSNPP),
    ("viirs-jpss1", Sensor::ViirsNOAA20),
];

fn archive_dir() -> PathBuf {
    match std::env::var_os("ARCHIVE_CACHE_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR),
    }
}

/// Type-0 rows of one sensor file, as `/api/series` and `/api/calendar` keep them.
fn load(dir: &Path, country: &str, sensor_dir: &str, sensor: Sensor, year: i32) -> Result<Vec<Hotspot>> {
    let path = dir
        .join(sensor_dir)
        .join(year.to_string())
        .join(format!("{sensor_dir}_{year}_{country}.csv"));
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
    let mut rows = parse_csv(&text, FirmsSource::new(sensor, Processing::Sp).as_str())
        .with_context(|| path.display().to_string())?;
    rows.retain(|h| h.fire_type.is_wildfire());
    Ok(rows)
}

fn load_year(dir: &Path, country: &str, year: i32) -> Result<[Vec<Hotspot>; 3]> {
    let [a, b, c] = SENSOR_FILES.map(|(d, s)| load(dir, country, d, s, year));
    Ok([a?, b?, c?])
}

fn pct(from: f64, to: f64) -> f64 {
    if from > 0.0 { (to / from - 1.0) * 100.0 } else { f64::NAN }
}

fn num(v: f64) -> String {
    if v.is_finite() { format!("{v:.4}") } else { "null".into() }
}

// ---------------------------------------------------------------- Amazon

const BRAZIL: &str = "Brazil";
const A_FROM: i32 = 2012;
const A_TO: i32 = 2021;
const BASE: (i32, i32) = (2012, 2018);
const ANOM: (i32, i32) = (2019, 2021);
/// Legal Amazon envelope [west, south, east, north]; includes cerrado transition cells, hence the
/// second, forest-biome-only region.
const LEGAL_AMAZON: [f64; 4] = [-74.0, -18.0, -44.0, 5.3];
/// S-NPP is present from the first year; NOAA-20's first Brazil row is 2018-04-01, so Ω is pooled
/// per era exactly as in C11 (one platform configuration per era).
const ERAS: [((i32, u32), (i32, u32)); 2] = [((2012, 1), (2018, 3)), ((2018, 4), (2021, 12))];

struct Region {
    id: &'static str,
    label: &'static str,
    forest_only: bool,
}

const REGIONS: [Region; 2] = [
    Region { id: "legal_amazon", label: "Legal Amazon (bbox)", forest_only: false },
    Region { id: "amazon_forest", label: "Legal Amazon, forest-biome cells", forest_only: true },
];

impl Region {
    fn contains(&self, h: &Hotspot) -> bool {
        h.lon >= LEGAL_AMAZON[0]
            && h.lat >= LEGAL_AMAZON[1]
            && h.lon <= LEGAL_AMAZON[2]
            && h.lat <= LEGAL_AMAZON[3]
            && (!self.forest_only || biome_at(h.lat, h.lon) == Some(Biome::Forest))
    }
}

/// Index 0 = MODIS, 1 = S-NPP, 2 = NOAA-20.
#[derive(Default, Clone, Copy)]
struct RawMonth {
    count: [u64; 3],
    frp: [f64; 3],
}

#[derive(Default)]
struct RegionResult {
    raw: BTreeMap<(i32, u32), RawMonth>,
    harmonized_year: BTreeMap<i32, usize>,
    static_masked: BTreeMap<i32, usize>,
    cells: Vec<CalendarCell>,
    omegas: Vec<(String, Option<f64>)>,
}

fn in_era(h: &HarmonizedHotspot, era: ((i32, u32), (i32, u32))) -> bool {
    let ym = (h.acquired.year(), h.acquired.month());
    ym >= era.0 && ym <= era.1
}

fn run_amazon(dir: &Path, config: &PipelineConfig) -> Result<Vec<RegionResult>> {
    let mut results: Vec<RegionResult> = REGIONS.iter().map(|_| RegionResult::default()).collect();
    for era in ERAS {
        let mut harmonized: Vec<Vec<HarmonizedHotspot>> = REGIONS.iter().map(|_| Vec::new()).collect();
        for year in era.0.0..=era.1.0 {
            let t = Instant::now();
            let rows = load_year(dir, BRAZIL, year)?;
            for (ri, region) in REGIONS.iter().enumerate() {
                let out = &mut results[ri];
                let [modis, snpp, noaa20] = rows.each_ref().map(|v| {
                    v.iter().filter(|h| region.contains(h)).cloned().collect::<Vec<_>>()
                });
                let first_visit = !out.harmonized_year.contains_key(&year);
                if first_visit {
                    for (i, set) in [&modis, &snpp, &noaa20].into_iter().enumerate() {
                        for h in set {
                            let m = out.raw.entry((year, h.acquired.month())).or_default();
                            m.count[i] += 1;
                            m.frp[i] += h.frp;
                        }
                    }
                }
                let mut viirs = snpp;
                viirs.extend(noaa20);
                let links = harmonize_with_links_config(modis, viirs, config);
                if first_visit {
                    out.harmonized_year.insert(year, links.hotspots.len());
                    out.static_masked.insert(year, links.static_masked);
                }
                harmonized[ri].extend(links.hotspots.into_iter().filter(|h| in_era(h, era)));
            }
            eprintln!("amazon {year}: {:.1}s", t.elapsed().as_secs_f64());
        }
        for (ri, hs) in harmonized.iter().enumerate() {
            let platforms: BTreeSet<(bool, Vec<Sensor>)> = {
                let mut per_month: BTreeMap<(i32, u32), (bool, BTreeSet<Sensor>)> = BTreeMap::new();
                for h in hs {
                    let k = per_month.entry((h.acquired.year(), h.acquired.month())).or_default();
                    k.0 |= h.platforms.iter().any(|s| s.is_modis());
                    k.1.extend(h.platforms.iter().copied().filter(|s| s.is_viirs()));
                }
                per_month.into_values().map(|(m, v)| (m, v.into_iter().collect())).collect()
            };
            if platforms.len() != 1 {
                eprintln!("warning: {} era {:?}-{:?} mixes {} platform configurations", REGIONS[ri].id, era.0, era.1, platforms.len());
            }
            let cells = build_calendar_with(hs, &config.uncertainty, &config.calibration);
            let omega = cells.iter().find_map(|c| c.effort_omega);
            results[ri].omegas.push((format!("{}-{:02}..{}-{:02}", era.0.0, era.0.1, era.1.0, era.1.1), omega));
            results[ri].cells.extend(cells);
        }
    }
    for r in &mut results {
        compute_anomalies(&mut r.cells);
    }
    Ok(results)
}

#[derive(Clone, Copy, Default)]
struct AmazonYear {
    year: i32,
    raw_count: [u64; 3],
    raw_frp: [f64; 3],
    harmonized: usize,
    count_normalized: f64,
    total_frp: f64,
    total_frp_lower: f64,
    total_frp_upper: f64,
    /// Jul-Oct, the fire season.
    season_raw_modis: u64,
    season_raw_total: u64,
    season_count_normalized: f64,
}

fn amazon_years(r: &RegionResult) -> Vec<AmazonYear> {
    (A_FROM..=A_TO)
        .map(|year| {
            let mut a = AmazonYear { year, harmonized: r.harmonized_year.get(&year).copied().unwrap_or(0), ..Default::default() };
            for ((_, m), raw) in r.raw.range((year, 1)..=(year, 12)) {
                for i in 0..3 {
                    a.raw_count[i] += raw.count[i];
                    a.raw_frp[i] += raw.frp[i];
                }
                if (7..=10).contains(m) {
                    a.season_raw_modis += raw.count[0];
                    a.season_raw_total += raw.count.iter().sum::<u64>();
                }
            }
            for c in r.cells.iter().filter(|c| c.year == year) {
                a.count_normalized += c.count_normalized.unwrap_or(0.0);
                a.total_frp += c.total_frp;
                a.total_frp_lower += c.total_frp_lower.unwrap_or(c.total_frp);
                a.total_frp_upper += c.total_frp_upper.unwrap_or(c.total_frp);
                if (7..=10).contains(&c.month) {
                    a.season_count_normalized += c.count_normalized.unwrap_or(0.0);
                }
            }
            a
        })
        .collect()
}

type Bar = (&'static str, &'static str, fn(&AmazonYear) -> f64);
type Metric = (&'static str, fn(&AmazonYear) -> f64);

const METRICS: [Metric; 7] = [
    ("raw_total_count", |a| a.raw_count.iter().sum::<u64>() as f64),
    ("raw_modis_count", |a| a.raw_count[0] as f64),
    ("raw_total_frp", |a| a.raw_frp.iter().sum()),
    ("count_normalized", |a| a.count_normalized),
    ("total_frp", |a| a.total_frp),
    ("season_raw_total_count", |a| a.season_raw_total as f64),
    ("season_count_normalized", |a| a.season_count_normalized),
];

fn base_mean(y: &[AmazonYear], f: fn(&AmazonYear) -> f64) -> f64 {
    let v: Vec<f64> = y.iter().filter(|a| (BASE.0..=BASE.1).contains(&a.year)).map(f).collect();
    v.iter().sum::<f64>() / v.len() as f64
}

fn base_sd(y: &[AmazonYear], f: fn(&AmazonYear) -> f64) -> f64 {
    let m = base_mean(y, f);
    let v: Vec<f64> = y.iter().filter(|a| (BASE.0..=BASE.1).contains(&a.year)).map(f).collect();
    (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64).sqrt()
}

fn write_amazon(out: &Path, results: &[RegionResult]) -> Result<Vec<Vec<AmazonYear>>> {
    let mut monthly = String::from(
        "region,year,month,raw_modis_count,raw_modis_frp_mw,raw_snpp_count,raw_snpp_frp_mw,raw_noaa20_count,raw_noaa20_frp_mw,raw_total_count,harmonized_count,effort_omega,count_normalized,total_frp_mw,total_frp_lower_mw,total_frp_upper_mw,anomaly_calendar\n",
    );
    let opt = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.4}"));
    for (region, r) in REGIONS.iter().zip(results) {
        for c in &r.cells {
            let raw = r.raw.get(&(c.year, c.month)).copied().unwrap_or_default();
            writeln!(
                monthly,
                "{},{},{},{},{:.2},{},{:.2},{},{:.2},{},{},{},{},{:.2},{},{},{}",
                region.id, c.year, c.month, raw.count[0], raw.frp[0], raw.count[1], raw.frp[1], raw.count[2], raw.frp[2],
                raw.count.iter().sum::<u64>(), c.count, opt(c.effort_omega), opt(c.count_normalized), c.total_frp,
                opt(c.total_frp_lower), opt(c.total_frp_upper), opt(c.anomaly),
            )?;
        }
    }
    std::fs::write(out.join("amazon_monthly.csv"), monthly)?;

    let all: Vec<Vec<AmazonYear>> = results.iter().map(amazon_years).collect();
    let mut annual = String::from(
        "region,year,raw_modis_count,raw_snpp_count,raw_noaa20_count,raw_total_count,raw_total_frp_mw,harmonized_count,static_masked,count_normalized,total_frp_mw,total_frp_lower_mw,total_frp_upper_mw,season_raw_total_count,season_raw_modis_count,season_count_normalized\n",
    );
    for ((region, r), years) in REGIONS.iter().zip(results).zip(&all) {
        for a in years {
            writeln!(
                annual,
                "{},{},{},{},{},{},{:.2},{},{},{:.2},{:.2},{:.2},{:.2},{},{},{:.2}",
                region.id, a.year, a.raw_count[0], a.raw_count[1], a.raw_count[2], a.raw_count.iter().sum::<u64>(),
                a.raw_frp.iter().sum::<f64>(), a.harmonized, r.static_masked.get(&a.year).copied().unwrap_or(0),
                a.count_normalized, a.total_frp, a.total_frp_lower, a.total_frp_upper,
                a.season_raw_total, a.season_raw_modis, a.season_count_normalized,
            )?;
        }
    }
    std::fs::write(out.join("amazon_annual.csv"), annual)?;

    let mut json = format!(
        "{{\"country\":\"{BRAZIL}\",\"years\":[{A_FROM},{A_TO}],\"baseline\":[{},{}],\"anomaly_years\":[{},{}],\"legal_amazon_bbox\":{:?},\"config\":\"PipelineConfig::default()\",\"regions\":[",
        BASE.0, BASE.1, ANOM.0, ANOM.1, LEGAL_AMAZON
    );
    for (i, ((region, r), years)) in REGIONS.iter().zip(results).zip(&all).enumerate() {
        if i > 0 {
            json.push(',');
        }
        let omegas = r.omegas.iter().map(|(k, v)| format!("\"{k}\":{}", v.map_or("null".into(), num))).collect::<Vec<_>>().join(",");
        write!(json, "{{\"id\":\"{}\",\"label\":\"{}\",\"omega\":{{{omegas}}},\"metrics\":{{", region.id, region.label)?;
        println!("== {} ==  Ω {:?}", region.label, r.omegas);
        println!("{:<24} {:>14} {:>9} {:>8} {:>8} {:>8} {:>8}", "metric", "base mean", "base sd", "2019 %", "2020 %", "2021 %", "z2019/20/21");
        for (j, (name, f)) in METRICS.iter().enumerate() {
            let m = base_mean(years, *f);
            let sd = base_sd(years, *f);
            let at = |y: i32| years.iter().find(|a| a.year == y).map_or(f64::NAN, f);
            let anom: Vec<f64> = (ANOM.0..=ANOM.1).map(|y| pct(m, at(y))).collect();
            let z: Vec<f64> = (ANOM.0..=ANOM.1).map(|y| (at(y) - m) / sd).collect();
            println!("{name:<24} {m:>14.1} {sd:>9.1} {:>+8.1} {:>+8.1} {:>+8.1}   {:+.2}/{:+.2}/{:+.2}", anom[0], anom[1], anom[2], z[0], z[1], z[2]);
            if j > 0 {
                json.push(',');
            }
            write!(
                json,
                "\"{name}\":{{\"baseline_mean\":{},\"baseline_sd\":{},\"anomaly_pct\":{{\"2019\":{},\"2020\":{},\"2021\":{}}},\"z\":{{\"2019\":{},\"2020\":{},\"2021\":{}}}}}",
                num(m), num(sd), num(anom[0]), num(anom[1]), num(anom[2]), num(z[0]), num(z[1]), num(z[2])
            )?;
        }
        json.push_str("}}");
    }
    json.push_str("]}\n");
    std::fs::write(out.join("amazon_summary.json"), json)?;
    Ok(all)
}

// ---------------------------------------------------------------- SVG

const BG: &str = "#0d0f13";
const TEXT: &str = "#f1f3f7";
const MUTED: &str = "#949cab";
const GRID: &str = "#222731";
const C_RAW: &str = "#d4d9e2";
const C_RAW2: &str = "#8d96a6";
const C_HARM: &str = "#3cc8f0";
const C_ALT: &str = "#f37819";

fn svg_open(w: f64, h: f64, title: &str, desc: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" font-family=\"Inter, 'Segoe UI', Helvetica, Arial, sans-serif\" role=\"img\" aria-labelledby=\"t d\">\n<title id=\"t\">{title}</title>\n<desc id=\"d\">{desc}</desc>\n<rect width=\"{w}\" height=\"{h}\" fill=\"{BG}\"/>\n"
    )
}

fn heading(s: &mut String, title: &str, sub: &str) {
    writeln!(s, "<text x=\"40\" y=\"44\" fill=\"{TEXT}\" font-size=\"24\" font-weight=\"600\">{title}</text>").ok();
    writeln!(s, "<text x=\"40\" y=\"68\" fill=\"{MUTED}\" font-size=\"14\">{sub}</text>").ok();
}

struct Series<'a> {
    label: &'a str,
    color: &'a str,
    dash: bool,
    values: Vec<f64>,
}

/// Panel with x categories, linear y axis, one polyline per series.
#[allow(clippy::too_many_arguments)]
fn line_panel(s: &mut String, x: f64, y: f64, w: f64, h: f64, title: &str, cats: &[String], series: &[Series], y_min: f64, y_max: f64, step: f64, suffix: &str, hline: Option<f64>) {
    let ymap = |v: f64| y + h - ((v - y_min) / (y_max - y_min)).clamp(0.0, 1.0) * h;
    let xmap = |i: usize| x + (i as f64 + 0.5) / cats.len() as f64 * w;
    writeln!(s, "<text x=\"{x}\" y=\"{}\" fill=\"{TEXT}\" font-size=\"16\" font-weight=\"600\">{title}</text>", y - 14.0).ok();
    let mut t = (y_min / step).ceil() * step;
    while t <= y_max + 1e-9 {
        writeln!(s, "<line x1=\"{x}\" x2=\"{x2}\" y1=\"{yy:.1}\" y2=\"{yy:.1}\" stroke=\"{GRID}\"/><text x=\"{}\" y=\"{:.1}\" fill=\"{MUTED}\" font-size=\"12\" text-anchor=\"end\">{t:.0}{suffix}</text>", x - 6.0, ymap(t) + 4.0, x2 = x + w, yy = ymap(t)).ok();
        t += step;
    }
    if let Some(v) = hline {
        writeln!(s, "<line x1=\"{x}\" x2=\"{x2}\" y1=\"{yy:.1}\" y2=\"{yy:.1}\" stroke=\"{MUTED}\" stroke-dasharray=\"2 4\"/>", x2 = x + w, yy = ymap(v)).ok();
    }
    for (i, c) in cats.iter().enumerate() {
        writeln!(s, "<text x=\"{:.1}\" y=\"{}\" fill=\"{MUTED}\" font-size=\"12\" text-anchor=\"middle\">{c}</text>", xmap(i), y + h + 18.0).ok();
    }
    for se in series {
        let pts: Vec<String> = se.values.iter().enumerate().filter(|(_, v)| v.is_finite()).map(|(i, v)| format!("{:.1},{:.1}", xmap(i), ymap(*v))).collect();
        let dash = if se.dash { " stroke-dasharray=\"6 4\"" } else { "" };
        writeln!(s, "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"2.5\"{dash}/>", pts.join(" "), se.color).ok();
        for p in &pts {
            let (px, py) = p.split_once(',').unwrap();
            writeln!(s, "<circle cx=\"{px}\" cy=\"{py}\" r=\"3.5\" fill=\"{BG}\" stroke=\"{}\" stroke-width=\"2\"/>", se.color).ok();
        }
    }
}

fn legend(s: &mut String, x: f64, y: f64, series: &[Series]) {
    for (i, se) in series.iter().enumerate() {
        let yy = y + i as f64 * 22.0;
        let dash = if se.dash { " stroke-dasharray=\"6 4\"" } else { "" };
        writeln!(s, "<line x1=\"{x}\" x2=\"{}\" y1=\"{yy}\" y2=\"{yy}\" stroke=\"{}\" stroke-width=\"2.5\"{dash}/><text x=\"{}\" y=\"{}\" fill=\"{TEXT}\" font-size=\"13\">{}</text>", x + 28.0, se.color, x + 36.0, yy + 4.0, se.label).ok();
    }
}

fn amazon_series_svg(years: &[AmazonYear]) -> String {
    let cats: Vec<String> = years.iter().map(|a| a.year.to_string()).collect();
    let idx = |f: fn(&AmazonYear) -> f64| -> Vec<f64> {
        let m = base_mean(years, f);
        years.iter().map(|a| f(a) / m * 100.0).collect()
    };
    let series = vec![
        Series { label: "Raw count, all sensors", color: C_RAW, dash: false, values: idx(|a| a.raw_count.iter().sum::<u64>() as f64) },
        Series { label: "Raw count, MODIS only (one sensor)", color: C_RAW2, dash: false, values: idx(|a| a.raw_count[0] as f64) },
        Series { label: "Harmonised count_normalized", color: C_HARM, dash: false, values: idx(|a| a.count_normalized) },
        Series { label: "Harmonised total_frp (sensor step remains)", color: C_ALT, dash: true, values: idx(|a| a.total_frp) },
    ];
    let mut s = svg_open(1000.0, 560.0, "Amazon fire activity 2012-2021, raw vs harmonised", "Annual index, 2012-2018 mean = 100, Legal Amazon forest-biome cells.");
    heading(&mut s, "Amazon forest cells 2012-2021: raw vs harmonised", "Annual index, 2012-2018 mean = 100. Raw sums add S-NPP in 2012 and NOAA-20 in 2018; count_normalized removes both steps.");
    let ymax = series.iter().flat_map(|x| x.values.iter()).fold(0.0_f64, |a, b| a.max(*b));
    let top = (ymax / 50.0).ceil() * 50.0;
    line_panel(&mut s, 80.0, 110.0, 880.0, 330.0, "", &cats, &series, 0.0, top, 50.0, "", Some(100.0));
    legend(&mut s, 100.0, 475.0, &series[..2]);
    legend(&mut s, 520.0, 475.0, &series[2..]);
    writeln!(s, "<text x=\"40\" y=\"540\" fill=\"{MUTED}\" font-size=\"12\">Source: NASA FIRMS SP country archive, Brazil; PyroSync default pipeline. docs/vaka/amazon_annual.csv</text>\n</svg>").ok();
    s
}

fn amazon_anomaly_svg(years: &[AmazonYear]) -> String {
    let mut s = svg_open(1000.0, 560.0, "Amazon 2019-2021 anomaly vs 2012-2018", "Percent change of each annual metric against the 2012-2018 mean.");
    heading(&mut s, "Amazon 2019-2021 anomaly vs 2012-2018 mean", "Same forest-biome cells, four metrics. Raw all-sensor counts inflate the anomaly with the NOAA-20 step.");
    let metrics: [Bar; 4] = [
        ("Raw, all sensors", C_RAW, METRICS[0].1),
        ("Raw, MODIS only", C_RAW2, METRICS[1].1),
        ("Harmonised count_normalized", C_HARM, METRICS[3].1),
        ("Harmonised total_frp", C_ALT, METRICS[4].1),
    ];
    let vals: Vec<Vec<f64>> = metrics
        .iter()
        .map(|(_, _, f)| {
            let m = base_mean(years, *f);
            (ANOM.0..=ANOM.1).map(|y| pct(m, years.iter().find(|a| a.year == y).map_or(f64::NAN, f))).collect()
        })
        .collect();
    let top = (vals.iter().flatten().fold(0.0_f64, |a, b| a.max(*b)) / 20.0).ceil() * 20.0;
    let (x0, y0, w, h) = (80.0, 110.0, 880.0, 330.0);
    let ymap = |v: f64| y0 + h - (v / top).clamp(0.0, 1.0) * h;
    let mut t = 0.0;
    while t <= top + 1e-9 {
        writeln!(s, "<line x1=\"{x0}\" x2=\"{x2}\" y1=\"{yy:.1}\" y2=\"{yy:.1}\" stroke=\"{GRID}\"/><text x=\"{}\" y=\"{:.1}\" fill=\"{MUTED}\" font-size=\"12\" text-anchor=\"end\">+{t:.0}%</text>", x0 - 6.0, ymap(t) + 4.0, x2 = x0 + w, yy = ymap(t)).ok();
        t += 20.0;
    }
    let group_w = w / 3.0;
    for (gi, year) in (ANOM.0..=ANOM.1).enumerate() {
        let gx = x0 + gi as f64 * group_w;
        writeln!(s, "<text x=\"{:.1}\" y=\"{}\" fill=\"{TEXT}\" font-size=\"15\" text-anchor=\"middle\">{year}</text>", gx + group_w / 2.0, y0 + h + 22.0).ok();
        let bw = 44.0;
        for (mi, (_, color, _)) in metrics.iter().enumerate() {
            let v = vals[mi][gi];
            let bx = gx + group_w / 2.0 - 2.0 * bw - 9.0 + mi as f64 * (bw + 6.0);
            let by = ymap(v.max(0.0));
            writeln!(s, "<rect x=\"{bx:.1}\" y=\"{by:.1}\" width=\"{bw}\" height=\"{:.1}\" fill=\"{color}\"/><text x=\"{:.1}\" y=\"{:.1}\" fill=\"{TEXT}\" font-size=\"12\" text-anchor=\"middle\">{v:+.0}%</text>", (ymap(0.0) - by).max(1.0), bx + bw / 2.0, by - 6.0).ok();
        }
    }
    for (i, (label, color, _)) in metrics.iter().enumerate() {
        let lx = 100.0 + (i % 2) as f64 * 420.0;
        let ly = 480.0 + (i / 2) as f64 * 22.0;
        writeln!(s, "<rect x=\"{lx}\" y=\"{}\" width=\"14\" height=\"14\" fill=\"{color}\"/><text x=\"{}\" y=\"{}\" fill=\"{TEXT}\" font-size=\"13\">{label}</text>", ly - 11.0, lx + 22.0, ly).ok();
    }
    writeln!(s, "<text x=\"40\" y=\"545\" fill=\"{MUTED}\" font-size=\"12\">Baseline = mean of 2012-2018 per metric. Rainfall not measured here: normal 2019-2021 rainfall is cited from [20]. docs/vaka/amazon_summary.json</text>\n</svg>").ok();
    s
}

// ---------------------------------------------------------------- India

const INDIA: &str = "India";
const I_YEARS: std::ops::RangeInclusive<i32> = 2019..=2023;
const HALF_KM_LAT: f64 = 10.0 / 111.32;

struct City {
    name: String,
    lat: f64,
    lon: f64,
    pop: u64,
}

impl City {
    fn half_lon(&self) -> f64 {
        HALF_KM_LAT / self.lat.to_radians().cos()
    }

    fn contains(&self, lat: f64, lon: f64) -> bool {
        (lat - self.lat).abs() <= HALF_KM_LAT && (lon - self.lon).abs() <= self.half_lon()
    }
}

fn read_cities(path: &Path) -> Result<Vec<City>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("{} (download GeoNames cities15000.zip)", path.display()))?;
    let mut v = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 15 || f[8] != "IN" {
            continue;
        }
        let pop: u64 = f[14].parse().unwrap_or(0);
        if pop > 100_000 {
            v.push(City { name: f[2].to_string(), lat: f[4].parse()?, lon: f[5].parse()?, pop });
        }
    }
    v.sort_by_key(|c| std::cmp::Reverse(c.pop));
    Ok(v)
}

/// 0.5-degree buckets of city boxes; a hotspot checks only the cities of its bucket.
struct CityIndex {
    cells: HashMap<(i32, i32), Vec<usize>>,
}

fn bucket(v: f64) -> i32 {
    (v * 2.0).floor() as i32
}

impl CityIndex {
    fn new(cities: &[City]) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, c) in cities.iter().enumerate() {
            for la in bucket(c.lat - HALF_KM_LAT)..=bucket(c.lat + HALF_KM_LAT) {
                for lo in bucket(c.lon - c.half_lon())..=bucket(c.lon + c.half_lon()) {
                    cells.entry((la, lo)).or_default().push(i);
                }
            }
        }
        Self { cells }
    }

    fn near(&self, cities: &[City], lat: f64, lon: f64) -> Vec<usize> {
        self.cells
            .get(&(bucket(lat), bucket(lon)))
            .map(|v| v.iter().copied().filter(|&i| cities[i].contains(lat, lon)).collect())
            .unwrap_or_default()
    }
}

/// Counters of one scope (a city or the union of all boxes) in one year.
#[derive(Default, Clone)]
struct Tally {
    raw_count: [u64; 3],
    raw_frp: [f64; 3],
    raw_small: [u64; 3],
    harmonized: u64,
    harmonized_frp: f64,
    harmonized_small: u64,
    matched: u64,
    modis_only: u64,
    viirs_only: u64,
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        for i in 0..3 {
            self.raw_count[i] += o.raw_count[i];
            self.raw_frp[i] += o.raw_frp[i];
            self.raw_small[i] += o.raw_small[i];
        }
        self.harmonized += o.harmonized;
        self.harmonized_frp += o.harmonized_frp;
        self.harmonized_small += o.harmonized_small;
        self.matched += o.matched;
        self.modis_only += o.modis_only;
        self.viirs_only += o.viirs_only;
    }

    fn raw_viirs(&self) -> u64 {
        self.raw_count[1] + self.raw_count[2]
    }

    fn ratio_snpp(&self) -> f64 {
        ratio(self.raw_count[1], self.raw_count[0])
    }

    fn ratio_raw(&self) -> f64 {
        ratio(self.raw_viirs(), self.raw_count[0])
    }

    fn ratio_harm(&self) -> f64 {
        ratio(self.matched + self.viirs_only, self.matched + self.modis_only)
    }

    fn raw_small_viirs(&self) -> f64 {
        share(self.raw_small[1] + self.raw_small[2], self.raw_viirs())
    }
}

fn ratio(a: u64, b: u64) -> f64 {
    if b > 0 { a as f64 / b as f64 } else { f64::NAN }
}

fn share(a: u64, b: u64) -> f64 {
    if b > 0 { 100.0 * a as f64 / b as f64 } else { f64::NAN }
}

fn tally_raw(t: &mut Tally, stream: usize, h: &Hotspot) {
    t.raw_count[stream] += 1;
    t.raw_frp[stream] += h.frp;
    t.raw_small[stream] += (h.frp < 1.0) as u64;
}

fn tally_harm(t: &mut Tally, h: &HarmonizedHotspot) {
    t.harmonized += 1;
    t.harmonized_frp += h.frp_corrected;
    t.harmonized_small += (h.frp < 1.0) as u64;
    match h.source {
        HotspotSource::Matched => t.matched += 1,
        HotspotSource::ModisOnly => t.modis_only += 1,
        HotspotSource::ViirsOnly => t.viirs_only += 1,
    }
}

struct IndiaResult {
    per_city: Vec<Tally>,
    union_year: BTreeMap<i32, Tally>,
    country_year: BTreeMap<i32, Tally>,
}

fn run_india(dir: &Path, cities: &[City], config: &PipelineConfig) -> Result<IndiaResult> {
    let index = CityIndex::new(cities);
    let mut res = IndiaResult { per_city: vec![Tally::default(); cities.len()], union_year: BTreeMap::new(), country_year: BTreeMap::new() };
    for year in I_YEARS {
        let t0 = Instant::now();
        let [modis, snpp, noaa20] = load_year(dir, INDIA, year)?;
        let union = res.union_year.entry(year).or_default();
        let country = res.country_year.entry(year).or_default();
        for (stream, rows) in [&modis, &snpp, &noaa20].into_iter().enumerate() {
            for h in rows {
                tally_raw(country, stream, h);
                let near = index.near(cities, h.lat, h.lon);
                if !near.is_empty() {
                    tally_raw(union, stream, h);
                }
                for i in near {
                    tally_raw(&mut res.per_city[i], stream, h);
                }
            }
        }
        let mut viirs = snpp;
        viirs.extend(noaa20);
        let links = harmonize_with_links_config(modis, viirs, config);
        for h in &links.hotspots {
            tally_harm(country, h);
            let near = index.near(cities, h.lat, h.lon);
            if !near.is_empty() {
                tally_harm(union, h);
            }
            for i in near {
                tally_harm(&mut res.per_city[i], h);
            }
        }
        eprintln!("india {year}: {:.1}s", t0.elapsed().as_secs_f64());
    }
    Ok(res)
}

fn write_india(out: &Path, cities: &[City], r: &IndiaResult) -> Result<()> {
    let mut total = Tally::default();
    for t in r.union_year.values() {
        total.add(t);
    }
    let mut country = Tally::default();
    for t in r.country_year.values() {
        country.add(t);
    }

    let mut annual = String::from("scope,year,raw_modis_count,raw_snpp_count,raw_noaa20_count,raw_modis_frp_mw,raw_viirs_frp_mw,harmonized_count,harmonized_frp_mw,matched,modis_only,viirs_only,ratio_snpp_over_modis_raw,ratio_viirs_over_modis_raw,ratio_viirs_over_modis_harmonized,raw_modis_lt1mw_pct,raw_viirs_lt1mw_pct,harmonized_lt1mw_pct\n");
    let mut row = |scope: &str, year: i32, t: &Tally| {
        writeln!(
            annual,
            "{scope},{year},{},{},{},{:.1},{:.1},{},{:.1},{},{},{},{:.3},{:.3},{:.3},{:.2},{:.2},{:.2}",
            t.raw_count[0], t.raw_count[1], t.raw_count[2], t.raw_frp[0], t.raw_frp[1] + t.raw_frp[2], t.harmonized, t.harmonized_frp,
            t.matched, t.modis_only, t.viirs_only, t.ratio_snpp(), t.ratio_raw(), t.ratio_harm(),
            share(t.raw_small[0], t.raw_count[0]), t.raw_small_viirs(), share(t.harmonized_small, t.harmonized),
        )
        .ok();
    };
    for (y, t) in &r.union_year {
        row("city_boxes", *y, t);
    }
    row("city_boxes", 0, &total);
    for (y, t) in &r.country_year {
        row("india_total", *y, t);
    }
    row("india_total", 0, &country);
    std::fs::write(out.join("india_annual.csv"), annual)?;

    let mut csv = String::from("city,lat,lon,population,raw_modis_count,raw_snpp_count,raw_noaa20_count,raw_modis_frp_mw,raw_viirs_frp_mw,harmonized_count,harmonized_frp_mw,matched,modis_only,viirs_only,ratio_viirs_over_modis_raw,ratio_viirs_over_modis_harmonized,raw_viirs_lt1mw_pct,harmonized_lt1mw_pct\n");
    for (c, t) in cities.iter().zip(&r.per_city) {
        writeln!(
            csv,
            "\"{}\",{:.4},{:.4},{},{},{},{},{:.1},{:.1},{},{:.1},{},{},{},{:.3},{:.3},{:.2},{:.2}",
            c.name, c.lat, c.lon, c.pop, t.raw_count[0], t.raw_count[1], t.raw_count[2], t.raw_frp[0], t.raw_frp[1] + t.raw_frp[2],
            t.harmonized, t.harmonized_frp, t.matched, t.modis_only, t.viirs_only, t.ratio_raw(), t.ratio_harm(),
            t.raw_small_viirs(), share(t.harmonized_small, t.harmonized),
        )?;
    }
    std::fs::write(out.join("india_cities.csv"), csv)?;

    let mut ratios: Vec<f64> = r.per_city.iter().filter(|t| t.raw_count[0] >= 20).map(Tally::ratio_raw).collect();
    ratios.sort_by(f64::total_cmp);
    let q = |p: f64| ratios.get(((ratios.len().max(1) - 1) as f64 * p).round() as usize).copied().unwrap_or(f64::NAN);
    let with_fire = r.per_city.iter().filter(|t| t.raw_count.iter().sum::<u64>() > 0).count();
    println!("cities {} (with detections {with_fire}), years {:?}", cities.len(), I_YEARS);
    println!("city boxes: raw MODIS {} SNPP {} N20 {} | harmonized {} (matched {} modis_only {} viirs_only {})", total.raw_count[0], total.raw_count[1], total.raw_count[2], total.harmonized, total.matched, total.modis_only, total.viirs_only);
    println!("ratio SNPP/MODIS {:.2}  VIIRS(SNPP+N20)/MODIS {:.2}  harmonized {:.2}", total.ratio_snpp(), total.ratio_raw(), total.ratio_harm());
    println!("<1MW share: MODIS {:.1}% VIIRS {:.1}% harmonized {:.1}%", share(total.raw_small[0], total.raw_count[0]), total.raw_small_viirs(), share(total.harmonized_small, total.harmonized));
    println!("India total: ratio SNPP/MODIS {:.2}, VIIRS/MODIS {:.2}, harmonized {:.2}; <1MW MODIS {:.1}% VIIRS {:.1}%", country.ratio_snpp(), country.ratio_raw(), country.ratio_harm(), share(country.raw_small[0], country.raw_count[0]), country.raw_small_viirs());
    println!("per-city raw ratio quantiles (MODIS>=20): p10 {:.2} p50 {:.2} p90 {:.2} (n={})", q(0.1), q(0.5), q(0.9), ratios.len());

    let mut json = format!(
        "{{\"country\":\"{INDIA}\",\"years\":[{},{}],\"cities\":{},\"cities_with_detections\":{with_fire},\"box_km\":20,\"city_source\":\"GeoNames cities15000, country IN, population>100000\",\"config\":\"PipelineConfig::default()\",",
        I_YEARS.start(), I_YEARS.end(), cities.len()
    );
    let scope = |t: &Tally| {
        format!(
            "{{\"raw_modis\":{},\"raw_snpp\":{},\"raw_noaa20\":{},\"raw_modis_frp_mw\":{:.1},\"raw_viirs_frp_mw\":{:.1},\"harmonized\":{},\"harmonized_frp_mw\":{:.1},\"matched\":{},\"modis_only\":{},\"viirs_only\":{},\"ratio_snpp_over_modis_raw\":{},\"ratio_viirs_over_modis_raw\":{},\"ratio_viirs_over_modis_harmonized\":{},\"raw_modis_lt1mw_pct\":{},\"raw_viirs_lt1mw_pct\":{},\"harmonized_lt1mw_pct\":{}}}",
            t.raw_count[0], t.raw_count[1], t.raw_count[2], t.raw_frp[0], t.raw_frp[1] + t.raw_frp[2], t.harmonized, t.harmonized_frp,
            t.matched, t.modis_only, t.viirs_only, num(t.ratio_snpp()), num(t.ratio_raw()), num(t.ratio_harm()),
            num(share(t.raw_small[0], t.raw_count[0])), num(t.raw_small_viirs()), num(share(t.harmonized_small, t.harmonized))
        )
    };
    writeln!(json, "\"city_boxes\":{},\"india_total\":{},\"per_city_ratio_raw\":{{\"n\":{},\"p10\":{},\"p50\":{},\"p90\":{}}}}}\n", scope(&total), scope(&country), ratios.len(), num(q(0.1)), num(q(0.5)), num(q(0.9)))?;
    std::fs::write(out.join("india_summary.json"), json)?;

    std::fs::write(out.join("india_ratio.svg"), india_ratio_svg(r))?;
    std::fs::write(out.join("india_cities.svg"), india_cities_svg(cities, r))?;
    Ok(())
}

fn india_ratio_svg(r: &IndiaResult) -> String {
    let cats: Vec<String> = r.union_year.keys().map(|y| y.to_string()).collect();
    let ys: Vec<&Tally> = r.union_year.values().collect();
    let ratio_series = vec![
        Series { label: "Raw VIIRS (S-NPP+NOAA-20) / MODIS", color: C_RAW, dash: false, values: ys.iter().map(|t| t.ratio_raw()).collect() },
        Series { label: "Raw S-NPP / MODIS", color: C_RAW2, dash: false, values: ys.iter().map(|t| t.ratio_snpp()).collect() },
        Series { label: "Harmonised VIIRS-detected / MODIS-detected", color: C_HARM, dash: false, values: ys.iter().map(|t| t.ratio_harm()).collect() },
    ];
    let small_series = vec![
        Series { label: "Raw MODIS", color: C_RAW2, dash: false, values: ys.iter().map(|t| share(t.raw_small[0], t.raw_count[0])).collect() },
        Series { label: "Raw VIIRS", color: C_RAW, dash: false, values: ys.iter().map(|t| t.raw_small_viirs()).collect() },
        Series { label: "Harmonised", color: C_HARM, dash: false, values: ys.iter().map(|t| share(t.harmonized_small, t.harmonized)).collect() },
    ];
    let mut s = svg_open(1200.0, 520.0, "India city boxes: VIIRS/MODIS ratio and small-fire share", "Raw and harmonised detections inside 20x20 km boxes around Indian cities with population above 100k.");
    heading(&mut s, "India city boxes: VIIRS/MODIS ratio and share of <1 MW detections", "20x20 km boxes around GeoNames cities (pop > 100k). Harmonisation merges the same fire seen by several satellites before the ratio is taken.");
    let rmax = (ratio_series.iter().flat_map(|x| x.values.iter()).fold(0.0_f64, |a, b| a.max(*b)) + 0.5).ceil();
    line_panel(&mut s, 70.0, 130.0, 500.0, 270.0, "Detection ratio (VIIRS / MODIS)", &cats, &ratio_series, 0.0, rmax, 1.0, "x", None);
    line_panel(&mut s, 680.0, 130.0, 480.0, 270.0, "Share of detections below 1 MW", &cats, &small_series, 0.0, 100.0, 20.0, "%", None);
    legend(&mut s, 70.0, 440.0, &ratio_series);
    legend(&mut s, 680.0, 440.0, &small_series);
    writeln!(s, "<text x=\"40\" y=\"505\" fill=\"{MUTED}\" font-size=\"12\">Source: NASA FIRMS SP country archive, India 2019-2023; PyroSync default pipeline. docs/vaka/india_annual.csv. Literature range 6.3-7.7x [21] is not reproduced here.</text>\n</svg>").ok();
    s
}

fn india_cities_svg(cities: &[City], r: &IndiaResult) -> String {
    let mut order: Vec<usize> = (0..cities.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(r.per_city[i].harmonized));
    let top = &order[..order.len().min(15)];
    let max = top.iter().map(|&i| r.per_city[i].raw_count.iter().sum::<u64>().max(r.per_city[i].harmonized)).max().unwrap_or(1) as f64;
    let mut s = svg_open(1000.0, 640.0, "India top 15 cities by harmonised fire detections", "Raw per-sensor sums vs harmonised detections within 20x20 km of each city, 2019-2023.");
    heading(&mut s, "Top 15 city boxes by harmonised detections, 2019-2023", "Raw sum counts one fire once per satellite; the harmonised count merges them.");
    let (x0, bar_w, row_h) = (190.0, 700.0, 30.0);
    for (k, &i) in top.iter().enumerate() {
        let y = 100.0 + k as f64 * row_h;
        let t = &r.per_city[i];
        let raw: u64 = t.raw_count.iter().sum();
        writeln!(s, "<text x=\"{}\" y=\"{:.1}\" fill=\"{TEXT}\" font-size=\"13\" text-anchor=\"end\">{}</text>", x0 - 10.0, y + 15.0, cities[i].name).ok();
        writeln!(s, "<rect x=\"{x0}\" y=\"{y:.1}\" width=\"{:.1}\" height=\"11\" fill=\"{C_RAW2}\"/><text x=\"{:.1}\" y=\"{:.1}\" fill=\"{MUTED}\" font-size=\"11\">{raw}</text>", raw as f64 / max * bar_w, x0 + raw as f64 / max * bar_w + 6.0, y + 10.0).ok();
        writeln!(s, "<rect x=\"{x0}\" y=\"{:.1}\" width=\"{:.1}\" height=\"11\" fill=\"{C_HARM}\"/><text x=\"{:.1}\" y=\"{:.1}\" fill=\"{TEXT}\" font-size=\"11\">{}</text>", y + 12.0, t.harmonized as f64 / max * bar_w, x0 + t.harmonized as f64 / max * bar_w + 6.0, y + 22.0, t.harmonized).ok();
    }
    writeln!(s, "<rect x=\"{x0}\" y=\"580\" width=\"14\" height=\"14\" fill=\"{C_RAW2}\"/><text x=\"{}\" y=\"592\" fill=\"{TEXT}\" font-size=\"13\">Raw (MODIS + S-NPP + NOAA-20 sum)</text><rect x=\"{}\" y=\"580\" width=\"14\" height=\"14\" fill=\"{C_HARM}\"/><text x=\"{}\" y=\"592\" fill=\"{TEXT}\" font-size=\"13\">Harmonised</text>", x0 + 22.0, x0 + 300.0, x0 + 322.0).ok();
    writeln!(s, "<text x=\"40\" y=\"625\" fill=\"{MUTED}\" font-size=\"12\">Source: NASA FIRMS SP country archive, India 2019-2023; GeoNames cities15000. docs/vaka/india_cities.csv</text>\n</svg>").ok();
    s
}

// ---------------------------------------------------------------- main

fn main() -> Result<()> {
    let started = Instant::now();
    let mut args = std::env::args().skip(1);
    let which = args.next().unwrap_or_else(|| "all".into());
    let out = args.next().map_or_else(|| PathBuf::from("docs/vaka"), PathBuf::from);
    if !matches!(which.as_str(), "amazon" | "india" | "all") {
        bail!("usage: amazon_india_case [amazon|india|all] [out-dir]");
    }
    std::fs::create_dir_all(&out)?;
    let dir = archive_dir();
    let config = PipelineConfig::default();
    if which != "india" {
        let results = run_amazon(&dir, &config)?;
        let all = write_amazon(&out, &results)?;
        std::fs::write(out.join("amazon_series.svg"), amazon_series_svg(&all[1]))?;
        std::fs::write(out.join("amazon_anomaly.svg"), amazon_anomaly_svg(&all[1]))?;
    }
    if which != "amazon" {
        let cities = read_cities(Path::new(CITIES_FILE))?;
        let r = run_india(&dir, &cities, &config)?;
        write_india(&out, &cities, &r)?;
    }
    eprintln!("done in {:.1}s, outputs in {}", started.elapsed().as_secs_f64(), out.display());
    Ok(())
}
