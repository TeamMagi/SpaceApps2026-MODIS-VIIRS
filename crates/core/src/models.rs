use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Sensor {
    ModisTerra,
    ModisAqua,
    ViirsSNPP,
    ViirsNOAA20,
    ViirsNOAA21,
}

impl Sensor {
    pub fn is_modis(self) -> bool {
        matches!(self, Self::ModisTerra | Self::ModisAqua)
    }

    pub fn is_viirs(self) -> bool {
        !self.is_modis()
    }

    pub fn firms_source(self) -> &'static str {
        match self {
            Self::ModisTerra | Self::ModisAqua => "MODIS_NRT",
            Self::ViirsSNPP => "VIIRS_SNPP_NRT",
            Self::ViirsNOAA20 => "VIIRS_NOAA20_NRT",
            Self::ViirsNOAA21 => "VIIRS_NOAA21_NRT",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Confidence {
    Low,
    Nominal,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FireType {
    VegetationFire,
    ActiveVolcano,
    OtherStaticLand,
    Offshore,
    Unknown,
}

impl FireType {
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => Self::VegetationFire,
            1 => Self::ActiveVolcano,
            2 => Self::OtherStaticLand,
            3 => Self::Offshore,
            _ => Self::Unknown,
        }
    }

    pub fn code(self) -> Option<u8> {
        match self {
            Self::VegetationFire => Some(0),
            Self::ActiveVolcano => Some(1),
            Self::OtherStaticLand => Some(2),
            Self::Offshore => Some(3),
            Self::Unknown => None,
        }
    }

    pub fn is_wildfire(self) -> bool {
        matches!(self, Self::VegetationFire)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hotspot {
    pub lat: f64,
    pub lon: f64,
    pub brightness: f64,
    pub frp: f64,
    pub confidence: Confidence,
    pub sensor: Sensor,
    pub acquired: DateTime<Utc>,
    pub scan: f64,
    pub track: f64,
    pub satellite: String,
    pub daynight: DayNight,
    pub vza: Option<f64>,
    // NRT feeds carry no `type` column, so callers default to VegetationFire rather than
    // Unknown; otherwise is_wildfire() filtering would silently discard every NRT detection.
    pub fire_type: FireType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DayNight {
    Day,
    Night,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarmonizedHotspot {
    pub lat: f64,
    pub lon: f64,
    pub frp: f64,
    pub frp_corrected: f64,
    pub vza_factor: Option<f64>,
    pub n_obs: u16,
    pub platforms: Vec<Sensor>,
    pub confidence: Confidence,
    pub source: HotspotSource,
    pub acquired: DateTime<Utc>,
    pub daynight: DayNight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HotspotSource {
    ModisOnly,
    ViirsOnly,
    Matched,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BBox {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl BBox {
    pub fn to_firms_string(&self) -> String {
        format!("{},{},{},{}", self.west, self.south, self.east, self.north)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarCell {
    pub year: i32,
    pub month: u32,
    pub count: u64,
    pub count_normalized: Option<f64>,
    pub effort_omega: Option<f64>,
    pub mean_frp: f64,
    pub mean_frp_raw: f64,
    pub anomaly: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::FireType;

    #[test]
    fn fire_type_code_round_trips_for_known_codes() {
        for code in 0..=3u8 {
            let fire_type = FireType::from_code(code);
            assert_eq!(fire_type.code(), Some(code));
        }
    }

    #[test]
    fn out_of_range_code_maps_to_unknown() {
        assert_eq!(FireType::from_code(4), FireType::Unknown);
        assert_eq!(FireType::from_code(255), FireType::Unknown);
        assert_eq!(FireType::Unknown.code(), None);
    }

    #[test]
    fn only_vegetation_fire_is_wildfire() {
        assert!(FireType::VegetationFire.is_wildfire());
        assert!(!FireType::ActiveVolcano.is_wildfire());
        assert!(!FireType::OtherStaticLand.is_wildfire());
        assert!(!FireType::Offshore.is_wildfire());
        assert!(!FireType::Unknown.is_wildfire());
    }
}
