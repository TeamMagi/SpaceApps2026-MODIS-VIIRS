use crate::models::{HarmonizedHotspot, Sensor};
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Minimum months sharing a platform configuration before its Ω is trusted.
const MIN_MONTHS_PER_CONFIG: usize = 3;

/// Observation effort for one platform configuration, measured against the MODIS channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffortFactor {
    pub has_modis: bool,
    pub viirs_platforms: Vec<Sensor>,
    pub platforms: Vec<Sensor>,
    pub months: usize,
    pub total: u64,
    pub modis_observable: u64,
    pub omega: Option<f64>,
}

type PoolingKey = (bool, Vec<Sensor>);

struct PoolAccumulator {
    platforms: BTreeSet<Sensor>,
    months: usize,
    total: u64,
    modis_observable: u64,
}

struct MonthBucket {
    platforms: BTreeSet<Sensor>,
    total: u64,
    modis_observable: u64,
}

// Pool by (has_modis, viirs_platforms) rather than exact sensor set: Ω is a VIIRS/MODIS ratio,
// so losing detections from one MODIS satellite in a low-activity month reflects fire activity
// rather than a fleet change and must not split the pool into a sub-threshold bucket.
fn pooling_key(platforms: &BTreeSet<Sensor>) -> PoolingKey {
    let has_modis = platforms.iter().any(|s| s.is_modis());
    let viirs_platforms = platforms.iter().copied().filter(|s| s.is_viirs()).collect();
    (has_modis, viirs_platforms)
}

fn aggregate_months(hotspots: &[HarmonizedHotspot]) -> HashMap<(i32, u32), MonthBucket> {
    let mut months: HashMap<(i32, u32), MonthBucket> = HashMap::new();

    for h in hotspots {
        let date = h.acquired.date_naive();
        let bucket = months
            .entry((date.year(), date.month()))
            .or_insert_with(|| MonthBucket {
                platforms: BTreeSet::new(),
                total: 0,
                modis_observable: 0,
            });
        bucket.total += 1;
        if h.platforms.iter().any(|s| s.is_modis()) {
            bucket.modis_observable += 1;
        }
        bucket.platforms.extend(h.platforms.iter().copied());
    }

    months
}

// Ω is derived from each series rather than hardcoded because the VIIRS/MODIS detection
// efficiency ratio varies more than 2x across regions (e.g. ~3.3 in Bulgaria vs ~7.3 in Syria).
// MODIS serves as the invariant reference channel because Terra/Aqua run across the full
// 2000–present archive with stable annual detection rates while the VIIRS fleet grows around them.
// Never inherit Ω from a superset configuration when a pool falls below MIN_MONTHS_PER_CONFIG:
// if a query starts shortly before a fleet transition (e.g. 2018-02 before NOAA-20), borrowing
// the post-transition Ω would over-divide pre-transition counts and create a reverse artificial jump.
fn factors_from_months(
    months: &HashMap<(i32, u32), MonthBucket>,
) -> (Vec<EffortFactor>, BTreeMap<PoolingKey, Option<f64>>) {
    let mut pooled: BTreeMap<PoolingKey, PoolAccumulator> = BTreeMap::new();

    for bucket in months.values() {
        let key = pooling_key(&bucket.platforms);
        let entry = pooled.entry(key).or_insert_with(|| PoolAccumulator {
            platforms: BTreeSet::new(),
            months: 0,
            total: 0,
            modis_observable: 0,
        });
        entry.platforms.extend(bucket.platforms.iter().copied());
        entry.months += 1;
        entry.total += bucket.total;
        entry.modis_observable += bucket.modis_observable;
    }

    let mut factors = Vec::with_capacity(pooled.len());
    let mut omega_by_config = BTreeMap::new();

    for ((has_modis, viirs_platforms), acc) in pooled {
        let omega = if acc.months >= MIN_MONTHS_PER_CONFIG && acc.modis_observable > 0 {
            let ratio = acc.total as f64 / acc.modis_observable as f64;
            ratio.is_finite().then_some(ratio)
        } else {
            None
        };
        omega_by_config.insert((has_modis, viirs_platforms.clone()), omega);
        factors.push(EffortFactor {
            has_modis,
            viirs_platforms,
            platforms: acc.platforms.into_iter().collect(),
            months: acc.months,
            total: acc.total,
            modis_observable: acc.modis_observable,
            omega,
        });
    }

    (factors, omega_by_config)
}

/// Estimate Ω per platform configuration from the harmonized series itself.
pub fn estimate_effort(hotspots: &[HarmonizedHotspot]) -> Vec<EffortFactor> {
    let months = aggregate_months(hotspots);
    factors_from_months(&months).0
}

