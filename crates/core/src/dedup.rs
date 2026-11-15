use std::ops::ControlFlow;

use crate::config::DedupConfig;
use crate::h3index::{TimedH3Index, k_ring_for_radius, shift_time_clamped};
use crate::models::Hotspot;

pub(crate) const MAX_TIME_DIFF_SECS: i64 = 8;
pub(crate) const MIN_OVERLAP_RATIO: f64 = 0.30;

/// Remove interscan duplicate detections within a single sensor's data.
///
/// Two detections are duplicates when |Δt| <= max_time_diff_secs (default 8 s) AND pixel
/// footprint overlap > 30%. Keeps the highest-confidence record and assigns
/// the group's mean FRP (Li et al. 2018).
pub fn remove_interscan_duplicates(hotspots: &mut Vec<Hotspot>) {
    remove_interscan_duplicates_with(hotspots, &DedupConfig::default());
}

/// Same as [`remove_interscan_duplicates`] with configurable thresholds.
pub fn remove_interscan_duplicates_with(hotspots: &mut Vec<Hotspot>, config: &DedupConfig) {
    remove_interscan_duplicates_indexed_with(hotspots, config);
}

/// Same as [`remove_interscan_duplicates`], returning for each surviving hotspot (at its
/// final index) the index it held in the input vector.
pub fn remove_interscan_duplicates_indexed(hotspots: &mut Vec<Hotspot>) -> Vec<usize> {
    remove_interscan_duplicates_indexed_with(hotspots, &DedupConfig::default())
}

