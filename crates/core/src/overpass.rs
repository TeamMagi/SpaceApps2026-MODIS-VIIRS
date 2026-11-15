//! Overpass-time medians per platform and day/night pass, computed at serve time from raw detections.
//!
//! The local solar time of a detection is its UTC time of day shifted by `lon / 15` hours. A
//! regional median of that value is an indicator of the platform's overpass time, not the
//! equator-crossing time: detections away from the sub-satellite track see local times spread
//! around the crossing time, and high-latitude passes shift it.
//!
//! Nominal design crossing times (standard mission values, for comparison by the caller):
//! Terra 10:30 descending (day) / 22:30 (night), Aqua 13:30 ascending (day) / 01:30 (night),
//! S-NPP, NOAA-20 and NOAA-21 13:30 (day) / 01:30 (night).

use crate::models::{DayNight, Hotspot, Sensor};
use chrono::Timelike;
use serde::Serialize;

/// Minimum detections for a platform x day/night group to be reported.
pub const MIN_OVERPASS_SAMPLES: usize = 30;

const DAY_MINUTES: f64 = 1440.0;

const PLATFORMS: [Sensor; 5] = [
    Sensor::ModisTerra,
    Sensor::ModisAqua,
    Sensor::ViirsSNPP,
    Sensor::ViirsNOAA20,
    Sensor::ViirsNOAA21,
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverpassMedian {
    pub platform: &'static str,
    pub daynight: DayNight,
    pub median_local_solar_time: String,
    /// Minutes after local solar midnight, in `[0, 1440)`.
    pub median_minutes: f64,
    pub n: usize,
}

pub fn platform_label(sensor: Sensor) -> &'static str {
    match sensor {
        Sensor::ModisTerra => "Terra",
        Sensor::ModisAqua => "Aqua",
        Sensor::ViirsSNPP => "S-NPP",
        Sensor::ViirsNOAA20 => "NOAA-20",
        Sensor::ViirsNOAA21 => "NOAA-21",
    }
}

/// Local solar time as minutes after local midnight, wrapped to `[0, 1440)`.
pub fn local_solar_minutes(hotspot: &Hotspot) -> f64 {
    let utc_minutes = f64::from(hotspot.acquired.time().num_seconds_from_midnight()) / 60.0;
    (utc_minutes + hotspot.lon * 4.0).rem_euclid(DAY_MINUTES)
}

/// `HH:MM` for minutes after midnight; rounds to the nearest minute and wraps 24:00 to 00:00.
pub fn format_hhmm(minutes: f64) -> String {
    let total = (minutes.rem_euclid(DAY_MINUTES).round() as u32) % 1440;
    format!("{:02}:{:02}", total / 60, total % 60)
}

/// Median on the 24 h circle. Values are rotated so their circular mean sits at midday, which
/// keeps a cluster straddling 00:00 contiguous; the median is then rotated back.
pub fn circular_median_minutes(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let to_angle = std::f64::consts::TAU / DAY_MINUTES;
    let (sin_sum, cos_sum) = values
        .iter()
        .fold((0.0, 0.0), |(s, c), v| (s + (v * to_angle).sin(), c + (v * to_angle).cos()));
    let center = if sin_sum.hypot(cos_sum) < 1e-9 * values.len() as f64 {
        DAY_MINUTES / 2.0
    } else {
        sin_sum.atan2(cos_sum).rem_euclid(std::f64::consts::TAU) / to_angle
    };
    let half = DAY_MINUTES / 2.0;
    let mut shifted: Vec<f64> = values
        .iter()
        .map(|v| (v - center + half).rem_euclid(DAY_MINUTES))
        .collect();
    shifted.sort_by(f64::total_cmp);
    let n = shifted.len();
    let median = if n % 2 == 1 {
        shifted[n / 2]
    } else {
        (shifted[n / 2 - 1] + shifted[n / 2]) / 2.0
    };
    Some((median + center - half).rem_euclid(DAY_MINUTES))
}

