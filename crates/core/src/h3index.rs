use std::collections::HashMap;

use h3o::{CellIndex, LatLng, Resolution};

// H3 resolution 8 has ~0.737 km² mean cell area, ~0.461 km edge, and ~0.80 km center spacing,
// matching MODIS's 1 km nadir pixel footprint.
const INDEX_RESOLUTION: Resolution = Resolution::Eight;

// k=3 gives guaranteed coverage of (2k + 1) * apothem ≈ 2.8 km from the query cell center,
// safely enclosing the 2.0 km match radius (k=2 only reaches ~2.0 km right at the boundary).
const SEARCH_K_RING: u32 = 3;

/// Spatial index over point positions, bucketed into H3 cells at resolution 8.
pub struct H3Index {
    buckets: HashMap<CellIndex, Vec<usize>>,
    len: usize,
}

impl H3Index {
    /// Build an index over the given (lat, lon) positions; the usize is the caller's own index.
    pub fn build(points: impl IntoIterator<Item = (f64, f64)>) -> Self {
        let mut buckets: HashMap<CellIndex, Vec<usize>> = HashMap::new();
        let mut len = 0usize;

        for (idx, (lat, lon)) in points.into_iter().enumerate() {
            let Some(cell) = to_cell(lat, lon) else {
                continue;
            };
            buckets.entry(cell).or_default().push(idx);
            len += 1;
        }

        Self { buckets, len }
    }

    /// Candidate point indices within the k-ring around the query position.
    /// Returns indices only; the caller applies the exact distance and time filters.
    pub fn candidates(&self, lat: f64, lon: f64) -> Vec<usize> {
        let Some(cell) = to_cell(lat, lon) else {
            return Vec::new();
        };
        let neighbors: Vec<CellIndex> = cell.grid_disk(SEARCH_K_RING);

        let mut indices = Vec::new();
        for neighbor in neighbors {
            if let Some(bucket) = self.buckets.get(&neighbor) {
                indices.extend_from_slice(bucket);
            }
        }
        indices.sort_unstable();
        indices.dedup();
        indices
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

fn to_cell(lat: f64, lon: f64) -> Option<CellIndex> {
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    LatLng::new(lat, lon)
        .ok()
        .map(|ll| ll.to_cell(INDEX_RESOLUTION))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dedup::haversine_km;

    /// Offset a (lat, lon) position by (north_km, east_km) using a spherical approximation.
    fn offset_km(lat: f64, lon: f64, north_km: f64, east_km: f64) -> (f64, f64) {
        const EARTH_RADIUS_KM: f64 = 6371.0;
        let dlat = (north_km / EARTH_RADIUS_KM).to_degrees();
        let dlon = (east_km / (EARTH_RADIUS_KM * lat.to_radians().cos())).to_degrees();
        (lat + dlat, lon + dlon)
    }

    #[test]
    fn len_matches_valid_point_count() {
        let points = vec![
            (39.0, 32.0),
            (39.0, 32.0),     // same cell as point 0
            (95.0, 200.0),    // invalid, skipped
            (39.5, 32.5),
            (f64::NAN, 10.0), // invalid, skipped
        ];
        let index = H3Index::build(points);
        assert_eq!(index.len(), 3);
        assert!(!index.is_empty());
    }

    #[test]
    fn multiple_points_at_same_location_are_all_returned() {
        let index = H3Index::build([(39.0, 32.0), (39.0, 32.0), (39.0, 32.0)]);
        assert_eq!(index.candidates(39.0, 32.0), vec![0, 1, 2]);
    }

    #[test]
    fn points_within_2km_are_always_candidates_across_latitudes() {
        for &base_lat in &[0.0, 40.0, 70.0] {
            let base_lon = 25.0;
            let mut points = Vec::new();

            // Sample 16 bearings at 1.95 km around the query point.
            for step in 0..16 {
                let angle = (step as f64) * std::f64::consts::TAU / 16.0;
                let (p_lat, p_lon) =
                    offset_km(base_lat, base_lon, 1.95 * angle.cos(), 1.95 * angle.sin());
                let dist = haversine_km(base_lat, base_lon, p_lat, p_lon);
                assert!(dist < 2.0, "expected < 2.0 km, got {dist}");
                points.push((p_lat, p_lon));
            }

            let index = H3Index::build(points);
            let candidates = index.candidates(base_lat, base_lon);
            assert_eq!(
                candidates,
                (0..16).collect::<Vec<_>>(),
                "missed a <2km candidate at lat={base_lat}"
            );
        }
    }

    #[test]
    fn points_1_9km_apart_across_cell_boundary_find_each_other() {
        let a = (39.0, 32.0);
        let cell_a = LatLng::new(a.0, a.1).unwrap().to_cell(INDEX_RESOLUTION);

        // Move 1.9 km east; at res 8 (~0.80 km spacing) this crosses at least one cell boundary.
        let b = offset_km(a.0, a.1, 0.0, 1.9);
        let cell_b = LatLng::new(b.0, b.1).unwrap().to_cell(INDEX_RESOLUTION);
        assert_ne!(cell_a, cell_b);

        let dist = haversine_km(a.0, a.1, b.0, b.1);
        assert!((dist - 1.9).abs() < 0.05, "unexpected distance: {dist}");

        let index = H3Index::build([a, b]);
        assert_eq!(index.candidates(a.0, a.1), vec![0, 1]);
        assert_eq!(index.candidates(b.0, b.1), vec![0, 1]);
    }

    #[test]
    fn distant_point_500km_away_is_excluded() {
        let origin = (39.0, 32.0);
        let far = offset_km(origin.0, origin.1, 500.0, 0.0);
        assert!(haversine_km(origin.0, origin.1, far.0, far.1) > 490.0);

        let index = H3Index::build([far]);
        assert!(index.candidates(origin.0, origin.1).is_empty());
    }

    #[test]
    fn candidates_are_sorted_ascending_and_unique() {
        // Insert points in reverse/mixed order across neighboring cells.
        let pts = vec![
            offset_km(39.0, 32.0, 1.2, 0.0),
            offset_km(39.0, 32.0, 0.0, 0.0),
            offset_km(39.0, 32.0, -1.2, 0.0),
            offset_km(39.0, 32.0, 0.0, 1.2),
            offset_km(39.0, 32.0, 0.0, 0.0),
        ];
        let index = H3Index::build(pts);
        let cands = index.candidates(39.0, 32.0);
        assert_eq!(cands, vec![0, 1, 2, 3, 4]);
        assert!(cands.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn invalid_coordinates_do_not_panic() {
        let index = H3Index::build([(95.0, 200.0), (f64::NAN, 0.0), (39.0, 32.0)]);
        assert_eq!(index.len(), 1);
        assert!(index.candidates(95.0, 200.0).is_empty());
        assert!(index.candidates(f64::INFINITY, 32.0).is_empty());
        // Valid index preserved original caller index 2 for (39.0, 32.0).
        assert_eq!(index.candidates(39.0, 32.0), vec![2]);
    }

    #[test]
    fn empty_index_returns_empty_candidates_and_is_empty() {
        let index = H3Index::build(std::iter::empty());
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert!(index.candidates(39.0, 32.0).is_empty());
    }
}