/// Ω for the configuration that the given year/month belongs to.
pub fn omega_for_month(
    factors: &[EffortFactor],
    hotspots: &[HarmonizedHotspot],
    year: i32,
    month: u32,
) -> Option<f64> {
    let mut sensors = BTreeSet::new();
    let mut found = false;

    for h in hotspots {
        let date = h.acquired.date_naive();
        if date.year() == year && date.month() == month {
            found = true;
            sensors.extend(h.platforms.iter().copied());
        }
    }

    if !found {
        return None;
    }

    let (has_modis, viirs_platforms) = pooling_key(&sensors);
    factors
        .iter()
        .find(|f| f.has_modis == has_modis && f.viirs_platforms == viirs_platforms)
        .and_then(|f| f.omega)
}

pub(crate) fn monthly_omegas(hotspots: &[HarmonizedHotspot]) -> HashMap<(i32, u32), Option<f64>> {
    let months = aggregate_months(hotspots);
    let (_, omega_by_config) = factors_from_months(&months);

    months
        .into_iter()
        .map(|(ym, bucket)| {
            let key = pooling_key(&bucket.platforms);
            let omega = omega_by_config.get(&key).copied().flatten();
            (ym, omega)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, DayNight, HotspotSource};
    use chrono::{TimeZone, Utc};

    fn make_hotspot(year: i32, month: u32, platforms: Vec<Sensor>) -> HarmonizedHotspot {
        let has_modis = platforms.iter().any(|s| s.is_modis());
        let has_viirs = platforms.iter().any(|s| s.is_viirs());
        let source = match (has_modis, has_viirs) {
            (true, true) => HotspotSource::Matched,
            (true, false) => HotspotSource::ModisOnly,
            (false, true) | (false, false) => HotspotSource::ViirsOnly,
        };
        HarmonizedHotspot {
            lat: 39.0,
            lon: 32.0,
            frp: 30.0,
            frp_corrected: 30.0,
            vza_factor: None,
            n_obs: platforms.len() as u16,
            platforms,
            confidence: Confidence::Nominal,
            source,
            acquired: Utc.with_ymd_and_hms(year, month, 15, 12, 0, 0).unwrap(),
            daynight: DayNight::Day,
        }
    }

    #[test]
    fn modis_only_three_months_has_omega_one() {
        let hotspots = vec![
            make_hotspot(2010, 6, vec![Sensor::ModisTerra]),
            make_hotspot(2010, 6, vec![Sensor::ModisAqua]),
            make_hotspot(2010, 7, vec![Sensor::ModisTerra]),
            make_hotspot(2010, 7, vec![Sensor::ModisAqua]),
            make_hotspot(2010, 8, vec![Sensor::ModisTerra]),
            make_hotspot(2010, 8, vec![Sensor::ModisAqua]),
        ];

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert_eq!(factors[0].platforms, vec![Sensor::ModisTerra, Sensor::ModisAqua]);
        assert_eq!(factors[0].months, 3);
        assert_eq!(factors[0].total, 6);
        assert_eq!(factors[0].modis_observable, 6);
        assert_eq!(factors[0].omega, Some(1.0));
        assert_eq!(omega_for_month(&factors, &hotspots, 2010, 7), Some(1.0));
    }

    #[test]
    fn modis_plus_snpp_four_to_one_yields_omega_four() {
        let mut hotspots = Vec::new();
        for month in 6..=8 {
            hotspots.push(make_hotspot(2015, month, vec![Sensor::ModisTerra]));
            for _ in 0..3 {
                hotspots.push(make_hotspot(2015, month, vec![Sensor::ViirsSNPP]));
            }
        }

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert_eq!(factors[0].platforms, vec![Sensor::ModisTerra, Sensor::ViirsSNPP]);
        assert_eq!(factors[0].months, 3);
        assert_eq!(factors[0].total, 12);
        assert_eq!(factors[0].modis_observable, 3);
        assert_eq!(factors[0].omega, Some(4.0));
        assert_eq!(omega_for_month(&factors, &hotspots, 2015, 6), Some(4.0));
    }

    #[test]
    fn fewer_than_three_months_yields_none_omega() {
        let hotspots = vec![
            make_hotspot(2015, 6, vec![Sensor::ModisTerra]),
            make_hotspot(2015, 6, vec![Sensor::ViirsSNPP]),
            make_hotspot(2015, 7, vec![Sensor::ModisTerra]),
            make_hotspot(2015, 7, vec![Sensor::ViirsSNPP]),
        ];

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert_eq!(factors[0].months, 2);
        assert_eq!(factors[0].omega, None);
        assert_eq!(omega_for_month(&factors, &hotspots, 2015, 6), None);
    }

    #[test]
    fn zero_modis_observable_yields_none_without_panic() {
        let hotspots = vec![
            make_hotspot(2020, 6, vec![Sensor::ViirsSNPP]),
            make_hotspot(2020, 7, vec![Sensor::ViirsSNPP]),
            make_hotspot(2020, 8, vec![Sensor::ViirsSNPP]),
        ];

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert_eq!(factors[0].months, 3);
        assert_eq!(factors[0].total, 3);
        assert_eq!(factors[0].modis_observable, 0);
        assert_eq!(factors[0].omega, None);
        assert_eq!(omega_for_month(&factors, &hotspots, 2020, 6), None);
    }

    #[test]
    fn two_configurations_estimated_independently() {
        let mut hotspots = Vec::new();
        for month in 6..=8 {
            hotspots.push(make_hotspot(2010, month, vec![Sensor::ModisTerra]));
        }
        for month in 6..=8 {
            hotspots.push(make_hotspot(
                2015,
                month,
                vec![Sensor::ModisTerra, Sensor::ViirsSNPP],
            ));
            for _ in 0..3 {
                hotspots.push(make_hotspot(2015, month, vec![Sensor::ViirsSNPP]));
            }
        }

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 2);
        assert_eq!(factors[0].platforms, vec![Sensor::ModisTerra]);
        assert_eq!(factors[0].omega, Some(1.0));
        assert_eq!(factors[1].platforms, vec![Sensor::ModisTerra, Sensor::ViirsSNPP]);
        assert_eq!(factors[1].omega, Some(4.0));
        assert_eq!(omega_for_month(&factors, &hotspots, 2010, 7), Some(1.0));
        assert_eq!(omega_for_month(&factors, &hotspots, 2015, 7), Some(4.0));
        assert_eq!(omega_for_month(&factors, &hotspots, 2018, 1), None);
    }

    #[test]
    fn empty_input_returns_empty_without_panic() {
        let factors = estimate_effort(&[]);
        assert!(factors.is_empty());
        assert_eq!(omega_for_month(&factors, &[], 2020, 1), None);
    }

    #[test]
    fn estimate_effort_output_is_deterministically_ordered() {
        let mut hotspots = Vec::new();
        for month in 1..=3 {
            hotspots.push(make_hotspot(2023, month, vec![Sensor::ViirsNOAA21]));
            hotspots.push(make_hotspot(
                2021,
                month,
                vec![Sensor::ModisAqua, Sensor::ViirsSNPP, Sensor::ViirsNOAA20],
            ));
            hotspots.push(make_hotspot(2020, month, vec![Sensor::ModisTerra, Sensor::ViirsSNPP]));
            hotspots.push(make_hotspot(2010, month, vec![Sensor::ModisTerra]));
            hotspots.push(make_hotspot(2024, month, vec![Sensor::ViirsSNPP]));
        }

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 5);
        assert!(!factors[0].has_modis);
        assert_eq!(factors[0].viirs_platforms, vec![Sensor::ViirsSNPP]);
        assert!(!factors[1].has_modis);
        assert_eq!(factors[1].viirs_platforms, vec![Sensor::ViirsNOAA21]);
        assert!(factors[2].has_modis);
        assert!(factors[2].viirs_platforms.is_empty());
        assert!(factors[3].has_modis);
        assert_eq!(factors[3].viirs_platforms, vec![Sensor::ViirsSNPP]);
        assert!(factors[4].has_modis);
        assert_eq!(
            factors[4].viirs_platforms,
            vec![Sensor::ViirsSNPP, Sensor::ViirsNOAA20]
        );
        assert!(factors
            .windows(2)
            .all(|w| (w[0].has_modis, &w[0].viirs_platforms) < (w[1].has_modis, &w[1].viirs_platforms)));
    }

    #[test]
    fn missing_one_modis_satellite_in_low_activity_month_shares_pool_and_omega() {
        // 2018-10 and 2018-11 have Terra + Aqua + SNPP + NOAA20; 2018-12 drops Terra.
        let mut hotspots = Vec::new();
        for month in [10, 11] {
            hotspots.push(make_hotspot(2018, month, vec![Sensor::ModisTerra]));
            hotspots.push(make_hotspot(2018, month, vec![Sensor::ModisAqua]));
            for _ in 0..5 {
                hotspots.push(make_hotspot(2018, month, vec![Sensor::ViirsSNPP]));
                hotspots.push(make_hotspot(2018, month, vec![Sensor::ViirsNOAA20]));
            }
        }
        // 2018-12: only Aqua on MODIS, plus both VIIRS platforms.
        hotspots.push(make_hotspot(2018, 12, vec![Sensor::ModisAqua]));
        for _ in 0..5 {
            hotspots.push(make_hotspot(2018, 12, vec![Sensor::ViirsSNPP]));
        }
        for _ in 0..6 {
            hotspots.push(make_hotspot(2018, 12, vec![Sensor::ViirsNOAA20]));
        }

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert!(factors[0].has_modis);
        assert_eq!(
            factors[0].viirs_platforms,
            vec![Sensor::ViirsSNPP, Sensor::ViirsNOAA20]
        );
        assert_eq!(factors[0].months, 3);
        assert_eq!(factors[0].total, 36);
        assert_eq!(factors[0].modis_observable, 5);

        let expected_omega = 36.0 / 5.0;
        assert_eq!(factors[0].omega, Some(expected_omega));
        assert_eq!(omega_for_month(&factors, &hotspots, 2018, 10), Some(expected_omega));
        assert_eq!(omega_for_month(&factors, &hotspots, 2018, 11), Some(expected_omega));
        assert_eq!(omega_for_month(&factors, &hotspots, 2018, 12), Some(expected_omega));
    }

    #[test]
    fn adding_noaa20_preserves_separate_pool_and_distinct_omega() {
        let mut hotspots = Vec::new();
        // Period 1: (true, [ViirsSNPP]), total=3 per month, modis=1 -> omega = 3.0
        for month in 1..=3 {
            hotspots.push(make_hotspot(2018, month, vec![Sensor::ModisTerra]));
            hotspots.push(make_hotspot(2018, month, vec![Sensor::ViirsSNPP]));
            hotspots.push(make_hotspot(2018, month, vec![Sensor::ViirsSNPP]));
        }
        // Period 2: (true, [ViirsSNPP, ViirsNOAA20]), total=6 per month, modis=1 -> omega = 6.0
        for month in 5..=7 {
            hotspots.push(make_hotspot(2018, month, vec![Sensor::ModisAqua]));
            for _ in 0..3 {
                hotspots.push(make_hotspot(2018, month, vec![Sensor::ViirsSNPP]));
            }
            for _ in 0..2 {
                hotspots.push(make_hotspot(2018, month, vec![Sensor::ViirsNOAA20]));
            }
        }

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 2);
        assert_eq!(factors[0].viirs_platforms, vec![Sensor::ViirsSNPP]);
        assert_eq!(factors[0].omega, Some(3.0));
        assert_eq!(
            factors[1].viirs_platforms,
            vec![Sensor::ViirsSNPP, Sensor::ViirsNOAA20]
        );
        assert_eq!(factors[1].omega, Some(6.0));
        assert_ne!(factors[0].omega, factors[1].omega);
        assert_eq!(omega_for_month(&factors, &hotspots, 2018, 2), Some(3.0));
        assert_eq!(omega_for_month(&factors, &hotspots, 2018, 6), Some(6.0));
    }

    #[test]
    fn pool_without_modis_detections_returns_none_without_panic() {
        let hotspots = vec![
            make_hotspot(2022, 1, vec![Sensor::ViirsSNPP]),
            make_hotspot(2022, 1, vec![Sensor::ViirsNOAA20]),
            make_hotspot(2022, 2, vec![Sensor::ViirsSNPP]),
            make_hotspot(2022, 2, vec![Sensor::ViirsNOAA20]),
            make_hotspot(2022, 3, vec![Sensor::ViirsSNPP]),
            make_hotspot(2022, 3, vec![Sensor::ViirsNOAA20]),
        ];

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert!(!factors[0].has_modis);
        assert_eq!(
            factors[0].viirs_platforms,
            vec![Sensor::ViirsSNPP, Sensor::ViirsNOAA20]
        );
        assert_eq!(factors[0].months, 3);
        assert_eq!(factors[0].total, 6);
        assert_eq!(factors[0].modis_observable, 0);
        assert_eq!(factors[0].omega, None);
        assert_eq!(omega_for_month(&factors, &hotspots, 2022, 2), None);
        let omegas = monthly_omegas(&hotspots);
        assert_eq!(omegas.get(&(2022, 1)), Some(&None));
    }

    #[test]
    fn platforms_field_is_union_of_all_sensors_seen_in_pool() {
        let hotspots = vec![
            make_hotspot(2019, 1, vec![Sensor::ModisTerra]),
            make_hotspot(2019, 1, vec![Sensor::ModisAqua]),
            make_hotspot(2019, 1, vec![Sensor::ViirsSNPP]),
            make_hotspot(2019, 2, vec![Sensor::ModisAqua]),
            make_hotspot(2019, 2, vec![Sensor::ViirsSNPP]),
            make_hotspot(2019, 3, vec![Sensor::ModisAqua]),
            make_hotspot(2019, 3, vec![Sensor::ViirsSNPP]),
        ];

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert!(factors[0].has_modis);
        assert_eq!(factors[0].viirs_platforms, vec![Sensor::ViirsSNPP]);
        assert_eq!(
            factors[0].platforms,
            vec![Sensor::ModisTerra, Sensor::ModisAqua, Sensor::ViirsSNPP]
        );
    }
}
