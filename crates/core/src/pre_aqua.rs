//! Recovery estimate for the Terra-only era (2000-11..2002-06), adapted from Remy & Kaiser (2014,
//! ACP 14, 13291).
//!
//! The paper regresses two-satellite FRP on Terra-only FRP in adaptive regions (2°x2° growing to
//! 12°x12°, N >= 400) after removing the 10 % largest cloud-difference outliers. FIRMS carries no
//! cloud data, so this is an ADAPTED variant: the ratio `MODIS total / Terra-visible` is pooled per
//! calendar month-of-year over the Terra+Aqua calibration era (the pool widens in time instead of
//! space) and the cloud filter is replaced by trimming the ratios farthest from the pool median.
//!
//! The result estimates MODIS-observable (Terra+Aqua-equivalent) detections, comparable with the
//! MODIS-only baseline era count, NOT with the fleet-normalized `count_normalized`.

use crate::models::{CalendarCell, HarmonizedHotspot, Sensor};
use chrono::Datelike;
use std::collections::BTreeMap;

/// First calibration month: Aqua's first full month.
pub const CALIBRATION_START: (i32, u32) = (2002, 7);
/// Last calibration month: before the 2021+ MODIS orbital drift.
pub const CALIBRATION_END: (i32, u32) = (2020, 12);
/// First month with Terra data after the incomplete early record.
pub const RECOVERY_START: (i32, u32) = (2000, 11);
/// Last Terra-only month.
pub const RECOVERY_END: (i32, u32) = (2002, 6);
/// Minimum pooled Terra-visible detections (the paper's N >= 400).
pub const MIN_POOLED_TERRA: u64 = 400;
/// Share of pooled month ratios dropped as outliers (the paper's 10 %).
pub const TRIM_FRACTION: f64 = 0.10;
/// Trimming and percentile bands need at least this many month ratios.
pub const MIN_MONTHS_FOR_TRIM: usize = 10;

/// Detections of one month: records containing Terra, and records containing any MODIS platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MonthTally {
    pub terra_visible: u64,
    pub modis_total: u64,
}

/// How far the month-of-year pool had to be widened to reach [`MIN_POOLED_TERRA`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolWidth {
    SameMonth,
    PlusMinus1,
    PlusMinus2,
    AllMonths,
}

impl PoolWidth {
    const WIDENING: [PoolWidth; 4] = [
        PoolWidth::SameMonth,
        PoolWidth::PlusMinus1,
        PoolWidth::PlusMinus2,
        PoolWidth::AllMonths,
    ];

    fn half_width(self) -> u32 {
        match self {
            Self::SameMonth => 0,
            Self::PlusMinus1 => 1,
            Self::PlusMinus2 => 2,
            Self::AllMonths => 6,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::SameMonth => "same month",
            Self::PlusMinus1 => "±1 month",
            Self::PlusMinus2 => "±2 months",
            Self::AllMonths => "all months",
        }
    }
}

/// Multiplier from Terra-visible to MODIS-observable detections for one month-of-year.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecoveryFactor {
    pub k: f64,
    pub lower: f64,
    pub upper: f64,
    pub pool: PoolWidth,
    /// Calibration months kept after trimming.
    pub months_used: usize,
    /// Terra-visible detections of the kept months (the paper's N).
    pub terra_used: u64,
}

/// Index 0 is January.
pub type RecoveryFactors = [Option<RecoveryFactor>; 12];

/// Recovered detection count of one Terra-only month.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recovered {
    pub count: f64,
    pub lower: f64,
    pub upper: f64,
}

pub fn month_tallies(hotspots: &[HarmonizedHotspot]) -> BTreeMap<(i32, u32), MonthTally> {
    let mut map: BTreeMap<(i32, u32), MonthTally> = BTreeMap::new();
    for h in hotspots {
        let date = h.acquired.date_naive();
        let tally = map.entry((date.year(), date.month())).or_default();
        if h.platforms.contains(&Sensor::ModisTerra) {
            tally.terra_visible += 1;
        }
        if h.platforms.iter().any(|s| s.is_modis()) {
            tally.modis_total += 1;
        }
    }
    map
}

