use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::Datelike;
use pyrosync_core::calibration::calibration_for;
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::harmonize::harmonize_with_links_config;
use pyrosync_core::models::{Hotspot, HotspotSource, Sensor};
use pyrosync_core::vza;
use pyrosync_firms::archive::ArchiveClient;
use pyrosync_firms::parse::retain_wildfires;
use tracing_subscriber::EnvFilter;

const ARCHIVE_FETCH_SENSORS: [Sensor; 3] =
    [Sensor::ModisTerra, Sensor::ViirsSNPP, Sensor::ViirsNOAA20];
const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";

struct Args {
    country: String,
    year: i32,
    month: Option<u32>,
    lat_min: Option<f64>,
    lat_max: Option<f64>,
}

fn print_usage() {
    println!(
        "\
Usage: calibration_check <Country> <year> [--month <1..=12>] [--lat-min <deg>] [--lat-max <deg>]

Compare empirical VIIRS / VZA-corrected-MODIS FRP ratios on Matched pairs against
the Li et al. (2018) latitude-band calibration model.
Archive directory: ARCHIVE_CACHE_DIR env var, or {DEFAULT_ARCHIVE_CACHE_DIR}."
    );
}

fn parse_args() -> Result<Option<Args>> {
    let mut raw_args = std::env::args().skip(1);
    let mut positionals: Vec<String> = Vec::new();
    let mut month: Option<u32> = None;
    let mut lat_min: Option<f64> = None;
    let mut lat_max: Option<f64> = None;

    while let Some(arg) = raw_args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return Ok(None);
            }
            "--month" => {
                let val = raw_args.next().context("missing value for --month")?;
                let m: u32 = val
                    .parse()
                    .with_context(|| format!("invalid --month: {val}"))?;
                if !(1..=12).contains(&m) {
                    bail!("--month must be in 1..=12, got {m}");
                }
                month = Some(m);
            }
            "--lat-min" => {
                let val = raw_args.next().context("missing value for --lat-min")?;
                lat_min = Some(
                    val.parse()
                        .with_context(|| format!("invalid --lat-min: {val}"))?,
                );
            }
            "--lat-max" => {
                let val = raw_args.next().context("missing value for --lat-max")?;
                lat_max = Some(
                    val.parse()
                        .with_context(|| format!("invalid --lat-max: {val}"))?,
                );
            }
            other if other.starts_with('-') => {
                bail!("unknown flag: {other} (try --help)");
            }
            other => positionals.push(other.to_string()),
        }
    }

    if positionals.len() != 2 {
        bail!("expected <Country> <year> [options] (try --help)");
    }

    let country = positionals.remove(0);
    let year_str = positionals.remove(0);
    let year: i32 = year_str
        .parse()
        .with_context(|| format!("invalid year: {year_str}"))?;

    if let (Some(lo), Some(hi)) = (lat_min, lat_max)
        && lo > hi
    {
        bail!("--lat-min ({lo}) must be <= --lat-max ({hi})");
    }

    Ok(Some(Args {
        country,
        year,
        month,
        lat_min,
        lat_max,
    }))
}

fn archive_cache_dir() -> PathBuf {
    match std::env::var_os("ARCHIVE_CACHE_DIR") {
        Some(val) if !val.is_empty() => PathBuf::from(val),
        _ => PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR),
    }
}

fn retain_filters(hotspots: &mut Vec<Hotspot>, args: &Args) {
    retain_wildfires(hotspots);
    hotspots.retain(|h| {
        if let Some(m) = args.month
            && h.acquired.month() != m
        {
            return false;
        }
        if let Some(min_lat) = args.lat_min
            && h.lat < min_lat
        {
            return false;
        }
        if let Some(max_lat) = args.lat_max
            && h.lat > max_lat
        {
            return false;
        }
        true
    });
}

