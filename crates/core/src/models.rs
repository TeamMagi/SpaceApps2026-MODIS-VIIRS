use crate::quality::QualityFlag;
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
    #[serde(default)]
    pub frp_lower: f64,
    #[serde(default)]
    pub frp_upper: f64,
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
    /// Native-pixel mean harmonized FRP; discontinuous across the MODIS->VIIRS sensor
    /// transition due to pixel-size differences.
    pub mean_frp: f64,
    pub mean_frp_raw: f64,
    #[serde(default)]
    pub mean_frp_lower: Option<f64>,
    #[serde(default)]
    pub mean_frp_upper: Option<f64>,
    /// Monthly total harmonized FRP (MW); applies pre-VIIRS completeness factor `k_c(lat)`
    /// in cells with no VIIRS detections when calibration is enabled, and divides by
    /// `frp_viirs_scale` in MODIS + multi-VIIRS pools with a single-VIIRS reference.
    #[serde(default)]
    pub total_frp: f64,
    #[serde(default)]
    pub total_frp_lower: Option<f64>,
    #[serde(default)]
    pub total_frp_upper: Option<f64>,
    #[serde(default)]
    pub frp_viirs_scale: Option<f64>,
    /// MODIS-equivalent FRP per normalized detection (`total_frp / count_normalized`).
    #[serde(default)]
    pub frp_per_detection_eq: Option<f64>,
    pub anomaly: Option<f64>,
    /// Blended robust z-score of `count_normalized`: `(x - median) / (0.65 * 1.4826 * MAD + 0.35 * sigma)`; same pool as `anomaly`.
    #[serde(default)]
    pub sfai: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quality_flags: Vec<QualityFlag>,
    /// Mondrian split conformal band for `total_frp` (sensor reproducibility, not ground truth);
    /// present only when a calibrated table is supplied and the cell is eligible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conformal: Option<crate::conformal::ConformalBand>,
}

/// How months are grouped into seasons for [`SeasonalityYear`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeasonYear {
    /// January-December, labeled by that year.
    Calendar,
    /// March of year Y through February of Y+1, labeled Y.
    FireYear,
}

/// Per-season-year concentration of monthly detection counts (Zhang & Qian 2003 PCD/PCP).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonalityYear {
    pub year: i32,
    pub complete: bool,
    pub total: u64,
    pub pcd: Option<f64>,
    pub pcp_deg: Option<f64>,
    /// Fractional calendar month of the concentration peak; 1.0 = January, range (0.5, 12.5].
    pub peak_month: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::{BBox, Confidence, FireType, Sensor};

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

    #[test]
    fn sensor_family_helpers_split_modis_and_viirs() {
        let cases = [
            (Sensor::ModisTerra, true),
            (Sensor::ModisAqua, true),
            (Sensor::ViirsSNPP, false),
            (Sensor::ViirsNOAA20, false),
            (Sensor::ViirsNOAA21, false),
        ];

        for (sensor, expected_modis) in cases {
            assert_eq!(sensor.is_modis(), expected_modis);
            assert_eq!(sensor.is_viirs(), !expected_modis);
        }
    }

    #[test]
    fn firms_source_ids_match_nrt_feeds() {
        assert_eq!(Sensor::ModisTerra.firms_source(), "MODIS_NRT");
        assert_eq!(Sensor::ModisAqua.firms_source(), "MODIS_NRT");
        assert_eq!(Sensor::ViirsSNPP.firms_source(), "VIIRS_SNPP_NRT");
        assert_eq!(Sensor::ViirsNOAA20.firms_source(), "VIIRS_NOAA20_NRT");
        assert_eq!(Sensor::ViirsNOAA21.firms_source(), "VIIRS_NOAA21_NRT");
    }

    #[test]
    fn bbox_to_firms_string_is_west_south_east_north() {
        let integer_edges = BBox {
            west: 26.0,
            south: 36.0,
            east: 45.0,
            north: 42.0,
        };
        assert_eq!(integer_edges.to_firms_string(), "26,36,45,42");

        let fractional_edges = BBox {
            west: 26.5,
            south: 36.25,
            east: 45.0,
            north: 42.75,
        };
        assert_eq!(fractional_edges.to_firms_string(), "26.5,36.25,45,42.75");
    }

    #[test]
    fn confidence_orders_low_nominal_high() {
        assert!(Confidence::Low < Confidence::Nominal);
        assert!(Confidence::Nominal < Confidence::High);

        let highest = [Confidence::Low, Confidence::Nominal, Confidence::High]
            .into_iter()
            .max()
            .unwrap();
        assert_eq!(highest, Confidence::High);
    }
}
