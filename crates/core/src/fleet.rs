use std::ops::ControlFlow;

use crate::config::{FleetConfig, VzaConfig};
use crate::dedup::haversine_km;
use crate::h3index::{TimedH3Index, k_ring_for_radius, shift_time_clamped};
use crate::models::{Hotspot, Sensor};
use crate::vza;

// Siblings in the VIIRS fleet pass 43-59 min apart in live FIRMS data.
pub(crate) const VIIRS_FLEET_WINDOW_SECS: i64 = 3600;
// Terra crosses near 10:30 local and Aqua near 13:30, and no measured Terra/Aqua
// pair of the same fire falls within 60 min, so MODIS needs the wider window.
pub(crate) const MODIS_FLEET_WINDOW_SECS: i64 = 10_800;
// Matches the cross-sensor spatial matching radius.
pub(crate) const FLEET_RADIUS_KM: f64 = 2.0;

/// A consolidated detection across platforms of the same sensor family (MODIS or VIIRS).
#[derive(Debug, Clone)]
pub struct FleetObservation {
    pub hotspot: Hotspot,
    pub frp_mean: f64,
    pub frp_corrected_mean: f64,
    pub n_obs: u16,
    pub platforms: Vec<Sensor>,
}

/// Consolidate near-coincident VIIRS detections (SNPP, NOAA-20, NOAA-21) into one
/// `FleetObservation` per group, sorted by `acquired`.
pub fn consolidate_viirs_fleet(viirs: &[Hotspot]) -> Vec<FleetObservation> {
    consolidate_viirs_fleet_with(viirs, &FleetConfig::default(), &VzaConfig::default())
}

/// Same as [`consolidate_viirs_fleet`] with configurable thresholds.
pub fn consolidate_viirs_fleet_with(
    viirs: &[Hotspot],
    fleet: &FleetConfig,
    vza: &VzaConfig,
) -> Vec<FleetObservation> {
    consolidate_viirs_fleet_indexed_with(viirs, fleet, vza).0
}

/// Same as [`consolidate_viirs_fleet`], also returning input member indices per observation.
pub fn consolidate_viirs_fleet_indexed(viirs: &[Hotspot]) -> (Vec<FleetObservation>, Vec<Vec<usize>>) {
    consolidate_viirs_fleet_indexed_with(viirs, &FleetConfig::default(), &VzaConfig::default())
}

/// Same as [`consolidate_viirs_fleet_indexed`] with configurable thresholds.
pub fn consolidate_viirs_fleet_indexed_with(
    viirs: &[Hotspot],
    fleet: &FleetConfig,
    vza: &VzaConfig,
) -> (Vec<FleetObservation>, Vec<Vec<usize>>) {
    consolidate_fleet_indexed_with(viirs, fleet.viirs_window_secs, fleet, vza)
}

/// Consolidate near-coincident MODIS detections (Terra, Aqua) into one
/// `FleetObservation` per group, sorted by `acquired`.
pub fn consolidate_modis_fleet(modis: &[Hotspot]) -> Vec<FleetObservation> {
    consolidate_modis_fleet_with(modis, &FleetConfig::default(), &VzaConfig::default())
}

/// Same as [`consolidate_modis_fleet`] with configurable thresholds.
pub fn consolidate_modis_fleet_with(
    modis: &[Hotspot],
    fleet: &FleetConfig,
    vza: &VzaConfig,
) -> Vec<FleetObservation> {
    consolidate_modis_fleet_indexed_with(modis, fleet, vza).0
}

/// Same as [`consolidate_modis_fleet`], also returning input member indices per observation.
pub fn consolidate_modis_fleet_indexed(modis: &[Hotspot]) -> (Vec<FleetObservation>, Vec<Vec<usize>>) {
    consolidate_modis_fleet_indexed_with(modis, &FleetConfig::default(), &VzaConfig::default())
}

/// Same as [`consolidate_modis_fleet_indexed`] with configurable thresholds.
pub fn consolidate_modis_fleet_indexed_with(
    modis: &[Hotspot],
    fleet: &FleetConfig,
    vza: &VzaConfig,
) -> (Vec<FleetObservation>, Vec<Vec<usize>>) {
    consolidate_fleet_indexed_with(modis, fleet.modis_window_secs, fleet, vza)
}

