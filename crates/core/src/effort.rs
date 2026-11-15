use crate::models::{DayNight, HarmonizedHotspot, Sensor};
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Minimum months sharing a platform configuration before its Ω is trusted.
const MIN_MONTHS_PER_CONFIG: usize = 3;

/// MODIS scan-edge detection limit ([16]): above this threshold detection probability no longer
/// depends on where in the swath the fire falls, removing the VIIRS small-fire sensitivity cliff.
pub const HIGH_INTENSITY_FRP_MW: f64 = 40.0;

/// Minimum MODIS-observable high-intensity sample count before computing a regime ratio;
/// ratios from tiny counts are noise and worse than reporting `None`.
const MIN_HIGH_INTENSITY_SAMPLE: u64 = 30;

/// First month with Terra and Aqua both flying (year, month). Aqua's first FIRMS detections are
/// in 2002-07; before that the MODIS reference channel itself had half the platforms, a change
/// in effort that Ω cannot measure by construction because Ω is defined relative to that channel.
pub const FIRST_TERRA_AQUA_MONTH: (i32, u32) = (2002, 7);

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
    pub total_high_intensity: u64,
    pub modis_observable_high_intensity: u64,
    pub omega_high_intensity: Option<f64>,
    /// Diagnostic of the diurnal/swath difference (~1.2x between S-NPP and Aqua, [16] §5.4);
    /// reported separately and not applied to any count.
    pub snpp_aqua_high_intensity_ratio: Option<f64>,
}

type PoolingKey = (bool, Vec<Sensor>);

struct PoolAccumulator {
    platforms: BTreeSet<Sensor>,
    months: usize,
    total: u64,
    modis_observable: u64,
    total_high: u64,
    modis_observable_high: u64,
    snpp_day_high: u64,
    aqua_day_high: u64,
}

struct MonthBucket {
    platforms: BTreeSet<Sensor>,
    total: u64,
    modis_observable: u64,
    total_high: u64,
    modis_observable_high: u64,
    snpp_day_high: u64,
    aqua_day_high: u64,
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
                total_high: 0,
                modis_observable_high: 0,
                snpp_day_high: 0,
                aqua_day_high: 0,
            });
        bucket.total += 1;
        let has_modis = h.platforms.iter().any(|s| s.is_modis());
        if has_modis {
            bucket.modis_observable += 1;
        }
        if h.frp.is_finite() && h.frp >= HIGH_INTENSITY_FRP_MW {
            bucket.total_high += 1;
            if has_modis {
                bucket.modis_observable_high += 1;
            }
            if h.daynight == DayNight::Day {
                if h.platforms.contains(&Sensor::ViirsSNPP) {
                    bucket.snpp_day_high += 1;
                }
                if h.platforms.contains(&Sensor::ModisAqua) {
                    bucket.aqua_day_high += 1;
                }
            }
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
            total_high: 0,
            modis_observable_high: 0,
            snpp_day_high: 0,
            aqua_day_high: 0,
        });
        entry.platforms.extend(bucket.platforms.iter().copied());
        entry.months += 1;
        entry.total += bucket.total;
        entry.modis_observable += bucket.modis_observable;
        entry.total_high += bucket.total_high;
        entry.modis_observable_high += bucket.modis_observable_high;
        entry.snpp_day_high += bucket.snpp_day_high;
        entry.aqua_day_high += bucket.aqua_day_high;
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
        let omega_high_intensity = if acc.months >= MIN_MONTHS_PER_CONFIG
            && acc.modis_observable_high >= MIN_HIGH_INTENSITY_SAMPLE
        {
            let ratio = acc.total_high as f64 / acc.modis_observable_high as f64;
            ratio.is_finite().then_some(ratio)
        } else {
            None
        };
        let snpp_aqua_high_intensity_ratio = if viirs_platforms.contains(&Sensor::ViirsSNPP)
            && acc.aqua_day_high >= MIN_HIGH_INTENSITY_SAMPLE
        {
            let ratio = acc.snpp_day_high as f64 / acc.aqua_day_high as f64;
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
            total_high_intensity: acc.total_high,
            modis_observable_high_intensity: acc.modis_observable_high,
            omega_high_intensity,
            snpp_aqua_high_intensity_ratio,
        });
    }

    (factors, omega_by_config)
}

