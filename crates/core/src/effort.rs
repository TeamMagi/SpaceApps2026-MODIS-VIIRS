use crate::models::{HarmonizedHotspot, Sensor};
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Minimum months sharing a platform configuration before its Ω is trusted.
const MIN_MONTHS_PER_CONFIG: usize = 3;

/// Observation effort for one platform configuration, measured against the MODIS channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffortFactor {
    pub platforms: Vec<Sensor>,
    pub months: usize,
    pub total: u64,
    pub modis_observable: u64,
    pub omega: Option<f64>,
}

struct MonthBucket {
    platforms: BTreeSet<Sensor>,
    total: u64,
    modis_observable: u64,
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
fn factors_from_months(
    months: &HashMap<(i32, u32), MonthBucket>,
) -> (Vec<EffortFactor>, BTreeMap<Vec<Sensor>, Option<f64>>) {
    let mut pooled: BTreeMap<Vec<Sensor>, (usize, u64, u64)> = BTreeMap::new();

    for bucket in months.values() {
        let key: Vec<Sensor> = bucket.platforms.iter().copied().collect();
        let entry = pooled.entry(key).or_insert((0, 0, 0));
        entry.0 += 1;
        entry.1 += bucket.total;
        entry.2 += bucket.modis_observable;
    }

    let mut factors = Vec::with_capacity(pooled.len());
    let mut omega_by_config = BTreeMap::new();

    for (platforms, (month_count, total, modis_observable)) in pooled {
        let omega = if month_count >= MIN_MONTHS_PER_CONFIG && modis_observable > 0 {
            let ratio = total as f64 / modis_observable as f64;
            ratio.is_finite().then_some(ratio)
        } else {
            None
        };
        omega_by_config.insert(platforms.clone(), omega);
        factors.push(EffortFactor {
            platforms,
            months: month_count,
            total,
            modis_observable,
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

    let config: Vec<Sensor> = sensors.into_iter().collect();
    factors
        .iter()
        .find(|f| f.platforms == config)
        .and_then(|f| f.omega)
}

pub(crate) fn monthly_omegas(hotspots: &[HarmonizedHotspot]) -> HashMap<(i32, u32), Option<f64>> {
    let months = aggregate_months(hotspots);
    let (_, omega_by_config) = factors_from_months(&months);

    months
        .into_iter()
        .map(|(ym, bucket)| {
            let key: Vec<Sensor> = bucket.platforms.into_iter().collect();
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
            hotspots.push(make_hotspot(2020, month, vec![Sensor::ModisTerra, Sensor::ViirsSNPP]));
            hotspots.push(make_hotspot(2010, month, vec![Sensor::ModisTerra]));
        }

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 3);
        assert_eq!(factors[0].platforms, vec![Sensor::ModisTerra]);
        assert_eq!(
            factors[1].platforms,
            vec![Sensor::ModisTerra, Sensor::ViirsSNPP]
        );
        assert_eq!(factors[2].platforms, vec![Sensor::ViirsNOAA21]);
        assert!(factors.windows(2).all(|w| w[0].platforms < w[1].platforms));
    }
}
