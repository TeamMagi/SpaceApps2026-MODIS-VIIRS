use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub mean_frp: f64,
    pub anomaly: Option<f64>,
}
