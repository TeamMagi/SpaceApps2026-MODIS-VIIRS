use crate::dedup::{haversine_km, remove_interscan_duplicates_indexed};
use crate::fleet::{self, FleetObservation};
use crate::h3index::H3Index;
use crate::models::*;
use crate::vza;

const MATCH_RADIUS_KM: f64 = 2.0;
// 180 minutes: orbital geometry separates MODIS and VIIRS overpasses of the same fire
// (min 34 min, median ~158 min in live FIRMS data), so a narrow window yields zero matches.
const MATCH_WINDOW_SECS: i64 = 10_800;

/// Output of [`harmonize_with_links`]: the harmonized hotspots plus, for every input row,
/// which output it became.
#[derive(Debug, Clone)]
pub struct HarmonizeLinks {
    pub hotspots: Vec<HarmonizedHotspot>,
    /// `modis_links[i]` is the index in `hotspots` produced by input MODIS row `i`,
    /// or `None` if interscan dedup removed that row.
    pub modis_links: Vec<Option<usize>>,
    /// Same as `modis_links`, for the VIIRS input rows.
    pub viirs_links: Vec<Option<usize>>,
    pub interscan_removed: usize,
    pub fleet_merged: usize,
    pub matched_pairs: usize,
}

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
pub fn harmonize(modis: Vec<Hotspot>, viirs: Vec<Hotspot>) -> Vec<HarmonizedHotspot> {
    harmonize_with_links(modis, viirs).hotspots
}