/// Consolidate near-coincident detections of the same fire across sibling platforms
/// within `window_secs` into one `FleetObservation` per group, sorted by `acquired`.
pub fn consolidate_fleet(hotspots: &[Hotspot], window_secs: i64) -> Vec<FleetObservation> {
    consolidate_fleet_indexed(hotspots, window_secs).0
}

/// Same as [`consolidate_fleet`], also returning `members[k]` = indices in `hotspots`
/// merged into the `k`-th output `FleetObservation`.
pub fn consolidate_fleet_indexed(
    hotspots: &[Hotspot],
    window_secs: i64,
) -> (Vec<FleetObservation>, Vec<Vec<usize>>) {
    consolidate_fleet_indexed_with(hotspots, window_secs, &FleetConfig::default(), &VzaConfig::default())
}

/// Same as [`consolidate_fleet_indexed`]; `window_secs` stays explicit because the VIIRS and
/// MODIS windows differ, while the merge radius comes from `fleet`.
pub fn consolidate_fleet_indexed_with(
    hotspots: &[Hotspot],
    window_secs: i64,
    fleet: &FleetConfig,
    vza_config: &VzaConfig,
) -> (Vec<FleetObservation>, Vec<Vec<usize>>) {
    if hotspots.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut order: Vec<usize> = (0..hotspots.len()).collect();
    order.sort_by_key(|&i| hotspots[i].acquired);

    let mut used = vec![false; order.len()];
    let mut entries: Vec<(FleetObservation, Vec<usize>)> = Vec::with_capacity(order.len());

    let use_spatial = fleet.radius_km.is_finite();
    let index = use_spatial.then(|| {
        TimedH3Index::build(
            order
                .iter()
                .map(|&idx| (hotspots[idx].lat, hotspots[idx].lon, hotspots[idx].acquired)),
        )
    });
    let search_k = if use_spatial {
        // +1 ring accounts for cell-edge queries and smaller icosahedron face-center cells so
        // the spatial filter remains a strict superset of `haversine_km <= fleet.radius_km`.
        k_ring_for_radius(fleet.radius_km) + 1
    } else {
        0
    };
    let mut candidates = Vec::new();

    for p in 0..order.len() {
        if used[p] {
            continue;
        }
        used[p] = true;

        let anchor_idx = order[p];
        let anchor = &hotspots[anchor_idx];
        let mut group = vec![anchor_idx];

        let mut try_candidate = |q: usize| -> ControlFlow<()> {
            if used[q] {
                return ControlFlow::Continue(());
            }
            let cand_idx = order[q];
            let cand = &hotspots[cand_idx];

            let dt = (cand.acquired - anchor.acquired).num_seconds();
            if dt > window_secs {
                return ControlFlow::Break(());
            }

            // Exclude day/night mismatches (e.g. 12h/23h overpass pairs are separate fire observations).
            if cand.daynight != anchor.daynight {
                return ControlFlow::Continue(());
            }

            // Interscan duplicates within a sensor are already removed; a second record from the
            // same platform indicates a distinct fire and limits over-merging on large fronts.
            if group.iter().any(|&idx| hotspots[idx].sensor == cand.sensor) {
                return ControlFlow::Continue(());
            }

            if haversine_km(anchor.lat, anchor.lon, cand.lat, cand.lon) <= fleet.radius_km {
                used[q] = true;
                group.push(cand_idx);
            }
            ControlFlow::Continue(())
        };

        let t_min = shift_time_clamped(anchor.acquired, -1);
        let t_max = shift_time_clamped(anchor.acquired, window_secs.saturating_add(1));
        let used_index = index.as_ref().is_some_and(|idx| {
            idx.candidates_for_pos_within_into(p, search_k, t_min, t_max, &mut candidates)
        });

        if used_index {
            // Process candidates in ascending `order` position so greedy same-sensor exclusion
            // and f64 summation order for frp_mean / frp_corrected_mean stay bit-identical.
            for &q in &candidates {
                if q <= p {
                    continue;
                }
                if try_candidate(q).is_break() {
                    break;
                }
            }
        } else {
            for q in (p + 1)..order.len() {
                if try_candidate(q).is_break() {
                    break;
                }
            }
        }

        let mut best_idx = group[0];
        for &idx in &group[1..] {
            let c_new = hotspots[idx].confidence;
            let c_best = hotspots[best_idx].confidence;
            if c_new > c_best || (c_new == c_best && idx < best_idx) {
                best_idx = idx;
            }
        }

        let group_len = group.len() as f64;
        let frp_mean = group.iter().map(|&idx| hotspots[idx].frp).sum::<f64>() / group_len;
        // Correct each member with its own viewing geometry before averaging: Terra and Aqua
        // view the same fire from different scan angles, so the representative's VZA does not
        // apply to its sibling.
        let frp_corrected_mean = group
            .iter()
            .map(|&idx| {
                let member = &hotspots[idx];
                vza::apply_vza_correction(member.frp, vza::vza_factor_with(member, vza_config))
            })
            .sum::<f64>()
            / group_len;
        let n_obs = u16::try_from(group.len()).unwrap_or(u16::MAX);

        let mut platforms: Vec<Sensor> = group.iter().map(|&idx| hotspots[idx].sensor).collect();
        platforms.sort();
        platforms.dedup();

        entries.push((
            FleetObservation {
                hotspot: hotspots[best_idx].clone(),
                frp_mean,
                frp_corrected_mean,
                n_obs,
                platforms,
            },
            group,
        ));
    }

    // Downstream sliding-window matching requires observations sorted by representative `acquired`.
    entries.sort_by_key(|(obs, _)| obs.hotspot.acquired);
    entries.into_iter().unzip()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, DayNight, FireType};
    use chrono::{DateTime, TimeDelta};

    fn make_hotspot(
        sensor: Sensor,
        lat: f64,
        lon: f64,
        offset_secs: i64,
        frp: f64,
        confidence: Confidence,
        daynight: DayNight,
    ) -> Hotspot {
        let base = DateTime::from_timestamp(1_700_000_000, 0).expect("valid timestamp");
        Hotspot {
            lat,
            lon,
            brightness: 320.0,
            frp,
            confidence,
            sensor,
            acquired: base + TimeDelta::seconds(offset_secs),
            scan: 0.4,
            track: 0.4,
            satellite: String::new(),
            daynight,
            vza: None,
            fire_type: FireType::VegetationFire,
        }
    }

    #[test]
    fn noaa20_and_noaa21_within_45_min_and_1km_merge() {
        // ~0.009 deg latitude is ~1.0 km
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsNOAA20,
                -14.0,
                25.0,
                0,
                30.0,
                Confidence::Nominal,
                DayNight::Day,
            ),
            make_hotspot(
                Sensor::ViirsNOAA21,
                -14.009,
                25.0,
                45 * 60,
                50.0,
                Confidence::High,
                DayNight::Day,
            ),
        ];

        let out = consolidate_viirs_fleet(&viirs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].n_obs, 2);
        assert_eq!(
            out[0].platforms,
            vec![Sensor::ViirsNOAA20, Sensor::ViirsNOAA21]
        );
        assert!((out[0].frp_mean - 40.0).abs() < 1e-9);
        assert_eq!(out[0].hotspot.sensor, Sensor::ViirsNOAA21);
        assert_eq!(out[0].hotspot.confidence, Confidence::High);
    }

    #[test]
    fn different_daynight_at_same_location_do_not_merge() {
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsNOAA20,
                -14.0,
                25.0,
                0,
                30.0,
                Confidence::High,
                DayNight::Day,
            ),
            make_hotspot(
                Sensor::ViirsNOAA21,
                -14.0,
                25.0,
                30 * 60,
                40.0,
                Confidence::High,
                DayNight::Night,
            ),
        ];

        let out = consolidate_viirs_fleet(&viirs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].n_obs, 1);
        assert_eq!(out[1].n_obs, 1);
    }

    #[test]
    fn detections_more_than_60_minutes_apart_do_not_merge() {
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsNOAA20,
                -14.0,
                25.0,
                0,
                30.0,
                Confidence::High,
                DayNight::Day,
            ),
            make_hotspot(
                Sensor::ViirsNOAA21,
                -14.0,
                25.0,
                70 * 60,
                40.0,
                Confidence::High,
                DayNight::Day,
            ),
        ];

        let out = consolidate_viirs_fleet(&viirs);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn detections_more_than_2km_apart_do_not_merge() {
        // ~0.045 deg latitude is ~5.0 km
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsNOAA20,
                -14.0,
                25.0,
                0,
                30.0,
                Confidence::High,
                DayNight::Day,
            ),
            make_hotspot(
                Sensor::ViirsNOAA21,
                -14.045,
                25.0,
                45 * 60,
                40.0,
                Confidence::High,
                DayNight::Day,
            ),
        ];

        let out = consolidate_viirs_fleet(&viirs);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn same_sensor_within_window_and_radius_does_not_merge() {
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsNOAA20,
                -14.0,
                25.0,
                0,
                30.0,
                Confidence::High,
                DayNight::Day,
            ),
            make_hotspot(
                Sensor::ViirsNOAA20,
                -14.0,
                25.0,
                10 * 60,
                40.0,
                Confidence::High,
                DayNight::Day,
            ),
        ];

        let out = consolidate_viirs_fleet(&viirs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].n_obs, 1);
        assert_eq!(out[1].n_obs, 1);
    }

    #[test]
    fn three_platforms_merge_with_sorted_platforms() {
        // Input deliberately ordered NOAA-21, SNPP, NOAA-20 to verify ascending platform sort.
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsNOAA21,
                -14.0,
                25.0,
                0,
                20.0,
                Confidence::Low,
                DayNight::Day,
            ),
            make_hotspot(
                Sensor::ViirsSNPP,
                -14.005,
                25.0,
                25 * 60,
                40.0,
                Confidence::Nominal,
                DayNight::Day,
            ),
            make_hotspot(
                Sensor::ViirsNOAA20,
                -14.008,
                25.0,
                50 * 60,
                60.0,
                Confidence::High,
                DayNight::Day,
            ),
        ];

        let out = consolidate_viirs_fleet(&viirs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].n_obs, 3);
        assert_eq!(
            out[0].platforms,
            vec![Sensor::ViirsSNPP, Sensor::ViirsNOAA20, Sensor::ViirsNOAA21]
        );
        assert!((out[0].frp_mean - 40.0).abs() < 1e-9);
        assert_eq!(out[0].hotspot.sensor, Sensor::ViirsNOAA20);
    }

    #[test]
    fn empty_input_returns_empty_without_panic() {
        assert!(consolidate_viirs_fleet(&[]).is_empty());
        assert!(consolidate_modis_fleet(&[]).is_empty());
    }

    #[test]
    fn confidence_tie_picks_smallest_index() {
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsSNPP,
                -14.0,
                25.0,
                0,
                25.0,
                Confidence::High,
                DayNight::Day,
            ),
            make_hotspot(
                Sensor::ViirsNOAA20,
                -14.002,
                25.0,
                45 * 60,
                35.0,
                Confidence::High,
                DayNight::Day,
            ),
        ];

        let out = consolidate_viirs_fleet(&viirs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].hotspot.sensor, Sensor::ViirsSNPP);
        assert!((out[0].hotspot.frp - 25.0).abs() < 1e-9);
        assert!((out[0].frp_mean - 30.0).abs() < 1e-9);
    }

    #[test]
    fn terra_and_aqua_150_min_and_1km_apart_merge_in_modis_fleet() {
        // ~0.009 deg latitude is ~1.0 km
        let modis = vec![
            make_hotspot(
                Sensor::ModisTerra,
                39.0,
                32.0,
                0,
                50.0,
                Confidence::Nominal,
                DayNight::Day,
            ),
            make_hotspot(
                Sensor::ModisAqua,
                39.009,
                32.0,
                150 * 60,
                70.0,
                Confidence::High,
                DayNight::Day,
            ),
        ];

        let out = consolidate_modis_fleet(&modis);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].n_obs, 2);
        assert_eq!(
            out[0].platforms,
            vec![Sensor::ModisTerra, Sensor::ModisAqua]
        );
    }

    #[test]
    fn terra_and_aqua_150_min_apart_do_not_merge_under_viirs_window() {
        let modis = vec![
            make_hotspot(
                Sensor::ModisTerra,
                39.0,
                32.0,
                0,
                50.0,
                Confidence::Nominal,
                DayNight::Day,
            ),
            make_hotspot(
                Sensor::ModisAqua,
                39.009,
                32.0,
                150 * 60,
                70.0,
                Confidence::High,
                DayNight::Day,
            ),
        ];

        let out = consolidate_viirs_fleet(&modis);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn modis_detections_200_min_apart_do_not_merge() {
        let modis = vec![
            make_hotspot(
                Sensor::ModisTerra,
                39.0,
                32.0,
                0,
                50.0,
                Confidence::High,
                DayNight::Day,
            ),
            make_hotspot(
                Sensor::ModisAqua,
                39.005,
                32.0,
                200 * 60,
                70.0,
                Confidence::High,
                DayNight::Day,
            ),
        ];

        let out = consolidate_modis_fleet(&modis);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn modis_fleet_applies_per_member_vza_to_frp_corrected_mean() {
        let mut terra = make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            0,
            100.0,
            Confidence::High,
            DayNight::Day,
        );
        terra.vza = Some(20.0);

        let mut aqua = make_hotspot(
            Sensor::ModisAqua,
            39.004,
            32.0,
            120 * 60,
            140.0,
            Confidence::Nominal,
            DayNight::Day,
        );
        aqua.vza = Some(50.0);

        let expected_terra = vza::apply_vza_correction(terra.frp, vza::vza_factor(&terra));
        let expected_aqua = vza::apply_vza_correction(aqua.frp, vza::vza_factor(&aqua));
        let expected_corrected_mean = 0.5 * (expected_terra + expected_aqua);

        let out = consolidate_modis_fleet(&[terra, aqua]);
        assert_eq!(out.len(), 1);
        assert!((out[0].frp_mean - 120.0).abs() < 1e-9);
        assert!((out[0].frp_corrected_mean - expected_corrected_mean).abs() < 1e-12);
        assert!(out[0].frp_corrected_mean > out[0].frp_mean);
    }

    #[test]
    fn viirs_fleet_keeps_frp_corrected_mean_equal_to_frp_mean() {
        let mut snpp = make_hotspot(
            Sensor::ViirsSNPP,
            -14.0,
            25.0,
            0,
            30.0,
            Confidence::Nominal,
            DayNight::Day,
        );
        snpp.vza = Some(45.0);

        let mut noaa20 = make_hotspot(
            Sensor::ViirsNOAA20,
            -14.005,
            25.0,
            40 * 60,
            50.0,
            Confidence::High,
            DayNight::Day,
        );
        noaa20.vza = Some(30.0);

        let out = consolidate_viirs_fleet(&[snpp, noaa20]);
        assert_eq!(out.len(), 1);
        assert!((out[0].frp_mean - 40.0).abs() < 1e-9);
        assert_eq!(out[0].frp_corrected_mean, out[0].frp_mean);
    }

    fn xorshift64(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    fn reference_fleet(
        hotspots: &[Hotspot],
        window_secs: i64,
        fleet: &FleetConfig,
        vza_config: &VzaConfig,
    ) -> (Vec<FleetObservation>, Vec<Vec<usize>>) {
        if hotspots.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut order: Vec<usize> = (0..hotspots.len()).collect();
        order.sort_by_key(|&i| hotspots[i].acquired);

        let mut used = vec![false; order.len()];
        let mut entries: Vec<(FleetObservation, Vec<usize>)> = Vec::with_capacity(order.len());

        for p in 0..order.len() {
            if used[p] {
                continue;
            }
            used[p] = true;

            let anchor_idx = order[p];
            let anchor = &hotspots[anchor_idx];
            let mut group = vec![anchor_idx];

            for q in (p + 1)..order.len() {
                if used[q] {
                    continue;
                }
                let cand_idx = order[q];
                let cand = &hotspots[cand_idx];

                let dt = (cand.acquired - anchor.acquired).num_seconds();
                if dt > window_secs {
                    break;
                }

                if cand.daynight != anchor.daynight {
                    continue;
                }

                if group.iter().any(|&idx| hotspots[idx].sensor == cand.sensor) {
                    continue;
                }

                if haversine_km(anchor.lat, anchor.lon, cand.lat, cand.lon) <= fleet.radius_km {
                    used[q] = true;
                    group.push(cand_idx);
                }
            }

            let mut best_idx = group[0];
            for &idx in &group[1..] {
                let c_new = hotspots[idx].confidence;
                let c_best = hotspots[best_idx].confidence;
                if c_new > c_best || (c_new == c_best && idx < best_idx) {
                    best_idx = idx;
                }
            }

            let group_len = group.len() as f64;
            let frp_mean = group.iter().map(|&idx| hotspots[idx].frp).sum::<f64>() / group_len;
            let frp_corrected_mean = group
                .iter()
                .map(|&idx| {
                    let member = &hotspots[idx];
                    vza::apply_vza_correction(member.frp, vza::vza_factor_with(member, vza_config))
                })
                .sum::<f64>()
                / group_len;
            let n_obs = u16::try_from(group.len()).unwrap_or(u16::MAX);

            let mut platforms: Vec<Sensor> = group.iter().map(|&idx| hotspots[idx].sensor).collect();
            platforms.sort();
            platforms.dedup();

            entries.push((
                FleetObservation {
                    hotspot: hotspots[best_idx].clone(),
                    frp_mean,
                    frp_corrected_mean,
                    n_obs,
                    platforms,
                },
                group,
            ));
        }

        entries.sort_by_key(|(obs, _)| obs.hotspot.acquired);
        entries.into_iter().unzip()
    }

    fn assert_fleet_bits_eq(
        actual: &(Vec<FleetObservation>, Vec<Vec<usize>>),
        expected: &(Vec<FleetObservation>, Vec<Vec<usize>>),
        ctx: &str,
    ) {
        assert_eq!(actual.0.len(), expected.0.len(), "{ctx}: obs len");
        assert_eq!(actual.1, expected.1, "{ctx}: members");

        for (i, (a, e)) in actual.0.iter().zip(expected.0.iter()).enumerate() {
            assert_eq!(a.hotspot.lat.to_bits(), e.hotspot.lat.to_bits(), "{ctx} [{i}]: lat");
            assert_eq!(a.hotspot.lon.to_bits(), e.hotspot.lon.to_bits(), "{ctx} [{i}]: lon");
            assert_eq!(
                a.hotspot.brightness.to_bits(),
                e.hotspot.brightness.to_bits(),
                "{ctx} [{i}]: brightness"
            );
            assert_eq!(a.hotspot.frp.to_bits(), e.hotspot.frp.to_bits(), "{ctx} [{i}]: frp");
            assert_eq!(a.hotspot.confidence, e.hotspot.confidence, "{ctx} [{i}]: confidence");
            assert_eq!(a.hotspot.sensor, e.hotspot.sensor, "{ctx} [{i}]: sensor");
            assert_eq!(a.hotspot.acquired, e.hotspot.acquired, "{ctx} [{i}]: acquired");
            assert_eq!(a.hotspot.scan.to_bits(), e.hotspot.scan.to_bits(), "{ctx} [{i}]: scan");
            assert_eq!(
                a.hotspot.track.to_bits(),
                e.hotspot.track.to_bits(),
                "{ctx} [{i}]: track"
            );
            assert_eq!(a.hotspot.daynight, e.hotspot.daynight, "{ctx} [{i}]: daynight");
            assert_eq!(
                a.hotspot.vza.map(f64::to_bits),
                e.hotspot.vza.map(f64::to_bits),
                "{ctx} [{i}]: vza"
            );
            assert_eq!(a.frp_mean.to_bits(), e.frp_mean.to_bits(), "{ctx} [{i}]: frp_mean");
            assert_eq!(
                a.frp_corrected_mean.to_bits(),
                e.frp_corrected_mean.to_bits(),
                "{ctx} [{i}]: frp_corrected_mean"
            );
            assert_eq!(a.n_obs, e.n_obs, "{ctx} [{i}]: n_obs");
            assert_eq!(a.platforms, e.platforms, "{ctx} [{i}]: platforms");
        }
    }

    fn synthetic_fleet_dataset(seed: u64, count: usize) -> Vec<Hotspot> {
        let mut rng = seed;
        let base = DateTime::from_timestamp(1_700_000_000, 0).expect("valid timestamp");
        let sensors = [
            Sensor::ViirsSNPP,
            Sensor::ViirsNOAA20,
            Sensor::ViirsNOAA21,
            Sensor::ModisTerra,
            Sensor::ModisAqua,
        ];
        let confs = [Confidence::Low, Confidence::Nominal, Confidence::High];
        let centers = [
            (39.0, 32.0),
            (-14.2, -52.8),
            (0.0, 179.995),
            (0.0, -179.995),
            (85.0, 10.0),
            (-85.0, -120.0),
        ];

        let mut out = Vec::with_capacity(count + 8);
        for _ in 0..count {
            let (clat, clon) = centers[(xorshift64(&mut rng) as usize) % centers.len()];
            // Include exact-zero offsets so zero-radius merging is also exercised.
            let zero_offset = xorshift64(&mut rng).is_multiple_of(5);
            let lat = if zero_offset {
                clat
            } else {
                let r1 = (xorshift64(&mut rng) % 10_000) as f64 / 10_000.0;
                (clat + (r1 - 0.5) * 0.05).clamp(-89.9, 89.9)
            };
            let mut lon = if zero_offset {
                clon
            } else {
                let r2 = (xorshift64(&mut rng) % 10_000) as f64 / 10_000.0;
                clon + (r2 - 0.5) * 0.05
            };
            if lon > 180.0 {
                lon -= 360.0;
            } else if lon < -180.0 {
                lon += 360.0;
            }

            let minute = (xorshift64(&mut rng) % 300) as i64;
            let subsec = ((xorshift64(&mut rng) % 4) as i64) * 250;
            let acquired = base + TimeDelta::minutes(minute) + TimeDelta::milliseconds(subsec);
            let sensor = sensors[(xorshift64(&mut rng) as usize) % sensors.len()];
            let conf = confs[(xorshift64(&mut rng) as usize) % confs.len()];
            let daynight = if xorshift64(&mut rng).is_multiple_of(2) {
                DayNight::Day
            } else {
                DayNight::Night
            };
            let frp = 2.0 + (xorshift64(&mut rng) % 500) as f64 * 0.41;

            let mut h = make_hotspot(sensor, lat, lon, 0, frp, conf, daynight);
            h.acquired = acquired;
            h.scan = 0.5 + (xorshift64(&mut rng) % 4) as f64 * 0.5;
            h.track = 0.5 + (xorshift64(&mut rng) % 4) as f64 * 0.5;
            h.vza = Some((xorshift64(&mut rng) % 60) as f64);
            out.push(h);
        }

        // Invalid coordinates (lat = 95.0) at equal and near-by timestamps.
        for k in 0..8 {
            out.push(make_hotspot(
                sensors[k % 3],
                95.0,
                20.0 + (k % 2) as f64 * 0.001,
                (k % 3) as i64 * 600,
                12.5 + k as f64,
                confs[k % 3],
                DayNight::Day,
            ));
        }
        out
    }

    #[test]
    fn timed_fleet_is_bit_identical_to_reference_across_seeds_and_configs() {
        let configs = [
            ("default_viirs", 3600_i64, FleetConfig::default()),
            ("default_modis", 10_800_i64, FleetConfig::default()),
            (
                "large_radius_10km",
                3600_i64,
                FleetConfig {
                    viirs_window_secs: 3600,
                    modis_window_secs: 10_800,
                    radius_km: 10.0,
                },
            ),
            (
                "zero_radius",
                3600_i64,
                FleetConfig {
                    viirs_window_secs: 3600,
                    modis_window_secs: 10_800,
                    radius_km: 0.0,
                },
            ),
            ("zero_window", 0_i64, FleetConfig::default()),
        ];

        let vza_cfg = VzaConfig::default();
        for seed in [0x0123_4567_89AB_CDEF_u64, 0xFEEDC0DE_12345678_u64] {
            let data = synthetic_fleet_dataset(seed, 2_000);
            for (label, window_secs, fleet_cfg) in &configs {
                let actual = consolidate_fleet_indexed_with(&data, *window_secs, fleet_cfg, &vza_cfg);
                let expected = reference_fleet(&data, *window_secs, fleet_cfg, &vza_cfg);
                assert_fleet_bits_eq(&actual, &expected, &format!("{label} seed={seed}"));
            }
        }
    }
}