/// Medians for every platform x day/night group with at least `min_samples` detections, ordered
/// Terra, Aqua, S-NPP, NOAA-20, NOAA-21 and Day before Night.
pub fn overpass_medians<'a>(
    hotspots: impl IntoIterator<Item = &'a Hotspot>,
    min_samples: usize,
) -> Vec<OverpassMedian> {
    let mut groups: [[Vec<f64>; 2]; 5] = Default::default();
    for h in hotspots {
        let Some(p) = PLATFORMS.iter().position(|s| *s == h.sensor) else {
            continue;
        };
        let d = usize::from(h.daynight == DayNight::Night);
        groups[p][d].push(local_solar_minutes(h));
    }
    let mut out = Vec::new();
    for (p, per_pass) in groups.iter().enumerate() {
        for (d, values) in per_pass.iter().enumerate() {
            if values.len() < min_samples.max(1) {
                continue;
            }
            let Some(median) = circular_median_minutes(values) else {
                continue;
            };
            out.push(OverpassMedian {
                platform: platform_label(PLATFORMS[p]),
                daynight: if d == 0 { DayNight::Day } else { DayNight::Night },
                median_local_solar_time: format_hhmm(median),
                median_minutes: median,
                n: values.len(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, FireType};
    use chrono::{TimeZone, Utc};

    fn hotspot(sensor: Sensor, daynight: DayNight, lon: f64, hour: u32, minute: u32) -> Hotspot {
        Hotspot {
            lat: 40.0,
            lon,
            brightness: 330.0,
            frp: 10.0,
            confidence: Confidence::Nominal,
            sensor,
            acquired: Utc.with_ymd_and_hms(2021, 8, 1, hour, minute, 0).unwrap(),
            scan: 1.0,
            track: 1.0,
            satellite: String::new(),
            daynight,
            vza: None,
            fire_type: FireType::VegetationFire,
        }
    }

    #[test]
    fn local_solar_time_adds_longitude_hours() {
        let h = hotspot(Sensor::ModisTerra, DayNight::Day, 30.0, 8, 30);
        assert_eq!(local_solar_minutes(&h), 630.0);
        let west = hotspot(Sensor::ModisTerra, DayNight::Day, -90.0, 17, 0);
        assert_eq!(local_solar_minutes(&west), 660.0);
    }

    #[test]
    fn local_solar_time_wraps_at_midnight() {
        let h = hotspot(Sensor::ModisAqua, DayNight::Night, 150.0, 22, 0);
        assert_eq!(local_solar_minutes(&h), 480.0);
        let west = hotspot(Sensor::ModisAqua, DayNight::Night, -150.0, 5, 0);
        assert_eq!(local_solar_minutes(&west), 1140.0);
    }

    #[test]
    fn format_wraps_and_rounds() {
        assert_eq!(format_hhmm(630.0), "10:30");
        assert_eq!(format_hhmm(1439.6), "00:00");
        assert_eq!(format_hhmm(-30.0), "23:30");
    }

    #[test]
    fn median_of_cluster_straddling_midnight_stays_near_midnight() {
        let values: Vec<f64> = [1420.0, 1430.0, 1435.0, 5.0, 10.0, 20.0, 1438.0]
            .into_iter()
            .collect();
        let m = circular_median_minutes(&values).unwrap();
        let dist = m.min(DAY_MINUTES - m);
        assert!(dist < 10.0, "median {m} should be within 10 min of midnight");
    }

    #[test]
    fn median_of_plain_cluster_is_ordinary_median() {
        let m = circular_median_minutes(&[600.0, 630.0, 700.0]).unwrap();
        assert!((m - 630.0).abs() < 1e-6);
        let even = circular_median_minutes(&[600.0, 620.0, 640.0, 660.0]).unwrap();
        assert!((even - 630.0).abs() < 1e-6);
        assert_eq!(circular_median_minutes(&[]), None);
    }

    #[test]
    fn groups_below_minimum_sample_are_dropped() {
        let terra: Vec<Hotspot> = (0..MIN_OVERPASS_SAMPLES)
            .map(|_| hotspot(Sensor::ModisTerra, DayNight::Day, 0.0, 10, 30))
            .collect();
        let few: Vec<Hotspot> = (0..MIN_OVERPASS_SAMPLES - 1)
            .map(|_| hotspot(Sensor::ModisAqua, DayNight::Day, 0.0, 13, 30))
            .collect();
        let all: Vec<&Hotspot> = terra.iter().chain(&few).collect();
        let out = overpass_medians(all, MIN_OVERPASS_SAMPLES);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].platform, "Terra");
        assert_eq!(out[0].median_local_solar_time, "10:30");
        assert_eq!(out[0].n, MIN_OVERPASS_SAMPLES);
    }

    #[test]
    fn output_order_is_platform_then_day_before_night() {
        let mut pts = Vec::new();
        for (sensor, dn, hour) in [
            (Sensor::ViirsNOAA20, DayNight::Night, 1),
            (Sensor::ViirsSNPP, DayNight::Day, 13),
            (Sensor::ModisAqua, DayNight::Night, 1),
            (Sensor::ModisTerra, DayNight::Night, 22),
            (Sensor::ModisTerra, DayNight::Day, 10),
        ] {
            for _ in 0..3 {
                pts.push(hotspot(sensor, dn, 0.0, hour, 30));
            }
        }
        let out = overpass_medians(&pts, 3);
        let order: Vec<(&str, DayNight)> = out.iter().map(|o| (o.platform, o.daynight)).collect();
        assert_eq!(
            order,
            vec![
                ("Terra", DayNight::Day),
                ("Terra", DayNight::Night),
                ("Aqua", DayNight::Night),
                ("S-NPP", DayNight::Day),
                ("NOAA-20", DayNight::Night),
            ]
        );
    }

    #[test]
    fn night_pass_straddling_midnight_reports_a_midnight_time() {
        let mut pts = Vec::new();
        for i in 0..40 {
            let (h, m) = if i % 2 == 0 { (23, 50) } else { (0, 10) };
            pts.push(hotspot(Sensor::ModisAqua, DayNight::Night, 0.0, h, m));
        }
        let out = overpass_medians(&pts, MIN_OVERPASS_SAMPLES);
        assert_eq!(out.len(), 1);
        let t = &out[0].median_local_solar_time;
        assert!(t == "23:59" || t == "00:00" || t == "00:01" || t == "23:58", "got {t}");
    }
}
