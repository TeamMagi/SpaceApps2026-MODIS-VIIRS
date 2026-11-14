use crate::effort::monthly_omegas;
use crate::models::{CalendarCell, HarmonizedHotspot};
use chrono::Datelike;
use std::collections::HashMap;

/// Build a burning activity calendar from harmonized hotspot data.
///
/// Groups hotspots by (year, month) and computes:
/// - count: raw number of detections
/// - effort_omega: estimated observation effort Ω relative to MODIS
/// - count_normalized: MODIS-equivalent detections (`count / effort_omega`)
/// - mean_frp: average harmonized fire radiative power (`frp_corrected`)
/// - mean_frp_raw: average raw fire radiative power (`frp`)
/// - anomaly: deviation from multi-year normalized monthly mean (if enough data)
pub fn build_calendar(hotspots: &[HarmonizedHotspot]) -> Vec<CalendarCell> {
    let omegas = monthly_omegas(hotspots);
    let mut buckets: HashMap<(i32, u32), (u64, f64, f64)> = HashMap::new();

    for h in hotspots {
        let date = h.acquired.date_naive();
        let entry = buckets
            .entry((date.year(), date.month()))
            .or_insert((0, 0.0, 0.0));
        entry.0 += 1;
        entry.1 += h.frp_corrected;
        entry.2 += h.frp;
    }

    let mut cells: Vec<CalendarCell> = buckets
        .into_iter()
        .map(|((year, month), (count, sum_corrected, sum_raw))| {
            let effort_omega = omegas.get(&(year, month)).copied().flatten();
            let count_normalized = effort_omega.and_then(|omega| {
                (omega > 0.0 && omega.is_finite()).then_some(count as f64 / omega)
            });

            CalendarCell {
                year,
                month,
                count,
                count_normalized,
                effort_omega,
                mean_frp: sum_corrected / count as f64,
                mean_frp_raw: sum_raw / count as f64,
                anomaly: None,
            }
        })
        .collect();

    compute_anomalies(&mut cells);
    cells.sort_by_key(|a| (a.year, a.month));
    cells
}

