//! Turkey case study (S7): derived files for `data/case-study/turkey/` from the offline FIRMS
//! country archive. Each region is harmonized over 2012-2025 (the VIIRS era: one instrument family
//! from S-NPP on, so a long same-sensor baseline), its monthly calendar is built with the core path
//! (`build_calendar_with`: effort-normalised `count_normalized` and its `anomaly`) and written in
//! the `/api/calendar` response shape to `calendar/<region>.json`. `stats.json` collects the counts,
//! peak months, trend and the 2021 anomaly evidence. The archive is never modified.
//!
//! Usage:
//!   ARCHIVE_CACHE_DIR=../../.cache/firms-archive \
//!     cargo run --release -p pyrosync-firms --example turkey_case_study -- [<out-dir>]
//!
//! `<out-dir>` defaults to `data/case-study/turkey`. Raw rows and harmonized hotspots of the
//! Mediterranean box are not written here: `sample_region` makes them (see the README).

use anyhow::{Context, Result, ensure};
use chrono::{Datelike, NaiveDate};
use pyrosync_core::calendar::{build_calendar_with, calendar_swath_gap, seasonality};
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::effort::estimate_effort;
use pyrosync_core::harmonize::harmonize_with_links_config;
use pyrosync_core::models::{BBox, CalendarCell, HarmonizedHotspot, Hotspot, SeasonYear, Sensor};
use pyrosync_firms::archive::{ArchiveClient, ArchiveError, filter_bbox};
use pyrosync_firms::parse::retain_wildfires;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

const COUNTRY: &str = "Turkey";
const DEFAULT_OUT_DIR: &str = "data/case-study/turkey";
const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const FROM_YEAR: i32 = 2012;
const TO_YEAR: i32 = 2025;
const EVENT_YEAR: i32 = 2021;
/// First year with the same instrument set (Terra, Aqua, S-NPP, NOAA-20): raw counts are comparable
/// from here on without effort normalisation.
const SAME_FLEET_FROM: i32 = 2018;
const SENSORS: [Sensor; 3] = [Sensor::ModisTerra, Sensor::ViirsSNPP, Sensor::ViirsNOAA20];
const COVERAGE_NOTE: &str = "The FIRMS country archive packages MODIS, VIIRS SNPP and VIIRS NOAA-20; NOAA-21 is not included, so this calendar covers a smaller fleet than the near-real-time endpoints from 2023 onward.";

struct Region {
    id: &'static str,
    name: &'static str,
    bbox: BBox,
}

fn regions() -> [Region; 4] {
    let bbox = |west, south, east, north| BBox { west, south, east, north };
    [
        Region { id: "turkey", name: "Turkey, whole-country box", bbox: bbox(26.0, 36.0, 45.0, 42.0) },
        Region { id: "mediterranean", name: "Antalya-Mugla Mediterranean coast", bbox: bbox(27.0, 36.0, 32.5, 37.5) },
        Region { id: "mugla", name: "Mugla (Marmaris, Milas, Bodrum, Koycegiz)", bbox: bbox(27.0, 36.0, 29.4, 37.5) },
        Region { id: "antalya", name: "Antalya (Manavgat, Kemer, Alanya, Gundogmus)", bbox: bbox(29.4, 36.0, 32.5, 37.5) },
    ]
}

fn archive_cache_dir() -> PathBuf {
    match std::env::var_os("ARCHIVE_CACHE_DIR") {
        Some(val) if !val.is_empty() => PathBuf::from(val),
        _ => PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR),
    }
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    std::fs::create_dir_all(path.parent().context("output path has no parent")?)?;
    let mut writer = BufWriter::new(File::create(path).with_context(|| format!("cannot create {}", path.display()))?);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writeln!(writer)?;
    writer.flush()?;
    Ok(())
}

/// Population mean and standard deviation, as `compute_anomalies` uses for the z-score.
fn mean_sd(values: &[f64]) -> Option<(f64, f64)> {
    if values.is_empty() {
        return None;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let sd = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt();
    Some((mean, sd))
}

/// Same rule as `compute_anomalies`: at least 3 baseline values and a non-zero spread.
fn z_score(value: f64, baseline: &[f64]) -> Option<f64> {
    mean_sd(baseline)
        .filter(|&(_, sd)| baseline.len() >= 3 && sd > 0.0)
        .map(|(mean, sd)| (value - mean) / sd)
}

/// 1 = largest value.
fn rank_of(value: f64, values: &[f64]) -> usize {
    1 + values.iter().filter(|&&v| v > value).count()
}

fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) { (values[mid - 1] + values[mid]) / 2.0 } else { values[mid] }
}