// Terra-only months are only excluded when the series also reaches the Terra+Aqua era; a series
// that lies wholly before it is internally comparable and stays untouched.
fn take_pre_aqua_months(months: &mut HashMap<(i32, u32), MonthBucket>) -> Vec<(i32, u32)> {
    if !months.keys().any(|&ym| ym >= FIRST_TERRA_AQUA_MONTH) {
        return Vec::new();
    }
    let mut excluded: Vec<(i32, u32)> = months
        .keys()
        .copied()
        .filter(|&ym| ym < FIRST_TERRA_AQUA_MONTH)
        .collect();
    for ym in &excluded {
        months.remove(ym);
    }
    excluded.sort_unstable();
    excluded
}

/// Months (year, month, ascending) that are not effort-comparable because they precede
/// Aqua (2002-07) while the series also contains later months.
pub fn pre_aqua_excluded_months(hotspots: &[HarmonizedHotspot]) -> Vec<(i32, u32)> {
    take_pre_aqua_months(&mut aggregate_months(hotspots))
}

/// Estimate Ω per platform configuration from the harmonized series itself.
pub fn estimate_effort(hotspots: &[HarmonizedHotspot]) -> Vec<EffortFactor> {
    let mut months = aggregate_months(hotspots);
    take_pre_aqua_months(&mut months);
    factors_from_months(&months).0
}

/// Divisor for monthly FRP on a single-VIIRS energy ruler; `None` when unavailable or inapplicable.
pub fn multi_viirs_frp_scale(factors: &[EffortFactor], platforms: &[Sensor]) -> Option<f64> {
    let sensors = platforms.iter().copied().collect();
    let (has_modis, viirs_platforms) = pooling_key(&sensors);
    if !has_modis || viirs_platforms.len() < 2 {
        return None;
    }
    let omega_for = |viirs: &[Sensor]| {
        factors
            .iter()
            .find(|f| f.has_modis && f.viirs_platforms == viirs)
            .and_then(|f| f.omega)
    };
    let omega_pool = omega_for(&viirs_platforms)?;
    // S-NPP anchors the energy ruler to the first VIIRS era as the fleet grows.
    let omega_ref =
        omega_for(&[Sensor::ViirsSNPP]).or_else(|| omega_for(&[Sensor::ViirsNOAA20]))?;
    (omega_pool.is_finite() && omega_pool > 0.0 && omega_ref.is_finite() && omega_ref > 0.0)
        .then_some(omega_pool / omega_ref)
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

    if !found || pre_aqua_excluded_months(hotspots).contains(&(year, month)) {
        return None;
    }

    let (has_modis, viirs_platforms) = pooling_key(&sensors);
    factors
        .iter()
        .find(|f| f.has_modis == has_modis && f.viirs_platforms == viirs_platforms)
        .and_then(|f| f.omega)
}

/// Half-width (degrees) of the latitude belt where MODIS leaves equatorial swath gaps.
pub const TROPICAL_BELT_ABS_LAT: f64 = 30.0;
/// Gap between adjacent MODIS orbits at the equator [14, Sec. 4.4].
pub const MODIS_SWATH_GAP_KM: f64 = 388.1;
/// Share of VIIRS FRP that falls in MODIS swath gaps: 10.7% annual, 12.5% monthly [14, Sec. 4.4].
pub const SWATH_GAP_MISSED_FRP_LOW_PCT: f64 = 10.7;
pub const SWATH_GAP_MISSED_FRP_HIGH_PCT: f64 = 12.5;
pub const SWATH_GAP_REFERENCE: &str = "[14] Biomass burning in Africa: FRP missed by MODIS (2020), Sec. 4.4: equatorial MODIS swath gaps (388.1 km) miss 10.7% (annual) to 12.5% (monthly) of VIIRS FRP";
const SWATH_GAP_NOTE: &str = "Between 30S and 30N adjacent MODIS orbits leave equatorial swath gaps (388.1 km) that miss roughly 10.7-12.5% of monthly VIIRS FRP. Months with VIIRS are unaffected (VIIRS-only detections fill the gaps) and effort_omega already counts them; pre-2012 MODIS-only total_frp is left uncorrected unless compensation is enabled.";