fn median(sorted: &[f64]) -> f64 {
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

/// Linear-interpolated percentile of an ascending slice, `q` in 0..=1.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

fn circular_month_distance(a: u32, b: u32) -> u32 {
    let d = a.abs_diff(b);
    d.min(12 - d)
}

/// `(ratio, terra_visible, modis_total)` per calibration month.
type PoolEntry = (f64, u64, u64);

fn factor_from_pool(pool: &[PoolEntry], width: PoolWidth) -> RecoveryFactor {
    let mut kept: Vec<PoolEntry> = pool.to_vec();
    if kept.len() >= MIN_MONTHS_FOR_TRIM {
        let mut ratios: Vec<f64> = kept.iter().map(|e| e.0).collect();
        ratios.sort_by(f64::total_cmp);
        let med = median(&ratios);
        kept.sort_by(|a, b| (a.0 - med).abs().total_cmp(&(b.0 - med).abs()));
        let drop = (kept.len() as f64 * TRIM_FRACTION).ceil() as usize;
        kept.truncate(kept.len() - drop);
    }
    let terra: u64 = kept.iter().map(|e| e.1).sum();
    let total: u64 = kept.iter().map(|e| e.2).sum();
    let k = total as f64 / terra as f64;
    let mut ratios: Vec<f64> = kept.iter().map(|e| e.0).collect();
    ratios.sort_by(f64::total_cmp);
    let (lower, upper) = if ratios.len() >= MIN_MONTHS_FOR_TRIM {
        (percentile(&ratios, 0.1), percentile(&ratios, 0.9))
    } else {
        (ratios[0], ratios[ratios.len() - 1])
    };
    RecoveryFactor {
        k,
        lower: lower.min(k),
        upper: upper.max(k),
        pool: width,
        months_used: kept.len(),
        terra_used: terra,
    }
}

/// Factors per month-of-year from month tallies. Only months inside
/// [`CALIBRATION_START`]..=[`CALIBRATION_END`] with Terra-visible detections are calibration
/// months; callers may pre-filter `tallies` (hold-out validation).
pub fn factors_from_tallies(tallies: &BTreeMap<(i32, u32), MonthTally>) -> RecoveryFactors {
    let era: Vec<(u32, PoolEntry)> = tallies
        .iter()
        .filter(|&(&ym, t)| {
            (CALIBRATION_START..=CALIBRATION_END).contains(&ym) && t.terra_visible > 0
        })
        .map(|(&(_, month), t)| {
            (
                month,
                (
                    t.modis_total as f64 / t.terra_visible as f64,
                    t.terra_visible,
                    t.modis_total,
                ),
            )
        })
        .collect();

    let mut factors: RecoveryFactors = [None; 12];
    for month in 1..=12u32 {
        for width in PoolWidth::WIDENING {
            let pool: Vec<PoolEntry> = era
                .iter()
                .filter(|(m, _)| circular_month_distance(*m, month) <= width.half_width())
                .map(|&(_, e)| e)
                .collect();
            if pool.iter().map(|e| e.1).sum::<u64>() >= MIN_POOLED_TERRA {
                factors[month as usize - 1] = Some(factor_from_pool(&pool, width));
                break;
            }
        }
    }
    factors
}

pub fn calibration_factors(hotspots: &[HarmonizedHotspot]) -> RecoveryFactors {
    factors_from_tallies(&month_tallies(hotspots))
}

/// `count x k` with the percentile band, `None` when no factor exists for the month.
pub fn recover_count(factors: &RecoveryFactors, month: u32, count: u64) -> Option<Recovered> {
    let f = factors.get((month as usize).checked_sub(1)?)?.as_ref()?;
    let c = count as f64;
    Some(Recovered {
        count: c * f.k,
        lower: c * f.lower,
        upper: c * f.upper,
    })
}

pub fn is_recovery_month(year: i32, month: u32) -> bool {
    (RECOVERY_START..=RECOVERY_END).contains(&(year, month))
}

/// Fill the `count_recovered*` fields of Terra-only months. `count_normalized`, `effort_omega`
/// and `anomaly` are never touched.
pub fn apply_recovery(hotspots: &[HarmonizedHotspot], cells: &mut [CalendarCell]) {
    if !cells.iter().any(|c| is_recovery_month(c.year, c.month)) {
        return;
    }
    let factors = calibration_factors(hotspots);
    for cell in cells.iter_mut().filter(|c| is_recovery_month(c.year, c.month)) {
        if let Some(r) = recover_count(&factors, cell.month, cell.count) {
            cell.count_recovered = Some(r.count);
            cell.count_recovered_lower = Some(r.lower);
            cell.count_recovered_upper = Some(r.upper);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::build_calendar;
    use crate::models::{Confidence, DayNight, HotspotSource};
    use chrono::{TimeZone, Utc};

    fn months_between(from: (i32, u32), to: (i32, u32)) -> Vec<(i32, u32)> {
        let mut out = Vec::new();
        let (mut y, mut m) = from;
        while (y, m) <= to {
            out.push((y, m));
            m += 1;
            if m == 13 {
                m = 1;
                y += 1;
            }
        }
        out
    }

    fn uniform(terra: u64, total: u64) -> BTreeMap<(i32, u32), MonthTally> {
        months_between(CALIBRATION_START, CALIBRATION_END)
            .into_iter()
            .map(|ym| {
                (
                    ym,
                    MonthTally {
                        terra_visible: terra,
                        modis_total: total,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn factor_equals_known_ratio_with_same_month_pool() {
        let f = factors_from_tallies(&uniform(100, 150));
        for slot in f {
            let slot = slot.unwrap();
            assert!((slot.k - 1.5).abs() < 1e-12);
            assert!((slot.lower - 1.5).abs() < 1e-12 && (slot.upper - 1.5).abs() < 1e-12);
            assert_eq!(slot.pool, PoolWidth::SameMonth);
        }
    }

    #[test]
    fn pool_widens_until_the_terra_minimum_is_met() {
        let width = |terra| factors_from_tallies(&uniform(terra, terra * 2))[5].map(|f| f.pool);
        assert_eq!(width(10), Some(PoolWidth::PlusMinus1));
        assert_eq!(width(5), Some(PoolWidth::PlusMinus2));
        assert_eq!(width(2), Some(PoolWidth::AllMonths));
        assert_eq!(width(1), None);
    }

    #[test]
    fn circular_pool_wraps_december_to_january() {
        assert_eq!(circular_month_distance(12, 1), 1);
        assert_eq!(circular_month_distance(1, 12), 1);
        assert_eq!(circular_month_distance(3, 9), 6);
    }

    #[test]
    fn trimming_drops_an_injected_outlier_month() {
        let mut t = uniform(100, 150);
        t.insert(
            (2010, 7),
            MonthTally {
                terra_visible: 100,
                modis_total: 1000,
            },
        );
        let f = factors_from_tallies(&t)[6].unwrap();
        assert!((f.k - 1.5).abs() < 1e-12, "k = {}", f.k);
        assert_eq!(f.months_used, 17);
        assert!((f.upper - 1.5).abs() < 1e-12);
    }

    #[test]
    fn small_pool_is_not_trimmed_and_band_is_min_max() {
        let mut t: BTreeMap<(i32, u32), MonthTally> = BTreeMap::new();
        for (i, total) in [100u64, 200, 300].into_iter().enumerate() {
            t.insert(
                (2003 + i as i32, 5),
                MonthTally {
                    terra_visible: 200,
                    modis_total: total,
                },
            );
        }
        let f = factors_from_tallies(&t)[4].unwrap();
        assert!((f.k - 1.0).abs() < 1e-12);
        assert!((f.lower - 0.5).abs() < 1e-12 && (f.upper - 1.5).abs() < 1e-12);
        assert_eq!(f.months_used, 3);
    }

    #[test]
    fn calibration_ignores_months_outside_the_era() {
        let mut t = uniform(100, 150);
        for ym in [(2002, 6), (2000, 12), (2021, 1), (2023, 7)] {
            t.insert(
                ym,
                MonthTally {
                    terra_visible: 1000,
                    modis_total: 5000,
                },
            );
        }
        let f = factors_from_tallies(&t);
        assert!(f.iter().all(|s| (s.unwrap().k - 1.5).abs() < 1e-12));
    }

    fn rec(year: i32, month: u32, platforms: Vec<Sensor>) -> HarmonizedHotspot {
        HarmonizedHotspot {
            lat: 39.0,
            lon: 32.0,
            frp: 20.0,
            frp_corrected: 20.0,
            frp_lower: 20.0,
            frp_upper: 20.0,
            vza_factor: None,
            n_obs: 1,
            platforms,
            confidence: Confidence::Nominal,
            source: HotspotSource::ModisOnly,
            acquired: Utc.with_ymd_and_hms(year, month, 10, 12, 0, 0).unwrap(),
            daynight: DayNight::Day,
        }
    }

    fn series_with_terra_only(extra: &[((i32, u32), usize)]) -> Vec<HarmonizedHotspot> {
        let mut out = Vec::new();
        for (y, m) in months_between(CALIBRATION_START, CALIBRATION_END) {
            for _ in 0..3 {
                out.push(rec(y, m, vec![Sensor::ModisTerra]));
            }
            out.push(rec(y, m, vec![Sensor::ModisAqua]));
        }
        for &((y, m), n) in extra {
            for _ in 0..n {
                out.push(rec(y, m, vec![Sensor::ModisTerra]));
            }
        }
        out
    }

    #[test]
    fn recovery_is_set_only_for_terra_only_months_and_keeps_k8_fields_none() {
        let hotspots = series_with_terra_only(&[((2000, 10), 5), ((2001, 1), 6), ((2002, 6), 9)]);
        let cells = build_calendar(&hotspots);
        let cell = |y, m| cells.iter().find(|c| c.year == y && c.month == m).unwrap();

        let jan = cell(2001, 1);
        assert!((jan.count_recovered.unwrap() - 8.0).abs() < 1e-9);
        assert!((cell(2002, 6).count_recovered.unwrap() - 12.0).abs() < 1e-9);
        assert!(jan.count_recovered_lower.unwrap() <= jan.count_recovered.unwrap());
        assert!(jan.count_recovered_upper.unwrap() >= jan.count_recovered.unwrap());
        for c in [jan, cell(2002, 6)] {
            assert_eq!(c.count_normalized, None);
            assert_eq!(c.effort_omega, None);
            assert_eq!(c.anomaly, None);
        }
        assert_eq!(cell(2000, 10).count_recovered, None);
        assert_eq!(cell(2003, 1).count_recovered, None);
        assert_eq!(cell(2002, 7).count_recovered, None);
    }

    #[test]
    fn series_entirely_before_aqua_gets_no_recovery() {
        let hotspots: Vec<HarmonizedHotspot> = (0..500)
            .map(|i| rec(2001, 1 + (i % 12) as u32, vec![Sensor::ModisTerra]))
            .collect();
        let cells = build_calendar(&hotspots);
        assert!(!cells.is_empty());
        assert!(cells.iter().all(|c| c.count_recovered.is_none()));
        assert!(cells.iter().all(|c| c.count_normalized.is_some()));
    }

    #[test]
    fn no_factor_means_no_recovery_even_for_terra_only_months() {
        let mut hotspots = vec![rec(2001, 1, vec![Sensor::ModisTerra]); 5];
        hotspots.push(rec(2010, 1, vec![Sensor::ModisTerra]));
        let cells = build_calendar(&hotspots);
        assert!(cells.iter().all(|c| c.count_recovered.is_none()));
    }

    #[test]
    fn percentile_interpolates_linearly() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((percentile(&v, 0.0) - 1.0).abs() < 1e-12);
        assert!((percentile(&v, 0.5) - 3.0).abs() < 1e-12);
        assert!((percentile(&v, 0.9) - 4.6).abs() < 1e-12);
    }
}
