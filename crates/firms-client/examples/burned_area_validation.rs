//! Y10 pilot: external validation of raw FIRMS and the PyroSync harmonised series against the
//! ESA Fire_cci v5.1 (FireCCI51) 0.25° monthly burned-area grid, 2012-2020, in four regions.
//!
//! Per region it compares, on 0.25° cell-months and on region-months, Fire_cci burned area with
//! the raw per-sensor FIRMS count/FRP and the `/api/calendar` harmonised `count_normalized` /
//! `total_frp`: Lin's CCC after the `ln(1 + x / mean(x))` transform, Pearson r on the same
//! transform, and cell-month omission/commission (IOE/ICE) of hotspot presence vs burned area.
//!
//! Fire_cci input is the flat CSV written by `tools/firecci_extract.py`.
//! Usage: cargo run --release -p pyrosync-firms --example burned_area_validation [firecci.csv] [out-dir]
//! (defaults `.cache/firecci/firecci51_nonzero.csv`, `docs/vaka`; FIRMS archive from
//! ARCHIVE_CACHE_DIR or `.cache/firms-archive`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Datelike;
use pyrosync_core::calendar::build_calendar_with;
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::harmonize::harmonize_with_links_config;
use pyrosync_core::models::{CalendarCell, HarmonizedHotspot, Hotspot, Sensor};
use pyrosync_firms::parse::parse_csv;
use pyrosync_firms::{FirmsSource, Processing};
use serde_json::json;

const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const DEFAULT_FIRECCI_CSV: &str = ".cache/firecci/firecci51_nonzero.csv";
const FROM: i32 = 2012;
const TO: i32 = 2020;
const CELL: f64 = 0.25;

/// Ω is pooled per platform configuration; the archive adds NOAA-20 on 2018-04-01, so one
/// calendar per era reproduces a single 2012-2020 `/api/calendar` call (see `china_ban_case`).
const ERAS: [((i32, u32), (i32, u32)); 2] = [((2012, 1), (2018, 3)), ((2018, 4), (2020, 12))];

/// [west, south, east, north], on the 0.25° grid and wholly inside the country, so that cells
/// holding another country's fires (absent from this country's FIRMS file) do not count as
/// omission.
type Bbox = [f64; 4];

struct Region {
    id: &'static str,
    label: &'static str,
    country: &'static str,
    boxes: &'static [Bbox],
}

const REGIONS: [Region; 4] = [
    Region {
        id: "turkey",
        label: "Türkiye (Anadolu iç kutusu)",
        country: "Turkey",
        boxes: &[[27.25, 37.25, 43.0, 41.0]],
    },
    Region {
        id: "brazil",
        label: "Brezilya (Cerrado / ormansızlaşma yayı)",
        country: "Brazil",
        boxes: &[[-58.0, -15.0, -44.0, -5.0]],
    },
    Region {
        id: "canada",
        label: "Kanada (Alberta/Saskatchewan/Manitoba boreal)",
        country: "Canada",
        boxes: &[[-120.0, 50.0, -95.0, 60.0]],
    },
    Region {
        id: "india",
        label: "Hindistan (orta Hindistan + Pencap/Haryana)",
        country: "India",
        boxes: &[[74.0, 16.0, 84.0, 27.0], [74.75, 27.0, 79.5, 30.75]],
    },
];

type CellId = (u32, u32);

fn cell_of(lat: f64, lon: f64) -> CellId {
    (
        ((lat + 90.0) / CELL).floor() as u32,
        ((lon + 180.0) / CELL).floor() as u32,
    )
}

fn centre((iy, ix): CellId) -> (f64, f64) {
    (
        iy as f64 * CELL - 90.0 + CELL / 2.0,
        ix as f64 * CELL - 180.0 + CELL / 2.0,
    )
}

impl Region {
    fn contains_cell(&self, c: CellId) -> bool {
        let (lat, lon) = centre(c);
        self.boxes
            .iter()
            .any(|b| lon > b[0] && lat > b[1] && lon < b[2] && lat < b[3])
    }

    fn cells(&self) -> Vec<CellId> {
        let mut set = BTreeSet::new();
        for b in self.boxes {
            let (lo, hi) = (cell_of(b[1], b[0]), cell_of(b[3] - 1e-9, b[2] - 1e-9));
            for iy in lo.0..=hi.0 {
                for ix in lo.1..=hi.1 {
                    if self.contains_cell((iy, ix)) {
                        set.insert((iy, ix));
                    }
                }
            }
        }
        set.into_iter().collect()
    }
}

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