/// Runs the same pipeline as [`harmonize`] and records which output each input row became.
pub fn harmonize_with_links(mut modis: Vec<Hotspot>, mut viirs: Vec<Hotspot>) -> HarmonizeLinks {
    let modis_input_len = modis.len();
    let viirs_input_len = viirs.len();

    // Step 1: Remove interscan duplicates (also sorts both vectors by `acquired` ascending)
    let modis_origin = remove_interscan_duplicates_indexed(&mut modis);
    let viirs_origin = remove_interscan_duplicates_indexed(&mut viirs);
    let interscan_removed =
        (modis_input_len - modis.len()) + (viirs_input_len - viirs.len());

    // Step 2: Consolidate both fleets before cross-sensor matching so sibling detections
    // (Terra/Aqua and SNPP/NOAA-20/NOAA-21) do not leak into the output as duplicate singletons.
    let (modis_fleet, modis_groups) = fleet::consolidate_modis_fleet_indexed(&modis);
    let (viirs_fleet, viirs_groups) = fleet::consolidate_viirs_fleet_indexed(&viirs);
    let fleet_merged =
        (modis.len() - modis_fleet.len()) + (viirs.len() - viirs_fleet.len());

    // Step 3 & 4: Match and fuse using H3 spatial index candidate lookup
    let index = H3Index::build(viirs_fleet.iter().map(|v| (v.hotspot.lat, v.hotspot.lon)));
    let mut result = Vec::with_capacity(modis_fleet.len() + viirs_fleet.len());
    let mut modis_fleet_out = vec![0usize; modis_fleet.len()];
    let mut viirs_fleet_out: Vec<Option<usize>> = vec![None; viirs_fleet.len()];
    let mut matched_pairs = 0usize;

    for (mi, m) in modis_fleet.iter().enumerate() {
        let mh = &m.hotspot;
        let mut best_match: Option<(usize, f64)> = None;

        for j in index.candidates(mh.lat, mh.lon) {
            if viirs_fleet_out[j].is_some() {
                continue;
            }

            let vh = &viirs_fleet[j].hotspot;
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

        modis_fleet_out[mi] = result.len();
        if let Some((j, _)) = best_match {
            viirs_fleet_out[j] = Some(result.len());
            matched_pairs += 1;
            result.push(fuse_matched(m, &viirs_fleet[j]));
        } else {
            result.push(from_modis_only(m));
        }
    }

    // Unmatched VIIRS detections
    for (j, v) in viirs_fleet.iter().enumerate() {
        if viirs_fleet_out[j].is_none() {
            viirs_fleet_out[j] = Some(result.len());
            result.push(from_viirs_only(v));
        }
    }

    // Stable index sort preserves the exact ordering of `result.sort_by_key(|a| a.acquired)`
    // while mapping fleet observations to their final positions in `hotspots`.
    let mut order: Vec<usize> = (0..result.len()).collect();
    order.sort_by_key(|&k| result[k].acquired);
    let mut final_pos = vec![0usize; result.len()];
    for (new_pos, &old_pos) in order.iter().enumerate() {
        final_pos[old_pos] = new_pos;
    }
    let mut slots: Vec<Option<HarmonizedHotspot>> = result.into_iter().map(Some).collect();
    let hotspots: Vec<HarmonizedHotspot> = order
        .iter()
        .map(|&k| slots[k].take().expect("each output taken once"))
        .collect();

    let mut modis_links = vec![None; modis_input_len];
    for (mi, members) in modis_groups.iter().enumerate() {
        let out = final_pos[modis_fleet_out[mi]];
        for &dedup_idx in members {
            modis_links[modis_origin[dedup_idx]] = Some(out);
        }
    }

    let mut viirs_links = vec![None; viirs_input_len];
    for (vj, members) in viirs_groups.iter().enumerate() {
        let out = final_pos[viirs_fleet_out[vj].expect("every VIIRS fleet group produces or joins an output")];
        for &dedup_idx in members {
            viirs_links[viirs_origin[dedup_idx]] = Some(out);
        }
    }

    HarmonizeLinks {
        hotspots,
        modis_links,
        viirs_links,
        interscan_removed,
        fleet_merged,
        matched_pairs,
    }
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
    use crate::dedup::remove_interscan_duplicates;
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

    #[test]
    fn empty_inputs_produce_empty_output() {
        assert!(harmonize(vec![], vec![]).is_empty());
    }

    #[test]
    fn every_modis_detection_matched_at_three_sites() {
        let sites = [(36.0, 30.0), (37.0, 31.0), (38.0, 32.0)];
        let mut modis = Vec::new();
        let mut viirs = Vec::new();

        for (i, &(lat, lon)) in sites.iter().enumerate() {
            let i = i as i64;
            modis.push(make_hotspot(
                Sensor::ModisTerra,
                lat,
                lon,
                i * 600,
                50.0,
                Confidence::Nominal,
            ));
            viirs.push(make_hotspot(
                Sensor::ViirsSNPP,
                lat + 0.003,
                lon,
                i * 600 + 300,
                40.0,
                Confidence::High,
            ));
        }

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|h| h.source == HotspotSource::Matched));
        assert!(out.iter().all(|h| h.n_obs == 2));
        assert!(out.windows(2).all(|w| w[0].acquired <= w[1].acquired));
    }

    #[test]
    fn no_detection_matched_when_viirs_is_5km_away() {
        let sites = [(36.0, 30.0), (37.0, 31.0), (38.0, 32.0)];
        let mut modis = Vec::new();
        let mut viirs = Vec::new();

        for (i, &(lat, lon)) in sites.iter().enumerate() {
            let i = i as i64;
            modis.push(make_hotspot(
                Sensor::ModisTerra,
                lat,
                lon,
                i * 600,
                50.0,
                Confidence::Nominal,
            ));
            viirs.push(make_hotspot(
                Sensor::ViirsSNPP,
                lat + 0.045,
                lon,
                i * 600 + 300,
                40.0,
                Confidence::High,
            ));
        }

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 6);
        assert_eq!(
            out.iter()
                .filter(|h| h.source == HotspotSource::ModisOnly)
                .count(),
            3
        );
        assert_eq!(
            out.iter()
                .filter(|h| h.source == HotspotSource::ViirsOnly)
                .count(),
            3
        );
        assert!(!out.iter().any(|h| h.source == HotspotSource::Matched));
    }

    fn synthetic(
        seed: u64,
        n_modis: usize,
        n_viirs: usize,
        pixel_km: f64,
    ) -> (Vec<Hotspot>, Vec<Hotspot>) {
        let mut state = seed;
        let mut next = || -> u64 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state >> 11
        };
        let confs = [Confidence::Low, Confidence::Nominal, Confidence::High];
        let mut make = |sensor: Sensor, i: usize| {
            let cluster = (next() % 25) as f64;
            let r1 = (next() % 10_000) as f64 / 10_000.0;
            let r2 = (next() % 10_000) as f64 / 10_000.0;
            let t = (next() % 6_000) as i64;
            let mut h = make_hotspot(
                sensor,
                36.0 + cluster * 0.2 + (r1 - 0.5) * 0.03,
                27.0 + cluster * 0.3 + (r2 - 0.5) * 0.03,
                t,
                5.0 + (i % 37) as f64,
                confs[(next() % 3) as usize],
            );
            h.scan = pixel_km;
            h.track = pixel_km;
            h.vza = Some((next() % 60) as f64);
            h
        };
        let modis = (0..n_modis)
            .map(|i| {
                make(
                    if i % 2 == 0 {
                        Sensor::ModisTerra
                    } else {
                        Sensor::ModisAqua
                    },
                    i,
                )
            })
            .collect();
        let viirs = (0..n_viirs).map(|i| make(Sensor::ViirsSNPP, i)).collect();
        (modis, viirs)
    }

    fn assert_links_valid(modis: &[Hotspot], viirs: &[Hotspot], out: &HarmonizeLinks) {
        let n_out = out.hotspots.len();
        assert_eq!(out.modis_links.len(), modis.len());
        assert_eq!(out.viirs_links.len(), viirs.len());
        assert_eq!(
            n_out,
            modis.len() + viirs.len() - out.interscan_removed - out.fleet_merged - out.matched_pairs
        );

        let mut modis_by_out: Vec<Vec<usize>> = vec![Vec::new(); n_out];
        let mut viirs_by_out: Vec<Vec<usize>> = vec![Vec::new(); n_out];
        for (i, link) in out.modis_links.iter().enumerate() {
            if let Some(k) = *link {
                assert!(k < n_out, "MODIS link {k} out of range {n_out}");
                modis_by_out[k].push(i);
            }
        }
        for (j, link) in out.viirs_links.iter().enumerate() {
            if let Some(k) = *link {
                assert!(k < n_out, "VIIRS link {k} out of range {n_out}");
                viirs_by_out[k].push(j);
            }
        }

        let mut matched = 0;
        for (k, h) in out.hotspots.iter().enumerate() {
            let (ms, vs) = (&modis_by_out[k], &viirs_by_out[k]);
            assert_eq!(h.n_obs as usize, ms.len() + vs.len(), "output {k} n_obs");
            for &i in ms {
                assert!(
                    h.platforms.contains(&modis[i].sensor),
                    "output {k} missing MODIS sensor"
                );
            }
            for &j in vs {
                assert!(
                    h.platforms.contains(&viirs[j].sensor),
                    "output {k} missing VIIRS sensor"
                );
            }
            match h.source {
                HotspotSource::Matched => {
                    matched += 1;
                    assert!(!ms.is_empty() && !vs.is_empty(), "output {k}");
                }
                HotspotSource::ModisOnly => {
                    assert!(!ms.is_empty() && vs.is_empty(), "output {k}");
                }
                HotspotSource::ViirsOnly => {
                    assert!(ms.is_empty() && !vs.is_empty(), "output {k}");
                }
            }
        }
        assert_eq!(matched, out.matched_pairs);
    }

    fn check_equivalence_and_links(modis: Vec<Hotspot>, viirs: Vec<Hotspot>) -> HarmonizeLinks {
        let expected = harmonize_brute_force(modis.clone(), viirs.clone());
        let plain = harmonize(modis.clone(), viirs.clone());
        let linked = harmonize_with_links(modis.clone(), viirs.clone());
        assert_harmonized_eq(&plain, &expected);
        assert_harmonized_eq(&linked.hotspots, &plain);
        assert_links_valid(&modis, &viirs, &linked);
        linked
    }

    #[test]
    fn with_links_matches_harmonize_on_empty_inputs() {
        let out = check_equivalence_and_links(Vec::new(), Vec::new());
        assert!(out.hotspots.is_empty());
        let (modis, _) = synthetic(1, 5, 0, 0.0);
        check_equivalence_and_links(modis, Vec::new());
        let (_, viirs) = synthetic(2, 0, 5, 0.0);
        check_equivalence_and_links(Vec::new(), viirs);
    }

    #[test]
    fn with_links_matches_harmonize_without_duplicates() {
        let (modis, viirs) = synthetic(7, 1_500, 2_000, 0.0);
        let out = check_equivalence_and_links(modis, viirs);
        assert_eq!(out.interscan_removed, 0);
        assert!(out.matched_pairs > 0);
    }

    #[test]
    fn with_links_matches_harmonize_with_interscan_duplicates() {
        for seed in [11, 12, 13] {
            let (modis, viirs) = synthetic(seed, 2_000, 2_500, 4.0);
            let out = check_equivalence_and_links(modis, viirs);
            assert!(out.interscan_removed > 0, "seed {seed} produced no duplicates");
            assert!(out.matched_pairs > 0, "seed {seed} produced no matches");
        }
    }

    #[test]
    fn with_links_on_hand_built_case() {
        // Two overlapping MODIS rows 2 s apart collapse into one, which then matches the VIIRS row.
        let mut m0 = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 10.0, Confidence::Low);
        let mut m1 = make_hotspot(Sensor::ModisTerra, 39.001, 32.0, 2, 30.0, Confidence::High);
        let far = make_hotspot(Sensor::ModisAqua, 41.0, 35.0, -500, 8.0, Confidence::Nominal);
        let v0 = make_hotspot(Sensor::ViirsSNPP, 39.0005, 32.0, 60, 12.0, Confidence::Nominal);
        let v1 = make_hotspot(Sensor::ViirsSNPP, 37.0, 30.0, 900, 4.0, Confidence::Low);
        m0.scan = 1.0;
        m0.track = 1.0;
        m1.scan = 1.0;
        m1.track = 1.0;

        let out = check_equivalence_and_links(vec![m0, m1, far], vec![v0, v1]);
        assert_eq!(out.interscan_removed, 1);
        assert_eq!(out.fleet_merged, 0);
        assert_eq!(out.matched_pairs, 1);
        assert_eq!(out.hotspots.len(), 3);
        assert_eq!(out.modis_links, vec![None, Some(1), Some(0)]);
        assert_eq!(out.viirs_links, vec![Some(1), Some(2)]);
        assert_eq!(out.hotspots[1].source, HotspotSource::Matched);
        assert_eq!(out.hotspots[2].source, HotspotSource::ViirsOnly);
    }

    #[test]
    fn with_links_tracks_fleet_consolidation() {
        // Terra and Aqua at the same fire merge in fleet consolidation, then match VIIRS.
        let terra = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 20.0, Confidence::Nominal);
        let aqua = make_hotspot(Sensor::ModisAqua, 39.002, 32.0, 1800, 25.0, Confidence::High);
        let snpp = make_hotspot(Sensor::ViirsSNPP, 39.001, 32.0, 2400, 18.0, Confidence::Nominal);

        let out = check_equivalence_and_links(vec![terra, aqua], vec![snpp]);
        assert_eq!(out.interscan_removed, 0);
        assert_eq!(out.fleet_merged, 1);
        assert_eq!(out.matched_pairs, 1);
        assert_eq!(out.hotspots.len(), 1);
        assert_eq!(out.modis_links, vec![Some(0), Some(0)]);
        assert_eq!(out.viirs_links, vec![Some(0)]);
        assert_eq!(out.hotspots[0].source, HotspotSource::Matched);
        assert_eq!(out.hotspots[0].n_obs, 3);
    }
}
