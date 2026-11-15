use std::collections::{HashMap, HashSet};

use chrono::Datelike;
use h3o::{LatLng, Resolution};

use crate::config::StaticMaskConfig;
use crate::models::Hotspot;

/// H3 resolution of the persistence grid (~5.2 km², ~1.2 km edge). Resolution 8 splits one
/// flare across neighbouring cells because VIIRS/MODIS geolocation scatter is ~0.4-1 km.
pub const MASK_RESOLUTION: Resolution = Resolution::Seven;

/// Months with a `type 0` detection in one calendar year that mark a cell as a static source.
///
/// Measured with the `static_mask_check` example on the FIRMS country archive (MODIS + S-NPP +
/// NOAA-20, 2013-2024). Cells holding any FIRMS `type 1/2` row serve as ground truth, so the
/// share of dropped `type 0` rows inside them is a lower bound on the mask's precision. 10 is
/// the smallest threshold at which that share is >= 80% in every sampled country:
///
/// | months | Nigeria | Iraq | Algeria | Russia | USA | Indonesia | Australia | Portugal |
/// |---|---|---|---|---|---|---|---|---|
/// | 8  | 86% | 89% | 95% | 91% | 79% | 69% | 62% | 100% |
/// | 9  | 88% | 90% | 95% | 92% | 85% | 80% | 70% | 100% |
/// | 10 | 89% | 96% | 96% | 93% | 89% | 89% | 84% | 100% |
///
/// At 10 months the mask drops 0.21% / 13.4% / 19.7% / 0.98% / 1.17% / 0.80% / 0.06% / 0.05%
/// of `type 0` rows in the same order; 6 months would already reach the Nigerian savanna (44%).
pub(crate) const MIN_ACTIVE_MONTHS: u8 = 10;

/// H3 cell (as `u64`) of the persistence grid containing `(lat, lon)`, or `None` off the globe.
pub fn mask_cell(lat: f64, lon: f64) -> Option<u64> {
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    LatLng::new(lat, lon)
        .ok()
        .map(|ll| u64::from(ll.to_cell(MASK_RESOLUTION)))
}

/// Cell-years that burn persistently (gas flares, industry, volcanoes) although FIRMS left the
/// detections as `type 0` vegetation fire.
///
/// A cell is masked in a calendar year when it holds a detection in at least
/// `min_active_months` distinct months of that year: vegetation fires in one ~5 km² cell are
/// seasonal (a few months a year), flares burn year-round. Judging each year on its own makes
/// the result independent of how a multi-year request is split into whole years, so the
/// per-year `/api/series` and the multi-year `/api/calendar` agree. Windows shorter than
/// `min_active_months` months (NRT requests) can never reach the threshold and are not masked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StaticMask {
    cell_years: HashSet<(u64, i32)>,
}

impl StaticMask {
    pub fn from_detections<'a>(
        rows: impl IntoIterator<Item = &'a Hotspot>,
        config: &StaticMaskConfig,
    ) -> Self {
        let mut months: HashMap<(u64, i32), u16> = HashMap::new();
        for h in rows {
            if let Some(cell) = mask_cell(h.lat, h.lon) {
                *months.entry((cell, h.acquired.year())).or_default() |= 1 << h.acquired.month0();
            }
        }
        let min = u32::from(config.min_active_months);
        let cell_years = months
            .into_iter()
            .filter(|(_, m)| m.count_ones() >= min)
            .map(|(key, _)| key)
            .collect();
        Self { cell_years }
    }

    pub fn contains(&self, h: &Hotspot) -> bool {
        !self.cell_years.is_empty()
            && mask_cell(h.lat, h.lon)
                .is_some_and(|c| self.cell_years.contains(&(c, h.acquired.year())))
    }

    /// Number of masked cell-years.
    pub fn len(&self) -> usize {
        self.cell_years.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cell_years.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, DayNight, FireType, Sensor};
    use chrono::{TimeZone, Utc};

    fn at(lat: f64, lon: f64, year: i32, month: u32) -> Hotspot {
        Hotspot {
            lat,
            lon,
            brightness: 330.0,
            frp: 5.0,
            confidence: Confidence::Nominal,
            sensor: Sensor::ViirsSNPP,
            acquired: Utc.with_ymd_and_hms(year, month, 10, 1, 0, 0).unwrap(),
            scan: 0.4,
            track: 0.4,
            satellite: "N".to_string(),
            daynight: DayNight::Night,
            vza: None,
            fire_type: FireType::VegetationFire,
        }
    }

    fn monthly(lat: f64, lon: f64, year: i32, months: u32) -> Vec<Hotspot> {
        (1..=months).map(|m| at(lat, lon, year, m)).collect()
    }

    fn config(min_active_months: u8) -> StaticMaskConfig {
        StaticMaskConfig {
            enabled: true,
            min_active_months,
        }
    }

    #[test]
    fn mask_cell_rejects_off_globe_coordinates() {
        assert!(mask_cell(4.8, 6.9).is_some());
        assert_eq!(mask_cell(91.0, 0.0), None);
        assert_eq!(mask_cell(0.0, -180.5), None);
        assert_eq!(mask_cell(f64::NAN, 0.0), None);
    }

    #[test]
    fn month_threshold_is_inclusive() {
        let rows = monthly(4.8, 6.9, 2020, 8);
        assert_eq!(StaticMask::from_detections(&rows, &config(8)).len(), 1);
        assert!(StaticMask::from_detections(&rows, &config(9)).is_empty());
        assert!(StaticMask::from_detections(&rows[..7], &config(8)).is_empty());
    }

    #[test]
    fn repeated_detections_in_one_month_count_once() {
        let rows: Vec<Hotspot> = (0..50).map(|_| at(4.8, 6.9, 2020, 3)).collect();
        assert!(StaticMask::from_detections(&rows, &config(2)).is_empty());
    }

    #[test]
    fn months_spread_over_years_do_not_add_up() {
        // Five months in each of two years is not ten months in one year.
        let mut rows = monthly(4.8, 6.9, 2018, 5);
        rows.extend(monthly(4.8, 6.9, 2019, 5));
        assert!(StaticMask::from_detections(&rows, &config(6)).is_empty());
    }

    #[test]
    fn each_year_is_judged_on_its_own() {
        let mut rows = monthly(4.8, 6.9, 2018, 12);
        rows.extend(monthly(4.8, 6.9, 2019, 3));
        let mask = StaticMask::from_detections(&rows, &config(8));
        assert_eq!(mask.len(), 1);
        assert!(mask.contains(&at(4.8, 6.9, 2018, 6)));
        assert!(!mask.contains(&at(4.8, 6.9, 2019, 2)));
        // The same verdict comes from the 2018 rows alone, so splitting by year changes nothing.
        assert_eq!(StaticMask::from_detections(&rows[..12], &config(8)), mask);
    }

    #[test]
    fn contains_matches_only_masked_cells() {
        let rows = monthly(4.8, 6.9, 2020, 12);
        let mask = StaticMask::from_detections(&rows, &config(8));
        assert!(mask.contains(&at(4.8, 6.9, 2020, 1)));
        assert!(!mask.contains(&at(-20.0, 30.0, 2020, 1)));
        assert!(!StaticMask::default().contains(&at(4.8, 6.9, 2020, 1)));
    }
}