/// Type-0 rows of one sensor file inside the region, as `/api/series` and `/api/calendar` keep them.
fn load(
    dir: &Path,
    region: &Region,
    sensor_dir: &str,
    sensor: Sensor,
    year: i32,
) -> Result<Vec<Hotspot>> {
    let path = dir
        .join(sensor_dir)
        .join(year.to_string())
        .join(format!("{sensor_dir}_{year}_{}.csv", region.country));
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
    let mut rows = parse_csv(&text, FirmsSource::new(sensor, Processing::Sp).as_str())
        .with_context(|| path.display().to_string())?;
    rows.retain(|h| h.fire_type.is_wildfire() && region.contains_cell(cell_of(h.lat, h.lon)));
    Ok(rows)
}

/// One cell-month. Raw index 0 = MODIS (Terra+Aqua), 1 = S-NPP, 2 = NOAA-20.
#[derive(Default, Clone, Copy)]
struct Agg {
    raw_count: [u32; 3],
    raw_frp: [f64; 3],
    harm_count: u32,
    harm_frp: f64,
}

type Ym = (i32, u32);

#[derive(Default)]
struct RegionData {
    cells: HashMap<(Ym, CellId), Agg>,
    calendar: BTreeMap<Ym, CalendarCell>,
    omegas: Vec<(String, Option<f64>)>,
}

fn in_era(h: &HarmonizedHotspot, era: (Ym, Ym)) -> bool {
    let ym = (h.acquired.year(), h.acquired.month());
    ym >= era.0 && ym <= era.1
}

fn platform_key(h: &HarmonizedHotspot) -> (bool, BTreeSet<Sensor>) {
    (
        h.platforms.iter().any(|s| s.is_modis()),
        h.platforms
            .iter()
            .copied()
            .filter(|s| s.is_viirs())
            .collect(),
    )
}

fn run_region(dir: &Path, region: &Region, config: &PipelineConfig) -> Result<RegionData> {
    let mut out = RegionData::default();
    let mut visited = BTreeSet::new();
    for era in ERAS {
        let mut harmonized: Vec<HarmonizedHotspot> = Vec::new();
        for year in era.0.0..=era.1.0 {
            let [modis, snpp, noaa20] = SENSOR_FILES.map(|(d, s)| load(dir, region, d, s, year));
            let (modis, snpp, noaa20) = (modis?, snpp?, noaa20?);
            if visited.insert(year) {
                for (i, rows) in [&modis, &snpp, &noaa20].into_iter().enumerate() {
                    for h in rows {
                        let a = out
                            .cells
                            .entry(((year, h.acquired.month()), cell_of(h.lat, h.lon)))
                            .or_default();
                        a.raw_count[i] += 1;
                        a.raw_frp[i] += h.frp;
                    }
                }
            }
            let mut viirs = snpp;
            viirs.extend(noaa20);
            // Whole calendar year per call, so the static mask sees the same cell-years as the app.
            let links = harmonize_with_links_config(modis, viirs, config);
            harmonized.extend(links.hotspots.into_iter().filter(|h| in_era(h, era)));
        }
        let mut month_keys: BTreeMap<Ym, (bool, BTreeSet<Sensor>)> = BTreeMap::new();
        for h in &harmonized {
            let (m, v) = platform_key(h);
            let k = month_keys
                .entry((h.acquired.year(), h.acquired.month()))
                .or_default();
            k.0 |= m;
            k.1.extend(v);
        }
        let configs: BTreeSet<_> = month_keys.values().collect();
        if configs.len() != 1 {
            eprintln!(
                "warning: {} era {:?}-{:?} mixes {} platform configurations",
                region.id,
                era.0,
                era.1,
                configs.len()
            );
        }
        for h in &harmonized {
            let a = out
                .cells
                .entry((
                    (h.acquired.year(), h.acquired.month()),
                    cell_of(h.lat, h.lon),
                ))
                .or_default();
            a.harm_count += 1;
            a.harm_frp += h.frp_corrected;
        }
        let cells = build_calendar_with(&harmonized, &config.uncertainty, &config.calibration);
        let omega = cells.iter().find_map(|c| c.effort_omega);
        out.omegas.push((
            format!("{}-{:02}..{}-{:02}", era.0.0, era.0.1, era.1.0, era.1.1),
            omega,
        ));
        for c in cells {
            out.calendar.insert((c.year, c.month), c);
        }
    }
    Ok(out)
}

