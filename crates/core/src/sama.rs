use crate::config::SamaConfig;
use crate::models::{DayNight, Hotspot};

// South Atlantic Magnetic Anomaly box of the VIIRS 375 m active fire algorithm: "from 11°E to
// 110°W" and down to 55°S (NASA DRL, VFIRE375 SPA User's Guide v1.9, Oct 2019, p. 3; the
// guide's northern edge "7°E" is read as 7°N). Inside it, proton hits raise the nighttime I4
// brightness temperature and fake fire pixels (Schroeder et al. 2014, RSE 143, Sec. 2.1).
pub(crate) const SAMA_WEST: f64 = -110.0;
pub(crate) const SAMA_EAST: f64 = 11.0;
pub(crate) const SAMA_SOUTH: f64 = -55.0;
pub(crate) const SAMA_NORTH: f64 = 7.0;

pub fn in_sama_box(lat: f64, lon: f64, config: &SamaConfig) -> bool {
    (config.south..=config.north).contains(&lat) && (config.west..=config.east).contains(&lon)
}

/// Nighttime VIIRS detection inside the SAMA box.
///
/// MODIS is left alone: the anomaly is documented for the VIIRS I4 (3.74 µm) band only, and
/// MOD14 has no SAMA test.
pub fn is_sama_noise(h: &Hotspot, config: &SamaConfig) -> bool {
    h.sensor.is_viirs() && h.daynight == DayNight::Night && in_sama_box(h.lat, h.lon, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, FireType, Sensor};
    use chrono::{TimeZone, Utc};

    fn hotspot(sensor: Sensor, lat: f64, lon: f64, daynight: DayNight) -> Hotspot {
        Hotspot {
            lat,
            lon,
            brightness: 300.0,
            frp: 1.0,
            confidence: Confidence::Nominal,
            sensor,
            acquired: Utc.with_ymd_and_hms(2019, 8, 20, 4, 0, 0).unwrap(),
            scan: 0.4,
            track: 0.4,
            satellite: "N".to_string(),
            daynight,
            vza: None,
            fire_type: FireType::VegetationFire,
        }
    }

    #[test]
    fn inside_box_at_night_is_noise_only_for_viirs() {
        let c = SamaConfig::default();
        for sensor in [Sensor::ViirsSNPP, Sensor::ViirsNOAA20, Sensor::ViirsNOAA21] {
            assert!(is_sama_noise(&hotspot(sensor, -15.0, -50.0, DayNight::Night), &c));
        }
        for sensor in [Sensor::ModisTerra, Sensor::ModisAqua] {
            assert!(!is_sama_noise(&hotspot(sensor, -15.0, -50.0, DayNight::Night), &c));
        }
    }

    #[test]
    fn inside_box_by_day_is_kept() {
        let c = SamaConfig::default();
        assert!(!is_sama_noise(&hotspot(Sensor::ViirsSNPP, -15.0, -50.0, DayNight::Day), &c));
    }

    #[test]
    fn outside_box_at_night_is_kept() {
        let c = SamaConfig::default();
        // Angola (east of 11°E), Mexico (north of 7°N), Patagonia tip (south of 55°S).
        for (lat, lon) in [(-12.0, 18.0), (20.0, -100.0), (-55.5, -68.0)] {
            assert!(!is_sama_noise(&hotspot(Sensor::ViirsSNPP, lat, lon, DayNight::Night), &c));
        }
    }

    #[test]
    fn box_edges_are_inclusive() {
        let c = SamaConfig::default();
        for (lat, lon) in [(SAMA_NORTH, -60.0), (SAMA_SOUTH, -60.0), (-20.0, SAMA_WEST), (-20.0, SAMA_EAST)] {
            assert!(in_sama_box(lat, lon, &c), "({lat}, {lon})");
        }
        assert!(!in_sama_box(SAMA_NORTH + 1e-9, -60.0, &c));
        assert!(!in_sama_box(-20.0, SAMA_EAST + 1e-9, &c));
    }
}
