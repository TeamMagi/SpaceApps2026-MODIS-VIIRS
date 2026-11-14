use crate::dedup::{haversine_km, remove_interscan_duplicates};
use crate::models::*;
use crate::vza;

const MATCH_RADIUS_KM: f64 = 2.0;
const MATCH_WINDOW_SECS: i64 = 300; // 5 minutes
const EARTH_RADIUS_KM: f64 = 6371.0;

/// Full harmonization pipeline:
/// 1. Deduplicate interscan overlaps
/// 2. Apply VZA correction to MODIS FRP
/// 3. Cross-sensor spatial-temporal matching
/// 4. Quality-priority fusion (VIIRS preferred)
pub fn harmonize(mut modis: Vec<Hotspot>, mut viirs: Vec<Hotspot>) -> Vec<HarmonizedHotspot> {
    // Step 1: Remove interscan duplicates (also sorts both vectors by `acquired` ascending)
    remove_interscan_duplicates(&mut modis);
    remove_interscan_duplicates(&mut viirs);

    // Step 2: VZA correction for MODIS
    vza::correct_batch(&mut modis);

    // Step 3 & 4: Match and fuse using a monotonic sliding time window over sorted VIIRS
    let mut result = Vec::with_capacity(modis.len() + viirs.len());
    let mut viirs_matched = vec![false; viirs.len()];
    let mut lo = 0usize;

    // Conservative angular bound (+0.1% margin for FP rounding) so the pre-filter never drops valid matches.
    let max_dlat_deg = (MATCH_RADIUS_KM / EARTH_RADIUS_KM).to_degrees() * 1.001;

    for m in &modis {
        // Advance `lo` purely by time, never skipping already-matched points, so a later MODIS
        // detection still sees the full contiguous [m.acquired - 300s, m.acquired + 300s] range.
        while lo < viirs.len()
            && (m.acquired - viirs[lo].acquired).num_seconds() > MATCH_WINDOW_SECS
        {
            lo += 1;
        }

        let cos_lat = m.lat.to_radians().cos();
        // Guard against cos(lat) near zero at the poles, where longitudes converge.
        let max_dlon_deg = if cos_lat > 0.01 {
            max_dlat_deg / cos_lat * 1.01
        } else {
            180.0
        };

        let mut best_match: Option<(usize, f64)> = None;

        for (j, v) in viirs.iter().enumerate().skip(lo) {
            let signed_dt = (v.acquired - m.acquired).num_seconds();
            if signed_dt > MATCH_WINDOW_SECS {
                break;
            }

            if viirs_matched[j] {
                continue;
            }

            if (m.lat - v.lat).abs() > max_dlat_deg {
                continue;
            }

            let mut dlon = (m.lon - v.lon).abs();
            if dlon > 180.0 {
                dlon = 360.0 - dlon;
            }
            if dlon > max_dlon_deg {
                continue;
            }

            let dist = haversine_km(m.lat, m.lon, v.lat, v.lon);
            if dist > MATCH_RADIUS_KM {
                continue;
            }

            let score = match_score(dist, signed_dt.abs());
            if best_match.is_none_or(|(_, best)| score > best) {
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

    result.sort_by_key(|a| a.acquired);
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};

    fn make_hotspot(
        sensor: Sensor,
        lat: f64,
        lon: f64,
        offset_secs: i64,
        frp: f64,
        confidence: Confidence,
    ) -> Hotspot {
        let base = Utc.with_ymd_and_hms(2025, 7, 15, 12, 0, 0).unwrap();
        Hotspot {
            lat,
            lon,
            brightness: 330.0,
            frp,
            confidence,
            sensor,
            acquired: base + Duration::seconds(offset_secs),
            scan: 0.0,
            track: 0.0,
            satellite: String::new(),
            daynight: DayNight::Day,
            vza: None,
            fire_type: FireType::VegetationFire,
        }
    }

    fn harmonize_brute_force(
        mut modis: Vec<Hotspot>,
        mut viirs: Vec<Hotspot>,
    ) -> Vec<HarmonizedHotspot> {
        remove_interscan_duplicates(&mut modis);
        remove_interscan_duplicates(&mut viirs);
        vza::correct_batch(&mut modis);

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

        result.sort_by_key(|a| a.acquired);
        result
    }

    fn assert_harmonized_eq(actual: &[HarmonizedHotspot], expected: &[HarmonizedHotspot]) {
        assert_eq!(actual.len(), expected.len());
        for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
            assert_eq!(a.lat, e.lat, "lat mismatch at index {i}");
            assert_eq!(a.lon, e.lon, "lon mismatch at index {i}");
            assert_eq!(a.frp, e.frp, "frp mismatch at index {i}");
            assert_eq!(
                a.frp_corrected, e.frp_corrected,
                "frp_corrected mismatch at index {i}"
            );
            assert!(a.confidence == e.confidence, "confidence mismatch at {i}");
            assert_eq!(a.source, e.source, "source mismatch at index {i}");
            assert_eq!(a.acquired, e.acquired, "acquired mismatch at index {i}");
            assert_eq!(a.daynight, e.daynight, "daynight mismatch at index {i}");
        }
    }

    #[test]
    fn pair_inside_both_windows_matches() {
        let modis = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            0,
            50.0,
            Confidence::High,
        )];
        let viirs = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.005,
            32.005,
            60,
            42.0,
            Confidence::Nominal,
        )];

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::Matched);
        assert_eq!(out[0].frp, 42.0);
        assert!(out[0].confidence == Confidence::High);
    }

    #[test]
    fn pair_3km_apart_does_not_match() {
        let modis = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            0,
            50.0,
            Confidence::Nominal,
        )];
        // 0.027 deg latitude is ~3.00 km at any longitude
        let viirs = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.027,
            32.0,
            0,
            40.0,
            Confidence::High,
        )];

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|h| h.source == HotspotSource::ModisOnly));
        assert!(out.iter().any(|h| h.source == HotspotSource::ViirsOnly));
    }

    #[test]
    fn pair_10_minutes_apart_does_not_match() {
        let modis = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            0,
            50.0,
            Confidence::Nominal,
        )];
        let viirs = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.0,
            32.0,
            600,
            40.0,
            Confidence::High,
        )];

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|h| h.source == HotspotSource::ModisOnly));
        assert!(out.iter().any(|h| h.source == HotspotSource::ViirsOnly));
    }

    #[test]
    fn single_viirs_cannot_serve_two_modis() {
        let modis = vec![
            make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 50.0, Confidence::Nominal),
            make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 10, 60.0, Confidence::Nominal),
        ];
        let viirs = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.0,
            32.0,
            5,
            45.0,
            Confidence::High,
        )];

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 2);
        let matched = out
            .iter()
            .filter(|h| h.source == HotspotSource::Matched)
            .count();
        let modis_only = out
            .iter()
            .filter(|h| h.source == HotspotSource::ModisOnly)
            .count();
        assert_eq!(matched, 1);
        assert_eq!(modis_only, 1);
    }

    #[test]
    fn exact_300s_boundary_matches_and_301s_does_not() {
        let modis_before = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            0,
            10.0,
            Confidence::Low,
        )];
        let viirs_plus_300 = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.0,
            32.0,
            300,
            20.0,
            Confidence::Nominal,
        )];
        let out_plus = harmonize(modis_before, viirs_plus_300);
        assert_eq!(out_plus.len(), 1);
        assert_eq!(out_plus[0].source, HotspotSource::Matched);

        let modis_after = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            300,
            10.0,
            Confidence::Low,
        )];
        let viirs_at_0 = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.0,
            32.0,
            0,
            20.0,
            Confidence::Nominal,
        )];
        let out_minus = harmonize(modis_after, viirs_at_0);
        assert_eq!(out_minus.len(), 1);
        assert_eq!(out_minus[0].source, HotspotSource::Matched);

        let modis_outside = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            0,
            10.0,
            Confidence::Low,
        )];
        let viirs_plus_301 = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.0,
            32.0,
            301,
            20.0,
            Confidence::Nominal,
        )];
        let out_outside = harmonize(modis_outside, viirs_plus_301);
        assert_eq!(out_outside.len(), 2);
        assert!(out_outside.iter().all(|h| h.source != HotspotSource::Matched));
    }

    #[test]
    fn early_matched_viirs_does_not_trap_lo_for_later_modis() {
        // m0 at t=0 matches v1 at t=200 (loc A); v0 at t=50 (loc B) remains unmatched for m1 at t=100.
        // Then m2 at t=400 (loc C) still sees v1 inside its [100, 700] window, skips matched v1,
        // and matches v2 at t=450 (loc C).
        let modis = vec![
            make_hotspot(Sensor::ModisTerra, 38.0, 30.0, 0, 10.0, Confidence::Nominal),
            make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 100, 11.0, Confidence::Nominal),
            make_hotspot(Sensor::ModisTerra, 40.0, 34.0, 400, 12.0, Confidence::Nominal),
        ];
        let viirs = vec![
            make_hotspot(Sensor::ViirsSNPP, 39.0, 32.0, 50, 20.0, Confidence::High),
            make_hotspot(Sensor::ViirsSNPP, 38.0, 30.0, 200, 21.0, Confidence::High),
            make_hotspot(Sensor::ViirsSNPP, 40.0, 34.0, 450, 22.0, Confidence::High),
        ];

        let expected = harmonize_brute_force(modis.clone(), viirs.clone());
        let actual = harmonize(modis, viirs);
        assert_harmonized_eq(&actual, &expected);
        assert_eq!(actual.len(), 3);
        assert!(actual.iter().all(|h| h.source == HotspotSource::Matched));
    }

    #[test]
    fn sliding_window_matches_brute_force_on_synthetic_dataset() {
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next_u64 = || -> u64 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            seed
        };

        let mut modis = Vec::with_capacity(2_500);
        let mut viirs = Vec::with_capacity(3_500);

        let confs = [Confidence::Low, Confidence::Nominal, Confidence::High];

        for i in 0..2_500 {
            let r1 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let r2 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let cluster = (next_u64() % 40) as f64;
            let lat = 36.0 + (cluster * 0.15) + (r1 - 0.5) * 0.04;
            let lon = 27.0 + (cluster * 0.25) + (r2 - 0.5) * 0.04;
            let t = ((next_u64() % 20_000) as i64) - 500;
            let conf = confs[(next_u64() % 3) as usize];
            modis.push(make_hotspot(
                Sensor::ModisTerra,
                lat,
                lon,
                t,
                10.0 + (i % 50) as f64,
                conf,
            ));
        }

        for i in 0..3_500 {
            let r1 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let r2 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let cluster = (next_u64() % 40) as f64;
            let lat = 36.0 + (cluster * 0.15) + (r1 - 0.5) * 0.04;
            let lon = 27.0 + (cluster * 0.25) + (r2 - 0.5) * 0.04;
            let t = ((next_u64() % 20_000) as i64) - 500;
            let conf = confs[(next_u64() % 3) as usize];
            viirs.push(make_hotspot(
                Sensor::ViirsSNPP,
                lat,
                lon,
                t,
                12.0 + (i % 50) as f64,
                conf,
            ));
        }

        // Include edge cases near the antimeridian and high latitudes
        modis.push(make_hotspot(
            Sensor::ModisTerra,
            10.0,
            179.995,
            1_000,
            25.0,
            Confidence::Nominal,
        ));
        viirs.push(make_hotspot(
            Sensor::ViirsSNPP,
            10.0,
            -179.995,
            1_100,
            30.0,
            Confidence::High,
        ));
        modis.push(make_hotspot(
            Sensor::ModisTerra,
            89.99,
            10.0,
            2_000,
            25.0,
            Confidence::Nominal,
        ));
        viirs.push(make_hotspot(
            Sensor::ViirsSNPP,
            89.99,
            170.0,
            2_100,
            30.0,
            Confidence::High,
        ));

        let expected = harmonize_brute_force(modis.clone(), viirs.clone());
        let actual = harmonize(modis, viirs);
        assert_harmonized_eq(&actual, &expected);
    }
}