/// Fire_cci burned area (m²) per (region index, month, cell), from `tools/firecci_extract.py`.
fn load_firecci(path: &Path) -> Result<HashMap<(usize, Ym, CellId), f64>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("{} (run tools/firecci_extract.py first)", path.display()))?;
    let mut out = HashMap::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        if f.len() < 5 {
            continue;
        }
        let (year, month): (i32, u32) = (f[0].parse()?, f[1].parse()?);
        if !(FROM..=TO).contains(&year) {
            continue;
        }
        let cell = cell_of(f[2].parse()?, f[3].parse()?);
        let ba: f64 = f[4].parse()?;
        for (i, region) in REGIONS.iter().enumerate() {
            if region.contains_cell(cell) {
                *out.entry((i, (year, month), cell)).or_default() += ba;
            }
        }
    }
    Ok(out)
}

// ---------- metrics ----------

/// `ln(1 + x / mean(x))`: removes the unit (km² vs detections vs MW) without z-scoring, which
/// would force equal means and variances and reduce CCC to Pearson r.
fn transform(x: &[f64]) -> Vec<f64> {
    let m = x.iter().sum::<f64>() / x.len() as f64;
    x.iter()
        .map(|v| if m > 0.0 { (1.0 + v / m).ln() } else { 0.0 })
        .collect()
}

fn moments(x: &[f64], y: &[f64]) -> (f64, f64, f64, f64, f64) {
    let n = x.len() as f64;
    let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        sxx += (a - mx) * (a - mx);
        syy += (b - my) * (b - my);
        sxy += (a - mx) * (b - my);
    }
    (mx, my, sxx / n, syy / n, sxy / n)
}

/// Lin's concordance correlation coefficient (population moments).
fn ccc(x: &[f64], y: &[f64]) -> f64 {
    let (mx, my, vx, vy, cxy) = moments(x, y);
    2.0 * cxy / (vx + vy + (mx - my).powi(2))
}

fn pearson(x: &[f64], y: &[f64]) -> f64 {
    let (_, _, vx, vy, cxy) = moments(x, y);
    cxy / (vx * vy).sqrt()
}

const SERIES: [&str; 4] = ["raw_modis", "raw_snpp", "raw_all", "harmonized"];

/// (count, FRP) of one series for a cell-month; the harmonised count is Ω-normalised.
fn pick(a: &Agg, series: usize, omega: Option<f64>) -> (f64, f64) {
    match series {
        0 | 1 => (a.raw_count[series] as f64, a.raw_frp[series]),
        2 => (
            a.raw_count.iter().sum::<u32>() as f64,
            a.raw_frp.iter().sum(),
        ),
        _ => (a.harm_count as f64 / omega.unwrap_or(1.0), a.harm_frp),
    }
}

fn months() -> Vec<Ym> {
    (FROM..=TO)
        .flat_map(|y| (1..=12).map(move |m| (y, m)))
        .collect()
}

struct Scores {
    ccc_count: f64,
    ccc_frp: f64,
    r_count: f64,
    r_frp: f64,
    /// CCC of the transformed region-month series after removing each calendar month's mean,
    /// i.e. interannual agreement without the shared seasonal cycle (NaN on cell-months).
    ccc_count_anom: f64,
    ccc_frp_anom: f64,
}

/// Subtracts the per-calendar-month mean; index 0 must be a January.
fn deseason(t: &[f64]) -> Vec<f64> {
    let mut mean = [0.0; 12];
    for (i, v) in t.iter().enumerate() {
        mean[i % 12] += v * 12.0 / t.len() as f64;
    }
    t.iter()
        .enumerate()
        .map(|(i, v)| v - mean[i % 12])
        .collect()
}

fn scores(ba: &[f64], count: &[f64], frp: &[f64], monthly: bool) -> Scores {
    let (b, c, f) = (transform(ba), transform(count), transform(frp));
    let anom = |x: &[f64]| {
        if monthly {
            ccc(&deseason(&b), &deseason(x))
        } else {
            f64::NAN
        }
    };
    Scores {
        ccc_count: ccc(&b, &c),
        ccc_frp: ccc(&b, &f),
        r_count: pearson(&b, &c),
        r_frp: pearson(&b, &f),
        ccc_count_anom: anom(&c),
        ccc_frp_anom: anom(&f),
    }
}