/// OLS slope per year and Pearson r of `(year, value)` points.
fn trend(points: &[(i32, f64)]) -> Option<(f64, f64)> {
    let n = points.len() as f64;
    let mean_x = points.iter().map(|p| f64::from(p.0)).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxy: f64 = points.iter().map(|p| (f64::from(p.0) - mean_x) * (p.1 - mean_y)).sum();
    let sxx: f64 = points.iter().map(|p| (f64::from(p.0) - mean_x).powi(2)).sum();
    let syy: f64 = points.iter().map(|p| (p.1 - mean_y).powi(2)).sum();
    (sxx > 0.0 && syy > 0.0).then(|| (sxy / sxx, sxy / (sxx * syy).sqrt()))
}

/// The core z-score of one cell, with the numbers needed to check it: same-calendar-month baseline,
/// ranks, the leave-one-out z (baseline without the cell itself) and a same-fleet check on raw
/// harmonized counts that needs no effort normalisation.
fn anomaly_evidence(cells: &[CalendarCell], year: i32, month: u32) -> Result<Value> {
    let Some(cell) = cells.iter().find(|c| c.year == year && c.month == month) else {
        return Ok(Value::Null);
    };
    let same_month: Vec<&CalendarCell> = cells.iter().filter(|c| c.month == month).collect();
    let normalized: Vec<f64> = same_month.iter().filter_map(|c| c.count_normalized).collect();
    let all_normalized: Vec<f64> = cells.iter().filter_map(|c| c.count_normalized).collect();
    let frp: Vec<f64> = same_month.iter().map(|c| c.total_frp).collect();
    let fleet: Vec<f64> = same_month.iter().filter(|c| c.year >= SAME_FLEET_FROM).map(|c| c.count as f64).collect();
    let others: Vec<f64> = same_month
        .iter()
        .filter(|c| c.year != year)
        .filter_map(|c| c.count_normalized)
        .collect();

    let others_median = (!others.is_empty()).then(|| median(others.clone()));
    let z = cell.count_normalized.and_then(|v| z_score(v, &normalized));
    if let (Some(mine), Some(core)) = (z, cell.anomaly) {
        ensure!((mine - core).abs() < 1e-9, "z-score mismatch for {year}-{month:02}: {mine} vs core {core}");
    }
    ensure!(z.is_some() == cell.anomaly.is_some(), "anomaly presence differs from the recomputed z for {year}-{month:02}");

    Ok(json!({
        "year": year,
        "month": month,
        "count": cell.count,
        "count_normalized": cell.count_normalized,
        "effort_omega": cell.effort_omega,
        "anomaly_z": cell.anomaly,
        "baseline_years": normalized.len(),
        "baseline_mean": mean_sd(&normalized).map(|m| m.0),
        "baseline_sd": mean_sd(&normalized).map(|m| m.1),
        "rank_same_month": cell.count_normalized.map(|v| rank_of(v, &normalized)),
        "of_same_month": normalized.len(),
        "rank_all_months": cell.count_normalized.map(|v| rank_of(v, &all_normalized)),
        "of_all_months": all_normalized.len(),
        "z_leave_one_out": cell.count_normalized.and_then(|v| z_score(v, &others)),
        "others_median": others_median,
        "ratio_to_others_median": cell.count_normalized.zip(others_median).filter(|&(_, m)| m > 0.0).map(|(v, m)| v / m),
        "total_frp_mw": cell.total_frp,
        "total_frp_rank_same_month": rank_of(cell.total_frp, &frp),
        "frp_per_detection_eq": cell.frp_per_detection_eq,
        "same_fleet_raw_count": {
            "from_year": SAME_FLEET_FROM,
            "years": fleet.len(),
            "z": z_score(cell.count as f64, &fleet),
            "rank": rank_of(cell.count as f64, &fleet),
        },
    }))
}

