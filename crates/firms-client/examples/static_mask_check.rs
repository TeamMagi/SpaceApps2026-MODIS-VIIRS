//! Measures the thresholds of the persistent hot-cell mask and the SAMA night filter on the
//! local FIRMS country archive (read-only).
//!
//! `mask`: per H3 resolution-7 cell, counts the months with a `type 0` detection in each year and
//! reports, for each month threshold, how many `type 0` rows a per-year mask would drop and how
//! many of them sit in cells FIRMS itself flags as static (`type 2`), i.e. leaked flare rows.
//! `sama`: compares nighttime detections inside and outside the SAMA box by how often they are
//! isolated (no other detection within ~2 km and ±2 days), the signature of single-pixel noise.
//! `pipeline`: runs the harmonization pipeline on whole years and prints the rows each filter
//! removed (default config, plus the SAMA filter switched on).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Datelike;
use pyrosync_core::config::{PipelineConfig, SamaConfig};
use pyrosync_core::harmonize::harmonize_with_links_config;
use pyrosync_core::models::{Confidence, DayNight, FireType, Hotspot, Sensor};
use pyrosync_core::sama::in_sama_box;
use pyrosync_core::static_mask::mask_cell;
use pyrosync_firms::parse::parse_csv;
use pyrosync_firms::{FirmsSource, Processing};

const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const SENSORS: [(&str, Sensor); 3] = [
    ("modis", Sensor::ModisTerra),
    ("viirs-snpp", Sensor::ViirsSNPP),
    ("viirs-jpss1", Sensor::ViirsNOAA20),
];

fn print_usage() {
    println!(
        "\
Usage: static_mask_check mask <from-year> <to-year> <Country>...
       static_mask_check sama <from-year> <to-year> <Country>...
       static_mask_check pipeline <from-year> <to-year> <Country>...

Archive directory: ARCHIVE_CACHE_DIR env var, or {DEFAULT_ARCHIVE_CACHE_DIR}."
    );
}

fn archive_dir() -> PathBuf {
    match std::env::var_os("ARCHIVE_CACHE_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR),
    }
}

fn load(dir: &Path, sensor_dir: &str, sensor: Sensor, year: i32, country: &str) -> Result<Vec<Hotspot>> {
    let path = dir
        .join(sensor_dir)
        .join(year.to_string())
        .join(format!("{sensor_dir}_{year}_{country}.csv"));
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
    parse_csv(&text, FirmsSource::new(sensor, Processing::Sp).as_str())
        .with_context(|| path.display().to_string())
}

#[derive(Default)]
struct CellYear {
    months: u16,
    rows: u64,
}

fn mask_report(dir: &Path, from: i32, to: i32, country: &str) -> Result<()> {
    let mut cells: HashMap<u64, BTreeMap<i32, CellYear>> = HashMap::new();
    let mut static_cells: HashSet<u64> = HashSet::new();
    let (mut type0, mut type2) = (0u64, 0u64);
    for year in from..=to {
        for (sensor_dir, sensor) in SENSORS {
            for h in load(dir, sensor_dir, sensor, year, country)? {
                let Some(cell) = mask_cell(h.lat, h.lon) else { continue };
                match h.fire_type {
                    FireType::VegetationFire => {
                        type0 += 1;
                        let cy = cells.entry(cell).or_default().entry(year).or_default();
                        cy.months |= 1 << h.acquired.month0();
                        cy.rows += 1;
                    }
                    FireType::OtherStaticLand | FireType::ActiveVolcano => {
                        type2 += 1;
                        static_cells.insert(cell);
                    }
                    _ => {}
                }
            }
        }
    }
    let leaked: u64 = cells
        .iter()
        .filter(|(c, _)| static_cells.contains(c))
        .flat_map(|(_, years)| years.values().map(|cy| cy.rows))
        .sum();
    println!(
        "\n== {country} {from}-{to}: type0 rows {type0}, type1/2 rows {type2}, type0 cells {}, static cells {}, leaked type0 rows in static cells {leaked} ({:.2}% of type0)",
        cells.len(),
        static_cells.len(),
        pct(leaked, type0)
    );
    println!(
        "  m | per-year mask: cell-years  rows-dropped  %type0  %dropped-in-static-cells  %leaked-caught | all-year mask (>=3 yrs): cells  %type0"
    );
    for m in [3u32, 4, 5, 6, 7, 8, 9, 10, 11, 12] {
        let (mut cy_n, mut dropped, mut dropped_static, mut leak_caught) = (0u64, 0u64, 0u64, 0u64);
        let (mut cells3, mut rows3) = (0u64, 0u64);
        for (cell, years) in &cells {
            let is_static = static_cells.contains(cell);
            let mut persistent_years = 0;
            for cy in years.values() {
                if cy.months.count_ones() >= m {
                    persistent_years += 1;
                    cy_n += 1;
                    dropped += cy.rows;
                    if is_static {
                        dropped_static += cy.rows;
                        leak_caught += cy.rows;
                    }
                }
            }
            if persistent_years >= 3 {
                cells3 += 1;
                rows3 += years.values().map(|cy| cy.rows).sum::<u64>();
            }
        }
        println!(
            "{m:>3} | {cy_n:>10}  {dropped:>12}  {:>6.2}  {:>24.1}  {:>14.1} | {cells3:>5}  {:>6.2}",
            pct(dropped, type0),
            pct(dropped_static, dropped),
            pct(leak_caught, leaked),
            pct(rows3, type0),
        );
    }
    Ok(())
}