/// Same as [`remove_interscan_duplicates_indexed`] with configurable thresholds.
pub fn remove_interscan_duplicates_indexed_with(
    hotspots: &mut Vec<Hotspot>,
    config: &DedupConfig,
) -> Vec<usize> {
    if hotspots.len() < 2 {
        return (0..hotspots.len()).collect();
    }

    // Sorting an index permutation yields the exact stable order of `sort_by_key(acquired)`
    // while preserving the input position of each survivor.
    let mut origin: Vec<usize> = (0..hotspots.len()).collect();
    origin.sort_by_key(|&i| hotspots[i].acquired);
    let mut slots: Vec<Option<Hotspot>> = std::mem::take(hotspots).into_iter().map(Some).collect();
    hotspots.extend(origin.iter().map(|&i| slots[i].take().expect("each index taken once")));

    let mut to_remove = vec![false; hotspots.len()];
    // Per-record raw FRP accumulator: (sum of raw FRPs, number of raw observations). A survivor
    // can anchor a later group, so averaging its already-averaged FRP would silently down-weight
    // the observations folded into it; carrying the accumulator keeps every raw observation equal.
    let mut frp_sum: Vec<f64> = hotspots.iter().map(|h| h.frp).collect();
    let mut frp_count: Vec<u32> = vec![1; hotspots.len()];

    let mut max_pixel_km = 0.0_f64;
    for h in hotspots.iter() {
        let size = h.scan.max(h.track);
        if size > max_pixel_km {
            max_pixel_km = size;
        }
    }

    // Negative min_overlap_ratio makes `0.0 > ratio` true at any distance (even dist >= max_extent),
    // so spatial pruning would drop distant matches; fall back to the full time-window scan.
    let use_spatial = config.min_overlap_ratio >= 0.0
        && config.min_overlap_ratio.is_finite()
        && max_pixel_km.is_finite();
    let index = use_spatial.then(|| {
        TimedH3Index::build(hotspots.iter().map(|h| (h.lat, h.lon, h.acquired)))
    });
    let mut candidates = Vec::new();

    for i in 0..hotspots.len() {
        if to_remove[i] {
            continue;
        }
        let a_size = hotspots[i].scan.max(hotspots[i].track);
        if index.is_some() && (a_size <= 0.0 || a_size.is_nan()) {
            continue;
        }
        let search_k = if index.is_some() {
            let max_extent = (a_size + max_pixel_km) * 0.5;
            let overlap_scale = (1.0 - config.min_overlap_ratio + 1e-12).clamp(0.0, 1.0);
            // +1 ring accounts for cell-edge queries and smaller icosahedron face-center cells.
            k_ring_for_radius(max_extent * overlap_scale) + 1
        } else {
            0
        };

        let mut group = vec![i];
        {
            let mut try_candidate = |j: usize| -> ControlFlow<()> {
                if to_remove[j] {
                    return ControlFlow::Continue(());
                }
                let dt = (hotspots[j].acquired - hotspots[i].acquired)
                    .num_seconds()
                    .abs();
                if dt > config.max_time_diff_secs {
                    return ControlFlow::Break(());
                }
                if hotspots[i].sensor != hotspots[j].sensor {
                    return ControlFlow::Continue(());
                }
                if pixel_overlap(&hotspots[i], &hotspots[j]) > config.min_overlap_ratio {
                    group.push(j);
                }
                ControlFlow::Continue(())
            };

            let t_min = shift_time_clamped(hotspots[i].acquired, -1);
            let t_max = shift_time_clamped(
                hotspots[i].acquired,
                config.max_time_diff_secs.saturating_add(1),
            );
            let used_index = index.as_ref().is_some_and(|idx| {
                idx.candidates_for_pos_within_into(i, search_k, t_min, t_max, &mut candidates)
            });

            if used_index {
                // Process candidates in ascending index order: group order breaks confidence ties,
                // fixes f64 summation order for mean_frp, and mutates hotspots[best].frp in place.
                for &j in &candidates {
                    if j <= i {
                        continue;
                    }
                    if try_candidate(j).is_break() {
                        break;
                    }
                }
            } else {
                for j in (i + 1)..hotspots.len() {
                    if try_candidate(j).is_break() {
                        break;
                    }
                }
            }
        }

        if group.len() > 1 {
            let mut best = group[0];
            for &idx in &group[1..] {
                if hotspots[idx].confidence > hotspots[best].confidence {
                    best = idx;
                }
            }

            let mut sum = 0.0_f64;
            let mut count = 0_u32;
            // `group` is in ascending index order (index candidates are sorted, the fallback scan
            // walks upward), which keeps the f64 summation order deterministic.
            for &idx in &group {
                sum += frp_sum[idx];
                count += frp_count[idx];
            }
            frp_sum[best] = sum;
            frp_count[best] = count;
            hotspots[best].frp = sum / f64::from(count);

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

    origin
        .into_iter()
        .zip(to_remove)
        .filter_map(|(i, removed)| (!removed).then_some(i))
        .collect()
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

    #[test]
    fn indexed_variant_reports_input_positions_of_survivors() {
        // Input is out of time order so the sort permutation and dedup retention both matter.
        let input = vec![
            make_hotspot(38.0, 30.0, 5.0, Confidence::Low, Sensor::ModisTerra, 100),
            make_hotspot(39.0, 32.0, 30.0, Confidence::High, Sensor::ModisTerra, 2),
            make_hotspot(39.001, 32.0, 10.0, Confidence::Nominal, Sensor::ModisTerra, 0),
            make_hotspot(40.0, 35.0, 7.0, Confidence::Nominal, Sensor::ModisTerra, 50),
        ];
        let mut plain = input.clone();
        remove_interscan_duplicates(&mut plain);

        let mut indexed = input.clone();
        let origin = remove_interscan_duplicates_indexed(&mut indexed);

        assert_eq!(origin, vec![1, 3, 0]);
        assert_eq!(indexed.len(), plain.len());
        for ((a, b), &i) in indexed.iter().zip(&plain).zip(&origin) {
            assert_eq!(a.acquired, b.acquired);
            assert_eq!(a.frp, b.frp);
            assert_eq!(a.lat, input[i].lat);
            assert_eq!(a.lon, input[i].lon);
        }
    }

    fn xorshift64(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    fn reference_dedup(hotspots: &mut Vec<Hotspot>, config: &DedupConfig) -> Vec<usize> {
        if hotspots.len() < 2 {
            return (0..hotspots.len()).collect();
        }

        let mut origin: Vec<usize> = (0..hotspots.len()).collect();
        origin.sort_by_key(|&i| hotspots[i].acquired);
        let mut slots: Vec<Option<Hotspot>> =
            std::mem::take(hotspots).into_iter().map(Some).collect();
        hotspots.extend(origin.iter().map(|&i| slots[i].take().expect("each index taken once")));

        let mut to_remove = vec![false; hotspots.len()];
        // Same per-record raw FRP accumulator semantics as the indexed implementation, so the
        // bit-equality test compares the two traversal strategies and not two averaging rules.
        let mut frp_sum: Vec<f64> = hotspots.iter().map(|h| h.frp).collect();
        let mut frp_count: Vec<u32> = vec![1; hotspots.len()];

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
                if dt > config.max_time_diff_secs {
                    break;
                }

                if hotspots[i].sensor != hotspots[j].sensor {
                    continue;
                }

                if pixel_overlap(&hotspots[i], &hotspots[j]) > config.min_overlap_ratio {
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

                let mut sum = 0.0_f64;
                let mut count = 0_u32;
                for &idx in &group {
                    sum += frp_sum[idx];
                    count += frp_count[idx];
                }
                frp_sum[best] = sum;
                frp_count[best] = count;
                hotspots[best].frp = sum / f64::from(count);

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

        origin
            .into_iter()
            .zip(to_remove)
            .filter_map(|(i, removed)| (!removed).then_some(i))
            .collect()
    }

    fn assert_hotspot_bits_eq(a: &Hotspot, b: &Hotspot, ctx: &str) {
        assert_eq!(a.lat.to_bits(), b.lat.to_bits(), "{ctx}: lat");
        assert_eq!(a.lon.to_bits(), b.lon.to_bits(), "{ctx}: lon");
        assert_eq!(a.brightness.to_bits(), b.brightness.to_bits(), "{ctx}: brightness");
        assert_eq!(a.frp.to_bits(), b.frp.to_bits(), "{ctx}: frp");
        assert_eq!(a.confidence, b.confidence, "{ctx}: confidence");
        assert_eq!(a.sensor, b.sensor, "{ctx}: sensor");
        assert_eq!(a.acquired, b.acquired, "{ctx}: acquired");
        assert_eq!(a.scan.to_bits(), b.scan.to_bits(), "{ctx}: scan");
        assert_eq!(a.track.to_bits(), b.track.to_bits(), "{ctx}: track");
        assert_eq!(a.satellite, b.satellite, "{ctx}: satellite");
        assert_eq!(a.daynight, b.daynight, "{ctx}: daynight");
        assert_eq!(
            a.vza.map(f64::to_bits),
            b.vza.map(f64::to_bits),
            "{ctx}: vza"
        );
        assert_eq!(a.fire_type, b.fire_type, "{ctx}: fire_type");
    }

    fn synthetic_dedup_dataset(seed: u64, count: usize) -> Vec<Hotspot> {
        let mut rng = seed;
        let base = DateTime::from_timestamp(1_700_000_000, 0).expect("valid timestamp");
        let sensors = [
            Sensor::ModisTerra,
            Sensor::ModisAqua,
            Sensor::ViirsSNPP,
            Sensor::ViirsNOAA20,
            Sensor::ViirsNOAA21,
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

        let mut out = Vec::with_capacity(count + 16);
        for i in 0..count {
            let (clat, clon) = centers[(xorshift64(&mut rng) as usize) % centers.len()];
            let r1 = (xorshift64(&mut rng) % 10_000) as f64 / 10_000.0;
            let r2 = (xorshift64(&mut rng) % 10_000) as f64 / 10_000.0;
            let lat = (clat + (r1 - 0.5) * 0.03).clamp(-89.9, 89.9);
            let mut lon = clon + (r2 - 0.5) * 0.03;
            if lon > 180.0 {
                lon -= 360.0;
            } else if lon < -180.0 {
                lon += 360.0;
            }

            // Mix same-minute bursts, equal timestamps, sub-second offsets, and hours-apart passes.
            let pass_offset = ((xorshift64(&mut rng) % 6) as i64) * 3_600;
            let burst_offset = ((xorshift64(&mut rng) % 4) as i64) * 4;
            let millis = ((xorshift64(&mut rng) % 5) as i64) * 250;
            let acquired = base + TimeDelta::seconds(pass_offset + burst_offset) + TimeDelta::milliseconds(millis);

            let sensor = sensors[(xorshift64(&mut rng) as usize) % sensors.len()];
            let conf = confs[(xorshift64(&mut rng) as usize) % confs.len()];
            let daynight = if xorshift64(&mut rng).is_multiple_of(2) {
                DayNight::Day
            } else {
                DayNight::Night
            };
            let scan = match xorshift64(&mut rng) % 5 {
                0 => 0.0,
                1 => 0.4,
                2 => 1.0,
                3 => 2.2,
                _ => 4.5,
            };
            let track = 0.5 + (xorshift64(&mut rng) % 4) as f64 * 0.6;
            let frp = 1.5 + (xorshift64(&mut rng) % 500) as f64 * 0.37;

            out.push(Hotspot {
                lat,
                lon,
                brightness: 310.0 + (i % 50) as f64,
                frp,
                confidence: conf,
                sensor,
                acquired,
                scan,
                track,
                satellite: format!("S{}", i % 3),
                daynight,
                vza: Some((xorshift64(&mut rng) % 60) as f64),
                fire_type: FireType::VegetationFire,
            });
        }

        // Inject invalid coordinates (lat = 95.0) at shared timestamps and near valid points.
        for k in 0..8 {
            let mut h = make_hotspot(
                95.0,
                20.0 + (k % 2) as f64 * 0.001,
                15.0 + k as f64,
                confs[k % 3],
                Sensor::ViirsSNPP,
                (k % 2) as i64 * 2,
            );
            h.scan = 1.5;
            h.track = 1.5;
            out.push(h);
        }

        out
    }

    fn frp_values(hotspots: &[Hotspot]) -> Vec<f64> {
        hotspots.iter().map(|h| h.frp).collect()
    }

    #[test]
    fn eight_seconds_apart_is_inside_time_window() {
        let mut hotspots = vec![
            make_hotspot(37.0, 29.0, 15.0, Confidence::Low, Sensor::ModisTerra, 0),
            make_hotspot(37.0, 29.0, 25.0, Confidence::High, Sensor::ModisTerra, 8),
        ];

        remove_interscan_duplicates(&mut hotspots);

        assert_eq!(hotspots.len(), 1);
    }

    #[test]
    fn interscan_mean_frp_weights_every_raw_observation_equally() {
        // Synthetic counterpart of Greece 2007-08-25 (MODIS): the first group keeps a member that
        // then anchors a second group, so its mean FRP must carry the raw observation count.
        let mut hotspots = vec![
            make_hotspot(37.0, 29.0, 31.1, Confidence::Low, Sensor::ModisTerra, 0),
            make_hotspot(37.0, 29.0, 1626.7, Confidence::High, Sensor::ModisTerra, 6),
            make_hotspot(37.0, 29.0, 39.5, Confidence::Low, Sensor::ModisTerra, 7),
            make_hotspot(
                37.0,
                29.0,
                1699.3,
                Confidence::Nominal,
                Sensor::ModisTerra,
                8,
            ),
            make_hotspot(
                37.0,
                29.0,
                104.4,
                Confidence::Nominal,
                Sensor::ModisTerra,
                9,
            ),
            make_hotspot(37.0, 29.0, 94.0, Confidence::Low, Sensor::ModisTerra, 13),
        ];

        remove_interscan_duplicates(&mut hotspots);

        assert_eq!(hotspots.len(), 1);
        assert_eq!(hotspots[0].confidence, Confidence::High);
        let expected = (31.1 + 1626.7 + 39.5 + 1699.3 + 104.4 + 94.0) / 6.0;
        assert!(
            (hotspots[0].frp - expected).abs() < 1e-9,
            "expected {expected}, got {}",
            hotspots[0].frp
        );
        assert!((hotspots[0].frp - 599.166_666_666_666_7).abs() < 1e-9);
    }

    #[test]
    fn chained_groups_accumulate_raw_observation_weights() {
        // anchor 0 -> best 1 -> best 2 -> group anchored at 2 (best == anchor).
        let data = vec![
            make_hotspot(37.0, 29.0, 10.0, Confidence::Low, Sensor::ModisTerra, 0),
            make_hotspot(37.0, 29.0, 20.0, Confidence::Nominal, Sensor::ModisTerra, 6),
            make_hotspot(37.0, 29.0, 30.0, Confidence::High, Sensor::ModisTerra, 12),
            make_hotspot(37.0, 29.0, 40.0, Confidence::High, Sensor::ModisTerra, 18),
        ];

        let mut hotspots = data.clone();
        remove_interscan_duplicates(&mut hotspots);

        assert_eq!(hotspots.len(), 1);
        assert_eq!(frp_values(&hotspots), vec![25.0]);

        let mut expected = data;
        reference_dedup(&mut expected, &DedupConfig::default());
        assert_eq!(expected.len(), 1);
        assert_eq!(hotspots[0].frp.to_bits(), expected[0].frp.to_bits());
    }

    #[test]
    fn group_anchored_at_highest_confidence_keeps_plain_mean() {
        let mut hotspots = vec![
            make_hotspot(37.0, 29.0, 10.0, Confidence::High, Sensor::ModisTerra, 0),
            make_hotspot(37.0, 29.0, 20.0, Confidence::Low, Sensor::ModisTerra, 3),
            make_hotspot(37.0, 29.0, 30.0, Confidence::Nominal, Sensor::ModisTerra, 6),
        ];

        remove_interscan_duplicates(&mut hotspots);

        assert_eq!(hotspots.len(), 1);
        assert_eq!(hotspots[0].confidence, Confidence::High);
        assert_eq!(frp_values(&hotspots), vec![20.0]);
    }

    #[test]
    fn timed_dedup_is_bit_identical_to_reference_across_seeds_and_configs() {
        let configs = [
            ("default", DedupConfig::default()),
            (
                "zero_window",
                DedupConfig {
                    max_time_diff_secs: 0,
                    min_overlap_ratio: 0.30,
                },
            ),
            (
                "negative_ratio_fallback",
                DedupConfig {
                    max_time_diff_secs: 8,
                    min_overlap_ratio: -0.1,
                },
            ),
            (
                "zero_ratio_large_window",
                DedupConfig {
                    max_time_diff_secs: 60,
                    min_overlap_ratio: 0.0,
                },
            ),
        ];

        for seed in [0xA1B2_C3D4_E5F6_0718_u64, 0xDEAD_BEEF_CAFE_BABE_u64] {
            // Keep negative-ratio fallback dataset slightly smaller since fallback is O(N^2) in window.
            let full_data = synthetic_dedup_dataset(seed, 2_400);
            let fallback_data = synthetic_dedup_dataset(seed ^ 0x55AA, 400);

            for (label, cfg) in &configs {
                let data = if cfg.min_overlap_ratio < 0.0 {
                    &fallback_data
                } else {
                    &full_data
                };
                let mut actual = data.clone();
                let mut expected = data.clone();

                let actual_origin = remove_interscan_duplicates_indexed_with(&mut actual, cfg);
                let expected_origin = reference_dedup(&mut expected, cfg);

                assert_eq!(actual_origin, expected_origin, "origin {label} seed={seed}");
                assert_eq!(actual.len(), expected.len(), "len {label} seed={seed}");
                for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
                    assert_hotspot_bits_eq(a, e, &format!("{label} seed={seed} idx={i}"));
                }
            }
        }
    }
}
