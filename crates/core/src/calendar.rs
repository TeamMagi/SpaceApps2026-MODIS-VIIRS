use crate::calibration::calibration_for;
use crate::config::CalibrationConfig;
use crate::effort::monthly_omegas;
use crate::models::{CalendarCell, HarmonizedHotspot};
use crate::uncertainty::{
    SensorFamily, UncertaintyParams, mean_frp_uncertainty, relative_uncertainty_for_obs,
    total_frp_uncertainty,
};
use chrono::Datelike;
use std::collections::HashMap;

struct BucketRecord {
    lat: f64,
    frp_corrected: f64,
    sensor_family: SensorFamily,
    vza_factor: Option<f64>,
    n_obs: u16,
}

struct MonthBucket {
    count: u64,
    sum_corrected: f64,
    sum_raw: f64,
    has_viirs: bool,
    records: Vec<BucketRecord>,
    mean_uncertainty_items: Vec<(f64, f64, f64)>,
}

/// Build a burning activity calendar from harmonized hotspot data using default uncertainty and calibration configs.
///
/// Groups hotspots by (year, month) and computes:
/// - count: raw number of detections
/// - effort_omega: estimated observation effort Ω relative to MODIS
/// - count_normalized: MODIS-equivalent detections (`count / effort_omega`)
/// - mean_frp: native-pixel average harmonized FRP (`frp_corrected`; discontinuous across the MODIS→VIIRS transition)
/// - mean_frp_raw: average raw fire radiative power (`frp`)
/// - mean_frp_lower / mean_frp_upper: combined analytical confidence bounds for `mean_frp`
/// - total_frp: monthly total `frp_corrected`, scaled by `k_c(lat)` in VIIRS-less cells when enabled
/// - total_frp_lower / total_frp_upper: combined analytical confidence bounds for `total_frp`
/// - frp_per_detection_eq: MODIS-equivalent FRP per normalized detection (`total_frp / count_normalized`)
/// - anomaly: deviation from multi-year normalized monthly mean (if enough data)
pub fn build_calendar(hotspots: &[HarmonizedHotspot]) -> Vec<CalendarCell> {
    build_calendar_with(
        hotspots,
        &UncertaintyParams::default(),
        &CalibrationConfig::default(),
    )
}

