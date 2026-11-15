//! Compare Turkey 2010 (pre-VIIRS) vs 2014 (MODIS + VIIRS SNPP) calendar totals to inspect
//! continuity of `total_frp` and MODIS-equivalent `frp_per_detection_eq` across the 2012 sensor
//! transition.
//!
//! Run with:
//!   ARCHIVE_CACHE_DIR=../../.cache/firms-archive \
//!     cargo run --release -p pyrosync-firms --example turkey_transition_check

use anyhow::{Context, Result};
use pyrosync_core::calendar::build_calendar;
use pyrosync_core::harmonize::harmonize;
use pyrosync_core::models::{CalendarCell, HarmonizedHotspot, Sensor};
use pyrosync_firms::archive::ArchiveClient;
use std::path::PathBuf;

fn archive_dir() -> PathBuf {
    std::env::var_os("ARCHIVE_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".cache/firms-archive"))
}

async fn load_and_harmonize_year(
    client: &ArchiveClient,
    country: &str,
    year: i32,
) -> Result<(usize, usize, Vec<HarmonizedHotspot>)> {
    let modis = client
        .fetch_country_year(Sensor::ModisTerra, country, year)
        .await
        .with_context(|| format!("failed to load MODIS archive for {country} {year}"))?;

    let viirs = if client.covers(Sensor::ViirsSNPP, year) {
        client
            .fetch_country_year(Sensor::ViirsSNPP, country, year)
            .await
            .with_context(|| format!("failed to load VIIRS SNPP archive for {country} {year}"))?
    } else {
        Vec::new()
    };

    let n_modis = modis.len();
    let n_viirs = viirs.len();
    let harmonized = harmonize(modis, viirs);
    Ok((n_modis, n_viirs, harmonized))
}

fn print_year_summary(year: i32, cells: &[CalendarCell]) {
    let year_cells: Vec<&CalendarCell> = cells.iter().filter(|c| c.year == year).collect();
    let total_count: u64 = year_cells.iter().map(|c| c.count).sum();
    let total_norm: f64 = year_cells.iter().filter_map(|c| c.count_normalized).sum();
    let total_frp: f64 = year_cells.iter().map(|c| c.total_frp).sum();
    let eq_values: Vec<f64> = year_cells
        .iter()
        .filter_map(|c| c.frp_per_detection_eq)
        .collect();
    let mean_monthly_f = if eq_values.is_empty() {
        f64::NAN
    } else {
        eq_values.iter().sum::<f64>() / eq_values.len() as f64
    };
    let annual_weighted_f = if total_norm > 0.0 {
        total_frp / total_norm
    } else {
        f64::NAN
    };
    let native_mean_frp = if total_count > 0 {
        year_cells
            .iter()
            .map(|c| c.mean_frp * c.count as f64)
            .sum::<f64>()
            / total_count as f64
    } else {
        f64::NAN
    };

    println!(
        "{year} | months={:>2} | count={:>6} | count_norm={:>9.1} | total_frp={:>11.1} MW | mean_F(monthly)={:>6.2} MW | F(total/norm)={:>6.2} MW | native_mean_frp={:>6.2} MW",
        year_cells.len(),
        total_count,
        total_norm,
        total_frp,
        mean_monthly_f,
        annual_weighted_f,
        native_mean_frp,
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    let cache_dir = archive_dir();
    let client = ArchiveClient::offline(cache_dir.clone());
    println!("Archive cache: {}", cache_dir.display());

    let (m10, v10, h10) = load_and_harmonize_year(&client, "Turkey", 2010).await?;
    let (m14, v14, h14) = load_and_harmonize_year(&client, "Turkey", 2014).await?;

    println!("Turkey 2010 raw: modis={m10}, viirs={v10}, harmonized={}", h10.len());
    println!("Turkey 2014 raw: modis={m14}, viirs={v14}, harmonized={}", h14.len());

    let mut combined = h10;
    combined.extend(h14);
    let cells = build_calendar(&combined);

    println!("\n=== Turkey 2010 vs 2014 Calendar Summary ===");
    print_year_summary(2010, &cells);
    print_year_summary(2014, &cells);

    Ok(())
}