/// Harmonized detections and summed corrected FRP per day over the event window, next to the median of the
/// same calendar day in the other same-fleet years.
fn daily_event_series(hotspots: &[HarmonizedHotspot]) -> Result<Value> {
    let mut by_day: BTreeMap<NaiveDate, (u64, f64)> = BTreeMap::new();
    for h in hotspots {
        let entry = by_day.entry(h.acquired.date_naive()).or_default();
        entry.0 += 1;
        entry.1 += h.frp_corrected;
    }
    let mut days = Vec::new();
    let mut day = NaiveDate::from_ymd_opt(EVENT_YEAR, 7, 20).context("invalid event window start")?;
    let last = NaiveDate::from_ymd_opt(EVENT_YEAR, 8, 15).context("invalid event window end")?;
    while day <= last {
        let reference: Vec<f64> = (SAME_FLEET_FROM..=TO_YEAR)
            .filter(|&y| y != EVENT_YEAR)
            .filter_map(|y| NaiveDate::from_ymd_opt(y, day.month(), day.day()))
            .map(|d| by_day.get(&d).map_or(0.0, |v| v.0 as f64))
            .collect();
        let (count, frp) = by_day.get(&day).copied().unwrap_or_default();
        days.push(json!({
            "date": day.to_string(),
            "harmonized_count": count,
            "total_frp_mw": frp,
            "same_day_median_other_years": median(reference),
        }));
        day = day.succ_opt().context("date overflow")?;
    }
    Ok(Value::Array(days))
}

struct Loaded {
    modis: Vec<Hotspot>,
    viirs: Vec<Hotspot>,
}

async fn load(archive: &ArchiveClient, bbox: &BBox) -> Result<Loaded> {
    let mut loaded = Loaded { modis: Vec::new(), viirs: Vec::new() };
    for year in FROM_YEAR..=TO_YEAR {
        for sensor in SENSORS {
            if !archive.covers(sensor, year) {
                continue;
            }
            let mut rows = match archive.fetch_country_year(sensor, COUNTRY, year).await {
                Ok(rows) => rows,
                // A completed archive year without this country file means zero detections.
                Err(ArchiveError::NotCached { .. }) if archive.sensor_year_complete(sensor, year) => continue,
                Err(err) => return Err(err).with_context(|| format!("failed to load {sensor:?} {COUNTRY} {year}")),
            };
            retain_wildfires(&mut rows);
            let mut rows = filter_bbox(rows, bbox);
            if sensor.is_modis() {
                loaded.modis.append(&mut rows);
            } else {
                loaded.viirs.append(&mut rows);
            }
        }
    }
    ensure!(!loaded.modis.is_empty() && !loaded.viirs.is_empty(), "no archive rows for {COUNTRY} {FROM_YEAR}..={TO_YEAR}");
    Ok(loaded)
}

