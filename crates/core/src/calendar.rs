use crate::calibration::{PowerLawCache, completeness_for_cached};
use crate::conformal::{ConformalEra, ConformalTable, conformal_interval};
use crate::config::CalibrationConfig;
use crate::effort::{
    FIRST_TERRA_AQUA_MONTH, SwathGapBasis, TropicalSwathGap, flag_tropical_swath_gap, in_tropical_belt, monthly_omegas,
    swath_gap_compensation,
};
use crate::models::{BBox, CalendarCell, DayNight, HarmonizedHotspot, SeasonYear, SeasonalityYear, Sensor};
use crate::quality::month_quality_flags;
use crate::uncertainty::{
    SensorFamily, UncertaintyParams, combine_factors, mean_frp_uncertainty,
    relative_uncertainty_for_obs, total_frp_uncertainty,
};
use chrono::Datelike;
use std::collections::HashMap;

struct BucketRecord {
    lat: f64,
    lon: f64,
    daynight: DayNight,
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
    platforms: Vec<Sensor>,
    records: Vec<BucketRecord>,
    mean_uncertainty_items: Vec<(f64, f64, f64)>,
}

fn mean_frp_uncertainty_item(
    h: &HarmonizedHotspot,
    uncertainty: &UncertaintyParams,
) -> (f64, f64, f64) {
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
    (h.frp_corrected, mean_rel.random, mean_rel.systematic)
}

fn mean_frp_bounds_from_items(
    sum_corrected: f64,
    items: &[(f64, f64, f64)],
    uncertainty: &UncertaintyParams,
) -> Option<(f64, f64, f64)> {
    if items.is_empty() {
        return None;
    }
    let mean = sum_corrected / items.len() as f64;
    let est = mean_frp_uncertainty(items, uncertainty)?;
    if !mean.is_finite()
        || !est.lower.is_finite()
        || !est.upper.is_finite()
        || est.lower < 0.0
        || est.lower > mean
        || mean > est.upper
    {
        return None;
    }
    Some((mean, est.lower, est.upper))
}

/// Compute `(mean_frp, mean_frp_lower, mean_frp_upper)` for a slice of harmonized hotspots using
/// the same native-pixel formula as [`build_calendar_with`]. Returns `None` when `hotspots` is
/// empty or any bound is non-finite.
pub fn mean_frp_with_uncertainty(
    hotspots: &[HarmonizedHotspot],
    uncertainty: &UncertaintyParams,
) -> Option<(f64, f64, f64)> {
    if hotspots.is_empty() {
        return None;
    }
    let mut sum_corrected = 0.0_f64;
    let mut items = Vec::with_capacity(hotspots.len());
    for h in hotspots {
        sum_corrected += h.frp_corrected;
        items.push(mean_frp_uncertainty_item(h, uncertainty));
    }
    mean_frp_bounds_from_items(sum_corrected, &items, uncertainty)
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
///   (and, only if `tropical_swath_gap_compensation` is on, by the swath-gap factor for |lat| <= 30°)
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
    build_calendar_with_conformal(hotspots, uncertainty, calibration, None)
}

