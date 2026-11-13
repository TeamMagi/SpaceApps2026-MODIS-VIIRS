use crate::dedup::{haversine_km, remove_interscan_duplicates};
use crate::models::*;
use crate::vza;

const MATCH_RADIUS_KM: f64 = 2.0;
const MATCH_WINDOW_SECS: i64 = 300; // 5 minutes

/// Full harmonization pipeline:
/// 1. Deduplicate interscan overlaps
/// 2. Apply VZA correction to MODIS FRP
/// 3. Cross-sensor spatial-temporal matching
/// 4. Quality-priority fusion (VIIRS preferred)
pub fn harmonize(mut modis: Vec<Hotspot>, mut viirs: Vec<Hotspot>) -> Vec<HarmonizedHotspot> {
    // Step 1: Remove interscan duplicates
    remove_interscan_duplicates(&mut modis);
    remove_interscan_duplicates(&mut viirs);

    // Step 2: VZA correction for MODIS
    vza::correct_batch(&mut modis);

    // Step 3 & 4: Match and fuse
    let mut result = Vec::new();
    let mut viirs_matched = vec![false; viirs.len()];

    for m in &modis {
        let mut best_match: Option<(usize, f64)> = None;

        for (j, v) in viirs.iter().enumerate() {
            if viirs_matched[j] {
                continue;
            }

            let dt = (m.acquired - v.acquired).num_seconds().abs();
            if dt > MATCH_WINDOW_SECS {
                continue;
            }

            let dist = haversine_km(m.lat, m.lon, v.lat, v.lon);
            if dist > MATCH_RADIUS_KM {
                continue;
            }

            let score = match_score(dist, dt);
            if best_match.is_none() || score > best_match.unwrap().1 {
                best_match = Some((j, score));
            }
        }

        if let Some((j, _)) = best_match {
            viirs_matched[j] = true;
            let v = &viirs[j];
            // Quality-priority: prefer VIIRS FRP (Zheng et al. 2026)
            result.push(HarmonizedHotspot {
                lat: v.lat,
                lon: v.lon,
                frp: v.frp,
                frp_corrected: v.frp,
                confidence: higher_confidence(m.confidence, v.confidence),
                source: HotspotSource::Matched,
                acquired: v.acquired,
                daynight: v.daynight,
            });
        } else {
            result.push(HarmonizedHotspot {
                lat: m.lat,
                lon: m.lon,
                frp: m.frp,
                frp_corrected: m.frp,
                confidence: m.confidence,
                source: HotspotSource::ModisOnly,
                acquired: m.acquired,
                daynight: m.daynight,
            });
        }
    }

    // Unmatched VIIRS detections
    for (j, v) in viirs.iter().enumerate() {
        if !viirs_matched[j] {
            result.push(HarmonizedHotspot {
                lat: v.lat,
                lon: v.lon,
                frp: v.frp,
                frp_corrected: v.frp,
                confidence: v.confidence,
                source: HotspotSource::ViirsOnly,
                acquired: v.acquired,
                daynight: v.daynight,
            });
        }
    }

    result.sort_by(|a, b| a.acquired.cmp(&b.acquired));
    result
}

fn match_score(dist_km: f64, dt_secs: i64) -> f64 {
    let spatial = 1.0 - (dist_km / MATCH_RADIUS_KM);
    let temporal = 1.0 - (dt_secs as f64 / MATCH_WINDOW_SECS as f64);
    0.6 * spatial + 0.4 * temporal
}

fn higher_confidence(a: Confidence, b: Confidence) -> Confidence {
    use Confidence::*;
    match (a, b) {
        (High, _) | (_, High) => High,
        (Nominal, _) | (_, Nominal) => Nominal,
        _ => Low,
    }
}
