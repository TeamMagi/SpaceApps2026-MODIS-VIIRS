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
        let mut group_frp = vec![hotspots[i].frp];
        let mut best = i;

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

            let overlap = pixel_overlap(&hotspots[i], &hotspots[j]);
            if overlap > MIN_OVERLAP_RATIO {
                group_frp.push(hotspots[j].frp);
                if hotspots[j].confidence > hotspots[best].confidence {
                    best = j;
                }
                if j != best {
                    to_remove[j] = true;
                }
            }
        }

        if group_frp.len() > 1 {
            let mean_frp: f64 = group_frp.iter().sum::<f64>() / group_frp.len() as f64;
            hotspots[best].frp = mean_frp;
            for k in [i, best] {
                if k != best {
                    to_remove[k] = true;
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
