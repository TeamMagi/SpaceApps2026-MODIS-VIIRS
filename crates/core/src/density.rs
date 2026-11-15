use std::collections::HashMap;

use chrono::{DateTime, Datelike, Utc};

use crate::models::{HarmonizedHotspot, Hotspot};

/// Cell sizes the density grid accepts, in degrees.
pub const ALLOWED_CELL_DEGREES: [f64; 4] = [0.1, 0.25, 0.5, 1.0];

/// Row/column of the equal-angle global grid (row 0 starts at lat -90, col 0 at lon -180).
/// The poles/antimeridian edge (lat 90, lon 180) is clamped into the last row/column.
pub fn cell_index(lat: f64, lon: f64, cell_deg: f64) -> (u32, u32) {
    let rows = (180.0 / cell_deg).round() as u32;
    let cols = (360.0 / cell_deg).round() as u32;
    let row = (((lat + 90.0) / cell_deg).floor().max(0.0) as u32).min(rows - 1);
    let col = (((lon + 180.0) / cell_deg).floor().max(0.0) as u32).min(cols - 1);
    (row, col)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CellStats {
    pub raw_count: u64,
    pub raw_frp: f64,
    pub harm_count: u64,
    pub harm_frp: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DensityCell {
    pub row: u32,
    pub col: u32,
    pub stats: CellStats,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonthDensity {
    pub month: u32,
    pub raw_total: u64,
    pub harmonized_total: u64,
    /// Non-empty cells only, sorted by `(row, col)`.
    pub cells: Vec<DensityCell>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DensityMax {
    pub raw_count: u64,
    pub raw_frp: f64,
    pub harm_count: u64,
    pub harm_frp: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DensityGrid {
    /// Always 12 entries, January first.
    pub months: Vec<MonthDensity>,
    pub max: DensityMax,
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn month_slot(acquired: DateTime<Utc>) -> usize {
    acquired.month0() as usize
}

/// Buckets raw and harmonized detections into per-month grid cells by the UTC month of
/// `acquired`. FRP sums are rounded to 0.1 MW. A harmonized record counts in its own month,
/// even when it merges raw rows from adjacent months.
pub fn aggregate_density<'a>(
    raw: impl IntoIterator<Item = &'a Hotspot>,
    harmonized: &[HarmonizedHotspot],
    cell_deg: f64,
) -> DensityGrid {
    let mut maps: Vec<HashMap<(u32, u32), CellStats>> = vec![HashMap::new(); 12];
    let mut raw_totals = [0u64; 12];
    let mut harm_totals = [0u64; 12];

    for h in raw {
        let m = month_slot(h.acquired);
        let cell = maps[m].entry(cell_index(h.lat, h.lon, cell_deg)).or_default();
        cell.raw_count += 1;
        cell.raw_frp += h.frp;
        raw_totals[m] += 1;
    }
    for h in harmonized {
        let m = month_slot(h.acquired);
        let cell = maps[m].entry(cell_index(h.lat, h.lon, cell_deg)).or_default();
        cell.harm_count += 1;
        cell.harm_frp += h.frp_corrected;
        harm_totals[m] += 1;
    }

    let mut max = DensityMax::default();
    let months = maps
        .into_iter()
        .enumerate()
        .map(|(i, map)| {
            let mut cells: Vec<DensityCell> = map
                .into_iter()
                .map(|((row, col), mut stats)| {
                    stats.raw_frp = round1(stats.raw_frp);
                    stats.harm_frp = round1(stats.harm_frp);
                    max.raw_count = max.raw_count.max(stats.raw_count);
                    max.raw_frp = max.raw_frp.max(stats.raw_frp);
                    max.harm_count = max.harm_count.max(stats.harm_count);
                    max.harm_frp = max.harm_frp.max(stats.harm_frp);
                    DensityCell { row, col, stats }
                })
                .collect();
            cells.sort_unstable_by_key(|c| (c.row, c.col));
            MonthDensity {
                month: i as u32 + 1,
                raw_total: raw_totals[i],
                harmonized_total: harm_totals[i],
                cells,
            }
        })
        .collect();

    DensityGrid { months, max }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, DayNight, FireType, HotspotSource, Sensor};
    use chrono::TimeZone;

    fn raw(lat: f64, lon: f64, frp: f64, y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> Hotspot {
        Hotspot {
            lat,
            lon,
            brightness: 330.0,
            frp,
            confidence: Confidence::Nominal,
            sensor: Sensor::ViirsSNPP,
            acquired: Utc.with_ymd_and_hms(y, mo, d, h, mi, s).unwrap(),
            scan: 0.5,
            track: 0.5,
            satellite: "N".into(),
            daynight: DayNight::Day,
            vza: None,
            fire_type: FireType::VegetationFire,
        }
    }

    fn harm(lat: f64, lon: f64, frp: f64, mo: u32, d: u32) -> HarmonizedHotspot {
        HarmonizedHotspot {
            lat,
            lon,
            frp,
            frp_corrected: frp,
            frp_lower: frp,
            frp_upper: frp,
            vza_factor: None,
            n_obs: 1,
            platforms: vec![Sensor::ViirsSNPP],
            confidence: Confidence::Nominal,
            source: HotspotSource::ViirsOnly,
            acquired: Utc.with_ymd_and_hms(2021, mo, d, 12, 0, 0).unwrap(),
            daynight: DayNight::Day,
        }
    }

    #[test]
    fn cell_index_edges_and_clamping() {
        assert_eq!(cell_index(-90.0, -180.0, 0.25), (0, 0));
        assert_eq!(cell_index(90.0, 180.0, 0.25), (719, 1439));
        assert_eq!(cell_index(90.0, 180.0, 1.0), (179, 359));
        assert_eq!(cell_index(90.0, 180.0, 0.1), (1799, 3599));
        assert_eq!(cell_index(0.0, 0.0, 1.0), (90, 180));
        assert_eq!(cell_index(-0.0001, -0.0001, 1.0), (89, 179));
        assert_eq!(cell_index(-33.9, -70.6, 1.0), (56, 109));
    }

    #[test]
    fn cell_index_exact_boundaries_belong_to_upper_cell() {
        assert_eq!(cell_index(-89.75, -179.5, 0.25), (1, 2));
        assert_eq!(cell_index(-89.5, -179.0, 0.5), (1, 2));
        assert_eq!(cell_index(10.0, 20.0, 1.0), (100, 200));
        assert_eq!(cell_index(9.999, 19.999, 1.0), (99, 199));
    }

    #[test]
    fn aggregation_sums_counts_and_max() {
        let r = vec![
            raw(10.1, 20.1, 5.04, 2021, 3, 1, 0, 0, 0),
            raw(10.2, 20.2, 3.0, 2021, 3, 2, 0, 0, 0),
            raw(40.5, -100.5, 1.0, 2021, 3, 2, 0, 0, 0),
            raw(10.1, 20.1, 7.0, 2021, 8, 2, 0, 0, 0),
        ];
        let h = vec![harm(10.1, 20.1, 6.0, 3, 1), harm(55.0, 5.0, 2.0, 11, 1)];
        let g = aggregate_density(&r, &h, 1.0);
        assert_eq!(g.months.len(), 12);
        let mar = &g.months[2];
        assert_eq!((mar.raw_total, mar.harmonized_total), (3, 1));
        assert_eq!(mar.cells.len(), 2);
        assert!(mar.cells.windows(2).all(|w| (w[0].row, w[0].col) < (w[1].row, w[1].col)));
        let c = mar.cells.iter().find(|c| (c.row, c.col) == (100, 200)).unwrap();
        assert_eq!(c.stats, CellStats { raw_count: 2, raw_frp: 8.0, harm_count: 1, harm_frp: 6.0 });
        let nov = &g.months[10];
        assert_eq!(nov.cells[0].stats.raw_count, 0);
        assert_eq!(nov.cells[0].stats.harm_count, 1);
        assert_eq!(g.max, DensityMax { raw_count: 2, raw_frp: 8.0, harm_count: 1, harm_frp: 6.0 });
        let raw_sum: u64 = g.months.iter().map(|m| m.raw_total).sum();
        assert_eq!(raw_sum, 4);
    }

    #[test]
    fn empty_input_still_has_twelve_months() {
        let g = aggregate_density(std::iter::empty(), &[], 0.5);
        assert_eq!(g.months.len(), 12);
        assert!(g.months.iter().enumerate().all(|(i, m)| m.month == i as u32 + 1 && m.cells.is_empty()));
        assert_eq!(g.max, DensityMax::default());
    }

    #[test]
    fn utc_month_boundary() {
        let r = vec![
            raw(1.0, 1.0, 1.0, 2021, 1, 31, 23, 59, 59),
            raw(1.0, 1.0, 1.0, 2021, 2, 1, 0, 0, 0),
        ];
        let g = aggregate_density(&r, &[], 1.0);
        assert_eq!((g.months[0].raw_total, g.months[1].raw_total), (1, 1));
    }
}
