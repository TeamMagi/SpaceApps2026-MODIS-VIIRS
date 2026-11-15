use crate::config::{MatchingConfig, PipelineConfig, VzaConfig};
use crate::dedup::{haversine_km, remove_interscan_duplicates_indexed_with};
use crate::fleet::{self, FleetObservation};
use crate::h3index::{TimedH3Index, k_ring_for_radius, shift_time_clamped};
use crate::models::*;
use crate::uncertainty::{self, SensorFamily, UncertaintyParams};
use crate::vza;

pub(crate) const MATCH_RADIUS_KM: f64 = 2.0;
// 180 minutes: orbital geometry separates MODIS and VIIRS overpasses of the same fire
// (min 34 min, median ~158 min in live FIRMS data), so a narrow window yields zero matches.
pub(crate) const MATCH_WINDOW_SECS: i64 = 10_800;

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
    harmonize_with_config(modis, viirs, &PipelineConfig::default())
}

/// Same as [`harmonize`] with configurable thresholds.
pub fn harmonize_with_config(
    modis: Vec<Hotspot>,
    viirs: Vec<Hotspot>,
    config: &PipelineConfig,
) -> Vec<HarmonizedHotspot> {
    harmonize_with_links_config(modis, viirs, config).hotspots
}

/// Runs the same pipeline as [`harmonize`] and records which output each input row became.
pub fn harmonize_with_links(modis: Vec<Hotspot>, viirs: Vec<Hotspot>) -> HarmonizeLinks {
    harmonize_with_links_config(modis, viirs, &PipelineConfig::default())
}

