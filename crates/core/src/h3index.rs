use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use chrono::{DateTime, TimeDelta, Utc};
use h3o::{CellIndex, LatLng, Resolution};

// H3 resolution 8 has ~0.737 km² mean cell area, ~0.461 km edge, and ~0.80 km center spacing,
// matching MODIS's 1 km nadir pixel footprint.
const INDEX_RESOLUTION: Resolution = Resolution::Eight;

// Ring used by `candidates` when the caller does not scale the search to a match radius.
const SEARCH_K_RING: u32 = 3;

// Resolution-8 hexagon apothem (center to edge midpoint) in km.
const APOTHEM_KM: f64 = 0.40;
// Safety factor on the required coverage so cell-edge geometry never drops a candidate.
const COVERAGE_MARGIN: f64 = 1.25;

/// Smallest k-ring size whose guaranteed coverage encloses `radius_km`.
///
/// A k-ring reaches at least (2k + 1) * apothem from the query cell center, so we need
/// (2k + 1) * APOTHEM_KM >= radius_km * COVERAGE_MARGIN. For 2.0 km this yields k=3 (2.8 km
/// coverage; k=2 only reaches 2.0 km, exactly on the boundary).
pub fn k_ring_for_radius(radius_km: f64) -> u32 {
    let k = ((radius_km * COVERAGE_MARGIN / APOTHEM_KM - 1.0) / 2.0).ceil();
    k.max(1.0) as u32
}

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
        self.candidates_within(lat, lon, SEARCH_K_RING)
    }

    /// Same as [`H3Index::candidates`] with an explicit k-ring size (see [`k_ring_for_radius`]).
    pub fn candidates_within(&self, lat: f64, lon: f64, k: u32) -> Vec<usize> {
        let Some(cell) = to_cell(lat, lon) else {
            return Vec::new();
        };
        let neighbors: Vec<CellIndex> = cell.grid_disk(k);

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

#[derive(Default)]
struct CellHasher(u64);

impl Hasher for CellHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ u64::from(b);
        }
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        let mut x = i.wrapping_add(0x9e37_79b9_7f4a_7c15);
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        self.0 = x ^ (x >> 31);
    }
}

type FastCellMap<V> = HashMap<CellIndex, V, BuildHasherDefault<CellHasher>>;

#[derive(Debug, Clone, Copy)]
struct TimedEntry {
    acquired: DateTime<Utc>,
    pos: usize,
}

/// Time-sliced H3 resolution-8 index over points sorted by `acquired` ascending.
///
/// Uses a flat CSR layout (`entries` + `offsets`) so building over millions of points avoids
/// per-cell `Vec` allocations while keeping each bucket's entries in ascending `pos` order for
/// `partition_point` time slicing. Points whose coordinates fail `to_cell` are kept in `invalid`
/// and merged into every query whose time range matches.
pub struct TimedH3Index {
    buckets: FastCellMap<u32>,
    offsets: Vec<u32>,
    entries: Vec<TimedEntry>,
    invalid: Vec<TimedEntry>,
    point_cells: Vec<Option<CellIndex>>,
    len: usize,
}

impl TimedH3Index {
    /// Build an index from `(lat, lon, acquired)` points already sorted by `acquired` ascending.
    pub fn build(points: impl IntoIterator<Item = (f64, f64, DateTime<Utc>)>) -> Self {
        let iter = points.into_iter();
        let (lower, _) = iter.size_hint();
        let mut buckets: FastCellMap<u32> =
            HashMap::with_capacity_and_hasher(lower / 4, BuildHasherDefault::default());
        let mut counts: Vec<u32> = Vec::with_capacity(lower / 4);
        let mut valid_items: Vec<(u32, TimedEntry)> = Vec::with_capacity(lower);
        let mut invalid = Vec::new();
        let mut point_cells = Vec::with_capacity(lower);
        let mut len = 0usize;
        #[cfg(debug_assertions)]
        let mut prev_acquired: Option<DateTime<Utc>> = None;

        for (pos, (lat, lon, acquired)) in iter.enumerate() {
            #[cfg(debug_assertions)]
            {
                if let Some(prev) = prev_acquired {
                    debug_assert!(
                        prev <= acquired,
                        "TimedH3Index::build requires points sorted by acquired ascending"
                    );
                }
                prev_acquired = Some(acquired);
            }

            let entry = TimedEntry { acquired, pos };
            let cell = to_cell(lat, lon);
            point_cells.push(cell);
            match cell {
                Some(c) => {
                    let bucket_id = *buckets.entry(c).or_insert_with(|| {
                        let id = counts.len() as u32;
                        counts.push(0);
                        id
                    });
                    counts[bucket_id as usize] += 1;
                    valid_items.push((bucket_id, entry));
                }
                None => invalid.push(entry),
            }
            len += 1;
        }

        let mut offsets = counts;
        let mut total = 0u32;
        for slot in &mut offsets {
            let count = *slot;
            *slot = total;
            total += count;
        }
        let mut cursors = offsets.clone();
        offsets.push(total);

        let mut entries = vec![
            TimedEntry {
                acquired: DateTime::<Utc>::MIN_UTC,
                pos: 0,
            };
            total as usize
        ];
        for (bucket_id, entry) in valid_items {
            let cursor = &mut cursors[bucket_id as usize];
            entries[*cursor as usize] = entry;
            *cursor += 1;
        }

        Self {
            buckets,
            offsets,
            entries,
            invalid,
            point_cells,
            len,
        }
    }

