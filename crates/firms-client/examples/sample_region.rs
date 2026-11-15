//! Cut one region out of the offline FIRMS country archive into demo files for frontend and
//! chart work: `modis.csv` and `viirs.csv` (original FIRMS rows inside the bounding box,
//! wildfire `type 0` only, i.e. exactly what the pipeline receives) and `harmonized.json` (the
//! `/api/harmonize` response shape, built with `PipelineConfig::default()` from those rows).
//! The archive is never modified.
//!
//! Usage:
//!   ARCHIVE_CACHE_DIR=../../.cache/firms-archive \
//!     cargo run --release -p pyrosync-firms --example sample_region -- \
//!     --out <dir> --name <label> --bbox <west,south,east,north> [--from 2020] [--to 2025] <Country>...
//!
//! `<Country>` is the archive file-name suffix (`Turkey`, `Australia`, `United_States`); a
//! region that crosses borders lists every country file it needs.

use anyhow::{Context, Result, bail, ensure};
use chrono::Datelike;
use pyrosync_core::calendar::{build_calendar_with, calendar_swath_gap};
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::effort::{estimate_effort, pre_aqua_excluded_months};
use pyrosync_core::harmonize::harmonize_with_links_config;
use pyrosync_core::models::{BBox, Hotspot, Sensor};
use pyrosync_firms::archive::{ArchiveClient, ArchiveError, archive_slug, csv_rows_in_bbox};
use pyrosync_firms::parse::parse_csv;
use pyrosync_firms::source::{FirmsSource, Processing};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

const SENSORS: [Sensor; 3] = [Sensor::ModisTerra, Sensor::ViirsSNPP, Sensor::ViirsNOAA20];
const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const DEFAULT_FROM_YEAR: i32 = 2020;
const DEFAULT_TO_YEAR: i32 = 2025;

struct Args {
    out: PathBuf,
    name: String,
    bbox: BBox,
    from: i32,
    to: i32,
    countries: Vec<String>,
}

/// Rows and parsed detections of one output file (MODIS, or SNPP plus NOAA-20 for VIIRS).
#[derive(Default)]
struct Family {
    header: String,
    rows: Vec<String>,
    hotspots: Vec<Hotspot>,
    non_wildfire: usize,
}

fn print_usage() {
    println!(
        "\
Usage: sample_region --out <dir> --name <label> --bbox <west,south,east,north> [--from {DEFAULT_FROM_YEAR}] [--to {DEFAULT_TO_YEAR}] <Country>...

Writes modis.csv, viirs.csv and harmonized.json for one region into <dir>.
Archive directory: ARCHIVE_CACHE_DIR env var, or {DEFAULT_ARCHIVE_CACHE_DIR}."
    );
}

fn parse_bbox(value: &str) -> Result<BBox> {
    let parts: Vec<f64> = value
        .split(',')
        .map(|part| part.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .with_context(|| format!("invalid --bbox {value:?}, expected west,south,east,north"))?;
    let [west, south, east, north] = parts[..] else {
        bail!("invalid --bbox {value:?}, expected west,south,east,north");
    };
    ensure!(
        (-180.0..=180.0).contains(&west) && (-180.0..=180.0).contains(&east) && west < east,
        "--bbox needs -180 <= west < east <= 180"
    );
    ensure!(
        (-90.0..=90.0).contains(&south) && (-90.0..=90.0).contains(&north) && south < north,
        "--bbox needs -90 <= south < north <= 90"
    );
    Ok(BBox { west, south, east, north })
}

fn parse_args() -> Result<Option<Args>> {
    let mut raw_args = std::env::args().skip(1);
    let mut out = None;
    let mut name = None;
    let mut bbox = None;
    let mut from = DEFAULT_FROM_YEAR;
    let mut to = DEFAULT_TO_YEAR;
    let mut countries = Vec::new();

    while let Some(arg) = raw_args.next() {
        let mut value = |flag: &str| raw_args.next().with_context(|| format!("missing value for {flag}"));
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return Ok(None);
            }
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            "--name" => name = Some(value("--name")?),
            "--bbox" => bbox = Some(parse_bbox(&value("--bbox")?)?),
            "--from" => from = value("--from")?.parse().context("invalid --from year")?,
            "--to" => to = value("--to")?.parse().context("invalid --to year")?,
            other if other.starts_with('-') => bail!("unknown flag: {other} (try --help)"),
            other => countries.push(other.to_string()),
        }
    }

    ensure!(from <= to, "--from ({from}) must be <= --to ({to})");
    ensure!(!countries.is_empty(), "expected at least one <Country> argument (try --help)");
    Ok(Some(Args {
        out: out.context("--out is required (try --help)")?,
        name: name.context("--name is required (try --help)")?,
        bbox: bbox.context("--bbox is required (try --help)")?,
        from,
        to,
        countries,
    }))
}