/// What the belt share of a [`TropicalSwathGap`] was measured on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SwathGapBasis {
    /// The query bounding box.
    Bbox,
    /// The latitude extent of the detections themselves (no bounding box was given).
    Detections,
}

/// Multiplier that restores the swath-gap loss in VIIRS-less tropical totals, with its 1σ.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SwathGapCompensation {
    pub factor: f64,
    pub factor_sigma: f64,
}

/// `1 / (1 - f)` at the midpoint of the documented missed share, with half the range
/// `[1 / (1 - 0.107), 1 / (1 - 0.125)]` as 1σ, so the 10.7-12.5% spread becomes uncertainty.
pub fn swath_gap_compensation() -> SwathGapCompensation {
    let low_share = SWATH_GAP_MISSED_FRP_LOW_PCT / 100.0;
    let high_share = SWATH_GAP_MISSED_FRP_HIGH_PCT / 100.0;
    let low = 1.0 / (1.0 - low_share);
    let high = 1.0 / (1.0 - high_share);
    let mid = 0.5 * (low_share + high_share);
    SwathGapCompensation {
        factor: 1.0 / (1.0 - mid),
        factor_sigma: 0.5 * (high - low),
    }
}

/// Inclusive belt test used per detection: `|lat| <= 30°`.
pub fn in_tropical_belt(lat: f64) -> bool {
    lat.is_finite() && lat.abs() <= TROPICAL_BELT_ABS_LAT
}

/// Area-weighted share of the latitude span `south..=north` that lies inside 30°S-30°N.
///
/// On a sphere the area between two parallels is proportional to `sin(north) - sin(south)`
/// whatever the longitude width, so the share is the clipped span over the full span in sine
/// space; a region straddling the belt edge therefore counts by area, not by degrees. A
/// zero-height span (a single latitude) is 1 inside the belt (edges included) and 0 outside.
/// Non-finite or inverted spans return 0.
pub fn tropical_belt_share(south: f64, north: f64) -> f64 {
    if !south.is_finite() || !north.is_finite() || south > north {
        return 0.0;
    }
    let south = south.clamp(-90.0, 90.0);
    let north = north.clamp(-90.0, 90.0);
    if south == north {
        return if in_tropical_belt(south) { 1.0 } else { 0.0 };
    }
    let lo = south.max(-TROPICAL_BELT_ABS_LAT);
    let hi = north.min(TROPICAL_BELT_ABS_LAT);
    if hi <= lo {
        return 0.0;
    }
    let sin = |deg: f64| deg.to_radians().sin();
    let denom = sin(north) - sin(south);
    if !denom.is_finite() || denom == 0.0 {
        return ((hi - lo) / (north - south)).clamp(0.0, 1.0);
    }
    ((sin(hi) - sin(lo)) / denom).clamp(0.0, 1.0)
}

/// `tropical_swath_gap` flag carried by calendar and effort metadata.
///
/// Only describes the MODIS limitation. Whether the loss is compensated is set by
/// `UncertaintyParams::tropical_swath_gap_compensation` and applied in
/// [`crate::calendar::build_calendar_with`]; Ω never compensates it (see [`flag_tropical_swath_gap`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TropicalSwathGap {
    pub belt_abs_lat: f64,
    /// Area-weighted share of the region inside the belt, `0 < share <= 1`.
    pub belt_share: f64,
    pub basis: SwathGapBasis,
    pub swath_gap_km: f64,
    pub missed_frp_low_pct: f64,
    pub missed_frp_high_pct: f64,
    /// `Some` when compensation is enabled; it then scales VIIRS-less in-belt `total_frp` once.
    pub compensation: Option<SwathGapCompensation>,
    pub note: String,
    pub reference: String,
}

