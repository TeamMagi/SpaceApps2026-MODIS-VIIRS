use crate::dedup::{haversine_km, remove_interscan_duplicates};
use crate::fleet::{self, FleetObservation};
use crate::h3index::H3Index;
use crate::models::*;
use crate::vza;

const MATCH_RADIUS_KM: f64 = 2.0;
// 180 minutes: orbital geometry separates MODIS and VIIRS overpasses of the same fire
// (min 34 min, median ~158 min in live FIRMS data), so a narrow window yields zero matches.
const MATCH_WINDOW_SECS: i64 = 10_800;

/// Full harmonization pipeline:
/// 1. Deduplicate interscan overlaps
/// 2. Consolidate multi-platform MODIS and VIIRS fleet detections
/// 3. Cross-sensor spatial-temporal matching
/// 4. Quality-priority fusion (VIIRS preferred) and non-mutating MODIS VZA correction
///
/// Cross-sensor semantics: `HotspotSource::Matched` represents observations of the same
/// fire event by both sensors rather than simultaneous observations. In live FIRMS data,
/// co-located MODIS/VIIRS pairs arrive at least 34 minutes apart with a median of ~158 minutes;
/// the previous 5-minute window produced zero matches. The 180-minute window exists to
/// prevent double-counting the same fire event, not for simultaneous radiometric comparison.
/// VZA coefficient calibration requires simultaneous pairs and cannot be performed from FIRMS
/// NRT data alone—it requires granule- or archive-level imagery.
pub fn harmonize(mut modis: Vec<Hotspot>, mut viirs: Vec<Hotspot>) -> Vec<HarmonizedHotspot> {
    // Step 1: Remove interscan duplicates (also sorts both vectors by `acquired` ascending)
    remove_interscan_duplicates(&mut modis);
    remove_interscan_duplicates(&mut viirs);

    // Consolidate both fleets before cross-sensor matching so sibling detections (Terra/Aqua
    // and SNPP/NOAA-20/NOAA-21) do not leak into the output as duplicate singletons.
    let modis = fleet::consolidate_modis_fleet(&modis);
    let viirs = fleet::consolidate_viirs_fleet(&viirs);

    // Step 2 & 3: Match and fuse using H3 spatial index candidate lookup
    let index = H3Index::build(viirs.iter().map(|v| (v.hotspot.lat, v.hotspot.lon)));
    let mut result = Vec::with_capacity(modis.len() + viirs.len());
    let mut viirs_matched = vec![false; viirs.len()];

    for m in &modis {
        let mh = &m.hotspot;
        let mut best_match: Option<(usize, f64)> = None;

        for j in index.candidates(mh.lat, mh.lon) {
            if viirs_matched[j] {
                continue;
            }

            let vh = &viirs[j].hotspot;
            let dt = (vh.acquired - mh.acquired).num_seconds().abs();
            if dt > MATCH_WINDOW_SECS {
                continue;
            }

            let dist = haversine_km(mh.lat, mh.lon, vh.lat, vh.lon);
            if dist > MATCH_RADIUS_KM {
                continue;
            }

            let score = match_score(dist, dt);
            if best_match.is_none_or(|(_, best)| score > best) {
                best_match = Some((j, score));
            }
        }

        if let Some((j, _)) = best_match {
            viirs_matched[j] = true;
            result.push(fuse_matched(m, &viirs[j]));
        } else {
            result.push(from_modis_only(m));
        }
    }

    // Unmatched VIIRS detections
    for (j, v) in viirs.iter().enumerate() {
        if !viirs_matched[j] {
            result.push(from_viirs_only(v));
        }
    }

    result.sort_by_key(|a| a.acquired);
    result
}