/// Same as [`build_calendar`] using explicit uncertainty and calibration parameters.
pub fn build_calendar_with(
    hotspots: &[HarmonizedHotspot],
    uncertainty: &UncertaintyParams,
    calibration: &CalibrationConfig,
) -> Vec<CalendarCell> {
    let omegas = monthly_omegas(hotspots);
    let mut buckets: HashMap<(i32, u32), MonthBucket> = HashMap::new();

    for h in hotspots {
        let date = h.acquired.date_naive();
        let entry = buckets
            .entry((date.year(), date.month()))
            .or_insert_with(|| MonthBucket {
                count: 0,
                sum_corrected: 0.0,
                sum_raw: 0.0,
                has_viirs: false,
                records: Vec::new(),
                mean_uncertainty_items: Vec::new(),
            });
        entry.count += 1;
        entry.sum_corrected += h.frp_corrected;
        entry.sum_raw += h.frp;
        if h.platforms.iter().any(|s| s.is_viirs()) {
            entry.has_viirs = true;
        }

        let sensor_family = SensorFamily::from(h.source);
        // Native-pixel `mean_frp` never includes completeness k_c, so its systematic uncertainty
        // comes solely from `modis_scale_sigma` (on ModisOnly) without `ratio_sigma`.
        let mean_rel = relative_uncertainty_for_obs(
            h.vza_factor,
            sensor_family,
            None,
            h.n_obs,
            uncertainty,
        );
        entry
            .mean_uncertainty_items
            .push((h.frp_corrected, mean_rel.random, mean_rel.systematic));
        entry.records.push(BucketRecord {
            lat: h.lat,
            frp_corrected: h.frp_corrected,
            sensor_family,
            vza_factor: h.vza_factor,
            n_obs: h.n_obs,
        });
    }

    let mut cells: Vec<CalendarCell> = buckets
        .into_iter()
        .map(|((year, month), bucket)| {
            let effort_omega = omegas.get(&(year, month)).copied().flatten();
            let count_normalized = effort_omega.and_then(|omega| {
                (omega > 0.0 && omega.is_finite()).then_some(bucket.count as f64 / omega)
            });
            let mean_estimate = mean_frp_uncertainty(&bucket.mean_uncertainty_items, uncertainty);

            // Apply empirical pre-VIIRS completeness k_c(lat) to `total_frp` ONLY when the cell
            // has no VIIRS detections at all (pre-2012 MODIS-only era). Once VIIRS is present,
            // `ViirsOnly` detections already carry small fires MODIS missed, so applying k_c would
            // double-count them. Limitation: a transition month with even minimal VIIRS presence
            // suppresses k_c for the whole cell, and k_c is a zonal mean ratio rather than a
            // per-fire sub-pixel reconstruction.
            let apply_completeness = calibration.enabled && !bucket.has_viirs;
            let (total_frp, total_estimate) = if apply_completeness {
                let mut sum = 0.0_f64;
                let mut total_items = Vec::with_capacity(bucket.records.len());
                for rec in &bucket.records {
                    let cal = calibration_for(rec.lat, calibration);
                    let contrib = rec.frp_corrected * cal.ratio;
                    sum += contrib;
                    let rel = relative_uncertainty_for_obs(
                        rec.vza_factor,
                        rec.sensor_family,
                        Some((cal.ratio, cal.ratio_sigma)),
                        rec.n_obs,
                        uncertainty,
                    );
                    total_items.push((contrib, rel.random, rel.systematic));
                }
                (sum, total_frp_uncertainty(&total_items, uncertainty))
            } else {
                (
                    bucket.sum_corrected,
                    total_frp_uncertainty(&bucket.mean_uncertainty_items, uncertainty),
                )
            };

            let frp_per_detection_eq = count_normalized
                .filter(|&c| c > 0.0 && c.is_finite())
                .map(|c| total_frp / c);

            CalendarCell {
                year,
                month,
                count: bucket.count,
                count_normalized,
                effort_omega,
                mean_frp: bucket.sum_corrected / bucket.count as f64,
                mean_frp_raw: bucket.sum_raw / bucket.count as f64,
                mean_frp_lower: mean_estimate.map(|e| e.lower),
                mean_frp_upper: mean_estimate.map(|e| e.upper),
                total_frp,
                total_frp_lower: total_estimate.map(|e| e.lower),
                total_frp_upper: total_estimate.map(|e| e.upper),
                frp_per_detection_eq,
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
            frp_lower: frp_corrected,
            frp_upper: frp_corrected,
            vza_factor: if (frp - frp_corrected).abs() > f64::EPSILON {
                Some(frp_corrected / frp)
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

    fn terra(year: i32, month: u32, n: u32) -> Vec<HarmonizedHotspot> {
        (1..=n)
            .map(|d| make_with_platforms(year, month, d, vec![Sensor::ModisTerra]))
            .collect()
    }

    #[test]
    fn pre_aqua_months_have_no_normalized_count_or_anomaly() {
        let mut hotspots = Vec::new();
        for year in 2001..=2003 {
            for month in 1..=12 {
                hotspots.extend(terra(year, month, 2));
            }
        }
        let cells = build_calendar(&hotspots);
        assert_eq!(cells.len(), 36);
        for c in &cells {
            if (c.year, c.month) < (2002, 7) {
                assert_eq!(c.count, 2);
                assert_eq!(c.count_normalized, None);
                assert_eq!(c.effort_omega, None);
                assert_eq!(c.anomaly, None);
            } else {
                assert_eq!(c.count_normalized, Some(c.count as f64));
            }
        }
    }

    #[test]
    fn series_inside_2001_is_unchanged() {
        let mut hotspots = Vec::new();
        for month in 1..=4 {
            hotspots.extend(terra(2001, month, 2));
        }
        let cells = build_calendar(&hotspots);
        assert!(cells.iter().all(|c| c.effort_omega == Some(1.0)));
        assert!(cells.iter().all(|c| c.count_normalized == Some(2.0)));
    }

    #[test]
    fn pre_aqua_months_do_not_shift_anomaly_baseline() {
        // July counts: 2001 -> 50 (Terra-only, excluded), 2002..=2004 -> 1, 2, 3.
        let mut hotspots = terra(2001, 7, 28);
        hotspots.extend(terra(2001, 7, 28).into_iter().map(|mut h| {
            h.acquired = Utc.with_ymd_and_hms(2001, 7, 29, 12, 0, 0).unwrap();
            h
        }));
        for (year, n) in [(2002, 1), (2003, 2), (2004, 3)] {
            hotspots.extend(terra(year, 7, n));
        }
        let cells = build_calendar(&hotspots);
        let anomalies: Vec<Option<f64>> = cells.iter().map(|c| c.anomaly).collect();
        assert_eq!(anomalies[0], None);
        assert!((anomalies[1].unwrap() - (-1.224744871391589)).abs() < 1e-9);
        assert!(anomalies[2].unwrap().abs() < 1e-9);
        assert!((anomalies[3].unwrap() - 1.224744871391589).abs() < 1e-9);
    }

    #[test]
    fn cell_uncertainty_bounds_bracket_mean_and_total_and_widen_with_completeness_sigma() {
        // Pre-VIIRS (ModisOnly) cells at mid-lat (ratio_sigma=0.15, ratio=1.18) vs tropics (0.08, 1.11).
        let mut mid_lat = make_harmonized(2025, 7, 1, 50.0, 50.0);
        mid_lat.lat = 40.0;
        mid_lat.vza_factor = Some(1.0);

        let mut tropics = make_harmonized(2025, 8, 1, 50.0, 50.0);
        tropics.lat = 10.0;
        tropics.vza_factor = Some(1.0);

        let cells = build_calendar(&[mid_lat, tropics]);
        assert_eq!(cells.len(), 2);

        for cell in &cells {
            let lower = cell.mean_frp_lower.expect("non-empty cell has lower bound");
            let upper = cell.mean_frp_upper.expect("non-empty cell has upper bound");
            assert!(
                lower <= cell.mean_frp && cell.mean_frp <= upper,
                "expected {lower} <= {} <= {upper}",
                cell.mean_frp
            );

            let t_lower = cell.total_frp_lower.expect("non-empty cell has total lower bound");
            let t_upper = cell.total_frp_upper.expect("non-empty cell has total upper bound");
            assert!(
                t_lower <= cell.total_frp && cell.total_frp <= t_upper,
                "expected {t_lower} <= {} <= {t_upper}",
                cell.total_frp
            );
        }

        let mid_total_rel_width = (cells[0].total_frp_upper.unwrap()
            - cells[0].total_frp_lower.unwrap())
            / cells[0].total_frp;
        let trop_total_rel_width = (cells[1].total_frp_upper.unwrap()
            - cells[1].total_frp_lower.unwrap())
            / cells[1].total_frp;
        assert!(
            mid_total_rel_width > trop_total_rel_width,
            "expected mid-lat k_c sigma relative width ({mid_total_rel_width}) > tropics ({trop_total_rel_width})"
        );

        let widened_z = UncertaintyParams {
            z: 2.0,
            ..UncertaintyParams::default()
        };
        let cells_z2 = build_calendar_with(
            &[make_harmonized(2025, 7, 1, 50.0, 50.0)],
            &widened_z,
            &CalibrationConfig::default(),
        );
        let z1_w = cells[0].mean_frp_upper.unwrap() - cells[0].mean_frp_lower.unwrap();
        let z2_w = cells_z2[0].mean_frp_upper.unwrap() - cells_z2[0].mean_frp_lower.unwrap();
        assert!(z2_w > z1_w);
    }

    #[test]
    fn total_frp_applies_completeness_only_when_cell_has_no_viirs_and_enabled() {
        let mut trop = make_harmonized(2010, 7, 1, 100.0, 100.0);
        trop.lat = 10.0; // k_c = 1.11
        trop.platforms = vec![Sensor::ModisTerra, Sensor::ModisAqua];

        let mut boreal = make_harmonized(2010, 8, 1, 100.0, 100.0);
        boreal.lat = 60.0; // k_c = 1.28
        boreal.platforms = vec![Sensor::ModisAqua];

        // Third MODIS-only month so the MODIS pool reaches MIN_MONTHS_PER_CONFIG (3) for omega.
        let mut third_modis = make_harmonized(2010, 9, 1, 50.0, 50.0);
        third_modis.lat = 10.0;

        // Cell with VIIRS present: completeness must NOT be applied even to MODIS records in the same cell.
        let mut mixed_modis = make_harmonized(2014, 7, 1, 100.0, 100.0);
        mixed_modis.lat = 10.0;
        let mut mixed_viirs = make_harmonized(2014, 7, 2, 40.0, 40.0);
        mixed_viirs.lat = 10.0;
        mixed_viirs.platforms = vec![Sensor::ViirsSNPP];
        mixed_viirs.source = HotspotSource::ViirsOnly;

        let cells = build_calendar(&[
            trop.clone(),
            boreal.clone(),
            third_modis,
            mixed_modis,
            mixed_viirs,
        ]);
        assert_eq!(cells.len(), 4);

        // 2010-07 (tropics, no VIIRS): 100 * 1.11 = 111.0
        assert!((cells[0].total_frp - 111.0).abs() < 1e-9);
        assert!(cells[0].total_frp_lower.unwrap() < cells[0].total_frp);
        assert!(cells[0].total_frp_upper.unwrap() > cells[0].total_frp);
        let expected_eq = cells[0].total_frp / cells[0].count_normalized.unwrap();
        assert!((cells[0].frp_per_detection_eq.unwrap() - expected_eq).abs() < 1e-12);

        // 2010-08 (boreal, no VIIRS): 100 * 1.28 = 128.0
        assert!((cells[1].total_frp - 128.0).abs() < 1e-9);

        // 2014-07 (has VIIRS): 100 + 40 = 140.0 (no k_c)
        assert!((cells[3].total_frp - 140.0).abs() < 1e-9);
        assert!(cells[3].frp_per_detection_eq.is_none());

        // Disabled calibration: never applies k_c even in VIIRS-less cells.
        let cfg_disabled = CalibrationConfig {
            enabled: false,
            ..Default::default()
        };
        let cells_disabled =
            build_calendar_with(&[trop, boreal], &UncertaintyParams::default(), &cfg_disabled);
        assert!((cells_disabled[0].total_frp - 100.0).abs() < 1e-9);
        assert!((cells_disabled[1].total_frp - 100.0).abs() < 1e-9);

        // When count_normalized is None (Terra-only month in a mixed pre/post 2002-07 series),
        // frp_per_detection_eq must also be None.
        let terra_2001 = make_harmonized(2001, 7, 1, 80.0, 80.0);
        let terra_2003 = make_harmonized(2003, 7, 1, 80.0, 80.0);
        let era_cells = build_calendar(&[terra_2001, terra_2003]);
        assert!(era_cells[0].count_normalized.is_none());
        assert!(era_cells[0].frp_per_detection_eq.is_none());
    }
}