/// Returns the `/api/calendar`-shaped response and the region's entry for `stats.json`.
fn run_region(region: &Region, all: &Loaded, archive: &ArchiveClient, config: &PipelineConfig) -> Result<(Value, Value)> {
    let modis = filter_bbox(all.modis.clone(), &region.bbox);
    let viirs = filter_bbox(all.viirs.clone(), &region.bbox);

    // Per archive file (MODIS stands for the combined Terra+Aqua file), as /api/calendar reports it.
    let mut tally: BTreeMap<(i32, Sensor), usize> = BTreeMap::new();
    for h in modis.iter().chain(&viirs) {
        let file_sensor = if h.sensor.is_modis() { Sensor::ModisTerra } else { h.sensor };
        *tally.entry((h.acquired.year(), file_sensor)).or_default() += 1;
    }
    let per_sensor_year: Vec<Value> = (FROM_YEAR..=TO_YEAR)
        .flat_map(|year| SENSORS.map(|sensor| (year, sensor)))
        .filter(|&(year, sensor)| archive.covers(sensor, year))
        .map(|(year, sensor)| json!({"sensor": sensor, "year": year, "detections": tally.get(&(year, sensor)).copied().unwrap_or(0)}))
        .collect();

    let raw_total = modis.len() + viirs.len();
    let links = harmonize_with_links_config(modis, viirs, config);
    let hotspots = &links.hotspots;
    let cells = build_calendar_with(hotspots, &config.uncertainty, &config.calibration);
    let first_month = (FROM_YEAR, 1_u32).max((2000, 11));
    let season_coverage = (first_month, (TO_YEAR, 12_u32));

    let calendar_json = json!({
        "country": COUNTRY,
        "years": {"requested_from": FROM_YEAR, "requested_to": TO_YEAR, "effective_from": FROM_YEAR, "effective_to": TO_YEAR},
        "bbox": region.bbox,
        "coverage": {
            "sensors_available": ["ModisTerra", "ModisAqua", "ViirsSNPP", "ViirsNOAA20"],
            "sensors_unavailable": ["ViirsNOAA21"],
            "note": COVERAGE_NOTE,
        },
        "counts": {"raw_detections": raw_total, "harmonized_records": hotspots.len()},
        "seasonality": {
            "metric": "count",
            "calendar_year": seasonality(&cells, SeasonYear::Calendar, season_coverage),
            "fire_year": seasonality(&cells, SeasonYear::FireYear, season_coverage),
        },
        "calendar": cells,
        "effort": {
            "reliable": true,
            "reason": null,
            "factors": estimate_effort(hotspots),
            "tropical_swath_gap": calendar_swath_gap(Some(&region.bbox), hotspots, &config.uncertainty),
        },
        "filters": {"min_confidence": "low", "excluded_detections": 0},
        "fetch": {"per_sensor_year": per_sensor_year, "failures": []},
        "warnings": [COVERAGE_NOTE],
        "meta": {
            "case_study": "turkey",
            "region": region.id,
            "name": region.name,
            "config": "PipelineConfig::default()",
            "wildfire_only": true,
            "static_masked": links.static_masked,
            "sama_removed": links.sama_removed,
            "pipeline": {
                "interscan_removed": links.interscan_removed,
                "fleet_merged": links.fleet_merged,
                "matched_pairs": links.matched_pairs,
            },
        },
    });

    let mut harmonized_by_year: BTreeMap<i32, usize> = BTreeMap::new();
    for h in hotspots {
        *harmonized_by_year.entry(h.acquired.year()).or_default() += 1;
    }
    let cells_of = |year: i32| cells.iter().filter(move |c| c.year == year);
    let per_year: Vec<Value> = (FROM_YEAR..=TO_YEAR)
        .map(|year| {
            let normalized: Vec<f64> = cells_of(year).filter_map(|c| c.count_normalized).collect();
            let peak = cells_of(year).max_by_key(|c| (c.count, std::cmp::Reverse(c.month)));
            let raw = |sensor| tally.get(&(year, sensor)).copied().unwrap_or(0);
            json!({
                "year": year,
                "raw_modis": raw(Sensor::ModisTerra),
                "raw_viirs_snpp": raw(Sensor::ViirsSNPP),
                "raw_viirs_noaa20": raw(Sensor::ViirsNOAA20),
                "raw_total": SENSORS.iter().map(|&s| raw(s)).sum::<usize>(),
                "harmonized": harmonized_by_year.get(&year).copied().unwrap_or(0),
                "months_with_data": cells_of(year).count(),
                "months_with_normalized": normalized.len(),
                "count_normalized_sum": (!normalized.is_empty()).then(|| normalized.iter().sum::<f64>()),
                "total_frp_mw": cells_of(year).map(|c| c.total_frp).sum::<f64>(),
                "peak_month": peak.map(|c| c.month),
                "peak_month_count": peak.map(|c| c.count),
            })
        })
        .collect();

    let climatology: Vec<Value> = (1..=12_u32)
        .map(|month| {
            let in_month: Vec<&CalendarCell> = cells.iter().filter(|c| c.month == month).collect();
            let normalized: Vec<f64> = in_month.iter().filter_map(|c| c.count_normalized).collect();
            json!({
                "month": month,
                "years_with_data": in_month.len(),
                "mean_count": (!in_month.is_empty()).then(|| in_month.iter().map(|c| c.count as f64).sum::<f64>() / in_month.len() as f64),
                "years_with_normalized": normalized.len(),
                "mean_count_normalized": mean_sd(&normalized).map(|m| m.0),
            })
        })
        .collect();

    // Only full years (12 months with a normalised count) enter the trend: partial years would bias the sum.
    let full_years: Vec<(i32, f64)> = (FROM_YEAR..=TO_YEAR)
        .filter_map(|year| {
            let normalized: Vec<f64> = cells_of(year).filter_map(|c| c.count_normalized).collect();
            (normalized.len() == 12).then(|| (year, normalized.iter().sum::<f64>()))
        })
        .collect();
    let trend_json = if full_years.len() >= 8 {
        let mean = full_years.iter().map(|p| p.1).sum::<f64>() / full_years.len() as f64;
        trend(&full_years).map(|(slope, r)| {
            json!({
                "metric": "annual sum of count_normalized",
                "years": full_years.iter().map(|p| p.0).collect::<Vec<_>>(),
                "slope_per_year": slope,
                "slope_pct_of_mean_per_year": 100.0 * slope / mean,
                "pearson_r": r,
            })
        })
    } else {
        None
    };

    let stats = json!({
        "name": region.name,
        "bbox": region.bbox,
        "totals": {
            "raw_total": raw_total,
            "raw_modis": tally.iter().filter(|(k, _)| k.1 == Sensor::ModisTerra).map(|(_, n)| n).sum::<usize>(),
            "raw_viirs_snpp": tally.iter().filter(|(k, _)| k.1 == Sensor::ViirsSNPP).map(|(_, n)| n).sum::<usize>(),
            "raw_viirs_noaa20": tally.iter().filter(|(k, _)| k.1 == Sensor::ViirsNOAA20).map(|(_, n)| n).sum::<usize>(),
            "harmonized": hotspots.len(),
            "static_masked": links.static_masked,
            "interscan_removed": links.interscan_removed,
            "fleet_merged": links.fleet_merged,
            "matched_pairs": links.matched_pairs,
        },
        "per_year": per_year,
        "climatology": climatology,
        "trend": trend_json,
        "anomaly_2021": {
            "july": anomaly_evidence(&cells, EVENT_YEAR, 7)?,
            "august": anomaly_evidence(&cells, EVENT_YEAR, 8)?,
        },
        "daily_2021": daily_event_series(hotspots)?,
    });
    Ok((calendar_json, stats))
}