fn band_bucket(abs_lat: f64) -> usize {
    if abs_lat <= 30.0 {
        0
    } else if abs_lat <= 50.0 {
        1
    } else if abs_lat < 55.0 {
        2
    } else if abs_lat <= 70.0 {
        3
    } else {
        4
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    debug_assert!(!sorted.is_empty());
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = p * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    sorted[lo] + frac * (sorted[hi] - sorted[lo])
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

    let mut modis = Vec::new();
    let mut viirs = Vec::new();

    for sensor in ARCHIVE_FETCH_SENSORS {
        if !archive.covers(sensor, args.year) {
            continue;
        }
        match archive
            .fetch_country_year(sensor, &args.country, args.year)
            .await
        {
            Ok(mut hotspots) => {
                retain_filters(&mut hotspots, &args);
                if sensor.is_modis() {
                    modis.append(&mut hotspots);
                } else {
                    viirs.append(&mut hotspots);
                }
            }
            Err(err) => {
                eprintln!("warning: {sensor:?} {} skipped: {err}", args.year);
            }
        }
    }

    if modis.is_empty() || viirs.is_empty() {
        eprintln!(
            "error: insufficient MODIS ({}) or VIIRS ({}) wildfire detections for {} {} in {}",
            modis.len(),
            viirs.len(),
            args.country,
            args.year,
            cache_dir.display()
        );
        std::process::exit(1);
    }

    let config = PipelineConfig::default();
    let links = harmonize_with_links_config(modis.clone(), viirs.clone(), &config);

    let mut modis_members: Vec<Vec<usize>> = vec![Vec::new(); links.hotspots.len()];
    for (idx, maybe_out) in links.modis_links.iter().enumerate() {
        if let Some(out_idx) = *maybe_out {
            modis_members[out_idx].push(idx);
        }
    }

    let mut viirs_members: Vec<Vec<usize>> = vec![Vec::new(); links.hotspots.len()];
    for (idx, maybe_out) in links.viirs_links.iter().enumerate() {
        if let Some(out_idx) = *maybe_out {
            viirs_members[out_idx].push(idx);
        }
    }

    const BAND_LABELS: [&str; 5] = [
        "Tropics (0..=30)",
        "Mid-lat (30..=50)",
        "Transition (50..55)",
        "Boreal (55..=70)",
        "Polar (>70)",
    ];
    let mut ratios_by_band: [Vec<f64>; 5] = Default::default();
    let mut model_by_band: [Vec<f64>; 5] = Default::default();

    for (out_idx, h) in links.hotspots.iter().enumerate() {
        if h.source != HotspotSource::Matched {
            continue;
        }
        let m_idxs = &modis_members[out_idx];
        let v_idxs = &viirs_members[out_idx];
        if m_idxs.is_empty() || v_idxs.is_empty() {
            continue;
        }

        let modis_vza_frp = m_idxs
            .iter()
            .map(|&i| {
                let member = &modis[i];
                vza::apply_vza_correction(member.frp, vza::vza_factor_with(member, &config.vza))
            })
            .sum::<f64>()
            / m_idxs.len() as f64;
        let viirs_frp = v_idxs.iter().map(|&j| viirs[j].frp).sum::<f64>() / v_idxs.len() as f64;

        if !modis_vza_frp.is_finite()
            || modis_vza_frp <= 0.0
            || !viirs_frp.is_finite()
            || viirs_frp <= 0.0
        {
            continue;
        }

        let bucket = band_bucket(h.lat.abs());
        ratios_by_band[bucket].push(viirs_frp / modis_vza_frp);
        model_by_band[bucket].push(calibration_for(h.lat, &config.calibration).ratio);
    }

    println!(
        "Calibration check: country={} year={} month={:?} lat={:?}..={:?}",
        args.country, args.year, args.month, args.lat_min, args.lat_max
    );
    println!(
        "Input detections: modis={} viirs={} | Matched pairs: {}",
        modis.len(),
        viirs.len(),
        links.matched_pairs
    );
    println!();
    println!(
        "{:<22} {:>8} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "band", "n", "emp_med", "q25", "q75", "iqr", "model"
    );
    println!("{}", "-".repeat(86));

    for (b, label) in BAND_LABELS.iter().enumerate() {
        let vals = &mut ratios_by_band[b];
        if vals.is_empty() {
            continue;
        }
        vals.sort_by(f64::total_cmp);
        let q25 = percentile(vals, 0.25);
        let med = percentile(vals, 0.50);
        let q75 = percentile(vals, 0.75);
        let iqr = q75 - q25;

        let models = &mut model_by_band[b];
        models.sort_by(f64::total_cmp);
        let model_med = percentile(models, 0.50);

        println!(
            "{:<22} {:>8} {:>10.4} {:>10.4} {:>10.4} {:>10.4} {:>10.4}",
            label,
            vals.len(),
            med,
            q25,
            q75,
            iqr,
            model_med
        );
    }

    Ok(())
}
