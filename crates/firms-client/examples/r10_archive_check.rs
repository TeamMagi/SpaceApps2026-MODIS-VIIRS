//! Offline R10 audit: reconstruct fleet sums and exhaustively check leftover pixels.
//! cargo run --release -p pyrosync-firms --example r10_archive_check
use anyhow::{Result, ensure};
use chrono::Datelike;
use pyrosync_core::{
    config::PipelineConfig,
    dedup::{haversine_km, remove_interscan_duplicates_indexed_with},
    fleet::{consolidate_modis_fleet_indexed_with, consolidate_viirs_fleet_indexed_with},
    harmonize::harmonize_with_links_config,
    models::{Hotspot, HotspotSource, Sensor},
    static_mask::StaticMask,
};
use pyrosync_firms::{archive::ArchiveClient, parse::retain_wildfires};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    let archive = ArchiveClient::offline(
        std::env::var("ARCHIVE_CACHE_DIR")
            .unwrap_or_else(|_| ".cache/firms-archive".into())
            .into(),
    );
    let config = PipelineConfig::default();
    let mut total_extra = 0;
    for (country, year, months) in [
        ("Cyprus", 2021, vec![7]),
        ("Turkey", 2021, vec![7, 8]),
        ("Angola", 2019, vec![4]),
    ] {
        let mut annual = Vec::new();
        for sensor in [Sensor::ModisTerra, Sensor::ViirsSNPP, Sensor::ViirsNOAA20] {
            let mut rows = archive.fetch_country_year(sensor, country, year).await?;
            retain_wildfires(&mut rows);
            annual.extend(rows);
        }
        let mask = StaticMask::from_detections(annual.iter(), &config.static_mask);
        for month in months {
            let (modis, viirs): (Vec<Hotspot>, Vec<Hotspot>) = annual
                .iter()
                .filter(|h| h.acquired.month() == month && !mask.contains(h))
                .cloned()
                .partition(|h| h.sensor.is_modis());
            let raw_count = modis.len() + viirs.len();
            let result = harmonize_with_links_config(modis.clone(), viirs.clone(), &config);
            let mut m = modis;
            let mut v = viirs;
            let mo = remove_interscan_duplicates_indexed_with(&mut m, &config.dedup);
            let vo = remove_interscan_duplicates_indexed_with(&mut v, &config.dedup);
            let (mf, mg) = consolidate_modis_fleet_indexed_with(&m, &config.fleet, &config.vza);
            let (vf, vg) = consolidate_viirs_fleet_indexed_with(&v, &config.fleet, &config.vza);
            let mout: Vec<usize> = mg
                .iter()
                .map(|g| result.modis_links[mo[g[0]]].unwrap())
                .collect();
            let vout: Vec<usize> = vg
                .iter()
                .map(|g| result.viirs_links[vo[g[0]]].unwrap())
                .collect();
            let mut violations = 0usize;
            let mut grouped = vec![Vec::new(); result.hotspots.len()];
            for (j, group) in vg.iter().enumerate() {
                for &member in group {
                    violations += usize::from(result.viirs_links[vo[member]] != Some(vout[j]));
                }
                grouped[vout[j]].push(j);
            }
            let mut leftovers = 0;
            let mut outside_footprint = 0;
            let mut max_error = 0.0_f64;
            let mut multi = 0;
            let mut max_pixels = 0;
            let mut matched_frp = 0.0;
            let mut expected_extra = 0;
            // No H3 matching lookup: scan every time-eligible MODIS fleet observation.
            let nearest: Vec<Option<usize>> = vf
                .iter()
                .map(|pixel| {
                    mf.iter()
                        .enumerate()
                        .filter_map(|(i, observation)| {
                            let dt = (pixel.hotspot.acquired - observation.hotspot.acquired)
                                .num_seconds()
                                .abs();
                            if dt > config.matching.window_secs {
                                return None;
                            }
                            let distance = haversine_km(
                                pixel.hotspot.lat,
                                pixel.hotspot.lon,
                                observation.hotspot.lat,
                                observation.hotspot.lon,
                            );
                            (distance <= config.matching.radius_km).then_some((i, distance))
                        })
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(i, _)| i)
                })
                .collect();
            for (i, observation) in mf.iter().enumerate() {
                let out = mout[i];
                let h = &result.hotspots[out];
                if h.source != HotspotSource::Matched {
                    continue;
                }
                let pixels = &grouped[out];
                violations += usize::from(pixels.is_empty());
                multi += usize::from(pixels.len() > 1);
                max_pixels = max_pixels.max(pixels.len());
                expected_extra += pixels.len().saturating_sub(1);
                let sum: f64 = pixels.iter().map(|&j| vf[j].frp_corrected_mean).sum();
                let error = (sum - h.frp_corrected).abs();
                max_error = max_error.max(error);
                violations += usize::from(error > 1e-9 * sum.abs().max(1.0));
                matched_frp += h.frp_corrected;
                let mh = &observation.hotspot;
                let radius = if mh.scan > 0.0
                    && mh.track > 0.0
                    && mh.scan.is_finite()
                    && mh.track.is_finite()
                {
                    (0.5 * mh.scan.hypot(mh.track))
                        .max(0.75)
                        .min(config.matching.radius_km)
                } else {
                    config.matching.radius_km
                };
                for &j in pixels {
                    violations += usize::from(nearest[j] != Some(i));
                    let vh = &vf[j].hotspot;
                    let is_seed = vh.acquired == h.acquired && vh.lat == h.lat && vh.lon == h.lon;
                    if !is_seed {
                        outside_footprint +=
                            usize::from(haversine_km(mh.lat, mh.lon, vh.lat, vh.lon) > radius);
                        violations +=
                            usize::from((vh.acquired - h.acquired).num_seconds().abs() > 600);
                    }
                }
            }
            for (j, pixel) in vf.iter().enumerate() {
                if result.hotspots[vout[j]].source != HotspotSource::ViirsOnly {
                    continue;
                }
                if let Some(i) = nearest[j] {
                    let h = &result.hotspots[mout[i]];
                    let m = &mf[i].hotspot;
                    let radius = if m.scan > 0.0
                        && m.track > 0.0
                        && m.scan.is_finite()
                        && m.track.is_finite()
                    {
                        (0.5 * m.scan.hypot(m.track))
                            .max(0.75)
                            .min(config.matching.radius_km)
                    } else {
                        config.matching.radius_km
                    };
                    if h.source == HotspotSource::Matched
                        && (pixel.hotspot.acquired - h.acquired).num_seconds().abs() <= 600
                        && haversine_km(m.lat, m.lon, pixel.hotspot.lat, pixel.hotspot.lon)
                            <= radius
                    {
                        leftovers += 1;
                    }
                }
            }
            let input_energy: f64 = vf.iter().map(|p| p.frp_corrected_mean).sum();
            let output_energy: f64 = result
                .hotspots
                .iter()
                .filter(|h| h.source != HotspotSource::ModisOnly)
                .map(|h| h.frp_corrected)
                .sum();
            let energy_error = (input_energy - output_energy).abs();
            violations += usize::from(energy_error > 1e-9 * input_energy.max(1.0));
            violations += usize::from(expected_extra != result.viirs_aggregated);
            violations += usize::from(
                result.hotspots.len() + result.matched_pairs + result.viirs_aggregated
                    != mf.len() + vf.len(),
            );
            total_extra += result.viirs_aggregated;
            println!(
                "{}",
                json!({"country": country, "year": year, "month": month,
                "raw": raw_count, "matched": result.matched_pairs, "multi_pixel_matches": multi,
                "extra_viirs_absorbed": result.viirs_aggregated, "max_pixels": max_pixels,
                "matched_frp_mw": matched_frp, "max_frp_error_mw": max_error,
                "viirs_energy_error_mw": energy_error, "eligible_viirs_only_leftovers": leftovers,
                "extra_pixels_outside_footprint": outside_footprint, "violations": violations})
            );
            ensure!(
                violations == 0 && leftovers == 0 && outside_footprint == 0,
                "R10 audit failed for {country}-{year}-{month}"
            );
        }
    }
    ensure!(total_extra > 0, "No multi-pixel aggregation exercised");
    Ok(())
}