fn fuse_matched(m: &FleetObservation, v: &FleetObservation) -> HarmonizedHotspot {
    let mh = &m.hotspot;
    let vh = &v.hotspot;
    let mut platforms = Vec::with_capacity(m.platforms.len() + v.platforms.len());
    platforms.extend_from_slice(&m.platforms);
    platforms.extend_from_slice(&v.platforms);
    platforms.sort();
    platforms.dedup();

    // Quality-priority: prefer VIIRS FRP (Zheng et al. 2026)
    HarmonizedHotspot {
        lat: vh.lat,
        lon: vh.lon,
        frp: vh.frp,
        frp_corrected: v.frp_corrected_mean,
        vza_factor: None,
        n_obs: m.n_obs.saturating_add(v.n_obs),
        platforms,
        confidence: higher_confidence(mh.confidence, vh.confidence),
        source: HotspotSource::Matched,
        acquired: vh.acquired,
        daynight: vh.daynight,
    }
}

fn from_modis_only(m: &FleetObservation) -> HarmonizedHotspot {
    let mh = &m.hotspot;
    let factor = vza::vza_factor(mh);

    HarmonizedHotspot {
        lat: mh.lat,
        lon: mh.lon,
        frp: mh.frp,
        frp_corrected: m.frp_corrected_mean,
        vza_factor: factor,
        n_obs: m.n_obs,
        platforms: m.platforms.clone(),
        confidence: mh.confidence,
        source: HotspotSource::ModisOnly,
        acquired: mh.acquired,
        daynight: mh.daynight,
    }
}

