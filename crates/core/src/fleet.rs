use crate::dedup::haversine_km;
use crate::models::{Hotspot, Sensor};

// Live FIRMS VIIRS observations show sibling platforms in the same orbital plane pass 43-59 min apart.
const FLEET_WINDOW_SECS: i64 = 3600;
// Matches the cross-sensor spatial matching radius.
const FLEET_RADIUS_KM: f64 = 2.0;

/// A VIIRS detection after fleet consolidation, carrying how many platforms saw it.
#[derive(Debug, Clone)]
pub struct FleetObservation {
    pub hotspot: Hotspot,
    pub frp_mean: f64,
    pub n_obs: u16,
    pub platforms: Vec<Sensor>,
}

/// Consolidate near-coincident detections of the same fire across VIIRS platforms
/// (SNPP, NOAA-20, NOAA-21) into one `FleetObservation` per group, sorted by `acquired`.
pub fn consolidate_fleet(viirs: &[Hotspot]) -> Vec<FleetObservation> {
    if viirs.is_empty() {
        return Vec::new();
    }

    let mut order: Vec<usize> = (0..viirs.len()).collect();
    order.sort_by_key(|&i| viirs[i].acquired);

    let mut used = vec![false; order.len()];
    let mut observations = Vec::with_capacity(order.len());

    for p in 0..order.len() {
        if used[p] {
            continue;
        }
        used[p] = true;

        let anchor_idx = order[p];
        let anchor = &viirs[anchor_idx];
        let mut group = vec![anchor_idx];

        for q in (p + 1)..order.len() {
            if used[q] {
                continue;
            }
            let cand_idx = order[q];
            let cand = &viirs[cand_idx];

            let dt = (cand.acquired - anchor.acquired).num_seconds();
            if dt > FLEET_WINDOW_SECS {
                break;
            }

            // Exclude day/night mismatches (e.g. 12h/23h overpass pairs are separate fire observations).
            if cand.daynight != anchor.daynight {
                continue;
            }

            // Interscan duplicates within a sensor are already removed; a second record from the
            // same platform indicates a distinct fire and limits over-merging on large fronts.
            if group.iter().any(|&idx| viirs[idx].sensor == cand.sensor) {
                continue;
            }

            if haversine_km(anchor.lat, anchor.lon, cand.lat, cand.lon) <= FLEET_RADIUS_KM {
                used[q] = true;
                group.push(cand_idx);
            }
        }

        let mut best_idx = group[0];
        for &idx in &group[1..] {
            let c_new = viirs[idx].confidence;
            let c_best = viirs[best_idx].confidence;
            if c_new > c_best || (c_new == c_best && idx < best_idx) {
                best_idx = idx;
            }
        }

        let frp_mean = group.iter().map(|&idx| viirs[idx].frp).sum::<f64>() / group.len() as f64;
        let n_obs = u16::try_from(group.len()).unwrap_or(u16::MAX);

        let mut platforms: Vec<Sensor> = group.iter().map(|&idx| viirs[idx].sensor).collect();
        platforms.sort();
        platforms.dedup();

        observations.push(FleetObservation {
            hotspot: viirs[best_idx].clone(),
            frp_mean,
            n_obs,
            platforms,
        });
    }

    // Downstream sliding-window matching requires observations sorted by representative `acquired`.
    observations.sort_by_key(|obs| obs.hotspot.acquired);
    observations
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

        let out = consolidate_fleet(&viirs);
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

        let out = consolidate_fleet(&viirs);
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

        let out = consolidate_fleet(&viirs);
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

        let out = consolidate_fleet(&viirs);
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

        let out = consolidate_fleet(&viirs);
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

        let out = consolidate_fleet(&viirs);
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
        let out = consolidate_fleet(&[]);
        assert!(out.is_empty());
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

        let out = consolidate_fleet(&viirs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].hotspot.sensor, Sensor::ViirsSNPP);
        assert!((out[0].hotspot.frp - 25.0).abs() < 1e-9);
        assert!((out[0].frp_mean - 30.0).abs() < 1e-9);
    }
}