/// Same as [`build_calendar_with`], additionally attaching `conformal` bands when a table is
/// given. Era: a cell with VIIRS is `Viirs`, a VIIRS-less cell is `PreViirs`, and Terra-only
/// months (before 2002-07) get no band.
pub fn build_calendar_with_conformal(
    hotspots: &[HarmonizedHotspot],
    uncertainty: &UncertaintyParams,
    calibration: &CalibrationConfig,
    conformal: Option<&ConformalTable>,
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
                platforms: Vec::new(),
                records: Vec::new(),
                mean_uncertainty_items: Vec::new(),
            });
        entry.count += 1;
        entry.sum_corrected += h.frp_corrected;
        entry.sum_raw += h.frp;
        if h.platforms.iter().any(|s| s.is_viirs()) {
            entry.has_viirs = true;
        }
        for &sensor in &h.platforms {
            if !entry.platforms.contains(&sensor) {
                entry.platforms.push(sensor);
            }
        }

        let sensor_family = SensorFamily::from(h.source);
        entry
            .mean_uncertainty_items
            .push(mean_frp_uncertainty_item(h, uncertainty));
        entry.records.push(BucketRecord {
            lat: h.lat,
            lon: h.lon,
            daynight: h.daynight,
            frp_corrected: h.frp_corrected,
            sensor_family,
            vza_factor: h.vza_factor,
            n_obs: h.n_obs,
        });
    }

    let mut law_cache = PowerLawCache::default();
    let mut cells: Vec<CalendarCell> = buckets
        .into_iter()
        .map(|((year, month), bucket)| {
            let effort_omega = omegas.get(&(year, month)).copied().flatten();
            let count_normalized = effort_omega.and_then(|omega| {
                (omega > 0.0 && omega.is_finite()).then_some(bucket.count as f64 / omega)
            });
            let mean_bounds = mean_frp_bounds_from_items(
                bucket.sum_corrected,
                &bucket.mean_uncertainty_items,
                uncertainty,
            );

            // Apply empirical pre-VIIRS completeness k_c(lat) to `total_frp` ONLY when the cell
            // has no VIIRS detections at all (pre-2012 MODIS-only era). Once VIIRS is present,
            // `ViirsOnly` detections already carry small fires MODIS missed, so applying k_c would
            // double-count them. Limitation: a transition month with even minimal VIIRS presence
            // suppresses k_c for the whole cell, and k_c is a zonal mean ratio rather than a
            // per-fire sub-pixel reconstruction.
            let apply_completeness = calibration.enabled && !bucket.has_viirs;
            // Optional tropical swath-gap compensation, gated exactly like k_c. It never double
            // counts with k_c: `calibration_grid_check` measures k_c only on 1°×1° cell-days that
            // BOTH Aqua MODIS and S-NPP observed within 20 min, so a cell-day inside a MODIS
            // inter-orbit gap (no MODIS rows) never enters the ratio; k_c is within-swath
            // small-fire completeness and the gap loss [14, Sec. 4.4] is disjoint from it, so the
            // product k_c × g applies each loss once. Cells with VIIRS get neither factor because
            // their ViirsOnly detections already fill the gaps, and Ω/counts are never touched
            // (see `effort::flag_tropical_swath_gap`).
            let apply_swath_gap = uncertainty.tropical_swath_gap_compensation && !bucket.has_viirs;
            let (total_frp, total_estimate) = if apply_completeness || apply_swath_gap {
                let gap = swath_gap_compensation();
                let mut sum = 0.0_f64;
                let mut total_items = Vec::with_capacity(bucket.records.len());
                for rec in &bucket.records {
                    let cal = completeness_for_cached(rec.lat, rec.lon, month, rec.daynight, calibration, &mut law_cache);
                    let completeness = (cal.ratio, cal.ratio_sigma);
                    let factor = if apply_swath_gap && in_tropical_belt(rec.lat) {
                        combine_factors(completeness, (gap.factor, gap.factor_sigma))
                    } else {
                        completeness
                    };
                    let contrib = rec.frp_corrected * factor.0;
                    sum += contrib;
                    let rel = relative_uncertainty_for_obs(
                        rec.vza_factor,
                        rec.sensor_family,
                        Some(factor),
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

            let era = if (year, month) < FIRST_TERRA_AQUA_MONTH {
                None
            } else if bucket.has_viirs {
                Some(ConformalEra::Viirs)
            } else {
                Some(ConformalEra::PreViirs)
            };
            let conformal_band = conformal
                .zip(era)
                .and_then(|(table, era)| conformal_interval(table, era, total_frp));

            CalendarCell {
                year,
                month,
                count: bucket.count,
                count_normalized,
                effort_omega,
                mean_frp: mean_bounds.map_or(bucket.sum_corrected / bucket.count as f64, |(m, _, _)| m),
                mean_frp_raw: bucket.sum_raw / bucket.count as f64,
                mean_frp_lower: mean_bounds.map(|(_, lower, _)| lower),
                mean_frp_upper: mean_bounds.map(|(_, _, upper)| upper),
                total_frp,
                total_frp_lower: total_estimate.map(|e| e.lower),
                total_frp_upper: total_estimate.map(|e| e.upper),
                frp_per_detection_eq,
                anomaly: None,
                sfai: None,
                quality_flags: month_quality_flags(year, month, &bucket.platforms),
                conformal: conformal_band,
                count_recovered: None,
                count_recovered_lower: None,
                count_recovered_upper: None,
            }
        })
        .collect();

    compute_anomalies(&mut cells);
    crate::pre_aqua::apply_recovery(hotspots, &mut cells);
    cells.sort_by_key(|a| (a.year, a.month));
    cells
}

/// Calendar `tropical_swath_gap` metadata: the belt share is taken from the query `bbox` when
/// given, otherwise from the latitude extent of `hotspots`; `None` when the region has no area
/// in 30°S-30°N. The compensation it reports matches what [`build_calendar_with`] applied with
/// the same `uncertainty` params.
pub fn calendar_swath_gap(
    bbox: Option<&BBox>,
    hotspots: &[HarmonizedHotspot],
    uncertainty: &UncertaintyParams,
) -> Option<TropicalSwathGap> {
    let compensate = uncertainty.tropical_swath_gap_compensation;
    if let Some(b) = bbox {
        return flag_tropical_swath_gap(b.south, b.north, SwathGapBasis::Bbox, compensate);
    }
    let (south, north) = hotspots
        .iter()
        .map(|h| h.lat)
        .filter(|lat| lat.is_finite())
        .fold(None, |acc: Option<(f64, f64)>, lat| {
            Some(acc.map_or((lat, lat), |(s, n)| (s.min(lat), n.max(lat))))
        })?;
    flag_tropical_swath_gap(south, north, SwathGapBasis::Detections, compensate)
}

/// Fill `anomaly` with the z-score of each cell's `count_normalized` against the same
/// calendar month across all cells; left `None` when that month has fewer than 3 cells or zero spread.
/// `sfai` is the blended robust counterpart over the same pool (see [`SFAI_MAD_WEIGHT`]).
pub fn compute_anomalies(cells: &mut [CalendarCell]) {
    let mut monthly_values: HashMap<u32, Vec<f64>> = HashMap::new();

    for cell in cells.iter() {
        if let Some(norm) = cell.count_normalized && norm.is_finite() {
            monthly_values.entry(cell.month).or_default().push(norm);
        }
    }

    let baselines: HashMap<u32, (f64, f64, f64, f64)> = monthly_values
        .into_iter()
        .filter(|(_, vals)| vals.len() >= 3)
        .map(|(month, vals)| {
            let n = vals.len() as f64;
            let mean = vals.iter().sum::<f64>() / n;
            let std = (vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt();
            let med = median(&vals);
            let deviations: Vec<f64> = vals.iter().map(|v| (v - med).abs()).collect();
            (month, (mean, std, med, median(&deviations)))
        })
        .collect();

    for cell in cells.iter_mut() {
        let finite = cell.count_normalized.filter(|v| v.is_finite());
        let baseline = baselines.get(&cell.month);
        cell.anomaly = match (finite, baseline) {
            (Some(norm), Some(&(mean, std, _, _))) if std > 0.0 => Some((norm - mean) / std),
            _ => None,
        };
        cell.sfai = match (finite, baseline) {
            (Some(norm), Some(&(_, std, med, mad))) => {
                let denom = SFAI_MAD_WEIGHT * MAD_TO_SIGMA * mad + SFAI_STD_WEIGHT * std;
                (denom > 0.0).then(|| (norm - med) / denom)
            }
            _ => None,
        };
    }
}

/// SFAI denominator follows the flare-chronos reference project: `0.65 * 1.4826 * MAD + 0.35 * sigma`.
/// The pure modified z-score (Iglewicz & Hoaglin) is the special case with MAD weight 1 and std weight 0.
/// The median numerator is our choice; the reference analysis only records the denominator.
const SFAI_MAD_WEIGHT: f64 = 0.65;
const SFAI_STD_WEIGHT: f64 = 0.35;
/// Makes the MAD a consistent estimator of sigma for normally distributed data.
const MAD_TO_SIGMA: f64 = 1.4826;

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

/// Zhang & Qian (2003) concentration degree and period over `(calendar_month, value)` pairs.
///
/// Returns `(pcd, peak_month)`: `pcd` in [0, 1] (`None` when the total is zero) and the fractional
/// calendar month of the vector mean direction in (0.5, 12.5] (`None` when the direction is undefined).
///
/// Month `m` gets the midpoint angle `(m - 0.5) * 30` degrees (Cui et al. 2021, Eq. 1-2), so a single
/// month yields exactly that month and Dec+Jan yields 12.5. The paper uses 36 ten-day intervals and
/// reports PCP in ten-day units; we use 12 monthly intervals, so values are not directly comparable.
pub fn concentration(values: &[(u32, f64)]) -> (Option<f64>, Option<f64>) {
    let (mut rx, mut ry, mut total) = (0.0_f64, 0.0_f64, 0.0_f64);
    for &(month, value) in values {
        let theta = (month as f64 - 0.5) * 30.0_f64.to_radians();
        rx += value * theta.sin();
        ry += value * theta.cos();
        total += value;
    }
    if total <= 0.0 || !total.is_finite() {
        return (None, None);
    }
    let pcd = rx.hypot(ry) / total;
    if pcd < 1e-9 {
        return (Some(pcd), None);
    }
    let pcp_deg = rx.atan2(ry).to_degrees().rem_euclid(360.0);
    let peak = pcp_deg / 30.0 + 0.5;
    (Some(pcd), Some(if peak <= 0.5 { peak + 12.0 } else { peak }))
}

/// Per-season-year PCD/PCP of monthly `count` over the inclusive observed `coverage`
/// (`(first (year, month), last (year, month))`).
///
/// Months inside coverage without a cell count as zero; a season year is `complete` only when all
/// 12 months are inside coverage, otherwise its metrics are `None`. Angles always follow the
/// calendar month so PCP is comparable between modes.
pub fn seasonality(
    cells: &[CalendarCell],
    mode: SeasonYear,
    coverage: ((i32, u32), (i32, u32)),
) -> Vec<SeasonalityYear> {
    let (first, last) = coverage;
    if first > last {
        return Vec::new();
    }
    let counts: HashMap<(i32, u32), u64> =
        cells.iter().map(|c| ((c.year, c.month), c.count)).collect();
    let season_of = |(year, month): (i32, u32)| match mode {
        SeasonYear::Calendar => year,
        SeasonYear::FireYear if month >= 3 => year,
        SeasonYear::FireYear => year - 1,
    };
    let months_of = |season: i32| -> Vec<(i32, u32)> {
        match mode {
            SeasonYear::Calendar => (1..=12).map(|m| (season, m)).collect(),
            SeasonYear::FireYear => (3..=12)
                .map(|m| (season, m))
                .chain((1..=2).map(|m| (season + 1, m)))
                .collect(),
        }
    };

    (season_of(first)..=season_of(last))
        .map(|year| {
            let months = months_of(year);
            let complete = months.iter().all(|&ym| ym >= first && ym <= last);
            let in_coverage = |ym: &&(i32, u32)| **ym >= first && **ym <= last;
            let values: Vec<(u32, f64)> = months
                .iter()
                .filter(in_coverage)
                .map(|ym| (ym.1, counts.get(ym).copied().unwrap_or(0) as f64))
                .collect();
            let total = months
                .iter()
                .filter(in_coverage)
                .map(|ym| counts.get(ym).copied().unwrap_or(0))
                .sum();
            let (pcd, peak_month) = if complete {
                concentration(&values)
            } else {
                (None, None)
            };
            SeasonalityYear {
                year,
                complete,
                total,
                pcd,
                pcp_deg: peak_month.map(|m| (m - 0.5) * 30.0),
                peak_month,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibration::CalibrationMode;
    use crate::models::{Confidence, HotspotSource};
    use crate::quality::QualityFlag;
    use chrono::{TimeZone, Utc};

    fn count_cell(year: i32, month: u32, count: u64, norm: Option<f64>) -> CalendarCell {
        CalendarCell {
            year,
            month,
            count,
            count_normalized: norm,
            effort_omega: None,
            mean_frp: 0.0,
            mean_frp_raw: 0.0,
            mean_frp_lower: None,
            mean_frp_upper: None,
            total_frp: 0.0,
            total_frp_lower: None,
            total_frp_upper: None,
            frp_per_detection_eq: None,
            anomaly: None,
            sfai: None,
            quality_flags: Vec::new(),
            conformal: None,
            count_recovered: None,
            count_recovered_lower: None,
            count_recovered_upper: None,
        }
    }

    fn sfai_cells(values: &[f64]) -> Vec<CalendarCell> {
        let mut cells: Vec<CalendarCell> = values
            .iter()
            .enumerate()
            .map(|(i, &v)| count_cell(2010 + i as i32, 7, v as u64, Some(v)))
            .collect();
        compute_anomalies(&mut cells);
        cells
    }

    #[test]
    fn concentration_single_month_is_fully_concentrated() {
        let (pcd, peak) = concentration(&[(8, 50.0)]);
        assert!((pcd.unwrap() - 1.0).abs() < 1e-12);
        assert!((peak.unwrap() - 8.0).abs() < 1e-9);
    }

    #[test]
    fn single_month_pcp_is_interval_midpoint() {
        for m in [1u32, 4, 7, 12] {
            let cells = full_year_cells(2020, &[(m, 10)]);
            let out = seasonality(&cells, SeasonYear::Calendar, ((2020, 1), (2020, 12)));
            assert!((out[0].pcp_deg.unwrap() - (m as f64 - 0.5) * 30.0).abs() < 1e-6);
            assert!((out[0].peak_month.unwrap() - m as f64).abs() < 1e-9);
        }
    }

    #[test]
    fn concentration_uniform_year_has_zero_pcd_and_no_direction() {
        let values: Vec<(u32, f64)> = (1..=12).map(|m| (m, 5.0)).collect();
        let (pcd, peak) = concentration(&values);
        assert!(pcd.unwrap() < 1e-9);
        assert_eq!(peak, None);
    }

    #[test]
    fn concentration_opposite_months_cancel() {
        let (pcd, _) = concentration(&[(3, 10.0), (9, 10.0)]);
        assert!(pcd.unwrap() < 1e-9);
    }

    #[test]
    fn concentration_wraps_across_year_end() {
        let (pcd, peak) = concentration(&[(12, 10.0), (1, 10.0)]);
        assert!(pcd.unwrap() > 0.9);
        assert!((peak.unwrap() - 12.5).abs() < 1e-9);
    }

    #[test]
    fn concentration_zero_total_is_none() {
        assert_eq!(concentration(&[(1, 0.0), (2, 0.0)]), (None, None));
        assert_eq!(concentration(&[]), (None, None));
    }

    fn full_year_cells(year: i32, month_counts: &[(u32, u64)]) -> Vec<CalendarCell> {
        month_counts
            .iter()
            .map(|&(m, c)| count_cell(year, m, c, Some(c as f64)))
            .collect()
    }

    #[test]
    fn seasonality_groups_calendar_and_fire_year_differently() {
        let mut cells = full_year_cells(2020, &[(1, 5), (7, 100), (12, 3)]);
        cells.extend(full_year_cells(2021, &[(1, 7), (2, 2), (7, 90)]));
        let coverage = ((2020, 1), (2022, 2));

        let cal = seasonality(&cells, SeasonYear::Calendar, coverage);
        assert_eq!(cal.iter().map(|y| y.year).collect::<Vec<_>>(), vec![2020, 2021, 2022]);
        assert_eq!(cal[0].total, 108);
        assert!(cal[0].complete && cal[1].complete && !cal[2].complete);

        let fire = seasonality(&cells, SeasonYear::FireYear, coverage);
        assert_eq!(fire.iter().map(|y| y.year).collect::<Vec<_>>(), vec![2019, 2020, 2021]);
        assert!(!fire[0].complete);
        // Fire year 2020 = Mar 2020 .. Feb 2021: Jul 100 + Dec 3 + Jan 7 + Feb 2.
        assert_eq!(fire[1].total, 112);
        assert!(fire[1].complete);
        assert_eq!(fire[2].total, 90);
        assert!(fire[2].complete);
    }

    #[test]
    fn seasonality_counts_missing_months_inside_coverage_as_zero() {
        let cells = full_year_cells(2020, &[(7, 40)]);
        let out = seasonality(&cells, SeasonYear::Calendar, ((2020, 1), (2020, 12)));
        assert_eq!(out.len(), 1);
        assert!(out[0].complete);
        assert!((out[0].pcd.unwrap() - 1.0).abs() < 1e-9);
        assert!((out[0].peak_month.unwrap() - 7.0).abs() < 1e-9);
        assert!((out[0].pcp_deg.unwrap() - 195.0).abs() < 1e-6);
    }

    #[test]
    fn seasonality_partial_year_is_incomplete_without_metrics() {
        let cells = full_year_cells(2020, &[(7, 40), (8, 10)]);
        let out = seasonality(&cells, SeasonYear::Calendar, ((2020, 3), (2020, 12)));
        assert_eq!(out.len(), 1);
        assert!(!out[0].complete);
        assert_eq!(out[0].total, 50);
        assert_eq!((out[0].pcd, out[0].pcp_deg, out[0].peak_month), (None, None, None));
    }

    #[test]
    fn seasonality_fire_year_past_coverage_end_is_incomplete() {
        let cells = full_year_cells(2020, &[(7, 40)]);
        let out = seasonality(&cells, SeasonYear::FireYear, ((2020, 1), (2021, 12)));
        let last = out.last().unwrap();
        assert_eq!(last.year, 2021);
        assert!(!last.complete);
        assert_eq!(last.pcd, None);
        assert!(out.iter().find(|y| y.year == 2020).unwrap().complete);
    }

    #[test]
    fn seasonality_inverted_coverage_is_empty() {
        assert!(seasonality(&[], SeasonYear::Calendar, ((2021, 1), (2020, 12))).is_empty());
    }

    #[test]
    fn sfai_matches_hand_computed_blended_score() {
        // median 12, MAD 1, population std = sqrt(1254.16).
        let cells = sfai_cells(&[10.0, 12.0, 11.0, 13.0, 100.0]);
        let denom = 0.65 * 1.4826 * 1.0 + 0.35 * 1254.16_f64.sqrt();
        assert!((cells[4].sfai.unwrap() - 88.0 / denom).abs() < 1e-9);
        assert!((cells[1].sfai.unwrap()).abs() < 1e-12);
    }

    #[test]
    fn sfai_uses_mean_of_middle_pair_for_even_counts() {
        // median 11.5, MAD 1.0, population std = sqrt(1.25).
        let cells = sfai_cells(&[10.0, 11.0, 12.0, 13.0]);
        let denom = 0.65 * 1.4826 + 0.35 * 1.25_f64.sqrt();
        assert!((cells[3].sfai.unwrap() - 1.5 / denom).abs() < 1e-9);
    }

    #[test]
    fn sfai_is_none_for_zero_spread_or_too_few_values() {
        assert!(sfai_cells(&[5.0, 5.0, 5.0]).iter().all(|c| c.sfai.is_none()));
        assert!(sfai_cells(&[1.0, 9.0]).iter().all(|c| c.sfai.is_none()));
    }

    #[test]
    fn sfai_with_zero_mad_falls_back_to_std_term() {
        // median 5, MAD 0, population std = sqrt(3).
        let cells = sfai_cells(&[5.0, 5.0, 5.0, 9.0]);
        let expected = 4.0 / (0.35 * 3.0_f64.sqrt());
        assert!((cells[3].sfai.unwrap() - expected).abs() < 1e-9);
        assert_eq!(cells[0].sfai, Some(0.0));
    }

    #[test]
    fn sfai_is_none_when_count_normalized_is_missing() {
        let mut cells = sfai_cells(&[10.0, 12.0, 11.0, 13.0]);
        cells.push(count_cell(2020, 7, 4, None));
        compute_anomalies(&mut cells);
        assert_eq!(cells[4].sfai, None);
        assert!(cells[0].sfai.is_some());
    }

    #[test]
    fn sfai_flags_outlier_more_strongly_than_z_score() {
        let cells = sfai_cells(&[10.0, 12.0, 11.0, 13.0, 100.0]);
        let z = cells[4].anomaly.unwrap();
        let sfai = cells[4].sfai.unwrap();
        assert!(z <= 2.0 + 1e-9);
        assert!(sfai > 3.0 * z);
    }

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

        let cells = build_calendar_with(
            &[trop.clone(), boreal.clone(), third_modis, mixed_modis, mixed_viirs],
            &UncertaintyParams::default(),
            &lat_band(),
        );
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

    #[test]
    fn biome_table_factor_scales_a_viirs_less_cell_and_leaves_viirs_cells_alone() {
        use crate::biome::Biome;
        use crate::calibration::BiomeCompleteness;
        let cfg = CalibrationConfig {
            biome_table: vec![BiomeCompleteness {
                biome: Biome::Cropland,
                month: Some(7),
                daynight: Some(DayNight::Night),
                ratio: 1.5,
                ratio_sigma: 0.1,
                n: 50,
            }],
            ..Default::default()
        };
        // make_harmonized sits at (39, 32); move to Ukraine cropland.
        let mut night = make_harmonized(2010, 7, 1, 100.0, 100.0);
        night.lat = 49.0;
        night.lon = 32.0;
        night.daynight = DayNight::Night;
        let mut day = night.clone();
        day.daynight = DayNight::Day;
        day.acquired = Utc.with_ymd_and_hms(2010, 8, 1, 12, 0, 0).unwrap();
        let mut third = night.clone();
        third.acquired = Utc.with_ymd_and_hms(2010, 9, 1, 12, 0, 0).unwrap();

        let cells = build_calendar_with(&[night.clone(), day, third], &UncertaintyParams::default(), &cfg);
        // July night hits the table; August day falls through to the 30-50 deg band (1.18).
        assert!((cells[0].total_frp - 150.0).abs() < 1e-9);
        assert!((cells[1].total_frp - 118.0).abs() < 1e-9);

        // A VIIRS detection in the cell switches k_c off entirely.
        let mut viirs = night.clone();
        viirs.platforms = vec![Sensor::ViirsSNPP];
        viirs.source = HotspotSource::ViirsOnly;
        let with_viirs = build_calendar_with(&[night, viirs], &UncertaintyParams::default(), &cfg);
        assert!((with_viirs[0].total_frp - 200.0).abs() < 1e-9);
    }

    #[test]
    fn default_config_gives_a_viirs_less_savanna_cell_its_measured_factor() {
        // Northern Australian savanna, July, day: measured (savanna, 7, Day) = 1.084.
        let mut day = make_harmonized(2010, 7, 1, 100.0, 100.0);
        day.lat = -13.0;
        day.lon = 132.0;
        day.daynight = DayNight::Day;
        let cells = build_calendar(&[day.clone()]);
        assert!((cells[0].total_frp - 108.4).abs() < 1e-9, "{}", cells[0].total_frp);

        // The same cell, night: savanna July night = 1.099, not the latitude band (1.11).
        let mut night = day.clone();
        night.daynight = DayNight::Night;
        let cells = build_calendar(&[night]);
        assert!((cells[0].total_frp - 109.9).abs() < 1e-9, "{}", cells[0].total_frp);

        // Pinned to the latitude band the old factor returns.
        let lat_band = CalibrationConfig {
            mode: crate::calibration::CalibrationMode::LatBand,
            ..Default::default()
        };
        let cells = build_calendar_with(&[day], &UncertaintyParams::default(), &lat_band);
        assert!((cells[0].total_frp - 111.0).abs() < 1e-9);
    }

    #[test]
    fn mean_frp_with_uncertainty_matches_calendar_cell_and_handles_edge_cases() {
        let params = UncertaintyParams::default();
        assert_eq!(mean_frp_with_uncertainty(&[], &params), None);

        let mut modis = make_harmonized(2021, 7, 1, 60.0, 72.0);
        modis.vza_factor = Some(1.2);
        let mut viirs = make_harmonized(2021, 7, 2, 24.0, 24.0);
        viirs.platforms = vec![Sensor::ViirsSNPP];
        viirs.source = HotspotSource::ViirsOnly;
        viirs.vza_factor = None;

        let hotspots = [modis.clone(), viirs];
        let cells = build_calendar_with(&hotspots, &params, &CalibrationConfig::default());
        assert_eq!(cells.len(), 1);

        let (mean, lower, upper) = mean_frp_with_uncertainty(&hotspots, &params).unwrap();
        assert_eq!(mean, cells[0].mean_frp);
        assert_eq!(Some(lower), cells[0].mean_frp_lower);
        assert_eq!(Some(upper), cells[0].mean_frp_upper);
        assert!(0.0 <= lower && lower <= mean && mean <= upper);

        let nan_frp = make_harmonized(2021, 7, 3, 10.0, f64::NAN);
        assert_eq!(mean_frp_with_uncertainty(&[nan_frp], &params), None);

        let neg_frp = make_harmonized(2021, 7, 3, -5.0, -5.0);
        assert_eq!(mean_frp_with_uncertainty(&[neg_frp], &params), None);
    }

    #[test]
    fn empty_quality_flags_omitted_when_serialized() {
        let cell = count_cell(2015, 7, 3, Some(3.0));
        let json = serde_json::to_value(&cell).unwrap();
        assert!(json.get("quality_flags").is_none());

        let mut flagged = cell;
        flagged.quality_flags = vec![QualityFlag::TerraEarlyRecord];
        let flagged_json = serde_json::to_value(&flagged).unwrap();
        assert_eq!(
            flagged_json["quality_flags"],
            serde_json::json!(["terra_early_record"])
        );
    }

    #[test]
    fn build_calendar_attaches_quality_flags_to_affected_months() {
        let terra_outage_month = make_harmonized(2000, 8, 20, 50.0, 50.0);
        let clean_modis = make_harmonized(2008, 7, 15, 50.0, 50.0);
        let mut snpp_early = make_harmonized(2012, 4, 20, 30.0, 30.0);
        snpp_early.platforms = vec![Sensor::ViirsSNPP];
        snpp_early.source = HotspotSource::ViirsOnly;
        let mut aqua_drift = make_harmonized(2021, 1, 10, 40.0, 40.0);
        aqua_drift.platforms = vec![Sensor::ModisAqua];

        let cells = build_calendar(&[terra_outage_month, clean_modis, snpp_early, aqua_drift]);
        assert_eq!(cells.len(), 4);
        assert_eq!(
            cells[0].quality_flags,
            vec![QualityFlag::TerraEarlyRecord, QualityFlag::TerraOutage]
        );
        assert!(cells[1].quality_flags.is_empty());
        assert_eq!(
            cells[2].quality_flags,
            vec![
                QualityFlag::SnppEarlyCalibration,
                QualityFlag::SnppSpuriousScanLines,
            ]
        );
        assert_eq!(cells[3].quality_flags, vec![QualityFlag::ModisOrbitalDrift]);
    }

    /// Three VIIRS-less months (tropics, belt edge, mid-latitude) plus one tropical VIIRS month.
    fn swath_gap_series() -> Vec<HarmonizedHotspot> {
        let at = |year, month, lat: f64| {
            let mut h = make_harmonized(year, month, 1, 100.0, 100.0);
            h.lat = lat;
            h
        };
        let mut viirs = at(2014, 7, 10.0);
        viirs.platforms = vec![Sensor::ViirsSNPP];
        viirs.source = HotspotSource::ViirsOnly;
        vec![
            at(2010, 7, 10.0),
            at(2010, 8, -30.0),
            at(2010, 9, 40.0),
            at(2014, 7, 10.0),
            viirs,
        ]
    }

    // These tests pin latitude-band arithmetic at lat 10, lon 32 (savanna): pin the mode explicitly.
    fn lat_band() -> CalibrationConfig {
        CalibrationConfig {
            mode: CalibrationMode::LatBand,
            ..Default::default()
        }
    }

    fn compensation_on() -> UncertaintyParams {
        UncertaintyParams {
            tropical_swath_gap_compensation: true,
            ..UncertaintyParams::default()
        }
    }

    #[test]
    fn swath_gap_compensation_off_keeps_totals_and_bands_identical_to_k_c_only() {
        let hotspots = swath_gap_series();
        let default_cells = build_calendar_with(&hotspots, &UncertaintyParams::default(), &lat_band());
        let explicit_off = build_calendar_with(
            &hotspots,
            &UncertaintyParams {
                tropical_swath_gap_compensation: false,
                ..UncertaintyParams::default()
            },
            &lat_band(),
        );
        assert_eq!(
            serde_json::to_string(&default_cells).unwrap(),
            serde_json::to_string(&explicit_off).unwrap()
        );

        // Pin the k_c-only arithmetic exactly (bitwise): 100 × 1.11 with the k_c sigma alone.
        let p = UncertaintyParams::default();
        let rel = relative_uncertainty_for_obs(None, SensorFamily::Modis, Some((1.11, 0.08)), 1, &p);
        let expected = total_frp_uncertainty(&[(100.0 * 1.11, rel.random, rel.systematic)], &p).unwrap();
        let trop = &default_cells[0];
        assert_eq!(trop.total_frp, 100.0 * 1.11);
        assert_eq!(trop.total_frp_lower, Some(expected.lower));
        assert_eq!(trop.total_frp_upper, Some(expected.upper));
        // VIIRS month: plain sum, no factor at all.
        assert_eq!(default_cells[3].total_frp, 200.0);
    }

    #[test]
    fn swath_gap_compensation_on_applies_factor_once_in_belt_and_widens_band() {
        let hotspots = swath_gap_series();
        let off = build_calendar_with(&hotspots, &UncertaintyParams::default(), &lat_band());
        let on = build_calendar_with(&hotspots, &compensation_on(), &lat_band());
        let g = swath_gap_compensation();
        assert_eq!(on.len(), off.len());

        // Tropics and the inclusive -30° edge: k_c × g, applied exactly once.
        for i in [0, 1] {
            let expected = off[i].total_frp * g.factor;
            assert!((on[i].total_frp - expected).abs() < 1e-9, "{i}: {}", on[i].total_frp);
            let rel_w = |c: &CalendarCell| {
                (c.total_frp_upper.unwrap() - c.total_frp_lower.unwrap()) / c.total_frp
            };
            assert!(rel_w(&on[i]) > rel_w(&off[i]), "band must widen at {i}");
            assert!(on[i].total_frp_lower.unwrap() < on[i].total_frp);
            assert!(on[i].total_frp_upper.unwrap() > on[i].total_frp);
        }
        assert!((on[0].total_frp - 100.0 * 1.11 * g.factor).abs() < 1e-9);

        // Outside the belt and in the VIIRS month nothing changes, bit for bit.
        for i in [2, 3] {
            assert_eq!(on[i].total_frp, off[i].total_frp);
            assert_eq!(on[i].total_frp_lower, off[i].total_frp_lower);
            assert_eq!(on[i].total_frp_upper, off[i].total_frp_upper);
        }

        // Counts, Ω, native-pixel means and anomalies are never compensated.
        for (a, b) in on.iter().zip(&off) {
            assert_eq!(a.count, b.count);
            assert_eq!(a.effort_omega, b.effort_omega);
            assert_eq!(a.count_normalized, b.count_normalized);
            assert_eq!(a.mean_frp, b.mean_frp);
            assert_eq!(a.mean_frp_lower, b.mean_frp_lower);
            assert_eq!(a.anomaly, b.anomaly);
        }

        // With k_c disabled the gap factor alone applies, still once.
        let no_kc = CalibrationConfig {
            enabled: false,
            ..Default::default()
        };
        let gap_only = build_calendar_with(&hotspots, &compensation_on(), &no_kc);
        assert!((gap_only[0].total_frp - 100.0 * g.factor).abs() < 1e-9);
        assert_eq!(gap_only[2].total_frp, 100.0);
    }

    #[test]
    fn belt_edge_detection_is_inclusive_at_30_degrees_only() {
        let at = |month, lat: f64| {
            let mut h = make_harmonized(2010, month, 1, 100.0, 100.0);
            h.lat = lat;
            h
        };
        let hotspots = [at(6, 30.0), at(7, 30.0001), at(8, 29.9999)];
        let cal_off = CalibrationConfig {
            enabled: false,
            ..Default::default()
        };
        let cells = build_calendar_with(&hotspots, &compensation_on(), &cal_off);
        let g = swath_gap_compensation().factor;
        assert!((cells[0].total_frp - 100.0 * g).abs() < 1e-9);
        assert_eq!(cells[1].total_frp, 100.0);
        assert!((cells[2].total_frp - 100.0 * g).abs() < 1e-9);
    }

    #[test]
    fn calendar_swath_gap_uses_bbox_then_detection_extent() {
        let off = UncertaintyParams::default();
        let bbox = |south, north| BBox {
            west: -60.0,
            south,
            east: -40.0,
            north,
        };

        let brazil = calendar_swath_gap(Some(&bbox(-34.0, 6.0)), &[], &off).unwrap();
        assert_eq!(brazil.basis, SwathGapBasis::Bbox);
        assert!(brazil.belt_share > 0.9 && brazil.belt_share < 1.0);
        assert_eq!(brazil.compensation, None);
        assert_eq!((brazil.missed_frp_low_pct, brazil.missed_frp_high_pct), (10.7, 12.5));

        // The bbox wins over detections that would say otherwise.
        let tropical_rows = swath_gap_series();
        assert!(calendar_swath_gap(Some(&bbox(36.0, 42.0)), &tropical_rows, &off).is_none());

        // Without a bbox the detections' latitude span is used.
        let from_rows = calendar_swath_gap(None, &tropical_rows, &off).unwrap();
        assert_eq!(from_rows.basis, SwathGapBasis::Detections);
        let expected = crate::effort::tropical_belt_share(-30.0, 40.0);
        assert!((from_rows.belt_share - expected).abs() < 1e-12);

        let turkey = make_harmonized(2010, 7, 1, 10.0, 10.0);
        assert!(calendar_swath_gap(None, &[turkey], &off).is_none());
        assert!(calendar_swath_gap(None, &[], &off).is_none());

        let on = calendar_swath_gap(Some(&bbox(-10.0, 5.0)), &[], &compensation_on()).unwrap();
        assert_eq!(on.belt_share, 1.0);
        assert_eq!(on.compensation, Some(swath_gap_compensation()));
    }

    fn unit_conformal_table() -> ConformalTable {
        use crate::conformal::ConformalEntry;
        let entry = |era| ConformalEntry {
            era,
            bin: 0,
            mu_lo: None,
            mu_hi: None,
            cal_mu_min: 0.0,
            cal_mu_max: 1e9,
            n_cal: 100,
            q68_lower: Some(-0.1),
            q68_upper: Some(0.1),
            q90_lower: Some(-0.2),
            q90_upper: Some(0.2),
        };
        ConformalTable {
            schema_version: 1,
            calibration_fingerprint: "test".into(),
            entries: vec![entry(ConformalEra::PreViirs), entry(ConformalEra::Viirs)],
            coverage: None,
            notes: None,
        }
    }

    fn conformal_cells(hotspots: &[HarmonizedHotspot], table: Option<&ConformalTable>) -> Vec<CalendarCell> {
        build_calendar_with_conformal(
            hotspots,
            &UncertaintyParams::default(),
            &CalibrationConfig::default(),
            table,
        )
    }

    #[test]
    fn conformal_era_follows_viirs_presence_and_terra_only_gets_none() {
        let hotspots = vec![
            make_with_platforms(2001, 3, 1, vec![Sensor::ModisTerra]),
            make_with_platforms(2010, 6, 1, vec![Sensor::ModisTerra]),
            make_with_platforms(2010, 6, 2, vec![Sensor::ModisAqua]),
            make_with_platforms(2015, 6, 1, vec![Sensor::ModisTerra]),
            make_with_platforms(2015, 6, 2, vec![Sensor::ViirsSNPP]),
        ];
        let table = unit_conformal_table();
        let cells = conformal_cells(&hotspots, Some(&table));
        let cell = |y: i32| cells.iter().find(|c| c.year == y).unwrap();

        assert!(cell(2001).conformal.is_none(), "Terra-only month");
        let pre = cell(2010).conformal.expect("pre-VIIRS band");
        assert_eq!(pre.era, ConformalEra::PreViirs);
        assert!((pre.p68_lower - cell(2010).total_frp * 0.9).abs() < 1e-9);
        assert!((pre.p90_upper - cell(2010).total_frp * 1.2).abs() < 1e-9);
        let viirs = cell(2015).conformal.expect("VIIRS band");
        assert_eq!(viirs.era, ConformalEra::Viirs);
        assert!((viirs.p68_upper - cell(2015).total_frp * 1.1).abs() < 1e-9);
    }

    #[test]
    fn terra_only_series_before_aqua_gets_no_conformal_band() {
        let hotspots = vec![make_with_platforms(2001, 3, 1, vec![Sensor::ModisTerra])];
        let cells = conformal_cells(&hotspots, Some(&unit_conformal_table()));
        assert_eq!(cells.len(), 1);
        assert!(cells[0].conformal.is_none());
    }

    #[test]
    fn without_a_table_there_is_no_conformal_field_and_values_are_unchanged() {
        let hotspots = vec![
            make_with_platforms(2010, 6, 1, vec![Sensor::ModisTerra]),
            make_with_platforms(2015, 6, 2, vec![Sensor::ViirsSNPP]),
        ];
        let plain = build_calendar(&hotspots);
        let none = conformal_cells(&hotspots, None);
        assert_eq!(
            serde_json::to_string(&plain).unwrap(),
            serde_json::to_string(&none).unwrap()
        );
        assert!(plain.iter().all(|c| c.conformal.is_none()));
        assert!(!serde_json::to_string(&plain).unwrap().contains("conformal"));
    }

    #[test]
    fn conformal_band_round_trips_through_cell_json() {
        let hotspots = vec![make_with_platforms(2015, 6, 2, vec![Sensor::ViirsSNPP])];
        let cells = conformal_cells(&hotspots, Some(&unit_conformal_table()));
        let json = serde_json::to_string(&cells[0]).unwrap();
        assert!(json.contains("\"conformal\""));
        assert!(json.contains("\"era\":\"viirs\""));
        let back: CalendarCell = serde_json::from_str(&json).unwrap();
        let (b, a) = (back.conformal.unwrap(), cells[0].conformal.unwrap());
        assert_eq!((b.era, b.bin, b.n_cal, b.extrapolated), (a.era, a.bin, a.n_cal, a.extrapolated));
        assert!((b.p90_upper - a.p90_upper).abs() < 1e-9);
        let legacy: CalendarCell = serde_json::from_str(
            &json.replace(&format!(",\"conformal\":{}", serde_json::to_string(&cells[0].conformal).unwrap()), ""),
        )
        .unwrap();
        assert!(legacy.conformal.is_none());
    }
}
