//! Fire-event clustering and detection-gap transparency.
//!
//! Clustering follows the eg-firetrack reference project (`events.py`): DBSCAN on harmonized
//! detections with `eps = 2 km`, `min_samples = 3`, plus an event for isolated detections that
//! persist on 2 consecutive days; the rest are "watch" points. Here DBSCAN is run in
//! space-time (ST-DBSCAN) with a one-day temporal neighbourhood so that the same place in
//! different weeks or years never merges.
//!
//! Detection gaps follow eg-firetrack's `no_detection_days` metadata and the Terra MOD14 data
//! faults documented by Remy & Kaiser (2014, ACP, "Daily global fire radiative power from
//! MODIS", Section 4).

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::Serialize;

use crate::dedup::haversine_km;
use crate::models::{BBox, HarmonizedHotspot};
use crate::uncertainty::SensorFamily;

const EARTH_RADIUS_KM: f64 = 6371.0;
// Slightly below the true km/degree of latitude (110.57-111.69) so grid cells are never
// smaller than eps.
const KM_PER_LAT_DEGREE: f64 = 110.5;
// Keeps cell widths finite next to the poles.
const MAX_GRID_LAT_DEG: f64 = 89.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventParams {
    pub eps_km: f64,
    pub min_samples: usize,
    /// Largest difference in UTC calendar days between two neighbouring detections.
    pub max_day_gap: i64,
}

impl Default for EventParams {
    fn default() -> Self {
        Self {
            eps_km: 2.0,
            min_samples: 3,
            max_day_gap: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// DBSCAN cluster: grown through core points.
    Dense,
    /// Sparse detections that persist on at least 2 distinct UTC days (eg-firetrack rule).
    ConsecutiveDays,
}

#[derive(Debug, Clone, Serialize)]
pub struct FireEvent {
    pub id: usize,
    pub kind: EventKind,
    pub detections: usize,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub days: usize,
    pub centroid_lat: f64,
    pub centroid_lon: f64,
    pub bbox: BBox,
    /// Sum of `frp_corrected`, MW.
    pub total_frp: f64,
}

#[derive(Debug, Clone)]
pub struct EventClustering {
    /// Event id per input detection; `None` for watch points.
    pub labels: Vec<Option<usize>>,
    pub events: Vec<FireEvent>,
    pub watch_points: usize,
}

struct Grid {
    dlat: f64,
    cells: HashMap<(i64, i64, i64), Vec<usize>>,
}

impl Grid {
    fn band(&self, lat: f64) -> i64 {
        (lat / self.dlat).floor() as i64
    }

    // Cell width in degrees of longitude for a latitude band: the band's widest-converging
    // edge decides, so a cell is never narrower than eps anywhere in the band.
    fn lon_width(&self, band: i64) -> f64 {
        let edge = (band as f64 * self.dlat)
            .abs()
            .max(((band + 1) as f64 * self.dlat).abs())
            .min(MAX_GRID_LAT_DEG);
        self.dlat / edge.to_radians().cos()
    }

    fn build(points: &[Point], eps_km: f64) -> Self {
        let mut grid = Self {
            dlat: eps_km / KM_PER_LAT_DEGREE,
            cells: HashMap::new(),
        };
        for (i, p) in points.iter().enumerate() {
            let band = grid.band(p.lat);
            let cell = (p.lon / grid.lon_width(band)).floor() as i64;
            grid.cells.entry((p.day, band, cell)).or_default().push(i);
        }
        grid
    }
}

struct Point {
    lat: f64,
    lon: f64,
    day: i64,
}

struct Neighbourhood<'a> {
    points: &'a [Point],
    grid: Grid,
    eps_km: f64,
    max_day_gap: i64,
}

impl Neighbourhood<'_> {
    /// Indices within `eps_km` and `max_day_gap` of point `i`, itself included, in grid order.
    // The antimeridian is not wrapped: detections on opposite sides of 180 degrees land in
    // distant cells and are not found, although distances themselves are exact.
    fn of(&self, i: usize) -> Vec<usize> {
        let p = &self.points[i];
        let ang = (self.eps_km / EARTH_RADIUS_KM).sin();
        let cos_lat = p.lat.to_radians().cos();
        let dlon = if cos_lat <= ang {
            180.0
        } else {
            (ang / cos_lat).asin().to_degrees() * 1.0001
        };
        let band0 = self.grid.band(p.lat);
        let mut out = Vec::new();
        for day in (p.day - self.max_day_gap)..=(p.day + self.max_day_gap) {
            for band in (band0 - 1)..=(band0 + 1) {
                let width = self.grid.lon_width(band);
                let first = ((p.lon - dlon) / width).floor() as i64;
                let last = ((p.lon + dlon) / width).floor() as i64;
                for cell in first..=last {
                    let Some(members) = self.grid.cells.get(&(day, band, cell)) else {
                        continue;
                    };
                    out.extend(members.iter().copied().filter(|&j| {
                        let q = &self.points[j];
                        haversine_km(p.lat, p.lon, q.lat, q.lon) <= self.eps_km
                    }));
                }
            }
        }
        out
    }
}