fn archive_cache_dir() -> PathBuf {
    match std::env::var_os("ARCHIVE_CACHE_DIR") {
        Some(val) if !val.is_empty() => PathBuf::from(val),
        _ => PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR),
    }
}

/// Orders rows by (`acq_date`, `acq_time`) so the merged VIIRS file reads chronologically.
fn sort_chronologically(header: &str, rows: &mut [String]) -> Result<()> {
    let column = |name: &str| {
        header
            .split(',')
            .position(|c| c == name)
            .with_context(|| format!("CSV header has no `{name}` column"))
    };
    let (date, time) = (column("acq_date")?, column("acq_time")?);
    rows.sort_by_cached_key(|row| {
        let fields: Vec<&str> = row.split(',').collect();
        (
            fields.get(date).copied().unwrap_or_default().to_string(),
            fields.get(time).and_then(|t| t.parse::<u32>().ok()).unwrap_or(0),
        )
    });
    Ok(())
}

fn write_csv(path: &Path, header: &str, rows: &[String]) -> Result<()> {
    let mut writer = BufWriter::new(File::create(path).with_context(|| format!("cannot create {}", path.display()))?);
    writeln!(writer, "{header}")?;
    for row in rows {
        writeln!(writer, "{row}")?;
    }
    writer.flush()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    let cache_dir = archive_cache_dir();
    let archive = ArchiveClient::offline(cache_dir.clone());
    println!("Archive: {}", cache_dir.display());
    println!(
        "Region {:?} bbox {} years {}..={} countries {}",
        args.name,
        args.bbox.to_firms_string(),
        args.from,
        args.to,
        args.countries.join(", ")
    );

    let mut modis = Family::default();
    let mut viirs = Family::default();
    // Rows inside the bbox before the type filter, per archive source; kept wildfire rows per year.
    let mut in_bbox: BTreeMap<&str, usize> = BTreeMap::new();
    let mut kept_by_year: BTreeMap<(i32, &str), usize> = BTreeMap::new();

    for year in args.from..=args.to {
        for sensor in SENSORS {
            if !archive.covers(sensor, year) {
                continue;
            }
            let slug = archive_slug(sensor).context("sensor without archive slug")?;
            let family = if sensor.is_modis() { &mut modis } else { &mut viirs };
            for country in &args.countries {
                let text = match archive.fetch_country_year_csv(sensor, country, year).await {
                    Ok(text) => text,
                    // A completed archive year without this country file means zero detections.
                    Err(ArchiveError::NotCached { .. }) if archive.sensor_year_complete(sensor, year) => continue,
                    Err(err) => {
                        return Err(err).with_context(|| format!("failed to load {slug} {country} {year}"));
                    }
                };
                let (header, rows) = csv_rows_in_bbox(&text, &args.bbox)
                    .with_context(|| format!("failed to cut {slug} {country} {year}"))?;
                if family.header.is_empty() {
                    family.header = header.to_string();
                }
                ensure!(family.header == header, "{slug} {country} {year}: CSV header differs from the other files of this output");
                *in_bbox.entry(slug).or_default() += rows.len();
                if rows.is_empty() {
                    continue;
                }

                let source = FirmsSource::new(sensor, Processing::Sp);
                let parsed = parse_csv(&format!("{header}\n{}\n", rows.join("\n")), source.as_str())?;
                ensure!(parsed.len() == rows.len(), "{slug} {country} {year}: parsed rows do not match cut rows");
                for (row, hotspot) in rows.into_iter().zip(parsed) {
                    if hotspot.fire_type.is_wildfire() {
                        family.rows.push(row.to_string());
                        family.hotspots.push(hotspot);
                        *kept_by_year.entry((year, slug)).or_default() += 1;
                    } else {
                        family.non_wildfire += 1;
                    }
                }
            }
        }
    }

    ensure!(!modis.rows.is_empty(), "no MODIS wildfire rows inside the bbox");
    ensure!(!viirs.rows.is_empty(), "no VIIRS wildfire rows inside the bbox");
    sort_chronologically(&modis.header, &mut modis.rows)?;
    sort_chronologically(&viirs.header, &mut viirs.rows)?;

    std::fs::create_dir_all(&args.out).with_context(|| format!("cannot create {}", args.out.display()))?;
    write_csv(&args.out.join("modis.csv"), &modis.header, &modis.rows)?;
    write_csv(&args.out.join("viirs.csv"), &viirs.header, &viirs.rows)?;

    let config = PipelineConfig::default();
    let (modis_count, viirs_count) = (modis.hotspots.len(), viirs.hotspots.len());
    let links = harmonize_with_links_config(modis.hotspots, viirs.hotspots, &config);
    let hotspots = &links.hotspots;
    let calendar = build_calendar_with(hotspots, &config.uncertainty, &config.calibration);
    let swath_gap = calendar_swath_gap(Some(&args.bbox), hotspots, &config.uncertainty);
    let pre_aqua = pre_aqua_excluded_months(hotspots).len();
    let warnings: Vec<String> = (pre_aqua > 0)
        .then(|| {
            format!("{pre_aqua} months before 2002-07 (Terra-only era) are not effort-comparable with later months and have no normalized count or anomaly.")
        })
        .into_iter()
        .collect();

    // Same top-level keys as GET /api/harmonize without `uncertainty` (static server text), plus `meta`.
    let response = serde_json::json!({
        "meta": {
            "region": args.name,
            "bbox": args.bbox,
            "years": [args.from, args.to],
            "countries": args.countries,
            "source": "FIRMS country archive (SP): modis, viirs-snpp, viirs-jpss1",
            "config": "PipelineConfig::default()",
            "wildfire_only": true,
            "non_wildfire_dropped": {"modis": modis.non_wildfire, "viirs": viirs.non_wildfire},
            "pipeline": {
                "interscan_removed": links.interscan_removed,
                "fleet_merged": links.fleet_merged,
                "matched_pairs": links.matched_pairs,
            },
        },
        "raw": {
            "modis_count": modis_count,
            "viirs_count": viirs_count,
            "total_raw": modis_count + viirs_count,
        },
        "harmonized": {
            "count": hotspots.len(),
            "static_masked": links.static_masked,
            "sama_removed": links.sama_removed,
            "hotspots": hotspots,
        },
        "calendar": calendar,
        "effort": {
            "reliable": true,
            "reason": null,
            "factors": estimate_effort(hotspots),
            "tropical_swath_gap": swath_gap,
        },
        "filters": {"min_confidence": "low", "excluded_detections": 0},
        "warnings": warnings,
        "report": {
            "per_source": in_bbox.iter().collect::<Vec<_>>(),
            "failures": [],
            "transactions_spent": 0,
            "uncovered": [],
        },
    });
    let json_path = args.out.join("harmonized.json");
    let mut writer = BufWriter::new(File::create(&json_path).with_context(|| format!("cannot create {}", json_path.display()))?);
    serde_json::to_writer(&mut writer, &response)?;
    writer.flush()?;

    let mut harmonized_by_year: BTreeMap<i32, usize> = BTreeMap::new();
    for hotspot in hotspots {
        *harmonized_by_year.entry(hotspot.acquired.year()).or_default() += 1;
    }
    println!("\nyear | modis | viirs-snpp | viirs-jpss1 | harmonized");
    for year in args.from..=args.to {
        let kept = |slug: &str| kept_by_year.get(&(year, slug)).copied().unwrap_or(0);
        println!(
            "{year} | {} | {} | {} | {}",
            kept("modis"),
            kept("viirs-snpp"),
            kept("viirs-jpss1"),
            harmonized_by_year.get(&year).copied().unwrap_or(0)
        );
    }
    println!(
        "total | modis={modis_count} viirs={viirs_count} harmonized={} static_masked={} sama_removed={} non_wildfire_dropped(modis={}, viirs={}) interscan_removed={} fleet_merged={} matched_pairs={}",
        hotspots.len(),
        links.static_masked,
        links.sama_removed,
        modis.non_wildfire,
        viirs.non_wildfire,
        links.interscan_removed,
        links.fleet_merged,
        links.matched_pairs
    );
    println!("Wrote {}", args.out.display());
    Ok(())
}