#[tokio::main]
async fn main() -> Result<()> {
    let out_dir = std::env::args().nth(1).map_or_else(|| PathBuf::from(DEFAULT_OUT_DIR), PathBuf::from);
    let cache_dir = archive_cache_dir();
    let archive = ArchiveClient::offline(cache_dir.clone());
    let config = PipelineConfig::default();
    let regions = regions();
    println!("Archive: {}", cache_dir.display());

    let all = load(&archive, &regions[0].bbox).await?;
    println!("Loaded {COUNTRY} {FROM_YEAR}..={TO_YEAR}: modis={} viirs={}", all.modis.len(), all.viirs.len());

    let mut stats = BTreeMap::new();
    for region in &regions {
        let (calendar_json, region_stats) = run_region(region, &all, &archive, &config)?;
        write_json(&out_dir.join("calendar").join(format!("{}.json", region.id)), &calendar_json)?;
        println!("\n== {} ({})", region.id, region.name);
        println!("totals: {}", region_stats["totals"]);
        for month in ["july", "august"] {
            let e = &region_stats["anomaly_2021"][month];
            println!(
                "2021 {month}: count={} normalized={} z={} rank={}/{} (all months {}/{}) z_loo={} x{} of the median of other years; same-fleet z={} rank={}/{}",
                e["count"], e["count_normalized"], e["anomaly_z"], e["rank_same_month"], e["of_same_month"],
                e["rank_all_months"], e["of_all_months"], e["z_leave_one_out"], e["ratio_to_others_median"],
                e["same_fleet_raw_count"]["z"], e["same_fleet_raw_count"]["rank"], e["same_fleet_raw_count"]["years"],
            );
        }
        stats.insert(region.id, region_stats);
    }

    let summary = json!({
        "meta": {
            "case_study": "turkey",
            "source": "FIRMS country archive (SP): modis, viirs-snpp, viirs-jpss1",
            "country_file": COUNTRY,
            "years": [FROM_YEAR, TO_YEAR],
            "event_year": EVENT_YEAR,
            "same_fleet_from": SAME_FLEET_FROM,
            "config": "PipelineConfig::default()",
            "wildfire_only": true,
            "mediterranean_hotspots": "data/sample/turkey (sample_region, 2020-2025, bbox 27,36,32.5,37.5): same box, same pipeline, so its per-year and per-day harmonized counts equal this file's",
            "anomaly": "core calendar path: z-score of the effort-normalised monthly count (count_normalized) against the same calendar month over 2012-2025, population standard deviation, the cell itself included in its baseline",
        },
        "regions": stats,
    });
    write_json(&out_dir.join("stats.json"), &summary)?;
    println!("\nWrote {}", out_dir.display());
    Ok(())
}