#[derive(Default, Clone, Copy)]
struct Confusion {
    both: u64,
    ba_only: u64,
    hot_only: u64,
}

impl Confusion {
    /// Omission: burned cell-months without a hotspot.
    fn ioe(&self) -> f64 {
        self.ba_only as f64 / (self.both + self.ba_only) as f64
    }
    /// Commission: hotspot cell-months without burned area.
    fn ice(&self) -> f64 {
        self.hot_only as f64 / (self.both + self.hot_only) as f64
    }
    fn dice(&self) -> f64 {
        2.0 * self.both as f64 / (2 * self.both + self.ba_only + self.hot_only) as f64
    }
}

struct RegionReport {
    region_month: Vec<Scores>,
    cell_month: Vec<Scores>,
    confusion: Vec<Confusion>,
    monthly_rows: String,
    n_cells: usize,
    ba_cell_months: u64,
    ba_km2: f64,
}

fn evaluate(
    ri: usize,
    region: &Region,
    d: &RegionData,
    ba: &HashMap<(usize, Ym, CellId), f64>,
) -> RegionReport {
    let cells = region.cells();
    let months = months();
    let omega = |ym: Ym| d.calendar.get(&ym).and_then(|c| c.effort_omega);
    let n = SERIES.len();

    let mut rm_ba = Vec::new();
    let mut rm: Vec<(Vec<f64>, Vec<f64>)> = vec![Default::default(); n];
    let mut cm_ba = Vec::new();
    let mut cm: Vec<(Vec<f64>, Vec<f64>)> = vec![Default::default(); n];
    let mut confusion = vec![Confusion::default(); n];
    let mut monthly_rows = String::new();
    let mut ba_cell_months = 0;
    for &ym in &months {
        let mut month_ba = 0.0;
        let mut month = vec![(0.0, 0.0); n];
        for &cell in &cells {
            let b = ba.get(&(ri, ym, cell)).copied().unwrap_or(0.0) / 1e6;
            let a = d.cells.get(&(ym, cell)).copied().unwrap_or_default();
            month_ba += b;
            cm_ba.push(b);
            ba_cell_months += u64::from(b > 0.0);
            for s in 0..n {
                let (c, f) = pick(&a, s, omega(ym));
                cm[s].0.push(c);
                cm[s].1.push(f);
                month[s].0 += c;
                month[s].1 += f;
                let hot = c > 0.0;
                let k = &mut confusion[s];
                match (b > 0.0, hot) {
                    (true, true) => k.both += 1,
                    (true, false) => k.ba_only += 1,
                    (false, true) => k.hot_only += 1,
                    _ => {}
                }
            }
        }
        // Region-month harmonised values are the calendar's own fields, not cell sums.
        let cal = d.calendar.get(&ym);
        month[3] = (
            cal.and_then(|c| c.count_normalized).unwrap_or(0.0),
            cal.map_or(0.0, |c| c.total_frp),
        );
        rm_ba.push(month_ba);
        for s in 0..n {
            rm[s].0.push(month[s].0);
            rm[s].1.push(month[s].1);
        }
        let _ = writeln!(
            monthly_rows,
            "{},{},{},{:.3},{},{:.1},{},{:.1},{},{:.1},{},{},{:.2},{:.1}",
            region.id,
            ym.0,
            ym.1,
            month_ba,
            month[0].0,
            month[0].1,
            month[1].0,
            month[1].1,
            month[2].0,
            month[2].1,
            cal.map_or(0, |c| c.count),
            omega(ym).map_or(String::new(), |o| format!("{o:.4}")),
            month[3].0,
            month[3].1,
        );
    }
    RegionReport {
        region_month: rm.iter().map(|(c, f)| scores(&rm_ba, c, f, true)).collect(),
        cell_month: cm
            .iter()
            .map(|(c, f)| scores(&cm_ba, c, f, false))
            .collect(),
        confusion,
        monthly_rows,
        n_cells: cells.len(),
        ba_cell_months,
        ba_km2: rm_ba.iter().sum(),
    }
}