/// Builds the flag for a region spanning `south..=north`; `None` when it has no area in the belt.
///
/// Double counting: Ω is `total / modis_observable`, and in VIIRS-era pools `total` already
/// contains the VIIRS-only detections that fall in MODIS gaps, so the count normalization
/// expresses every month in MODIS-observable units, which miss the gaps by construction in both
/// eras. Compensating counts would therefore break that common reference; the flag never changes
/// Ω or `count_normalized`, only (optionally) VIIRS-less `total_frp`.
pub fn flag_tropical_swath_gap(
    south: f64,
    north: f64,
    basis: SwathGapBasis,
    compensate: bool,
) -> Option<TropicalSwathGap> {
    let share = tropical_belt_share(south, north);
    (share > 0.0).then(|| TropicalSwathGap {
        belt_abs_lat: TROPICAL_BELT_ABS_LAT,
        belt_share: share,
        basis,
        swath_gap_km: MODIS_SWATH_GAP_KM,
        missed_frp_low_pct: SWATH_GAP_MISSED_FRP_LOW_PCT,
        missed_frp_high_pct: SWATH_GAP_MISSED_FRP_HIGH_PCT,
        compensation: compensate.then(swath_gap_compensation),
        note: SWATH_GAP_NOTE.to_string(),
        reference: SWATH_GAP_REFERENCE.to_string(),
    })
}