/// Same as [`harmonize_with_links`] with configurable thresholds.
pub fn harmonize_with_links_config(
    mut modis: Vec<Hotspot>,
    mut viirs: Vec<Hotspot>,
    config: &PipelineConfig,
) -> HarmonizeLinks {
    let modis_input_len = modis.len();
    let viirs_input_len = viirs.len();

    // Step 1: Remove interscan duplicates (also sorts both vectors by `acquired` ascending)
    let modis_origin = remove_interscan_duplicates_indexed_with(&mut modis, &config.dedup);
    let viirs_origin = remove_interscan_duplicates_indexed_with(&mut viirs, &config.dedup);
    let interscan_removed =
        (modis_input_len - modis.len()) + (viirs_input_len - viirs.len());

    // Step 2: Consolidate both fleets before cross-sensor matching so sibling detections
    // (Terra/Aqua and SNPP/NOAA-20/NOAA-21) do not leak into the output as duplicate singletons.
    let (modis_fleet, modis_groups) = fleet::consolidate_modis_fleet_indexed_with(&modis, &config.fleet, &config.vza);
    let (viirs_fleet, viirs_groups) = fleet::consolidate_viirs_fleet_indexed_with(&viirs, &config.fleet, &config.vza);
    let fleet_merged =
        (modis.len() - modis_fleet.len()) + (viirs.len() - viirs_fleet.len());

    // Step 3 & 4: Match and fuse using time-sliced H3 spatial index candidate lookup
    let use_spatial = config.matching.radius_km.is_finite();
    let index = use_spatial.then(|| {
        TimedH3Index::build(
            viirs_fleet
                .iter()
                .map(|v| (v.hotspot.lat, v.hotspot.lon, v.hotspot.acquired)),
        )
    });
    let mut result = Vec::with_capacity(modis_fleet.len() + viirs_fleet.len());
    let mut modis_fleet_out = vec![0usize; modis_fleet.len()];
    let mut viirs_fleet_out: Vec<Option<usize>> = vec![None; viirs_fleet.len()];
    let mut matched_pairs = 0usize;
    let search_k = if use_spatial {
        k_ring_for_radius(config.matching.radius_km)
    } else {
        0
    };
    let pad = config.matching.window_secs.saturating_add(1);
    let mut candidates = Vec::new();

    for (mi, m) in modis_fleet.iter().enumerate() {
        let mh = &m.hotspot;
        let mut best_match: Option<(usize, f64)> = None;

        let t_min = shift_time_clamped(mh.acquired, pad.saturating_neg());
        let t_max = shift_time_clamped(mh.acquired, pad);
        let used_index = index.as_ref().is_some_and(|idx| {
            idx.candidates_within_into(mh.lat, mh.lon, search_k, t_min, t_max, &mut candidates)
        });

        let mut consider = |j: usize| {
            if viirs_fleet_out[j].is_some() {
                return;
            }

            let vh = &viirs_fleet[j].hotspot;
            let dt = (vh.acquired - mh.acquired).num_seconds().abs();
            if dt > config.matching.window_secs {
                return;
            }

            let dist = haversine_km(mh.lat, mh.lon, vh.lat, vh.lon);
            if dist > config.matching.radius_km {
                return;
            }

            let score = match_score_with(dist, dt, &config.matching);
            if best_match.is_none_or(|(_, best)| score > best) {
                best_match = Some((j, score));
            }
        };

        if used_index {
            // Iterate candidates in ascending index order: `score > best` keeps the first on ties.
            for &j in &candidates {
                consider(j);
            }
        } else {
            for j in 0..viirs_fleet.len() {
                consider(j);
            }
        }

        modis_fleet_out[mi] = result.len();
        if let Some((j, _)) = best_match {
            viirs_fleet_out[j] = Some(result.len());
            matched_pairs += 1;
            result.push(fuse_matched(m, &viirs_fleet[j], &config.uncertainty));
        } else {
            result.push(from_modis_only_with(
                m,
                &config.vza,
                &config.uncertainty,
            ));
        }
    }

    // Unmatched VIIRS detections
    for (j, v) in viirs_fleet.iter().enumerate() {
        if viirs_fleet_out[j].is_none() {
            viirs_fleet_out[j] = Some(result.len());
            result.push(from_viirs_only(v, &config.uncertainty));
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

fn fuse_matched(
    m: &FleetObservation,
    v: &FleetObservation,
    uncertainty_params: &UncertaintyParams,
) -> HarmonizedHotspot {
    let mh = &m.hotspot;
    let vh = &v.hotspot;
    let mut platforms = Vec::with_capacity(m.platforms.len() + v.platforms.len());
    platforms.extend_from_slice(&m.platforms);
    platforms.extend_from_slice(&v.platforms);
    platforms.sort();
    platforms.dedup();

    let n_obs = m.n_obs.saturating_add(v.n_obs);
    // The FRP value comes only from the VIIRS members, so the confidence interval must scale its
    // random term by the VIIRS member count alone; including MODIS members would narrow an
    // interval that no MODIS observation informs. `n_obs` still reports the total member count.
    let est = uncertainty::frp_uncertainty_for_obs(
        v.frp_corrected_mean,
        None,
        SensorFamily::from(HotspotSource::Matched),
        None,
        v.n_obs,
        uncertainty_params,
    );

    // Quality-priority: prefer VIIRS FRP (Zheng et al. 2026)
    HarmonizedHotspot {
        lat: vh.lat,
        lon: vh.lon,
        frp: vh.frp,
        frp_corrected: v.frp_corrected_mean,
        frp_lower: est.lower,
        frp_upper: est.upper,
        vza_factor: None,
        n_obs,
        platforms,
        confidence: higher_confidence(mh.confidence, vh.confidence),
        source: HotspotSource::Matched,
        acquired: vh.acquired,
        daynight: vh.daynight,
    }
}

fn from_modis_only_with(
    m: &FleetObservation,
    vza_config: &VzaConfig,
    uncertainty_params: &UncertaintyParams,
) -> HarmonizedHotspot {
    let mh = &m.hotspot;
    let factor = vza::vza_factor_with(mh, vza_config);
    // Per-detection latitude scaling is not applied: completeness differences between
    // MODIS and VIIRS belong to cell-level total FRP in the pre-VIIRS era, not to
    // individual detections (which would inflate 2000-2011 per-fire FRP and double-count
    // small fires already captured as ViirsOnly once VIIRS is present).
    let frp_corrected = m.frp_corrected_mean;

    // Random sensor/VZA errors shrink by 1/√n_obs across fleet members, whereas
    // MODIS scale/proxy uncertainty (`modis_scale_sigma`) is shared and stays unscaled.
    let est = uncertainty::frp_uncertainty_for_obs(
        frp_corrected,
        factor,
        SensorFamily::from(HotspotSource::ModisOnly),
        None,
        m.n_obs,
        uncertainty_params,
    );

    HarmonizedHotspot {
        lat: mh.lat,
        lon: mh.lon,
        frp: mh.frp,
        frp_corrected,
        frp_lower: est.lower,
        frp_upper: est.upper,
        vza_factor: factor,
        n_obs: m.n_obs,
        platforms: m.platforms.clone(),
        confidence: mh.confidence,
        source: HotspotSource::ModisOnly,
        acquired: mh.acquired,
        daynight: mh.daynight,
    }
}

#[cfg(test)]
fn from_modis_only(m: &FleetObservation) -> HarmonizedHotspot {
    from_modis_only_with(m, &VzaConfig::default(), &UncertaintyParams::default())
}

fn from_viirs_only(
    v: &FleetObservation,
    uncertainty_params: &UncertaintyParams,
) -> HarmonizedHotspot {
    let vh = &v.hotspot;
    // Random sensor noise shrinks by 1/√n_obs across merged VIIRS passes; no VZA or scale term.
    let est = uncertainty::frp_uncertainty_for_obs(
        v.frp_corrected_mean,
        None,
        SensorFamily::from(HotspotSource::ViirsOnly),
        None,
        v.n_obs,
        uncertainty_params,
    );

    HarmonizedHotspot {
        lat: vh.lat,
        lon: vh.lon,
        frp: vh.frp,
        frp_corrected: v.frp_corrected_mean,
        frp_lower: est.lower,
        frp_upper: est.upper,
        vza_factor: None,
        n_obs: v.n_obs,
        platforms: v.platforms.clone(),
        confidence: vh.confidence,
        source: HotspotSource::ViirsOnly,
        acquired: vh.acquired,
        daynight: vh.daynight,
    }
}

fn match_score_with(dist_km: f64, dt_secs: i64, config: &MatchingConfig) -> f64 {
    let spatial = 1.0 - (dist_km / config.radius_km);
    // A zero window only admits dt == 0, which is a perfect temporal match (and avoids 0/0).
    let temporal = if config.window_secs == 0 {
        1.0
    } else {
        1.0 - (dt_secs as f64 / config.window_secs as f64)
    };
    0.6 * spatial + 0.4 * temporal
}

#[cfg(test)]
fn match_score(dist_km: f64, dt_secs: i64) -> f64 {
    match_score_with(dist_km, dt_secs, &MatchingConfig::default())
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
                result.push(fuse_matched(m, &viirs[j], &UncertaintyParams::default()));
            } else {
                result.push(from_modis_only(m));
            }
        }

        for (j, v) in viirs.iter().enumerate() {
            if !viirs_matched[j] {
                result.push(from_viirs_only(v, &UncertaintyParams::default()));
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
                a.frp_lower, e.frp_lower,
                "frp_lower mismatch at index {i}"
            );
            assert_eq!(
                a.frp_upper, e.frp_upper,
                "frp_upper mismatch at index {i}"
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
        assert!(out[0].frp_corrected > out[0].frp);
        assert!((out[0].frp_corrected - (100.0 * factor)).abs() < 1e-12);
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
        assert!((out[0].frp_corrected - 80.0).abs() < 1e-12);
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

    #[test]
    fn narrower_match_window_splits_a_matched_pair() {
        let modis = vec![make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 50.0, Confidence::High)];
        let viirs = vec![make_hotspot(Sensor::ViirsSNPP, 39.009, 32.0, 3600, 40.0, Confidence::High)];

        let default_out = harmonize(modis.clone(), viirs.clone());
        assert_eq!(default_out.len(), 1);
        assert_eq!(default_out[0].source, HotspotSource::Matched);

        let mut config = PipelineConfig::default();
        config.matching.window_secs = 600;
        let out = harmonize_with_config(modis, viirs, &config);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|h| h.source != HotspotSource::Matched));
    }

    #[test]
    fn stricter_dedup_time_diff_keeps_both_detections() {
        let mut a = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 10.0, Confidence::Low);
        let mut b = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 8, 20.0, Confidence::High);
        a.scan = 1.0;
        a.track = 1.0;
        b.scan = 1.0;
        b.track = 1.0;

        assert_eq!(harmonize(vec![a.clone(), b.clone()], vec![]).len(), 1);

        let mut config = PipelineConfig::default();
        config.dedup.max_time_diff_secs = 2;
        let links = harmonize_with_links_config(vec![a, b], vec![], &config);
        assert_eq!(links.interscan_removed, 0);
        // Still within the fleet radius/window, but same platform records never merge there.
        assert_eq!(links.hotspots.len(), 2);
    }

    #[test]
    fn larger_match_radius_matches_pair_five_km_apart() {
        let modis = vec![make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 50.0, Confidence::Nominal)];
        // 0.045 deg latitude is ~5.0 km
        let viirs = vec![make_hotspot(Sensor::ViirsSNPP, 39.045, 32.0, 0, 40.0, Confidence::High)];

        assert_eq!(harmonize(modis.clone(), viirs.clone()).len(), 2);

        let mut config = PipelineConfig::default();
        config.matching.radius_km = 6.0;
        let out = harmonize_with_config(modis, viirs, &config);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::Matched);
    }

    #[test]
    fn identity_vza_coefficients_leave_frp_uncorrected() {
        let mut modis = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 50.0, Confidence::High);
        modis.vza = Some(45.0);

        let default_out = harmonize(vec![modis.clone()], vec![]);
        assert!(default_out[0].frp_corrected > default_out[0].frp);

        let mut config = PipelineConfig::default();
        config.vza = crate::config::VzaConfig { a: 0.0, b: 0.0, c: 1.0 };
        let out = harmonize_with_config(vec![modis], vec![], &config);
        assert_eq!(out[0].frp_corrected, out[0].frp);
    }

    fn xorshift64(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    fn reference_harmonize_with_links_config(
        mut modis: Vec<Hotspot>,
        mut viirs: Vec<Hotspot>,
        config: &PipelineConfig,
    ) -> HarmonizeLinks {
        let modis_input_len = modis.len();
        let viirs_input_len = viirs.len();

        let modis_origin = remove_interscan_duplicates_indexed_with(&mut modis, &config.dedup);
        let viirs_origin = remove_interscan_duplicates_indexed_with(&mut viirs, &config.dedup);
        let interscan_removed =
            (modis_input_len - modis.len()) + (viirs_input_len - viirs.len());

        let (modis_fleet, modis_groups) =
            fleet::consolidate_modis_fleet_indexed_with(&modis, &config.fleet, &config.vza);
        let (viirs_fleet, viirs_groups) =
            fleet::consolidate_viirs_fleet_indexed_with(&viirs, &config.fleet, &config.vza);
        let fleet_merged =
            (modis.len() - modis_fleet.len()) + (viirs.len() - viirs_fleet.len());

        let mut result = Vec::with_capacity(modis_fleet.len() + viirs_fleet.len());
        let mut modis_fleet_out = vec![0usize; modis_fleet.len()];
        let mut viirs_fleet_out: Vec<Option<usize>> = vec![None; viirs_fleet.len()];
        let mut matched_pairs = 0usize;

        for (mi, m) in modis_fleet.iter().enumerate() {
            let mh = &m.hotspot;
            let mut best_match: Option<(usize, f64)> = None;

            for (j, v) in viirs_fleet.iter().enumerate() {
                if viirs_fleet_out[j].is_some() {
                    continue;
                }

                let vh = &v.hotspot;
                let dt = (vh.acquired - mh.acquired).num_seconds().abs();
                if dt > config.matching.window_secs {
                    continue;
                }

                let dist = haversine_km(mh.lat, mh.lon, vh.lat, vh.lon);
                if dist > config.matching.radius_km {
                    continue;
                }

                let score = match_score_with(dist, dt, &config.matching);
                if best_match.is_none_or(|(_, best)| score > best) {
                    best_match = Some((j, score));
                }
            }

            modis_fleet_out[mi] = result.len();
            if let Some((j, _)) = best_match {
                viirs_fleet_out[j] = Some(result.len());
                matched_pairs += 1;
                result.push(fuse_matched(m, &viirs_fleet[j], &config.uncertainty));
            } else {
                result.push(from_modis_only_with(
                    m,
                    &config.vza,
                    &config.uncertainty,
                ));
            }
        }

        for (j, v) in viirs_fleet.iter().enumerate() {
            if viirs_fleet_out[j].is_none() {
                viirs_fleet_out[j] = Some(result.len());
                result.push(from_viirs_only(v, &config.uncertainty));
            }
        }

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
            let out =
                final_pos[viirs_fleet_out[vj].expect("every VIIRS fleet group produces or joins an output")];
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

    fn assert_links_bits_eq(actual: &HarmonizeLinks, expected: &HarmonizeLinks, ctx: &str) {
        assert_eq!(
            actual.interscan_removed, expected.interscan_removed,
            "{ctx}: interscan_removed"
        );
        assert_eq!(actual.fleet_merged, expected.fleet_merged, "{ctx}: fleet_merged");
        assert_eq!(actual.matched_pairs, expected.matched_pairs, "{ctx}: matched_pairs");
        assert_eq!(actual.modis_links, expected.modis_links, "{ctx}: modis_links");
        assert_eq!(actual.viirs_links, expected.viirs_links, "{ctx}: viirs_links");
        assert_eq!(actual.hotspots.len(), expected.hotspots.len(), "{ctx}: len");

        for (i, (a, e)) in actual
            .hotspots
            .iter()
            .zip(expected.hotspots.iter())
            .enumerate()
        {
            assert_eq!(a.lat.to_bits(), e.lat.to_bits(), "{ctx} [{i}]: lat");
            assert_eq!(a.lon.to_bits(), e.lon.to_bits(), "{ctx} [{i}]: lon");
            assert_eq!(a.frp.to_bits(), e.frp.to_bits(), "{ctx} [{i}]: frp");
            assert_eq!(
                a.frp_corrected.to_bits(),
                e.frp_corrected.to_bits(),
                "{ctx} [{i}]: frp_corrected"
            );
            assert_eq!(
                a.frp_lower.to_bits(),
                e.frp_lower.to_bits(),
                "{ctx} [{i}]: frp_lower"
            );
            assert_eq!(
                a.frp_upper.to_bits(),
                e.frp_upper.to_bits(),
                "{ctx} [{i}]: frp_upper"
            );
            assert_eq!(
                a.vza_factor.map(f64::to_bits),
                e.vza_factor.map(f64::to_bits),
                "{ctx} [{i}]: vza_factor"
            );
            assert_eq!(a.n_obs, e.n_obs, "{ctx} [{i}]: n_obs");
            assert_eq!(a.platforms, e.platforms, "{ctx} [{i}]: platforms");
            assert_eq!(a.confidence, e.confidence, "{ctx} [{i}]: confidence");
            assert_eq!(a.source, e.source, "{ctx} [{i}]: source");
            assert_eq!(a.acquired, e.acquired, "{ctx} [{i}]: acquired");
            assert_eq!(a.daynight, e.daynight, "{ctx} [{i}]: daynight");
        }
    }

    fn synthetic_pipeline_dataset(
        seed: u64,
        n_modis: usize,
        n_viirs: usize,
    ) -> (Vec<Hotspot>, Vec<Hotspot>) {
        let mut rng = seed;
        let base = Utc.with_ymd_and_hms(2025, 7, 15, 12, 0, 0).unwrap();
        let confs = [Confidence::Low, Confidence::Nominal, Confidence::High];
        let modis_sensors = [Sensor::ModisTerra, Sensor::ModisAqua];
        let viirs_sensors = [Sensor::ViirsSNPP, Sensor::ViirsNOAA20, Sensor::ViirsNOAA21];
        let centers = [
            (39.0, 32.0),
            (-14.2, -52.8),
            (0.0, 179.995),
            (0.0, -179.995),
            (85.0, 10.0),
            (-85.0, -120.0),
        ];

        let mut make_row = |sensor: Sensor| {
            let (clat, clon) = centers[(xorshift64(&mut rng) as usize) % centers.len()];
            let exact = xorshift64(&mut rng).is_multiple_of(8);
            let lat = if exact {
                clat
            } else {
                let r1 = (xorshift64(&mut rng) % 10_000) as f64 / 10_000.0;
                (clat + (r1 - 0.5) * 0.04).clamp(-89.9, 89.9)
            };
            let mut lon = if exact {
                clon
            } else {
                let r2 = (xorshift64(&mut rng) % 10_000) as f64 / 10_000.0;
                clon + (r2 - 0.5) * 0.04
            };
            if lon > 180.0 {
                lon -= 360.0;
            } else if lon < -180.0 {
                lon += 360.0;
            }

            let minute = (xorshift64(&mut rng) % 360) as i64;
            let burst_sec = ((xorshift64(&mut rng) % 4) as i64) * 3;
            let millis = ((xorshift64(&mut rng) % 4) as i64) * 250;
            let acquired = base + Duration::minutes(minute) + Duration::seconds(burst_sec) + Duration::milliseconds(millis);

            let mut h = make_hotspot(
                sensor,
                lat,
                lon,
                0,
                5.0 + (xorshift64(&mut rng) % 400) as f64 * 0.25,
                confs[(xorshift64(&mut rng) % 3) as usize],
            );
            h.acquired = acquired;
            h.daynight = if xorshift64(&mut rng).is_multiple_of(2) {
                DayNight::Day
            } else {
                DayNight::Night
            };
            h.scan = 0.5 + (xorshift64(&mut rng) % 4) as f64 * 0.6;
            h.track = 0.5 + (xorshift64(&mut rng) % 4) as f64 * 0.6;
            h.vza = Some((xorshift64(&mut rng) % 60) as f64);
            h
        };

        let mut modis: Vec<Hotspot> = (0..n_modis)
            .map(|i| make_row(modis_sensors[i % modis_sensors.len()]))
            .collect();
        let mut viirs: Vec<Hotspot> = (0..n_viirs)
            .map(|i| make_row(viirs_sensors[i % viirs_sensors.len()]))
            .collect();

        // Inject invalid coordinates (lat = 95.0) in both streams.
        for k in 0..6 {
            let mut m = make_hotspot(
                modis_sensors[k % 2],
                95.0,
                20.0 + (k % 2) as f64 * 0.001,
                (k % 3) as i64 * 600,
                20.0 + k as f64,
                confs[k % 3],
            );
            m.scan = 1.5;
            m.track = 1.5;
            modis.push(m);

            let mut v = make_hotspot(
                viirs_sensors[k % 3],
                95.0,
                20.0 + (k % 2) as f64 * 0.001,
                (k % 3) as i64 * 600 + 30,
                18.0 + k as f64,
                confs[(k + 1) % 3],
            );
            v.scan = 1.5;
            v.track = 1.5;
            viirs.push(v);
        }

        (modis, viirs)
    }

    #[test]
    fn timed_harmonize_with_links_is_bit_identical_to_reference_across_seeds_and_configs() {
        let mut large_radius = PipelineConfig::default();
        large_radius.fleet.radius_km = 10.0;
        large_radius.matching.radius_km = 10.0;

        let mut zero_radius = PipelineConfig::default();
        zero_radius.fleet.radius_km = 0.0;
        zero_radius.matching.radius_km = 0.0;

        let mut neg_dedup_ratio = PipelineConfig::default();
        neg_dedup_ratio.dedup.min_overlap_ratio = -0.1;

        let mut zero_window = PipelineConfig::default();
        zero_window.dedup.max_time_diff_secs = 0;
        zero_window.fleet.viirs_window_secs = 0;
        zero_window.fleet.modis_window_secs = 0;
        zero_window.matching.window_secs = 0;

        let configs = [
            ("default", PipelineConfig::default()),
            ("large_radius_10km", large_radius),
            ("zero_radius", zero_radius),
            ("neg_dedup_ratio", neg_dedup_ratio),
            ("zero_window", zero_window),
        ];

        for seed in [0x9E37_79B9_7F4A_7C15_u64, 0xCAFE_BABE_1357_2468_u64] {
            let full = synthetic_pipeline_dataset(seed, 1_200, 1_800);
            let fallback = synthetic_pipeline_dataset(seed ^ 0x33CC, 300, 450);

            for (label, cfg) in &configs {
                let (modis, viirs) = if cfg.dedup.min_overlap_ratio < 0.0 {
                    &fallback
                } else {
                    &full
                };
                let actual = harmonize_with_links_config(modis.clone(), viirs.clone(), cfg);
                let expected =
                    reference_harmonize_with_links_config(modis.clone(), viirs.clone(), cfg);
                assert_links_bits_eq(&actual, &expected, &format!("{label} seed={seed}"));
            }
        }
    }

    #[test]
    fn calibration_is_not_applied_per_detection_across_latitudes() {
        let mut modis_tropics =
            make_hotspot(Sensor::ModisTerra, 10.0, 20.0, 0, 100.0, Confidence::High);
        modis_tropics.vza = Some(30.0);

        let mut modis_boreal =
            make_hotspot(Sensor::ModisTerra, 60.0, 90.0, 10, 100.0, Confidence::High);
        modis_boreal.vza = Some(30.0);

        let mut modis_for_match =
            make_hotspot(Sensor::ModisAqua, -10.0, 30.0, 20, 100.0, Confidence::Nominal);
        modis_for_match.vza = Some(30.0);
        let viirs_matched =
            make_hotspot(Sensor::ViirsSNPP, -10.002, 30.002, 80, 50.0, Confidence::High);

        let viirs_only =
            make_hotspot(Sensor::ViirsNOAA20, 62.0, 95.0, 120, 40.0, Confidence::High);

        let out = harmonize(
            vec![modis_tropics, modis_boreal, modis_for_match],
            vec![viirs_matched, viirs_only],
        );
        assert_eq!(out.len(), 4);

        let tropics = &out[0];
        let boreal = &out[1];
        let matched = &out[2];
        let v_only = &out[3];

        assert_eq!(tropics.source, HotspotSource::ModisOnly);
        assert_eq!(boreal.source, HotspotSource::ModisOnly);
        assert_eq!(tropics.frp, 100.0);
        assert_eq!(boreal.frp, 100.0);
        // Same sensor, scan/VZA, and raw FRP at tropical vs boreal latitudes must yield identical
        // per-detection frp_corrected (completeness k_c belongs only to pre-VIIRS calendar totals).
        assert_eq!(tropics.frp_corrected.to_bits(), boreal.frp_corrected.to_bits());

        let factor = tropics.vza_factor.unwrap();
        assert!((tropics.frp_corrected - 100.0 * factor).abs() < 1e-12);
        assert!(tropics.frp_corrected > tropics.frp);

        assert_eq!(matched.source, HotspotSource::Matched);
        assert_eq!(matched.frp_corrected, 50.0);

        assert_eq!(v_only.source, HotspotSource::ViirsOnly);
        assert_eq!(v_only.frp_corrected, 40.0);
    }

    #[test]
    fn disabled_calibration_does_not_change_per_detection_vza_correction() {
        let mut m_trop = make_hotspot(Sensor::ModisTerra, 10.0, 20.0, 0, 100.0, Confidence::High);
        m_trop.vza = Some(40.0);
        let mut m_boreal = make_hotspot(Sensor::ModisAqua, 60.0, 90.0, 60, 75.0, Confidence::Nominal);
        m_boreal.vza = Some(25.0);

        let cfg_disabled = PipelineConfig {
            calibration: crate::config::CalibrationConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let out_enabled = harmonize(vec![m_trop.clone(), m_boreal.clone()], vec![]);
        let out_disabled =
            harmonize_with_config(vec![m_trop.clone(), m_boreal.clone()], vec![], &cfg_disabled);
        assert_eq!(out_disabled.len(), 2);

        let expected_trop = vza::apply_vza_correction(m_trop.frp, vza::vza_factor(&m_trop));
        let expected_boreal = vza::apply_vza_correction(m_boreal.frp, vza::vza_factor(&m_boreal));

        assert_eq!(out_disabled[0].frp_corrected.to_bits(), expected_trop.to_bits());
        assert_eq!(out_disabled[1].frp_corrected.to_bits(), expected_boreal.to_bits());
        assert_eq!(out_enabled[0].frp_corrected.to_bits(), out_disabled[0].frp_corrected.to_bits());
        assert_eq!(out_enabled[1].frp_corrected.to_bits(), out_disabled[1].frp_corrected.to_bits());
    }

    #[test]
    fn modis_only_interval_widens_strictly_with_scan_vza() {
        let scans = [1.0, 1.5, 2.5, 4.0];
        let mut widths_equal_corrected = Vec::new();
        let mut rel_widths_fixed_raw = Vec::new();
        let mut prev_factor = 0.0;

        for scan in scans {
            let mut m_raw = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 100.0, Confidence::High);
            m_raw.scan = scan;
            let out_raw = harmonize(vec![m_raw], vec![]);
            assert_eq!(out_raw.len(), 1);
            let h = &out_raw[0];
            assert!(h.frp_lower <= h.frp_corrected && h.frp_corrected <= h.frp_upper);
            let factor = h.vza_factor.expect("scan > 0 yields vza_factor");
            assert!(factor > prev_factor);
            prev_factor = factor;
            rel_widths_fixed_raw.push((h.frp_upper - h.frp_lower) / h.frp_corrected);

            // Same footprint-scaled FRP across scan angles: frp_corrected = raw_frp * factor.
            let mut m_same_fire =
                make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 100.0 / factor, Confidence::High);
            m_same_fire.scan = scan;
            let out_same = harmonize(vec![m_same_fire], vec![]);
            let hs = &out_same[0];
            assert!(hs.frp_lower <= hs.frp_corrected && hs.frp_corrected <= hs.frp_upper);
            widths_equal_corrected.push(hs.frp_upper - hs.frp_lower);
        }

        for pair in widths_equal_corrected.windows(2) {
            assert!(
                pair[1] > pair[0],
                "expected strictly increasing interval width with scan VZA, got {widths_equal_corrected:?}"
            );
        }
        for pair in rel_widths_fixed_raw.windows(2) {
            assert!(
                pair[1] > pair[0],
                "expected strictly increasing relative width with scan VZA, got {rel_widths_fixed_raw:?}"
            );
        }
    }

    #[test]
    fn matched_and_viirs_only_have_intervals_unaffected_by_vza() {
        let mut m_nadir = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 100.0, Confidence::High);
        m_nadir.scan = 1.0;
        let v_for_nadir = make_hotspot(Sensor::ViirsSNPP, 39.001, 32.001, 60, 50.0, Confidence::High);

        let mut m_edge = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 100.0, Confidence::High);
        m_edge.scan = 4.0;
        let v_for_edge = make_hotspot(Sensor::ViirsSNPP, 39.001, 32.001, 60, 50.0, Confidence::High);

        let out_nadir = harmonize(vec![m_nadir], vec![v_for_nadir]);
        let out_edge = harmonize(vec![m_edge], vec![v_for_edge]);
        assert_eq!(out_nadir[0].source, HotspotSource::Matched);
        assert_eq!(out_edge[0].source, HotspotSource::Matched);
        assert!(out_nadir[0].frp_lower < out_nadir[0].frp_corrected);
        assert!(out_nadir[0].frp_corrected < out_nadir[0].frp_upper);
        assert_eq!(out_nadir[0].frp_lower.to_bits(), out_edge[0].frp_lower.to_bits());
        assert_eq!(out_nadir[0].frp_upper.to_bits(), out_edge[0].frp_upper.to_bits());

        let mut v_nadir = make_hotspot(Sensor::ViirsNOAA20, 39.0, 32.0, 0, 40.0, Confidence::Nominal);
        v_nadir.scan = 0.4;
        let mut v_edge = make_hotspot(Sensor::ViirsNOAA20, 39.0, 32.0, 0, 40.0, Confidence::Nominal);
        v_edge.scan = 1.6;
        v_edge.vza = Some(50.0);

        let vo_nadir = harmonize(vec![], vec![v_nadir]);
        let vo_edge = harmonize(vec![], vec![v_edge]);
        assert_eq!(vo_nadir[0].source, HotspotSource::ViirsOnly);
        assert!(vo_nadir[0].frp_lower < vo_nadir[0].frp_corrected);
        assert!(vo_nadir[0].frp_corrected < vo_nadir[0].frp_upper);
        assert_eq!(vo_nadir[0].frp_lower.to_bits(), vo_edge[0].frp_lower.to_bits());
        assert_eq!(vo_nadir[0].frp_upper.to_bits(), vo_edge[0].frp_upper.to_bits());
    }

    #[test]
    fn matched_interval_equals_viirs_only_interval_for_same_viirs_observation() {
        // MODIS contributes no FRP information to a Matched record (FRP comes from VIIRS), so the
        // interval must match the same VIIRS fleet observation processed on its own.
        let viirs_row =
            make_hotspot(Sensor::ViirsSNPP, 39.001, 32.001, 60, 50.0, Confidence::High);
        let matched = harmonize(
            vec![make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 80.0, Confidence::High)],
            vec![viirs_row.clone()],
        );
        let viirs_only = harmonize(vec![], vec![viirs_row]);

        assert_eq!(matched[0].source, HotspotSource::Matched);
        assert_eq!(matched[0].n_obs, 2);
        assert_eq!(viirs_only[0].source, HotspotSource::ViirsOnly);
        assert_eq!(viirs_only[0].n_obs, 1);
        assert_eq!(matched[0].frp_corrected, viirs_only[0].frp_corrected);
        assert_eq!(
            matched[0].frp_lower.to_bits(),
            viirs_only[0].frp_lower.to_bits()
        );
        assert_eq!(
            matched[0].frp_upper.to_bits(),
            viirs_only[0].frp_upper.to_bits()
        );
    }

    #[test]
    fn matched_interval_follows_viirs_member_count_for_two_platform_viirs_fleet() {
        let modis = vec![
            make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 80.0, Confidence::High),
            make_hotspot(Sensor::ModisAqua, 39.0, 32.0, 300, 90.0, Confidence::Nominal),
        ];
        let viirs = vec![
            make_hotspot(Sensor::ViirsSNPP, 39.001, 32.0, 600, 50.0, Confidence::High),
            make_hotspot(Sensor::ViirsNOAA20, 39.001, 32.0, 900, 60.0, Confidence::High),
        ];

        let matched = harmonize(modis, viirs.clone());
        let viirs_only = harmonize(vec![], viirs);

        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].source, HotspotSource::Matched);
        assert_eq!(matched[0].n_obs, 4);
        assert_eq!(viirs_only.len(), 1);
        assert_eq!(viirs_only[0].source, HotspotSource::ViirsOnly);
        assert_eq!(viirs_only[0].n_obs, 2);
        assert_eq!(matched[0].frp_corrected, viirs_only[0].frp_corrected);
        assert_eq!(
            matched[0].frp_lower.to_bits(),
            viirs_only[0].frp_lower.to_bits()
        );
        assert_eq!(
            matched[0].frp_upper.to_bits(),
            viirs_only[0].frp_upper.to_bits()
        );
    }

    #[test]
    fn fuse_matched_interval_ignores_modis_member_count() {
        // Fleet groups cannot hold three MODIS members (only Terra and Aqua exist), so the
        // MODIS member count is exercised through a hand-built fleet observation.
        let params = UncertaintyParams::default();
        let v = FleetObservation {
            hotspot: make_hotspot(Sensor::ViirsSNPP, 39.0, 32.0, 0, 50.0, Confidence::High),
            frp_mean: 50.0,
            frp_corrected_mean: 50.0,
            n_obs: 2,
            platforms: vec![Sensor::ViirsSNPP, Sensor::ViirsNOAA20],
        };
        let m = FleetObservation {
            hotspot: make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 80.0, Confidence::High),
            frp_mean: 80.0,
            frp_corrected_mean: 80.0,
            n_obs: 3,
            platforms: vec![Sensor::ModisTerra, Sensor::ModisAqua],
        };

        let matched = fuse_matched(&m, &v, &params);
        let viirs_only = from_viirs_only(&v, &params);

        assert_eq!(matched.n_obs, 5);
        assert_eq!(matched.frp_lower.to_bits(), viirs_only.frp_lower.to_bits());
        assert_eq!(matched.frp_upper.to_bits(), viirs_only.frp_upper.to_bits());
    }

    #[test]
    fn higher_n_obs_narrows_random_uncertainty_but_preserves_systematic() {
        // ViirsOnly (n_obs=1) vs Matched with 3 VIIRS + 1 MODIS (output n_obs=4) at identical
        // frp_corrected=50: only the three VIIRS observations enter the random term.
        let single_viirs = harmonize(
            vec![],
            vec![make_hotspot(Sensor::ViirsSNPP, 39.0, 32.0, 0, 50.0, Confidence::High)],
        );
        let four_obs_matched = harmonize(
            vec![make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 80.0, Confidence::High)],
            vec![
                make_hotspot(Sensor::ViirsSNPP, 39.001, 32.0, 10, 50.0, Confidence::High),
                make_hotspot(Sensor::ViirsNOAA20, 39.001, 32.0, 20, 50.0, Confidence::High),
                make_hotspot(Sensor::ViirsNOAA21, 39.001, 32.0, 30, 50.0, Confidence::High),
            ],
        );
        assert_eq!(single_viirs[0].n_obs, 1);
        assert_eq!(four_obs_matched[0].n_obs, 4);
        let w1 = single_viirs[0].frp_upper - single_viirs[0].frp_lower;
        let w4 = four_obs_matched[0].frp_upper - four_obs_matched[0].frp_lower;
        assert!(w4 < w1, "expected n_obs=4 width ({w4}) < n_obs=1 width ({w1})");
        assert!((w4 - w1 / 3f64.sqrt()).abs() < 1e-12);

        // ModisOnly: with zero random sensor/VZA sigma, systematic calibration sigma stays unscaled by n_obs.
        let obs1 = FleetObservation {
            hotspot: make_hotspot(Sensor::ModisTerra, 60.0, 20.0, 0, 100.0, Confidence::High),
            frp_mean: 100.0,
            frp_corrected_mean: 100.0,
            n_obs: 1,
            platforms: vec![Sensor::ModisTerra],
        };
        let mut obs4 = obs1.clone();
        obs4.n_obs = 4;
        let sys_only = UncertaintyParams {
            modis_sensor_sigma: 0.0,
            viirs_sensor_sigma: 0.0,
            vza_error_coeff: 0.0,
            ..UncertaintyParams::default()
        };
        let h1 = from_modis_only_with(&obs1, &VzaConfig::default(), &sys_only);
        let h4 = from_modis_only_with(&obs4, &VzaConfig::default(), &sys_only);
        assert!((h1.frp_lower - h4.frp_lower).abs() < 1e-12);
        assert!((h1.frp_upper - h4.frp_upper).abs() < 1e-12);
    }
}