    /// Candidate positions within the `k`-ring around `(lat, lon)` whose `acquired` falls within
    /// `[t_min, t_max]`. Returns `None` if `(lat, lon)` fails `to_cell` so callers fall back to
    /// a full scan. Returned positions are strictly ascending and deduplicated.
    pub fn candidates_within(
        &self,
        lat: f64,
        lon: f64,
        k: u32,
        t_min: DateTime<Utc>,
        t_max: DateTime<Utc>,
    ) -> Option<Vec<usize>> {
        let mut out = Vec::new();
        if !self.candidates_within_into(lat, lon, k, t_min, t_max, &mut out) {
            return None;
        }
        Some(out)
    }

    /// Reuses `out` to return strictly ascending, deduplicated candidate positions within the
    /// `k`-ring around `(lat, lon)` and `[t_min, t_max]`. Returns `false` if `(lat, lon)` fails
    /// `to_cell` (leaving `out` empty).
    pub fn candidates_within_into(
        &self,
        lat: f64,
        lon: f64,
        k: u32,
        t_min: DateTime<Utc>,
        t_max: DateTime<Utc>,
        out: &mut Vec<usize>,
    ) -> bool {
        out.clear();
        let Some(cell) = to_cell(lat, lon) else {
            return false;
        };
        self.query_cell_into(cell, k, t_min, t_max, out);
        true
    }

    /// Same as [`TimedH3Index::candidates_within_into`], reusing the H3 cell computed during
    /// `build` for indexed position `pos` instead of re-projecting `(lat, lon)`.
    pub fn candidates_for_pos_within_into(
        &self,
        pos: usize,
        k: u32,
        t_min: DateTime<Utc>,
        t_max: DateTime<Utc>,
        out: &mut Vec<usize>,
    ) -> bool {
        out.clear();
        let Some(cell) = self.point_cells[pos] else {
            return false;
        };
        self.query_cell_into(cell, k, t_min, t_max, out);
        true
    }