#[derive(Default)]
struct Group {
    rows: u64,
    isolated: u64,
    low: u64,
    frp: Vec<f64>,
}

impl Group {
    fn line(&mut self, name: &str) -> String {
        self.frp.sort_by(f64::total_cmp);
        let median = self.frp.get(self.frp.len() / 2).copied().unwrap_or(f64::NAN);
        format!(
            "{name:<22} rows {:>9}  isolated {:>5.1}%  low-conf {:>5.1}%  median FRP {median:>6.2}",
            self.rows,
            pct(self.isolated, self.rows),
            pct(self.low, self.rows)
        )
    }
}

fn sama_report(dir: &Path, from: i32, to: i32, country: &str) -> Result<()> {
    let sama = SamaConfig::default();
    // [family][0 = day, 1 = night inside box, 2 = night outside box]
    let mut groups: [[Group; 3]; 2] = Default::default();
    for year in from..=to {
        let mut rows = Vec::new();
        for (sensor_dir, sensor) in SENSORS {
            rows.extend(
                load(dir, sensor_dir, sensor, year, country)?
                    .into_iter()
                    .filter(|h| h.fire_type.is_wildfire()),
            );
        }
        // ~2.2 km buckets; a 3x3 neighbourhood covers every pair within 2.2 km.
        let key = |h: &Hotspot| ((h.lat / 0.02).floor() as i64, (h.lon / 0.02).floor() as i64);
        let mut buckets: HashMap<(i64, i64), Vec<(i64, usize)>> = HashMap::new();
        for (i, h) in rows.iter().enumerate() {
            buckets
                .entry(key(h))
                .or_default()
                .push((h.acquired.num_days_from_ce() as i64, i));
        }
        for (i, h) in rows.iter().enumerate() {
            let (ky, kx) = key(h);
            let day = h.acquired.num_days_from_ce() as i64;
            let isolated = !(-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    buckets.get(&(ky + dy, kx + dx)).is_some_and(|v| {
                        v.iter().any(|&(d, j)| j != i && (d - day).abs() <= 2)
                    })
                })
            });
            let family = usize::from(h.sensor.is_viirs());
            let slot = match h.daynight {
                DayNight::Day => 0,
                DayNight::Night if in_sama_box(h.lat, h.lon, &sama) => 1,
                DayNight::Night => 2,
            };
            let g = &mut groups[family][slot];
            g.rows += 1;
            g.isolated += u64::from(isolated);
            g.low += u64::from(h.confidence == Confidence::Low);
            g.frp.push(h.frp);
        }
    }
    println!("\n== {country} {from}-{to}");
    for (family, name) in [(0, "MODIS"), (1, "VIIRS")] {
        let total: u64 = groups[family].iter().map(|g| g.rows).sum();
        for (slot, label) in [(0, "day"), (1, "night in SAMA box"), (2, "night outside box")] {
            let g = &mut groups[family][slot];
            println!("  {name} {}  share {:>5.2}%", g.line(label), pct(g.rows, total));
        }
    }
    Ok(())
}

fn pipeline_report(dir: &Path, from: i32, to: i32, country: &str) -> Result<()> {
    let default = PipelineConfig::default();
    let mut with_sama = default.clone();
    with_sama.sama.enabled = true;
    for year in from..=to {
        let (mut modis, mut viirs) = (Vec::new(), Vec::new());
        for (sensor_dir, sensor) in SENSORS {
            let rows = load(dir, sensor_dir, sensor, year, country)?
                .into_iter()
                .filter(|h| h.fire_type.is_wildfire());
            if sensor.is_modis() { modis.extend(rows) } else { viirs.extend(rows) }
        }
        let input = (modis.len() + viirs.len()) as u64;
        let on = harmonize_with_links_config(modis.clone(), viirs.clone(), &default);
        let sama = harmonize_with_links_config(modis, viirs, &with_sama);
        println!(
            "{country} {year}: type0 rows {input}, static_masked {} ({:.2}%), sama_removed if enabled {} ({:.2}%), harmonized {}",
            on.static_masked,
            pct(on.static_masked as u64, input),
            sama.sama_removed,
            pct(sama.sama_removed as u64, input),
            on.hotspots.len()
        );
    }
    Ok(())
}

fn pct(a: u64, b: u64) -> f64 {
    if b == 0 { 0.0 } else { 100.0 * a as f64 / b as f64 }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return Ok(());
    }
    if args.len() < 4 {
        bail!("expected <mask|sama|pipeline> <from-year> <to-year> <Country>... (try --help)");
    }
    let from: i32 = args[1].parse().context("invalid from-year")?;
    let to: i32 = args[2].parse().context("invalid to-year")?;
    let dir = archive_dir();
    for country in &args[3..] {
        match args[0].as_str() {
            "mask" => mask_report(&dir, from, to, country)?,
            "sama" => sama_report(&dir, from, to, country)?,
            "pipeline" => pipeline_report(&dir, from, to, country)?,
            other => bail!("unknown mode {other} (try --help)"),
        }
    }
    Ok(())
}