fn from_viirs_only(v: &FleetObservation) -> HarmonizedHotspot {
    let vh = &v.hotspot;
    HarmonizedHotspot {
        lat: vh.lat,
        lon: vh.lon,
        frp: vh.frp,
        frp_corrected: v.frp_corrected_mean,
        vza_factor: None,
        n_obs: v.n_obs,
        platforms: v.platforms.clone(),
        confidence: vh.confidence,
        source: HotspotSource::ViirsOnly,
        acquired: vh.acquired,
        daynight: vh.daynight,
    }
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
        let modis = fleet::consolidate_modis_fleet(&modis);
        let viirs = fleet::consolidate_viirs_fleet(&viirs);

        let mut result = Vec::new();
        let mut viirs_matched = vec![false; viirs.len()];

        for m in &modis {
            let mh = &m.hotspot;
            let mut best_match: Option<(usize, f64)> = None;

            for (j, v) in viirs.iter().enumerate() {
                if viirs_matched[j] {
                    continue;
                }

                let vh = &v.hotspot;
                let dt = (mh.acquired - vh.acquired).num_seconds().abs();
                if dt > MATCH_WINDOW_SECS {
                    continue;
                }

                let dist = haversine_km(mh.lat, mh.lon, vh.lat, vh.lon);
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
                result.push(fuse_matched(m, &viirs[j]));
            } else {
                result.push(from_modis_only(m));
            }
        }

        for (j, v) in viirs.iter().enumerate() {
            if !viirs_matched[j] {
                result.push(from_viirs_only(v));
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
            assert_eq!(
                a.vza_factor, e.vza_factor,
                "vza_factor mismatch at index {i}"
            );
            assert_eq!(a.n_obs, e.n_obs, "n_obs mismatch at index {i}");
            assert_eq!(a.platforms, e.platforms, "platforms mismatch at index {i}");
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
    fn pair_190_minutes_apart_does_not_match() {
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
            MATCH_WINDOW_SECS + 600,
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
    fn exact_window_boundary_matches_and_one_second_past_does_not() {
        let modis_before = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            0,
            10.0,
            Confidence::Low,
        )];
        let viirs_at_boundary = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.0,
            32.0,
            MATCH_WINDOW_SECS,
            20.0,
            Confidence::Nominal,
        )];
        let out_plus = harmonize(modis_before, viirs_at_boundary);
        assert_eq!(out_plus.len(), 1);
        assert_eq!(out_plus[0].source, HotspotSource::Matched);

        let modis_after = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            MATCH_WINDOW_SECS,
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
        let viirs_past_boundary = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.0,
            32.0,
            MATCH_WINDOW_SECS + 1,
            20.0,
            Confidence::Nominal,
        )];
        let out_outside = harmonize(modis_outside, viirs_past_boundary);
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

        // Dense clusters within ~2 km radius and a shared 300s window to stress H3 cell boundaries.
        let dense_centers = [
            (12.0, 15.0),
            (38.5, 31.2),
            (70.0, 24.5),
            (-15.3, -47.8),
        ];
        for i in 0..600 {
            let (clat, clon) = dense_centers[(next_u64() as usize) % dense_centers.len()];
            let r1 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let r2 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let lat = clat + (r1 - 0.5) * 0.018;
            let lon = clon + (r2 - 0.5) * (0.018 / clat.to_radians().cos());
            let t = 5_000 + (next_u64() % 280) as i64;
            let conf = confs[(next_u64() % 3) as usize];
            modis.push(make_hotspot(
                Sensor::ModisTerra,
                lat,
                lon,
                t,
                15.0 + (i % 40) as f64,
                conf,
            ));
        }

        for i in 0..1_400 {
            let (clat, clon) = dense_centers[(next_u64() as usize) % dense_centers.len()];
            let r1 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let r2 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let lat = clat + (r1 - 0.5) * 0.018;
            let lon = clon + (r2 - 0.5) * (0.018 / clat.to_radians().cos());
            let t = 5_000 + (next_u64() % 280) as i64;
            let conf = confs[(next_u64() % 3) as usize];
            viirs.push(make_hotspot(
                Sensor::ViirsSNPP,
                lat,
                lon,
                t,
                18.0 + (i % 40) as f64,
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
        let matched_count = actual
            .iter()
            .filter(|h| h.source == HotspotSource::Matched)
            .count();
        let expected_matched_count = expected
            .iter()
            .filter(|h| h.source == HotspotSource::Matched)
            .count();
        assert_eq!(matched_count, expected_matched_count);
        assert!(matched_count > 400);
    }

    #[test]
    fn modis_only_with_vza_preserves_raw_frp_and_applies_factor() {
        let mut m = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 100.0, Confidence::High);
        m.vza = Some(45.0);

        let out = harmonize(vec![m], vec![]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::ModisOnly);
        assert_eq!(out[0].frp, 100.0);
        let factor = out[0].vza_factor.expect("expected Some(vza_factor)");
        assert!(factor > 1.0);
        assert!(out[0].frp_corrected < out[0].frp);
        assert!((out[0].frp_corrected - (100.0 / factor)).abs() < 1e-12);
        assert_eq!(out[0].n_obs, 1);
        assert_eq!(out[0].platforms, vec![Sensor::ModisTerra]);
    }

    #[test]
    fn modis_only_without_vza_keeps_frp_equal_and_factor_none() {
        let m = make_hotspot(Sensor::ModisAqua, 39.0, 32.0, 0, 80.0, Confidence::Nominal);

        let out = harmonize(vec![m], vec![]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::ModisOnly);
        assert_eq!(out[0].frp, 80.0);
        assert_eq!(out[0].frp_corrected, 80.0);
        assert_eq!(out[0].vza_factor, None);
        assert_eq!(out[0].n_obs, 1);
        assert_eq!(out[0].platforms, vec![Sensor::ModisAqua]);
    }

    #[test]
    fn matched_hotspot_has_n_obs_two_and_sorted_platforms() {
        let mut m = make_hotspot(Sensor::ModisAqua, 39.0, 32.0, 0, 90.0, Confidence::Nominal);
        m.vza = Some(45.0);
        let v = make_hotspot(Sensor::ViirsNOAA20, 39.002, 32.002, 30, 55.0, Confidence::High);

        let out = harmonize(vec![m], vec![v]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::Matched);
        assert_eq!(out[0].frp, 55.0);
        assert_eq!(out[0].frp_corrected, 55.0);
        assert_eq!(out[0].vza_factor, None);
        assert_eq!(out[0].n_obs, 2);
        assert_eq!(
            out[0].platforms,
            vec![Sensor::ModisAqua, Sensor::ViirsNOAA20]
        );
    }

    #[test]
    fn viirs_only_has_n_obs_one_and_no_vza_factor() {
        let mut v = make_hotspot(Sensor::ViirsNOAA21, 39.0, 32.0, 0, 37.5, Confidence::High);
        v.vza = Some(30.0);

        let out = harmonize(vec![], vec![v]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::ViirsOnly);
        assert_eq!(out[0].frp, 37.5);
        assert_eq!(out[0].frp_corrected, 37.5);
        assert_eq!(out[0].vza_factor, None);
        assert_eq!(out[0].n_obs, 1);
        assert_eq!(out[0].platforms, vec![Sensor::ViirsNOAA21]);
    }

    #[test]
    fn fleet_consolidates_before_modis_matching_so_no_viirs_leaks() {
        // NOAA-20 (High) at t=0 becomes the fleet representative; NOAA-21 (Nominal) at t=45m
        // joins the fleet. MODIS at t=120s is within 300s of the representative.
        let modis = vec![make_hotspot(
            Sensor::ModisTerra,
            -14.0,
            25.0,
            120,
            80.0,
            Confidence::Nominal,
        )];
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsNOAA20,
                -14.0,
                25.0,
                0,
                40.0,
                Confidence::High,
            ),
            make_hotspot(
                Sensor::ViirsNOAA21,
                -14.005,
                25.0,
                45 * 60,
                60.0,
                Confidence::Nominal,
            ),
        ];

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::Matched);
        assert_eq!(out[0].n_obs, 3);
        assert!(!out.iter().any(|h| h.source == HotspotSource::ViirsOnly));
        assert!((out[0].frp - 40.0).abs() < 1e-9);
        assert!((out[0].frp_corrected - 50.0).abs() < 1e-9);
        assert_eq!(out[0].vza_factor, None);
        assert_eq!(
            out[0].platforms,
            vec![Sensor::ModisTerra, Sensor::ViirsNOAA20, Sensor::ViirsNOAA21]
        );
    }

    #[test]
    fn modis_and_viirs_two_hours_apart_match_as_one_fire_event() {
        // 120 minutes is a realistic separation close to the live FIRMS median (~158 min).
        let modis = vec![make_hotspot(
            Sensor::ModisAqua,
            39.0,
            32.0,
            0,
            65.0,
            Confidence::Nominal,
        )];
        let viirs = vec![make_hotspot(
            Sensor::ViirsNOAA20,
            39.003,
            32.002,
            120 * 60,
            48.0,
            Confidence::High,
        )];
        debug_assert!(haversine_km(39.0, 32.0, 39.003, 32.002) < 0.5);

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::Matched);
        assert_eq!(out[0].n_obs, 2);
        assert!(out[0].platforms.contains(&Sensor::ModisAqua));
        assert!(out[0].platforms.contains(&Sensor::ViirsNOAA20));
        assert!(!out.iter().any(|h| h.source == HotspotSource::ModisOnly));
        assert!(!out.iter().any(|h| h.source == HotspotSource::ViirsOnly));
    }

    #[test]
    fn terra_and_aqua_150_min_apart_consolidate_to_single_modis_only() {
        let modis = vec![
            make_hotspot(
                Sensor::ModisTerra,
                39.0,
                32.0,
                0,
                50.0,
                Confidence::Nominal,
            ),
            make_hotspot(
                Sensor::ModisAqua,
                39.0,
                32.0,
                150 * 60,
                70.0,
                Confidence::High,
            ),
        ];

        let out = harmonize(modis, vec![]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::ModisOnly);
        assert_eq!(out[0].n_obs, 2);
        assert_eq!(
            out[0].platforms,
            vec![Sensor::ModisTerra, Sensor::ModisAqua]
        );
    }
}
