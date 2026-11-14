use crate::models::Hotspot;

const MAX_TIME_DIFF_SECS: i64 = 8;
const MIN_OVERLAP_RATIO: f64 = 0.30;

/// Remove interscan duplicate detections within a single sensor's data.
///
/// Two detections are duplicates when |Δt| < 8s AND pixel footprint
/// overlap > 30%. Keeps the highest-confidence record and assigns
/// the group's mean FRP (Li et al. 2018).
pub fn remove_interscan_duplicates(hotspots: &mut Vec<Hotspot>) {
    if hotspots.len() < 2 {
        return;
    }

    hotspots.sort_by_key(|a| a.acquired);

    let mut to_remove = vec![false; hotspots.len()];

    for i in 0..hotspots.len() {
        if to_remove[i] {
            continue;
        }
        let mut group = vec![i];

        for j in (i + 1)..hotspots.len() {
            if to_remove[j] {
                continue;
            }
            let dt = (hotspots[j].acquired - hotspots[i].acquired)
                .num_seconds()
                .abs();
            if dt > MAX_TIME_DIFF_SECS {
                break;
            }

            if hotspots[i].sensor != hotspots[j].sensor {
                continue;
            }

            if pixel_overlap(&hotspots[i], &hotspots[j]) > MIN_OVERLAP_RATIO {
                group.push(j);
            }
        }

        if group.len() > 1 {
            let mut best = group[0];
            for &idx in &group[1..] {
                if hotspots[idx].confidence > hotspots[best].confidence {
                    best = idx;
                }
            }

            let mean_frp: f64 =
                group.iter().map(|&idx| hotspots[idx].frp).sum::<f64>() / group.len() as f64;
            hotspots[best].frp = mean_frp;

            for &idx in &group {
                if idx != best {
                    to_remove[idx] = true;
                }
            }
        }
    }

    let mut idx = 0;
    hotspots.retain(|_| {
        let keep = !to_remove[idx];
        idx += 1;
        keep
    });
}

fn pixel_overlap(a: &Hotspot, b: &Hotspot) -> f64 {
    let a_size = a.scan.max(a.track);
    let b_size = b.scan.max(b.track);

    if a_size <= 0.0 || b_size <= 0.0 {
        return 0.0;
    }

    let dist_km = haversine_km(a.lat, a.lon, b.lat, b.lon);
    let max_extent = (a_size + b_size) / 2.0;

    if dist_km >= max_extent {
        0.0
    } else {
        1.0 - (dist_km / max_extent)
    }
}

pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6371.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    R * 2.0 * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, DayNight, FireType, Sensor};
    use chrono::{DateTime, TimeDelta};

    fn make_hotspot(
        lat: f64,
        lon: f64,
        frp: f64,
        confidence: Confidence,
        sensor: Sensor,
        offset_secs: i64,
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
            scan: 1.0,
            track: 1.0,
            satellite: "Test".to_string(),
            daynight: DayNight::Day,
            vza: None,
            fire_type: FireType::VegetationFire,
        }
    }

    #[test]
    fn ascending_confidence_keeps_single_highest_with_mean_frp() {
        let mut hotspots = vec![
            make_hotspot(37.0, 29.0, 10.0, Confidence::Low, Sensor::ModisTerra, 0),
            make_hotspot(
                37.001,
                29.001,
                20.0,
                Confidence::Nominal,
                Sensor::ModisTerra,
                3,
            ),
            make_hotspot(37.002, 29.002, 30.0, Confidence::High, Sensor::ModisTerra, 6),
        ];

        remove_interscan_duplicates(&mut hotspots);

        assert_eq!(hotspots.len(), 1);
        assert_eq!(hotspots[0].confidence, Confidence::High);
        assert!((hotspots[0].frp - 20.0).abs() < 1e-9);
    }

    #[test]
    fn descending_confidence_keeps_single_highest_with_mean_frp() {
        let mut hotspots = vec![
            make_hotspot(37.0, 29.0, 12.0, Confidence::High, Sensor::ModisAqua, 0),
            make_hotspot(
                37.001,
                29.001,
                24.0,
                Confidence::Nominal,
                Sensor::ModisAqua,
                2,
            ),
            make_hotspot(37.002, 29.002, 36.0, Confidence::Low, Sensor::ModisAqua, 5),
        ];

        remove_interscan_duplicates(&mut hotspots);

        assert_eq!(hotspots.len(), 1);
        assert_eq!(hotspots[0].confidence, Confidence::High);
        assert!((hotspots[0].frp - 24.0).abs() < 1e-9);
    }

    #[test]
    fn different_sensors_are_not_deduplicated() {
        let mut hotspots = vec![
            make_hotspot(37.0, 29.0, 15.0, Confidence::Low, Sensor::ModisTerra, 0),
            make_hotspot(37.0, 29.0, 25.0, Confidence::High, Sensor::ViirsSNPP, 0),
        ];

        remove_interscan_duplicates(&mut hotspots);

        assert_eq!(hotspots.len(), 2);
    }

    #[test]
    fn outside_time_window_not_deduplicated() {
        let mut hotspots = vec![
            make_hotspot(37.0, 29.0, 15.0, Confidence::Low, Sensor::ModisTerra, 0),
            make_hotspot(37.0, 29.0, 25.0, Confidence::High, Sensor::ModisTerra, 9),
        ];

        remove_interscan_duplicates(&mut hotspots);

        assert_eq!(hotspots.len(), 2);
    }

    #[test]
    fn non_overlapping_locations_not_deduplicated() {
        let mut hotspots = vec![
            make_hotspot(37.0, 29.0, 15.0, Confidence::Low, Sensor::ModisTerra, 0),
            make_hotspot(37.45, 29.0, 25.0, Confidence::High, Sensor::ModisTerra, 0),
        ];

        remove_interscan_duplicates(&mut hotspots);

        assert_eq!(hotspots.len(), 2);
    }

    #[test]
    fn empty_and_single_element_do_not_panic() {
        let mut empty: Vec<Hotspot> = Vec::new();
        remove_interscan_duplicates(&mut empty);
        assert!(empty.is_empty());

        let mut single = vec![make_hotspot(
            37.0,
            29.0,
            42.5,
            Confidence::Nominal,
            Sensor::ViirsNOAA20,
            0,
        )];
        remove_interscan_duplicates(&mut single);
        assert_eq!(single.len(), 1);
        assert!((single[0].frp - 42.5).abs() < 1e-9);
    }

    #[test]
    fn output_remains_sorted_by_acquired() {
        let mut hotspots = vec![
            make_hotspot(37.45, 29.0, 30.0, Confidence::High, Sensor::ModisTerra, 30),
            make_hotspot(
                37.001,
                29.001,
                20.0,
                Confidence::High,
                Sensor::ModisTerra,
                14,
            ),
            make_hotspot(37.0, 29.0, 10.0, Confidence::Low, Sensor::ModisTerra, 10),
            make_hotspot(38.0, 29.0, 5.0, Confidence::Nominal, Sensor::ModisTerra, 2),
        ];

        remove_interscan_duplicates(&mut hotspots);

        assert_eq!(hotspots.len(), 3);
        assert!(hotspots.windows(2).all(|w| w[0].acquired <= w[1].acquired));
    }

    #[test]
    fn haversine_zero_for_identical_points() {
        assert_eq!(haversine_km(39.0, 32.0, 39.0, 32.0), 0.0);
    }

    #[test]
    fn haversine_one_degree_of_latitude() {
        let d = haversine_km(0.0, 30.0, 1.0, 30.0);
        assert!((d - 111.19492664455873).abs() < 1e-9);
    }

    #[test]
    fn haversine_is_symmetric() {
        let forward = haversine_km(37.0, 29.0, 38.5, 31.2);
        let backward = haversine_km(38.5, 31.2, 37.0, 29.0);
        assert!((forward - backward).abs() < 1e-12);
    }

    #[test]
    fn haversine_crosses_antimeridian_the_short_way() {
        let d = haversine_km(0.0, 179.5, 0.0, -179.5);
        assert!((d - 111.19492664455873).abs() < 1e-6);
    }

    #[test]
    fn known_duplicate_set_reduces_total_frp_by_exactly_twenty_percent() {
        let mut hotspots = vec![
            make_hotspot(37.0, 29.0, 30.0, Confidence::Nominal, Sensor::ModisTerra, 0),
            make_hotspot(37.001, 29.0, 50.0, Confidence::High, Sensor::ModisTerra, 4),
            make_hotspot(37.5, 29.0, 60.0, Confidence::High, Sensor::ModisTerra, 0),
            make_hotspot(38.0, 29.0, 60.0, Confidence::High, Sensor::ModisTerra, 0),
        ];
        let frp_before: f64 = hotspots.iter().map(|h| h.frp).sum();

        remove_interscan_duplicates(&mut hotspots);

        assert_eq!(hotspots.len(), 3);
        let frp_after: f64 = hotspots.iter().map(|h| h.frp).sum();
        assert_eq!(frp_before, 200.0);
        assert!((frp_after - 160.0).abs() < 1e-9);
        assert!(((frp_before - frp_after) / frp_before - 0.2).abs() < 1e-12);

        let survivor = hotspots
            .iter()
            .find(|h| (h.lat - 37.0).abs() < 0.01)
            .expect("duplicate survivor should remain");
        assert_eq!(survivor.confidence, Confidence::High);
        assert!((survivor.frp - 40.0).abs() < 1e-9);
    }

    #[test]
    fn overlap_threshold_is_thirty_percent() {
        let deg_per_km = 1.0 / 111.19492664455873;

        let mut near = vec![
            make_hotspot(37.0, 29.0, 15.0, Confidence::Low, Sensor::ModisTerra, 0),
            make_hotspot(
                37.0 + 0.65 * deg_per_km,
                29.0,
                25.0,
                Confidence::High,
                Sensor::ModisTerra,
                2,
            ),
        ];
        remove_interscan_duplicates(&mut near);
        assert_eq!(near.len(), 1);

        let mut far = vec![
            make_hotspot(37.0, 29.0, 15.0, Confidence::Low, Sensor::ModisTerra, 0),
            make_hotspot(
                37.0 + 0.75 * deg_per_km,
                29.0,
                25.0,
                Confidence::High,
                Sensor::ModisTerra,
                2,
            ),
        ];
        remove_interscan_duplicates(&mut far);
        assert_eq!(far.len(), 2);
    }

    #[test]
    fn seven_seconds_apart_is_inside_time_window() {
        let mut hotspots = vec![
            make_hotspot(37.0, 29.0, 15.0, Confidence::Low, Sensor::ModisTerra, 0),
            make_hotspot(37.0, 29.0, 25.0, Confidence::High, Sensor::ModisTerra, 7),
        ];

        remove_interscan_duplicates(&mut hotspots);

        assert_eq!(hotspots.len(), 1);
    }
}