/// ST-DBSCAN plus eg-firetrack's consecutive-day rule. Neighbour search uses a spatial hash, so
/// the cost is roughly linear in the number of detections. Event ids are assigned in order of
/// each event's earliest detection (ties: lowest input index), so they are stable for a given
/// input.
pub fn cluster_events(hotspots: &[HarmonizedHotspot], params: &EventParams) -> EventClustering {
    let n = hotspots.len();
    let points: Vec<Point> = hotspots
        .iter()
        .map(|h| Point {
            lat: h.lat,
            lon: h.lon,
            day: i64::from(h.acquired.date_naive().num_days_from_ce()),
        })
        .collect();
    let eps_km = params.eps_km.max(1e-3);
    let min_samples = params.min_samples.max(1);
    let nb = Neighbourhood {
        points: &points,
        grid: Grid::build(&points, eps_km),
        eps_km,
        max_day_gap: params.max_day_gap.max(0),
    };

    let mut cluster: Vec<Option<usize>> = vec![None; n];
    let mut kinds: Vec<EventKind> = Vec::new();
    let mut visited = vec![false; n];
    for i in 0..n {
        if visited[i] {
            continue;
        }
        visited[i] = true;
        let seeds = nb.of(i);
        if seeds.len() < min_samples {
            continue;
        }
        let c = kinds.len();
        kinds.push(EventKind::Dense);
        cluster[i] = Some(c);
        let mut queue = seeds;
        let mut head = 0;
        while head < queue.len() {
            let j = queue[head];
            head += 1;
            if cluster[j].is_none() {
                cluster[j] = Some(c);
            }
            if visited[j] {
                continue;
            }
            visited[j] = true;
            let around = nb.of(j);
            if around.len() >= min_samples {
                queue.extend(around);
            }
        }
    }

    let mut component: Vec<Option<usize>> = vec![None; n];
    let mut seen = vec![false; n];
    for start in 0..n {
        if cluster[start].is_some() || seen[start] {
            continue;
        }
        let mut members = vec![start];
        seen[start] = true;
        let mut head = 0;
        while head < members.len() {
            let j = members[head];
            head += 1;
            for k in nb.of(j) {
                if cluster[k].is_none() && !seen[k] {
                    seen[k] = true;
                    members.push(k);
                }
            }
        }
        let days: BTreeSet<i64> = members.iter().map(|&m| points[m].day).collect();
        if days.len() >= 2 {
            let c = kinds.len();
            kinds.push(EventKind::ConsecutiveDays);
            for m in members {
                component[m] = Some(c);
            }
        }
    }

    let provisional: Vec<Option<usize>> = (0..n).map(|i| cluster[i].or(component[i])).collect();
    assemble(hotspots, &provisional, &kinds)
}

fn assemble(
    hotspots: &[HarmonizedHotspot],
    provisional: &[Option<usize>],
    kinds: &[EventKind],
) -> EventClustering {
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); kinds.len()];
    for (i, label) in provisional.iter().enumerate() {
        if let Some(c) = label {
            members[*c].push(i);
        }
    }
    let earliest = |m: &Vec<usize>| {
        m.iter()
            .map(|&i| (hotspots[i].acquired, i))
            .min()
            .expect("clusters are never empty")
    };
    let mut order: Vec<usize> = (0..kinds.len()).collect();
    order.sort_by_key(|&c| earliest(&members[c]));

    let mut new_id = vec![0usize; kinds.len()];
    let mut events = Vec::with_capacity(kinds.len());
    for (id, &c) in order.iter().enumerate() {
        new_id[c] = id;
        events.push(build_event(id, kinds[c], &members[c], hotspots));
    }
    let labels: Vec<Option<usize>> = provisional.iter().map(|l| l.map(|c| new_id[c])).collect();
    let watch_points = labels.iter().filter(|l| l.is_none()).count();
    EventClustering {
        labels,
        events,
        watch_points,
    }
}