    fn query_cell_into(
        &self,
        cell: CellIndex,
        k: u32,
        t_min: DateTime<Utc>,
        t_max: DateTime<Utc>,
        out: &mut Vec<usize>,
    ) {
        if t_min > t_max {
            return;
        }

        let neighbors: Vec<CellIndex> = cell.grid_disk(k);
        let mut slice_count = 0usize;

        for neighbor in neighbors {
            if let Some(&bucket_id) = self.buckets.get(&neighbor) {
                let start = self.offsets[bucket_id as usize] as usize;
                let end = self.offsets[bucket_id as usize + 1] as usize;
                let slice = time_slice(&self.entries[start..end], t_min, t_max);
                if !slice.is_empty() {
                    out.extend(slice.iter().map(|e| e.pos));
                    slice_count += 1;
                }
            }
        }

        if !self.invalid.is_empty() {
            let slice = time_slice(&self.invalid, t_min, t_max);
            if !slice.is_empty() {
                out.extend(slice.iter().map(|e| e.pos));
                slice_count += 1;
            }
        }

        // Single-slice hits already preserve the strictly ascending insertion order.
        if slice_count > 1 {
            out.sort_unstable();
            out.dedup();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

fn time_slice(
    entries: &[TimedEntry],
    t_min: DateTime<Utc>,
    t_max: DateTime<Utc>,
) -> &[TimedEntry] {
    if entries.is_empty() || entries[entries.len() - 1].acquired < t_min || entries[0].acquired > t_max {
        return &[];
    }
    let start = entries.partition_point(|e| e.acquired < t_min);
    let end = start + entries[start..].partition_point(|e| e.acquired <= t_max);
    &entries[start..end]
}

/// Shift `t` by `delta_secs`, clamping to UTC bounds instead of overflowing on extreme configs.
pub(crate) fn shift_time_clamped(t: DateTime<Utc>, delta_secs: i64) -> DateTime<Utc> {
    TimeDelta::try_seconds(delta_secs)
        .and_then(|d| t.checked_add_signed(d))
        .unwrap_or(if delta_secs < 0 {
            DateTime::<Utc>::MIN_UTC
        } else {
            DateTime::<Utc>::MAX_UTC
        })
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
    fn k_ring_for_two_km_is_three() {
        assert_eq!(k_ring_for_radius(2.0), 3);
        assert_eq!(k_ring_for_radius(2.0), SEARCH_K_RING);
    }

    #[test]
    fn k_ring_is_monotonic_and_at_least_one() {
        let mut prev = 0;
        let mut r = 0.5;
        while r <= 10.0 {
            let k = k_ring_for_radius(r);
            assert!(k >= 1 && k >= prev, "k={k} prev={prev} at r={r}");
            prev = k;
            r += 0.25;
        }
        assert_eq!(k_ring_for_radius(0.01), 1);
    }

    #[test]
    fn six_km_apart_found_with_scaled_ring_only() {
        let a = (39.0, 32.0);
        let b = offset_km(a.0, a.1, 6.0, 0.0);
        let index = H3Index::build([b]);
        assert_eq!(index.candidates_within(a.0, a.1, k_ring_for_radius(7.0)), vec![0]);
        assert!(index.candidates_within(a.0, a.1, k_ring_for_radius(2.0)).is_empty());
    }

    #[test]
    fn empty_index_returns_empty_candidates_and_is_empty() {
        let index = H3Index::build(std::iter::empty());
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert!(index.candidates(39.0, 32.0).is_empty());
    }

    #[test]
    fn timed_index_matches_h3_index_intersected_with_time_window() {
        let base = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let mut seed: u64 = 0x1234_5678_9ABC_DEF0;
        let mut next_u64 = || -> u64 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        let centers = [
            (39.0, 32.0),
            (0.0, 179.995),
            (0.0, -179.995),
            (85.0, 15.0),
            (-85.0, -45.0),
        ];

        let mut points = Vec::with_capacity(1_500);
        for i in 0..1_500 {
            let (clat, clon) = centers[(next_u64() as usize) % centers.len()];
            let dlat = ((next_u64() % 10_000) as f64 / 10_000.0 - 0.5) * 0.04;
            let dlon = ((next_u64() % 10_000) as f64 / 10_000.0 - 0.5) * 0.04;
            let lat = (clat + dlat).clamp(-89.9, 89.9);
            let mut lon = clon + dlon;
            if lon > 180.0 {
                lon -= 360.0;
            } else if lon < -180.0 {
                lon += 360.0;
            }
            // Quantize some timestamps to test equal timestamps and sub-second offsets.
            let sec = ((i / 5) * 30 + (next_u64() % 20) as usize) as i64;
            let millis = ((next_u64() % 4) * 250) as i64;
            let acquired = base + TimeDelta::seconds(sec) + TimeDelta::milliseconds(millis);
            points.push((lat, lon, acquired));
        }
        points.sort_by_key(|p| p.2);

        let spatial = H3Index::build(points.iter().map(|&(lat, lon, _)| (lat, lon)));
        let timed = TimedH3Index::build(points.iter().copied());
        assert_eq!(timed.len(), points.len());
        assert!(!timed.is_empty());

        for k in [1, 3, 6] {
            for &(qlat, qlon) in &centers {
                for &(t0_off, t1_off) in &[
                    (-100, -10),
                    (0, 0),
                    (120, 600),
                    (1_500, 3_600),
                    (500, 400), // empty reversed window
                ] {
                    let t_min = base + TimeDelta::seconds(t0_off);
                    let t_max = base + TimeDelta::seconds(t1_off);

                    let actual = timed
                        .candidates_within(qlat, qlon, k, t_min, t_max)
                        .expect("valid query coords");
                    assert!(actual.windows(2).all(|w| w[0] < w[1]));

                    let expected: Vec<usize> = if t_min > t_max {
                        Vec::new()
                    } else {
                        spatial
                            .candidates_within(qlat, qlon, k)
                            .into_iter()
                            .filter(|&pos| points[pos].2 >= t_min && points[pos].2 <= t_max)
                            .collect()
                    };
                    assert_eq!(actual, expected, "k={k} q=({qlat},{qlon})");
                }
            }
        }
    }

    #[test]
    fn timed_index_preserves_invalid_points_and_returns_none_for_invalid_query() {
        let base = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let points = vec![
            (95.0, 20.0, base),
            (39.0, 32.0, base + TimeDelta::seconds(10)),
            (f64::NAN, 10.0, base + TimeDelta::seconds(20)),
            (39.001, 32.001, base + TimeDelta::seconds(30)),
        ];
        let timed = TimedH3Index::build(points);
        assert_eq!(timed.len(), 4);

        // Querying valid point returns valid neighbors in window PLUS invalid points in window.
        assert_eq!(
            timed.candidates_within(
                39.0,
                32.0,
                3,
                base,
                base + TimeDelta::seconds(25),
            ),
            Some(vec![0, 1, 2])
        );
        // Invalid point outside time window is excluded.
        assert_eq!(
            timed.candidates_within(
                39.0,
                32.0,
                3,
                base + TimeDelta::seconds(5),
                base + TimeDelta::seconds(15),
            ),
            Some(vec![1])
        );
        // Invalid query coordinate returns None so caller falls back to full scan.
        assert_eq!(
            timed.candidates_within(95.0, 20.0, 3, base, base + TimeDelta::seconds(30)),
            None
        );
        assert_eq!(
            timed.candidates_within(f64::NAN, 0.0, 3, base, base + TimeDelta::seconds(30)),
            None
        );

        let empty = TimedH3Index::build(std::iter::empty());
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert_eq!(empty.candidates_within(39.0, 32.0, 3, base, base), Some(vec![]));
    }
}