pub(crate) fn monthly_omegas(hotspots: &[HarmonizedHotspot]) -> HashMap<(i32, u32), Option<f64>> {
    let mut months = aggregate_months(hotspots);
    take_pre_aqua_months(&mut months);
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

    fn make_hotspot_with(
        year: i32,
        month: u32,
        platforms: Vec<Sensor>,
        frp: f64,
        daynight: DayNight,
    ) -> HarmonizedHotspot {
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
            frp,
            frp_corrected: frp,
            frp_lower: frp * 0.92,
            frp_upper: frp * 1.08,
            vza_factor: None,
            n_obs: platforms.len() as u16,
            platforms,
            confidence: Confidence::Nominal,
            source,
            acquired: Utc.with_ymd_and_hms(year, month, 15, 12, 0, 0).unwrap(),
            daynight,
        }
    }

    fn make_hotspot(year: i32, month: u32, platforms: Vec<Sensor>) -> HarmonizedHotspot {
        make_hotspot_with(year, month, platforms, 30.0, DayNight::Day)
    }

    fn ruler_factors() -> Vec<EffortFactor> {
        let mut hotspots = Vec::new();
        for month in 1..=3 {
            for (year, viirs) in [
                (2015, vec![Sensor::ViirsSNPP, Sensor::ViirsSNPP]),
                (2018, vec![Sensor::ViirsNOAA20]),
                (
                    2020,
                    vec![Sensor::ViirsSNPP, Sensor::ViirsNOAA20, Sensor::ViirsNOAA21],
                ),
            ] {
                hotspots.push(make_hotspot(year, month, vec![Sensor::ModisAqua]));
                for sensor in viirs {
                    hotspots.push(make_hotspot(year, month, vec![sensor]));
                }
            }
        }
        estimate_effort(&hotspots)
    }

    #[test]
    fn multi_viirs_frp_scale_prefers_snpp_and_falls_back_to_noaa20() {
        let mut factors = ruler_factors();
        let platforms = [
            Sensor::ViirsNOAA21,
            Sensor::ViirsNOAA20,
            Sensor::ModisTerra,
            Sensor::ViirsSNPP,
        ];
        assert_eq!(multi_viirs_frp_scale(&factors, &platforms), Some(4.0 / 3.0));
        factors
            .iter_mut()
            .find(|f| f.viirs_platforms == [Sensor::ViirsSNPP])
            .unwrap()
            .omega = None;
        assert_eq!(multi_viirs_frp_scale(&factors, &platforms), Some(2.0));
        factors.retain(|f| f.viirs_platforms != [Sensor::ViirsSNPP]);
        assert_eq!(multi_viirs_frp_scale(&factors, &platforms), Some(2.0));
        factors.retain(|f| f.viirs_platforms != [Sensor::ViirsNOAA20]);
        assert_eq!(multi_viirs_frp_scale(&factors, &platforms), None);
    }

    #[test]
    fn multi_viirs_frp_scale_requires_modis_and_multiple_viirs() {
        let factors = ruler_factors();
        for platforms in [
            vec![],
            vec![Sensor::ModisTerra],
            vec![Sensor::ModisAqua, Sensor::ViirsSNPP],
            vec![Sensor::ViirsSNPP, Sensor::ViirsNOAA20, Sensor::ViirsNOAA21],
            vec![Sensor::ModisAqua, Sensor::ViirsSNPP, Sensor::ViirsNOAA20],
        ] {
            assert_eq!(multi_viirs_frp_scale(&factors, &platforms), None);
        }
        let mut factors = factors;
        factors
            .iter_mut()
            .find(|f| f.viirs_platforms == [Sensor::ViirsSNPP])
            .unwrap()
            .has_modis = false;
        factors
            .iter_mut()
            .find(|f| f.viirs_platforms == [Sensor::ViirsNOAA20])
            .unwrap()
            .has_modis = false;
        assert_eq!(
            multi_viirs_frp_scale(
                &factors,
                &[
                    Sensor::ModisAqua,
                    Sensor::ViirsSNPP,
                    Sensor::ViirsNOAA20,
                    Sensor::ViirsNOAA21,
                ]
            ),
            None
        );
    }

    #[test]
    fn multi_viirs_frp_scale_rejects_invalid_omegas_without_clamping() {
        let platforms = [
            Sensor::ModisAqua,
            Sensor::ViirsSNPP,
            Sensor::ViirsNOAA20,
            Sensor::ViirsNOAA21,
        ];
        for invalid in [
            None,
            Some(0.0),
            Some(-1.0),
            Some(f64::NAN),
            Some(f64::INFINITY),
            Some(f64::NEG_INFINITY),
        ] {
            for reference in [false, true] {
                let mut factors = ruler_factors();
                factors.retain(|f| f.viirs_platforms != [Sensor::ViirsNOAA20]);
                factors
                    .iter_mut()
                    .find(|f| (f.viirs_platforms.len() == 1) == reference)
                    .unwrap()
                    .omega = invalid;
                assert_eq!(multi_viirs_frp_scale(&factors, &platforms), None);
            }
        }
        let mut factors = ruler_factors();
        factors
            .iter_mut()
            .find(|f| f.viirs_platforms.len() == 3)
            .unwrap()
            .omega = Some(1.5);
        assert_eq!(multi_viirs_frp_scale(&factors, &platforms), Some(0.5));
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

    #[test]
    fn pre_aqua_months_are_excluded_when_series_crosses_boundary() {
        let mut hotspots = Vec::new();
        for year in 2001..=2003 {
            for month in 1..=12 {
                hotspots.push(make_hotspot(year, month, vec![Sensor::ModisTerra]));
            }
        }
        let excluded = pre_aqua_excluded_months(&hotspots);
        assert_eq!(excluded.len(), 18);
        assert_eq!(excluded.first(), Some(&(2001, 1)));
        assert_eq!(excluded.last(), Some(&(2002, 6)));

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert_eq!(factors[0].months, 18);
        assert_eq!(omega_for_month(&factors, &hotspots, 2002, 6), None);
        assert_eq!(omega_for_month(&factors, &hotspots, 2002, 7), Some(1.0));
        assert_eq!(monthly_omegas(&hotspots).get(&(2001, 5)), None);
    }

    #[test]
    fn belt_share_edges_at_plus_minus_30_degrees() {
        // Touching the belt from outside has no area in it.
        assert_eq!(tropical_belt_share(30.0, 40.0), 0.0);
        assert_eq!(tropical_belt_share(-45.0, -30.0), 0.0);
        // Touching it from inside is wholly in it.
        assert_eq!(tropical_belt_share(20.0, 30.0), 1.0);
        assert_eq!(tropical_belt_share(-30.0, -20.0), 1.0);
        assert_eq!(tropical_belt_share(-30.0, 30.0), 1.0);
        // Just across the edge gives a small positive share.
        let sliver = tropical_belt_share(29.9, 40.0);
        assert!(sliver > 0.0 && sliver < 0.02, "{sliver}");
        // Single-latitude spans are inclusive at ±30°.
        assert_eq!(tropical_belt_share(30.0, 30.0), 1.0);
        assert_eq!(tropical_belt_share(-30.0, -30.0), 1.0);
        assert_eq!(tropical_belt_share(30.0001, 30.0001), 0.0);
        assert!(flag_tropical_swath_gap(30.0, 40.0, SwathGapBasis::Bbox, false).is_none());
        assert!(flag_tropical_swath_gap(29.9, 40.0, SwathGapBasis::Bbox, false).is_some());
    }

    #[test]
    fn partial_overlap_is_area_weighted_not_degree_weighted() {
        // 20..40: half the degrees are in the belt, but sin-weighted area gives more than half,
        // because parallels nearer the equator enclose more area.
        let share = tropical_belt_share(20.0, 40.0);
        let sin = |d: f64| d.to_radians().sin();
        let expected = (sin(30.0) - sin(20.0)) / (sin(40.0) - sin(20.0));
        assert!((share - expected).abs() < 1e-12);
        assert!(share > 0.5);
        // Symmetric across the equator, and spans reaching past both edges.
        assert!((tropical_belt_share(-40.0, -20.0) - share).abs() < 1e-12);
        let wide = tropical_belt_share(-60.0, 60.0);
        assert!((wide - sin(30.0) / sin(60.0)).abs() < 1e-12);
    }

    #[test]
    fn non_tropical_or_invalid_regions_have_no_flag() {
        for (s, n) in [(36.0, 42.0), (-55.0, -35.0), (60.0, 80.0)] {
            assert_eq!(tropical_belt_share(s, n), 0.0);
            assert!(flag_tropical_swath_gap(s, n, SwathGapBasis::Bbox, true).is_none());
        }
        for (s, n) in [(f64::NAN, 10.0), (0.0, f64::INFINITY), (10.0, -10.0)] {
            assert_eq!(tropical_belt_share(s, n), 0.0);
        }
        assert!(!in_tropical_belt(f64::NAN));
    }

    #[test]
    fn flag_reports_documented_range_and_compensation_only_when_enabled() {
        let off = flag_tropical_swath_gap(-34.0, 6.0, SwathGapBasis::Bbox, false).unwrap();
        assert_eq!(off.belt_abs_lat, 30.0);
        assert_eq!(off.swath_gap_km, 388.1);
        assert_eq!((off.missed_frp_low_pct, off.missed_frp_high_pct), (10.7, 12.5));
        assert_eq!(off.compensation, None);
        assert!(off.reference.contains("[14]"));

        let on = flag_tropical_swath_gap(-34.0, 6.0, SwathGapBasis::Bbox, true).unwrap();
        let c = on.compensation.unwrap();
        assert!((c.factor - 1.0 / (1.0 - 0.116)).abs() < 1e-12);
        let low = 1.0 / (1.0 - 0.107);
        let high = 1.0 / (1.0 - 0.125);
        assert!(low < c.factor && c.factor < high);
        assert!((c.factor_sigma - 0.5 * (high - low)).abs() < 1e-12);
    }

    #[test]
    fn omega_has_no_latitude_term_so_the_swath_gap_is_never_compensated_in_counts() {
        // Same platform mix in the tropics and at mid-latitude must give the same Ω.
        let build = |lat: f64| {
            let mut hotspots = Vec::new();
            for month in 6..=8 {
                hotspots.push(make_hotspot(2015, month, vec![Sensor::ModisTerra]));
                for _ in 0..3 {
                    hotspots.push(make_hotspot(2015, month, vec![Sensor::ViirsSNPP]));
                }
            }
            for h in &mut hotspots {
                h.lat = lat;
            }
            estimate_effort(&hotspots)
        };
        let tropics = build(5.0);
        let mid = build(39.0);
        assert_eq!(tropics[0].omega, Some(4.0));
        assert_eq!(tropics[0].omega, mid[0].omega);
    }

    #[test]
    fn series_entirely_before_aqua_is_unchanged() {
        let hotspots: Vec<_> = (1..=4)
            .map(|m| make_hotspot(2001, m, vec![Sensor::ModisTerra]))
            .collect();
        assert!(pre_aqua_excluded_months(&hotspots).is_empty());
        let factors = estimate_effort(&hotspots);
        assert_eq!(factors[0].months, 4);
        assert_eq!(omega_for_month(&factors, &hotspots, 2001, 2), Some(1.0));
    }

    #[test]
    fn high_intensity_threshold_matches_modis_edge_detection_limit() {
        let coeffs = crate::vza::detection_limit_coefficients(Sensor::ModisTerra, DayNight::Day);
        let edge_limit = crate::vza::detection_limit_mw(&coeffs, 9.66).unwrap();
        assert!(
            (edge_limit - HIGH_INTENSITY_FRP_MW).abs() < 5.0,
            "MODIS scan-edge limit {edge_limit} MW should be within a few MW of {HIGH_INTENSITY_FRP_MW} MW"
        );
    }

    #[test]
    fn small_fire_cliff_inflates_omega_but_not_high_intensity_omega() {
        let mut hotspots = Vec::new();
        for month in 1..=3 {
            for _ in 0..100 {
                hotspots.push(make_hotspot_with(2015, month, vec![Sensor::ViirsSNPP], 5.0, DayNight::Day));
            }
            for _ in 0..10 {
                hotspots.push(make_hotspot_with(2015, month, vec![Sensor::ModisAqua], 50.0, DayNight::Day));
                hotspots.push(make_hotspot_with(2015, month, vec![Sensor::ViirsSNPP], 50.0, DayNight::Day));
            }
        }

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert_eq!(factors[0].omega, Some(12.0));
        assert_eq!(factors[0].total_high_intensity, 60);
        assert_eq!(factors[0].modis_observable_high_intensity, 30);
        assert_eq!(factors[0].omega_high_intensity, Some(2.0));
    }

    #[test]
    fn below_min_high_intensity_sample_yields_none_regime_omega() {
        let mut hotspots = Vec::new();
        for month in 1..=3 {
            for _ in 0..5 {
                hotspots.push(make_hotspot_with(2016, month, vec![Sensor::ModisAqua], 20.0, DayNight::Day));
            }
            for _ in 0..9 {
                hotspots.push(make_hotspot_with(2016, month, vec![Sensor::ModisAqua], 50.0, DayNight::Day));
                hotspots.push(make_hotspot_with(2016, month, vec![Sensor::ViirsSNPP], 50.0, DayNight::Day));
            }
        }

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert!(factors[0].omega.is_some());
        assert_eq!(factors[0].modis_observable_high_intensity, 27);
        assert_eq!(factors[0].omega_high_intensity, None);
    }

    #[test]
    fn modis_only_high_intensity_pool_has_omega_one() {
        let mut hotspots = Vec::new();
        for month in 1..=3 {
            for _ in 0..10 {
                hotspots.push(make_hotspot_with(2010, month, vec![Sensor::ModisTerra], 50.0, DayNight::Day));
            }
        }

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert_eq!(factors[0].total_high_intensity, 30);
        assert_eq!(factors[0].modis_observable_high_intensity, 30);
        assert_eq!(factors[0].omega_high_intensity, Some(1.0));
        assert_eq!(factors[0].snpp_aqua_high_intensity_ratio, None);
    }

    #[test]
    fn high_intensity_threshold_boundary_and_nan_handling() {
        let mut hotspots = Vec::new();
        for month in 1..=3 {
            for _ in 0..10 {
                hotspots.push(make_hotspot_with(2018, month, vec![Sensor::ModisTerra], 40.0, DayNight::Day));
                hotspots.push(make_hotspot_with(2018, month, vec![Sensor::ViirsSNPP], 40.0, DayNight::Day));
                hotspots.push(make_hotspot_with(2018, month, vec![Sensor::ModisTerra], 39.999, DayNight::Day));
            }
            for _ in 0..5 {
                hotspots.push(make_hotspot_with(2018, month, vec![Sensor::ModisTerra], f64::NAN, DayNight::Day));
                hotspots.push(make_hotspot_with(2018, month, vec![Sensor::ViirsSNPP], f64::NAN, DayNight::Day));
            }
        }

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert_eq!(factors[0].total, 120);
        assert_eq!(factors[0].modis_observable, 75);
        assert_eq!(factors[0].total_high_intensity, 60);
        assert_eq!(factors[0].modis_observable_high_intensity, 30);
        assert_eq!(factors[0].omega_high_intensity, Some(2.0));
    }

    #[test]
    fn pre_aqua_exclusion_applies_to_high_intensity_pools() {
        let mut hotspots = Vec::new();
        for month in [5, 6] {
            for _ in 0..20 {
                hotspots.push(make_hotspot_with(2002, month, vec![Sensor::ModisTerra], 50.0, DayNight::Day));
            }
        }
        for month in 7..=9 {
            for _ in 0..10 {
                hotspots.push(make_hotspot_with(2002, month, vec![Sensor::ModisTerra], 50.0, DayNight::Day));
                hotspots.push(make_hotspot_with(2002, month, vec![Sensor::ModisAqua], 50.0, DayNight::Day));
            }
        }

        let factors = estimate_effort(&hotspots);
        assert_eq!(factors.len(), 1);
        assert_eq!(factors[0].months, 3);
        assert_eq!(factors[0].total_high_intensity, 60);
        assert_eq!(factors[0].modis_observable_high_intensity, 60);
        assert_eq!(factors[0].omega_high_intensity, Some(1.0));
    }

    #[test]
    fn snpp_aqua_high_intensity_ratio_daytime_only_and_requires_snpp() {
        let mut hotspots = Vec::new();
        for month in 1..=3 {
            for _ in 0..20 {
                hotspots.push(make_hotspot_with(2015, month, vec![Sensor::ModisAqua], 50.0, DayNight::Day));
            }
            for _ in 0..24 {
                hotspots.push(make_hotspot_with(2015, month, vec![Sensor::ViirsSNPP], 50.0, DayNight::Day));
            }
            for _ in 0..15 {
                hotspots.push(make_hotspot_with(2015, month, vec![Sensor::ModisAqua], 50.0, DayNight::Night));
                hotspots.push(make_hotspot_with(2015, month, vec![Sensor::ViirsSNPP], 50.0, DayNight::Night));
            }
            for _ in 0..10 {
                hotspots.push(make_hotspot_with(2015, month, vec![Sensor::ModisAqua], 20.0, DayNight::Day));
            }
        }
        for month in 4..=6 {
            for _ in 0..20 {
                hotspots.push(make_hotspot_with(2015, month, vec![Sensor::ModisAqua], 50.0, DayNight::Day));
                hotspots.push(make_hotspot_with(2015, month, vec![Sensor::ViirsNOAA20], 50.0, DayNight::Day));
            }
        }

        let factors = estimate_effort(&hotspots);
        let snpp_pool = factors
            .iter()
            .find(|f| f.viirs_platforms == vec![Sensor::ViirsSNPP])
            .unwrap();
        let noaa20_pool = factors
            .iter()
            .find(|f| f.viirs_platforms == vec![Sensor::ViirsNOAA20])
            .unwrap();

        let ratio = snpp_pool.snpp_aqua_high_intensity_ratio.unwrap();
        assert!((ratio - 1.2).abs() < 1e-12, "expected 1.2, got {ratio}");
        assert_eq!(noaa20_pool.snpp_aqua_high_intensity_ratio, None);
    }

    #[test]
    fn effort_factor_serializes_high_intensity_and_diurnal_fields() {
        let mut hotspots = Vec::new();
        for month in 1..=3 {
            for _ in 0..10 {
                hotspots.push(make_hotspot_with(2015, month, vec![Sensor::ModisAqua], 50.0, DayNight::Day));
                hotspots.push(make_hotspot_with(2015, month, vec![Sensor::ViirsSNPP], 50.0, DayNight::Day));
            }
        }

        let factors = estimate_effort(&hotspots);
        let value = serde_json::to_value(&factors[0]).unwrap();
        let obj = value.as_object().unwrap();
        assert!(obj.contains_key("total_high_intensity"));
        assert!(obj.contains_key("modis_observable_high_intensity"));
        assert!(obj.contains_key("omega_high_intensity"));
        assert!(obj.contains_key("snpp_aqua_high_intensity_ratio"));
    }

    #[test]
    fn tropical_belt_share_subnormal_span_avoids_nan() {
        let share = tropical_belt_share(0.0, 5e-324);
        assert!(!share.is_nan(), "expected finite share, got NaN");
        assert_eq!(share, 1.0);
    }
}