fn main() -> Result<()> {
    let started = Instant::now();
    let mut args = std::env::args().skip(1);
    let firecci = args
        .next()
        .map_or_else(|| PathBuf::from(DEFAULT_FIRECCI_CSV), PathBuf::from);
    let out_dir = args
        .next()
        .map_or_else(|| PathBuf::from("docs/vaka"), PathBuf::from);
    std::fs::create_dir_all(&out_dir)?;
    let dir = archive_dir();
    let config = PipelineConfig::default();
    let ba = load_firecci(&firecci)?;

    let mut monthly = String::from(
        "region,year,month,firecci_ba_km2,raw_modis_count,raw_modis_frp_mw,raw_snpp_count,raw_snpp_frp_mw,raw_all_count,raw_all_frp_mw,harmonized_count,effort_omega,count_normalized,total_frp_mw\n",
    );
    let mut regions_json = Vec::new();
    for (ri, region) in REGIONS.iter().enumerate() {
        let t = Instant::now();
        let d = run_region(&dir, region, &config)?;
        let r = evaluate(ri, region, &d, &ba);
        eprintln!("{}: done in {:.1}s", region.id, t.elapsed().as_secs_f64());
        monthly.push_str(&r.monthly_rows);

        println!(
            "\n## {} — {} cells, {} burned cell-months, Fire_cci {:.0} km² ({FROM}-{TO}); Ω {:?}",
            region.label, r.n_cells, r.ba_cell_months, r.ba_km2, d.omegas
        );
        println!(
            "| series | CCC count (RM) | CCC FRP (RM) | r count (RM) | r FRP (RM) | CCC count anom (RM) | CCC FRP anom (RM) | CCC count (CM) | CCC FRP (CM) | IOE | ICE | Dice |"
        );
        println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
        let mut series_json = serde_json::Map::new();
        for (s, name) in SERIES.iter().enumerate() {
            let (a, b, k) = (&r.region_month[s], &r.cell_month[s], r.confusion[s]);
            println!(
                "| {name} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
                a.ccc_count,
                a.ccc_frp,
                a.r_count,
                a.r_frp,
                a.ccc_count_anom,
                a.ccc_frp_anom,
                b.ccc_count,
                b.ccc_frp,
                k.ioe(),
                k.ice(),
                k.dice()
            );
            series_json.insert(
                name.to_string(),
                json!({
                    "region_month": {"ccc_count": a.ccc_count, "ccc_frp": a.ccc_frp, "pearson_count": a.r_count, "pearson_frp": a.r_frp,
                                     "ccc_count_deseasonalized": a.ccc_count_anom, "ccc_frp_deseasonalized": a.ccc_frp_anom},
                    "cell_month": {"ccc_count": b.ccc_count, "ccc_frp": b.ccc_frp, "pearson_count": b.r_count, "pearson_frp": b.r_frp},
                    "confusion": {"both": k.both, "burned_without_hotspot": k.ba_only, "hotspot_without_burned": k.hot_only,
                                  "ioe": k.ioe(), "ice": k.ice(), "dice": k.dice()},
                }),
            );
        }
        regions_json.push(json!({
            "id": region.id,
            "label": region.label,
            "country_file": region.country,
            "boxes_wsen": region.boxes,
            "cells": r.n_cells,
            "burned_cell_months": r.ba_cell_months,
            "firecci_ba_km2": r.ba_km2,
            "effort_omega": d.omegas.iter().map(|(e, o)| json!({"era": e, "omega": o})).collect::<Vec<_>>(),
            "series": series_json,
        }));
    }
    std::fs::write(out_dir.join("firecci_validation_monthly.csv"), monthly)?;
    let summary = json!({
        "reference": "ESA Fire_cci v5.1 (FireCCI51) grid, 0.25 deg monthly burned_area",
        "years": [FROM, TO],
        "config": "PipelineConfig::default()",
        "transform": "ln(1 + x / mean(x)) per series before CCC and Pearson",
        "presence": "burned_area > 0 vs >= 1 detection in the cell-month",
        "regions": regions_json,
    });
    std::fs::write(
        out_dir.join("firecci_validation_summary.json"),
        serde_json::to_string_pretty(&summary)? + "\n",
    )?;
    eprintln!("total {:.1}s", started.elapsed().as_secs_f64());
    Ok(())
}