fn build_event(
    id: usize,
    kind: EventKind,
    members: &[usize],
    hotspots: &[HarmonizedHotspot],
) -> FireEvent {
    let first = &hotspots[members[0]];
    let mut bbox = BBox {
        west: first.lon,
        south: first.lat,
        east: first.lon,
        north: first.lat,
    };
    let (mut start, mut end) = (first.acquired, first.acquired);
    let (mut sum_lat, mut sum_lon, mut total_frp) = (0.0, 0.0, 0.0);
    let mut days = BTreeSet::new();
    for &i in members {
        let h = &hotspots[i];
        bbox.west = bbox.west.min(h.lon);
        bbox.east = bbox.east.max(h.lon);
        bbox.south = bbox.south.min(h.lat);
        bbox.north = bbox.north.max(h.lat);
        start = start.min(h.acquired);
        end = end.max(h.acquired);
        sum_lat += h.lat;
        sum_lon += h.lon;
        total_frp += h.frp_corrected;
        days.insert(h.acquired.date_naive());
    }
    let count = members.len() as f64;
    FireEvent {
        id,
        kind,
        detections: members.len(),
        start,
        end,
        days: days.len(),
        centroid_lat: sum_lat / count,
        centroid_lon: sum_lon / count,
        bbox,
        total_frp,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoDetectionReason {
    /// Date lies in a documented data fault (see [`KNOWN_OUTAGES`]).
    KnownOutage,
    /// No detections of this family in this region-day. By itself this cannot distinguish a
    /// real zero-fire day from a data gap; `other_family_detected` is only a hint toward a gap.
    NoDetections,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoDetectionDay {
    pub date: NaiveDate,
    pub family: SensorFamily,
    pub reason: NoDetectionReason,
    pub other_family_detected: bool,
}

type Ymd = (i32, u32, u32);

/// Terra MOD14 days without fire detections caused by a data fault, not by absence of fire:
/// Remy & Kaiser (2014, ACP), Section 4. All precede Aqua (first FIRMS detections 2002-07, see
/// `effort::FIRST_TERRA_AQUA_MONTH`), so Terra was the only MODIS platform and the whole MODIS
/// family was affected. Inclusive (start, end).
pub const KNOWN_OUTAGES: [(Ymd, Ymd); 3] = [
    ((2000, 8, 6), (2000, 8, 17)),
    ((2001, 6, 16), (2001, 7, 2)),
    ((2002, 3, 21), (2002, 3, 29)),
];

fn in_known_outage(date: NaiveDate) -> bool {
    let ymd = |(y, m, d): (i32, u32, u32)| NaiveDate::from_ymd_opt(y, m, d).expect("valid date");
    KNOWN_OUTAGES
        .iter()
        .any(|&(start, end)| ymd(start) <= date && date <= ymd(end))
}

/// Days inside `period` (inclusive) and a family's archive coverage on which that family has no
/// detection in the region. Days outside a family's coverage are not listed; a family without
/// coverage (`None`) contributes nothing. Sorted by (date, MODIS before VIIRS).
pub fn no_detection_days(
    modis_days: &BTreeSet<NaiveDate>,
    viirs_days: &BTreeSet<NaiveDate>,
    period: (NaiveDate, NaiveDate),
    modis_coverage: Option<(NaiveDate, NaiveDate)>,
    viirs_coverage: Option<(NaiveDate, NaiveDate)>,
) -> Vec<NoDetectionDay> {
    let clip = |coverage: Option<(NaiveDate, NaiveDate)>| {
        coverage.map(|(first, last)| (first.max(period.0), last.min(period.1)))
    };
    let modis_range = clip(modis_coverage);
    let viirs_range = clip(viirs_coverage);
    let within = |range: Option<(NaiveDate, NaiveDate)>, d: NaiveDate| {
        range.is_some_and(|(first, last)| first <= d && d <= last)
    };

    let mut out = Vec::new();
    for date in period.0.iter_days().take_while(|d| *d <= period.1) {
        let (has_modis, has_viirs) = (modis_days.contains(&date), viirs_days.contains(&date));
        if within(modis_range, date) && !has_modis {
            out.push(NoDetectionDay {
                date,
                family: SensorFamily::Modis,
                reason: if in_known_outage(date) {
                    NoDetectionReason::KnownOutage
                } else {
                    NoDetectionReason::NoDetections
                },
                other_family_detected: has_viirs,
            });
        }
        if within(viirs_range, date) && !has_viirs {
            out.push(NoDetectionDay {
                date,
                family: SensorFamily::Viirs,
                reason: NoDetectionReason::NoDetections,
                other_family_detected: has_modis,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, DayNight, HotspotSource, Sensor};
    use chrono::{TimeZone, TimeDelta};

    fn hs(lat: f64, lon: f64, year: i32, month: u32, day: u32) -> HarmonizedHotspot {
        HarmonizedHotspot {
            lat,
            lon,
            frp: 10.0,
            frp_corrected: 10.0,
            frp_lower: 9.0,
            frp_upper: 11.0,
            vza_factor: None,
            n_obs: 1,
            platforms: vec![Sensor::ViirsSNPP],
            confidence: Confidence::Nominal,
            source: HotspotSource::ViirsOnly,
            acquired: Utc.with_ymd_and_hms(year, month, day, 10, 0, 0).unwrap(),
            daynight: DayNight::Day,
        }
    }

    // 1 degree of latitude ~ 111.19 km on the haversine sphere.
    const KM: f64 = 1.0 / 111.195;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn three_close_points_same_day_form_one_dense_event() {
        let pts = [
            hs(40.0, 30.0, 2021, 7, 1),
            hs(40.0 + 0.5 * KM, 30.0, 2021, 7, 1),
            hs(40.0 + 1.0 * KM, 30.0, 2021, 7, 1),
        ];
        let c = cluster_events(&pts, &EventParams::default());
        assert_eq!(c.events.len(), 1);
        assert_eq!(c.events[0].kind, EventKind::Dense);
        assert_eq!(c.events[0].detections, 3);
        assert_eq!(c.events[0].days, 1);
        assert!((c.events[0].total_frp - 30.0).abs() < 1e-9);
        assert_eq!(c.watch_points, 0);
        assert_eq!(c.labels, vec![Some(0); 3]);
    }

    #[test]
    fn two_points_same_day_are_watch_points() {
        let pts = [hs(40.0, 30.0, 2021, 7, 1), hs(40.0 + KM, 30.0, 2021, 7, 1)];
        let c = cluster_events(&pts, &EventParams::default());
        assert!(c.events.is_empty());
        assert_eq!(c.watch_points, 2);
        assert_eq!(c.labels, vec![None, None]);
    }

    #[test]
    fn same_spot_on_consecutive_days_is_consecutive_days_event() {
        let pts = [hs(40.0, 30.0, 2021, 7, 1), hs(40.0, 30.0, 2021, 7, 2)];
        let c = cluster_events(&pts, &EventParams::default());
        assert_eq!(c.events.len(), 1);
        assert_eq!(c.events[0].kind, EventKind::ConsecutiveDays);
        assert_eq!(c.events[0].days, 2);
        assert_eq!(c.watch_points, 0);
    }

    #[test]
    fn two_day_gap_does_not_merge() {
        let pts = [hs(40.0, 30.0, 2021, 7, 1), hs(40.0, 30.0, 2021, 7, 3)];
        let c = cluster_events(&pts, &EventParams::default());
        assert!(c.events.is_empty());
        assert_eq!(c.watch_points, 2);
    }

    #[test]
    fn chain_grows_through_core_points() {
        let pts = [
            hs(40.0, 30.0, 2021, 7, 1),
            hs(40.0 + 1.5 * KM, 30.0, 2021, 7, 1),
            hs(40.0 + 3.0 * KM, 30.0, 2021, 7, 1),
        ];
        let c = cluster_events(&pts, &EventParams::default());
        assert_eq!(c.events.len(), 1);
        assert_eq!(c.events[0].detections, 3);
        assert_eq!(c.watch_points, 0);
    }

    #[test]
    fn chain_of_four_links_end_points_through_core_points() {
        let pts: Vec<_> = (0..4)
            .map(|k| hs(40.0 + 1.5 * KM * k as f64, 30.0, 2021, 7, 1))
            .collect();
        let c = cluster_events(&pts, &EventParams::default());
        assert_eq!(c.events.len(), 1);
        assert_eq!(c.events[0].detections, 4);
    }

    #[test]
    fn eps_boundary_is_inclusive_and_exact() {
        let near = [
            hs(40.0, 30.0, 2021, 7, 1),
            hs(40.0 + 1.99 * KM, 30.0, 2021, 7, 1),
        ];
        let far = [
            hs(40.0, 30.0, 2021, 7, 1),
            hs(40.0 + 2.01 * KM, 30.0, 2021, 7, 1),
        ];
        let params = EventParams {
            min_samples: 2,
            ..EventParams::default()
        };
        assert_eq!(cluster_events(&near, &params).events.len(), 1);
        assert_eq!(cluster_events(&far, &params).events.len(), 0);
    }

    #[test]
    fn same_location_different_years_stays_separate() {
        let mut pts = Vec::new();
        for year in [2020, 2021] {
            for k in 0..3 {
                pts.push(hs(40.0 + 0.3 * KM * k as f64, 30.0, year, 7, 1));
            }
        }
        let c = cluster_events(&pts, &EventParams::default());
        assert_eq!(c.events.len(), 2);
        assert!(c.events.iter().all(|e| e.detections == 3));
    }

    #[test]
    fn event_ids_follow_earliest_detection() {
        let pts = [
            hs(10.0, 10.0, 2021, 8, 1),
            hs(10.0, 10.0, 2021, 8, 1),
            hs(10.0, 10.0, 2021, 8, 1),
            hs(40.0, 30.0, 2021, 7, 1),
            hs(40.0, 30.0, 2021, 7, 1),
            hs(40.0, 30.0, 2021, 7, 1),
        ];
        let c = cluster_events(&pts, &EventParams::default());
        assert_eq!(c.events.len(), 2);
        assert!(c.events[0].start < c.events[1].start);
        assert_eq!(c.labels[3], Some(0));
        assert_eq!(c.labels[0], Some(1));
        assert_eq!(c.events[0].id, 0);
    }

    #[test]
    fn output_is_stable_across_runs() {
        let pts: Vec<_> = (0..40)
            .map(|k| hs(40.0 + 0.7 * KM * (k % 9) as f64, 30.0 + 0.001 * k as f64, 2021, 7, 1 + (k % 3) as u32))
            .collect();
        let a = cluster_events(&pts, &EventParams::default());
        let b = cluster_events(&pts, &EventParams::default());
        assert_eq!(a.labels, b.labels);
        assert_eq!(a.events.len(), b.events.len());
        assert_eq!(a.watch_points, b.watch_points);
    }

    #[test]
    fn east_west_neighbours_found_at_high_latitude() {
        let dlon = 1.9 / (111.195 * 65.0_f64.to_radians().cos());
        let pts = [hs(65.0, 10.0, 2021, 7, 1), hs(65.0, 10.0 + dlon, 2021, 7, 1)];
        let params = EventParams {
            min_samples: 2,
            ..EventParams::default()
        };
        assert_eq!(cluster_events(&pts, &params).events.len(), 1);
        let apart = [hs(65.0, 10.0, 2021, 7, 1), hs(65.0, 10.0 + dlon * 1.2, 2021, 7, 1)];
        assert_eq!(cluster_events(&apart, &params).events.len(), 0);
    }

    #[test]
    fn grid_matches_brute_force_on_scattered_points() {
        let mut pts = Vec::new();
        let mut s = 12345u64;
        let mut rnd = || {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (s >> 33) as f64 / (1u64 << 31) as f64
        };
        for _ in 0..300 {
            pts.push(hs(60.0 + rnd() * 0.3, 20.0 + rnd() * 0.6, 2021, 7, 1 + (rnd() * 2.0) as u32));
        }
        let points: Vec<Point> = pts
            .iter()
            .map(|h| Point {
                lat: h.lat,
                lon: h.lon,
                day: i64::from(h.acquired.date_naive().num_days_from_ce()),
            })
            .collect();
        let nb = Neighbourhood {
            points: &points,
            grid: Grid::build(&points, 2.0),
            eps_km: 2.0,
            max_day_gap: 1,
        };
        for i in 0..points.len() {
            let mut fast = nb.of(i);
            fast.sort_unstable();
            let slow: Vec<usize> = (0..points.len())
                .filter(|&j| {
                    (points[i].day - points[j].day).abs() <= 1
                        && haversine_km(points[i].lat, points[i].lon, points[j].lat, points[j].lon) <= 2.0
                })
                .collect();
            assert_eq!(fast, slow, "point {i}");
        }
    }

    #[test]
    fn event_aggregates_bbox_centroid_and_time() {
        let mut a = hs(40.0, 30.0, 2021, 7, 1);
        let mut b = hs(40.0 + KM, 30.0, 2021, 7, 2);
        let c3 = hs(40.0 + 0.5 * KM, 30.0, 2021, 7, 2);
        a.frp_corrected = 5.0;
        b.frp_corrected = 7.0;
        let c = cluster_events(&[a, b, c3], &EventParams::default());
        let e = &c.events[0];
        assert_eq!(e.days, 2);
        assert_eq!(e.end - e.start, TimeDelta::days(1));
        assert!((e.bbox.south - 40.0).abs() < 1e-12);
        assert!((e.bbox.north - (40.0 + KM)).abs() < 1e-12);
        assert!((e.centroid_lat - (40.0 + 0.5 * KM)).abs() < 1e-9);
        assert!((e.total_frp - 22.0).abs() < 1e-9);
    }

    #[test]
    fn empty_input_gives_empty_clustering() {
        let c = cluster_events(&[], &EventParams::default());
        assert!(c.events.is_empty() && c.labels.is_empty());
        assert_eq!(c.watch_points, 0);
    }

    fn days(list: &[NaiveDate]) -> BTreeSet<NaiveDate> {
        list.iter().copied().collect()
    }

    #[test]
    fn known_outage_labelled_for_modis_only() {
        let period = (date(2000, 8, 5), date(2000, 8, 7));
        let cover = Some((date(2000, 1, 1), date(2000, 12, 31)));
        let out = no_detection_days(&days(&[]), &days(&[]), period, cover, cover);
        let modis: Vec<_> = out.iter().filter(|d| d.family == SensorFamily::Modis).collect();
        assert_eq!(modis[0].reason, NoDetectionReason::NoDetections);
        assert_eq!(modis[1].reason, NoDetectionReason::KnownOutage);
        assert_eq!(modis[2].reason, NoDetectionReason::KnownOutage);
        assert!(out
            .iter()
            .filter(|d| d.family == SensorFamily::Viirs)
            .all(|d| d.reason == NoDetectionReason::NoDetections));
    }

    #[test]
    fn all_three_remy_kaiser_windows_are_covered() {
        for d in [date(2000, 8, 6), date(2000, 8, 17), date(2001, 6, 16), date(2001, 7, 2), date(2002, 3, 21), date(2002, 3, 29)] {
            assert!(in_known_outage(d), "{d}");
        }
        for d in [date(2000, 8, 5), date(2000, 8, 18), date(2001, 7, 3), date(2002, 3, 30)] {
            assert!(!in_known_outage(d), "{d}");
        }
    }

    #[test]
    fn other_family_hint_and_detected_days_skipped() {
        let period = (date(2021, 7, 1), date(2021, 7, 3));
        let cover = Some(period);
        let out = no_detection_days(
            &days(&[date(2021, 7, 1)]),
            &days(&[date(2021, 7, 1), date(2021, 7, 2)]),
            period,
            cover,
            cover,
        );
        assert_eq!(
            out,
            vec![
                NoDetectionDay {
                    date: date(2021, 7, 2),
                    family: SensorFamily::Modis,
                    reason: NoDetectionReason::NoDetections,
                    other_family_detected: true,
                },
                NoDetectionDay {
                    date: date(2021, 7, 3),
                    family: SensorFamily::Modis,
                    reason: NoDetectionReason::NoDetections,
                    other_family_detected: false,
                },
                NoDetectionDay {
                    date: date(2021, 7, 3),
                    family: SensorFamily::Viirs,
                    reason: NoDetectionReason::NoDetections,
                    other_family_detected: false,
                },
            ]
        );
    }

    #[test]
    fn days_outside_coverage_and_period_are_omitted() {
        let period = (date(2021, 7, 1), date(2021, 7, 10));
        let out = no_detection_days(
            &days(&[]),
            &days(&[]),
            period,
            Some((date(2021, 7, 4), date(2021, 7, 5))),
            None,
        );
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|d| d.family == SensorFamily::Modis));
        assert_eq!(out[0].date, date(2021, 7, 4));
        let clipped = no_detection_days(
            &days(&[]),
            &days(&[]),
            (date(2021, 7, 1), date(2021, 7, 2)),
            Some((date(2020, 1, 1), date(2022, 1, 1))),
            None,
        );
        assert_eq!(clipped.len(), 2);
    }
}