/// Fill `anomaly` with the z-score of each cell's `count_normalized` against the same
/// calendar month across all cells; left `None` when that month has fewer than 3 cells or zero spread.
pub fn compute_anomalies(cells: &mut [CalendarCell]) {
    let mut monthly_values: HashMap<u32, Vec<f64>> = HashMap::new();

    for cell in cells.iter() {
        if let Some(norm) = cell.count_normalized && norm.is_finite() {
            monthly_values.entry(cell.month).or_default().push(norm);
        }
    }

    let baselines: HashMap<u32, (f64, f64)> = monthly_values
        .into_iter()
        .filter(|(_, vals)| vals.len() >= 3)
        .map(|(month, vals)| {
            let n = vals.len() as f64;
            let mean = vals.iter().sum::<f64>() / n;
            let std = (vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt();
            (month, (mean, std))
        })
        .collect();

    for cell in cells.iter_mut() {
        cell.anomaly = match (cell.count_normalized, baselines.get(&cell.month)) {
            (Some(norm), Some(&(mean, std))) if norm.is_finite() && std > 0.0 => {
                Some((norm - mean) / std)
            }
            _ => None,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, DayNight, HotspotSource, Sensor};
    use chrono::{TimeZone, Utc};

    fn make_harmonized(
        year: i32,
        month: u32,
        day: u32,
        frp: f64,
        frp_corrected: f64,
    ) -> HarmonizedHotspot {
        HarmonizedHotspot {
            lat: 39.0,
            lon: 32.0,
            frp,
            frp_corrected,
            vza_factor: if (frp - frp_corrected).abs() > f64::EPSILON {
                Some(frp / frp_corrected)
            } else {
                None
            },
            n_obs: 1,
            platforms: vec![Sensor::ModisTerra],
            confidence: Confidence::Nominal,
            source: HotspotSource::ModisOnly,
            acquired: Utc.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap(),
            daynight: DayNight::Day,
        }
    }

    fn make_with_platforms(
        year: i32,
        month: u32,
        day: u32,
        platforms: Vec<Sensor>,
    ) -> HarmonizedHotspot {
        let mut h = make_harmonized(year, month, day, 40.0, 40.0);
        let has_modis = platforms.iter().any(|s| s.is_modis());
        let has_viirs = platforms.iter().any(|s| s.is_viirs());
        h.source = match (has_modis, has_viirs) {
            (true, true) => HotspotSource::Matched,
            (true, false) => HotspotSource::ModisOnly,
            (false, true) | (false, false) => HotspotSource::ViirsOnly,
        };
        h.n_obs = platforms.len() as u16;
        h.platforms = platforms;
        h
    }

    #[test]
    fn calendar_distinguishes_raw_and_corrected_mean_frp() {
        let hotspots = vec![
            make_harmonized(2025, 7, 1, 100.0, 50.0),
            make_harmonized(2025, 7, 2, 60.0, 30.0),
        ];

        let cells = build_calendar(&hotspots);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].year, 2025);
        assert_eq!(cells[0].month, 7);
        assert_eq!(cells[0].count, 2);
        assert!((cells[0].mean_frp_raw - 80.0).abs() < 1e-12);
        assert!((cells[0].mean_frp - 40.0).abs() < 1e-12);
        assert!(cells[0].mean_frp < cells[0].mean_frp_raw);
        assert_eq!(cells[0].anomaly, None);
    }

    #[test]
    fn calendar_computes_count_anomaly_when_three_or_more_years_exist() {
        let hotspots = vec![
            make_harmonized(2023, 8, 1, 40.0, 40.0),
            make_harmonized(2024, 8, 1, 40.0, 40.0),
            make_harmonized(2025, 8, 1, 40.0, 40.0),
            make_harmonized(2025, 8, 2, 60.0, 30.0),
            make_harmonized(2025, 8, 3, 60.0, 30.0),
            make_harmonized(2025, 8, 4, 60.0, 30.0),
        ];

        let cells = build_calendar(&hotspots);
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].year, 2023);
        assert_eq!(cells[1].year, 2024);
        assert_eq!(cells[2].year, 2025);
        assert!(cells.iter().all(|c| c.anomaly.is_some()));
        assert!(cells[2].anomaly.unwrap() > 0.0);
    }

    #[test]
    fn modis_only_months_keep_count_normalized_equal_to_count() {
        let hotspots = vec![
            make_with_platforms(2010, 6, 1, vec![Sensor::ModisTerra]),
            make_with_platforms(2010, 6, 2, vec![Sensor::ModisTerra]),
            make_with_platforms(2010, 7, 1, vec![Sensor::ModisTerra]),
            make_with_platforms(2010, 8, 1, vec![Sensor::ModisTerra]),
            make_with_platforms(2010, 8, 2, vec![Sensor::ModisTerra]),
            make_with_platforms(2010, 8, 3, vec![Sensor::ModisTerra]),
        ];

        let cells = build_calendar(&hotspots);
        assert_eq!(cells.len(), 3);
        for cell in &cells {
            assert_eq!(cell.effort_omega, Some(1.0));
            assert_eq!(cell.count_normalized, Some(cell.count as f64));
        }
    }

    #[test]
    fn modis_plus_snpp_months_normalize_below_raw_count() {
        let mut hotspots = Vec::new();
        for month in 6..=8 {
            hotspots.push(make_with_platforms(2015, month, 1, vec![Sensor::ModisTerra]));
            for d in 2..=4 {
                hotspots.push(make_with_platforms(2015, month, d, vec![Sensor::ViirsSNPP]));
            }
        }

        let cells = build_calendar(&hotspots);
        assert_eq!(cells.len(), 3);
        for cell in &cells {
            let omega = cell.effort_omega.expect("omega should be present");
            assert!((omega - 4.0).abs() < 1e-12);
            let norm = cell.count_normalized.expect("normalized count should be present");
            assert!(norm < cell.count as f64);
            assert!((norm - cell.count as f64 / omega).abs() < 1e-12);
        }
    }

    #[test]
    fn synthetic_sensor_transition_eliminates_artificial_jump_and_false_anomaly() {
        let mut hotspots = Vec::new();
        let modis_per_month = 10;
        let viirs_extra = [(2012, 30), (2013, 31), (2014, 29)];

        for year in 2009..=2011 {
            for month in 6..=8 {
                for i in 0..modis_per_month {
                    let day = (i % 28) + 1;
                    hotspots.push(make_with_platforms(year, month, day, vec![Sensor::ModisTerra]));
                }
            }
        }

        for &(year, extra) in &viirs_extra {
            for month in 6..=8 {
                for i in 0..modis_per_month {
                    let day = (i % 28) + 1;
                    hotspots.push(make_with_platforms(
                        year,
                        month,
                        day,
                        vec![Sensor::ModisTerra, Sensor::ViirsSNPP],
                    ));
                }
                for i in 0..extra {
                    let day = (i % 28) + 1;
                    hotspots.push(make_with_platforms(year, month, day, vec![Sensor::ViirsSNPP]));
                }
            }
        }

        let cells = build_calendar(&hotspots);
        assert_eq!(cells.len(), 18);

        let pre_july = cells.iter().find(|c| c.year == 2011 && c.month == 7).unwrap();
        let post_july = cells.iter().find(|c| c.year == 2012 && c.month == 7).unwrap();

        let raw_jump = post_july.count as f64 / pre_july.count as f64;
        assert!((raw_jump - 4.0).abs() < 0.2);

        let pre_norm_mean = cells
            .iter()
            .filter(|c| c.year <= 2011)
            .map(|c| c.count_normalized.unwrap())
            .sum::<f64>()
            / 9.0;
        let post_norm_mean = cells
            .iter()
            .filter(|c| c.year >= 2012)
            .map(|c| c.count_normalized.unwrap())
            .sum::<f64>()
            / 9.0;

        let rel_diff = (post_norm_mean - pre_norm_mean).abs() / pre_norm_mean;
        assert!(rel_diff < 0.05, "post/pre normalized diff too high: {rel_diff}");

        for cell in &cells {
            let norm = cell.count_normalized.expect("every cell should normalize");
            let cell_diff = (norm - pre_norm_mean).abs() / pre_norm_mean;
            assert!(cell_diff < 0.05, "cell normalized diff too high: {cell_diff}");
            if let Some(anomaly) = cell.anomaly {
                assert!(
                    anomaly.abs() < 2.0,
                    "false anomaly at {}-{}: {anomaly}",
                    cell.year,
                    cell.month
                );
            }
        }
    }

    #[test]
    fn single_month_leaves_count_normalized_and_anomaly_none() {
        let hotspots = vec![
            make_with_platforms(2024, 7, 1, vec![Sensor::ModisTerra]),
            make_with_platforms(2024, 7, 2, vec![Sensor::ViirsSNPP]),
        ];

        let cells = build_calendar(&hotspots);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].effort_omega, None);
        assert_eq!(cells[0].count_normalized, None);
        assert_eq!(cells[0].anomaly, None);
    }

    #[test]
    fn empty_input_yields_empty_calendar() {
        assert!(build_calendar(&[]).is_empty());
    }

    #[test]
    fn twenty_four_months_yield_twenty_four_sorted_cells() {
        let mut hotspots = Vec::new();
        for year in 2023..=2024 {
            for month in 1..=12 {
                hotspots.push(make_harmonized(year, month, 15, 40.0, 40.0));
            }
        }

        let cells = build_calendar(&hotspots);
        assert_eq!(cells.len(), 24);
        assert_eq!((cells[0].year, cells[0].month), (2023, 1));
        assert_eq!((cells[23].year, cells[23].month), (2024, 12));
        assert!(
            cells
                .windows(2)
                .all(|w| (w[0].year, w[0].month) < (w[1].year, w[1].month))
        );
        assert!(cells.iter().all(|c| c.count == 1));
        assert!(cells.iter().all(|c| c.anomaly.is_none()));
    }

    #[test]
    fn identical_counts_across_years_give_no_anomaly() {
        let hotspots = vec![
            make_harmonized(2022, 8, 1, 40.0, 40.0),
            make_harmonized(2023, 8, 1, 40.0, 40.0),
            make_harmonized(2024, 8, 1, 40.0, 40.0),
        ];

        let cells = build_calendar(&hotspots);
        assert_eq!(cells.len(), 3);
        assert!(cells.iter().all(|c| c.anomaly.is_none()));
    }

    #[test]
    fn anomaly_is_population_z_score_of_normalized_counts() {
        let hotspots = vec![
            make_harmonized(2021, 7, 1, 40.0, 40.0),
            make_harmonized(2022, 7, 1, 40.0, 40.0),
            make_harmonized(2022, 7, 2, 40.0, 40.0),
            make_harmonized(2023, 7, 1, 40.0, 40.0),
            make_harmonized(2023, 7, 2, 40.0, 40.0),
            make_harmonized(2023, 7, 3, 40.0, 40.0),
        ];

        let cells = build_calendar(&hotspots);
        assert_eq!(cells.len(), 3);
        // MODIS-only months have unit effort, so the normalized counts equal the raw 1, 2, 3.
        assert!(cells.iter().all(|c| c.effort_omega == Some(1.0)));
        let anomalies: Vec<f64> = cells.iter().map(|c| c.anomaly.unwrap()).collect();
        assert!((anomalies[0] - (-1.224744871391589)).abs() < 1e-9);
        assert!(anomalies[1].abs() < 1e-9);
        assert!((anomalies[2] - 1.224744871391589).abs() < 1e-9);
    }

    #[test]
    fn month_boundary_uses_utc_calendar_month() {
        let mut first = make_harmonized(2025, 7, 1, 40.0, 40.0);
        first.acquired = Utc.with_ymd_and_hms(2025, 7, 31, 23, 59, 59).unwrap();
        let mut second = make_harmonized(2025, 8, 1, 40.0, 40.0);
        second.acquired = Utc.with_ymd_and_hms(2025, 8, 1, 0, 0, 0).unwrap();

        let cells = build_calendar(&[first, second]);
        assert_eq!(cells.len(), 2);
        assert_eq!(
            (cells[0].year, cells[0].month, cells[0].count),
            (2025, 7, 1)
        );
        assert_eq!(
            (cells[1].year, cells[1].month, cells[1].count),
            (2025, 8, 1)
        );
    }
}
