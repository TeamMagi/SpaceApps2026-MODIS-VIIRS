use std::fmt;
use std::str::FromStr;

use anyhow::bail;
use pyrosync_core::models::Sensor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Processing {
    Nrt,
    Sp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmsSource {
    pub sensor: Sensor,
    pub processing: Processing,
}

impl FirmsSource {
    pub fn new(sensor: Sensor, processing: Processing) -> Self {
        Self { sensor, processing }
    }

    pub fn as_str(self) -> &'static str {
        match (self.sensor, self.processing) {
            (Sensor::ModisTerra | Sensor::ModisAqua, Processing::Nrt) => "MODIS_NRT",
            (Sensor::ModisTerra | Sensor::ModisAqua, Processing::Sp) => "MODIS_SP",
            (Sensor::ViirsSNPP, Processing::Nrt) => "VIIRS_SNPP_NRT",
            (Sensor::ViirsSNPP, Processing::Sp) => "VIIRS_SNPP_SP",
            (Sensor::ViirsNOAA20, Processing::Nrt) => "VIIRS_NOAA20_NRT",
            (Sensor::ViirsNOAA20, Processing::Sp) => "VIIRS_NOAA20_SP",
            (Sensor::ViirsNOAA21, Processing::Nrt) => "VIIRS_NOAA21_NRT",
            (Sensor::ViirsNOAA21, Processing::Sp) => "VIIRS_NOAA21_SP",
        }
    }

    pub fn all_for(sensor: Sensor) -> [FirmsSource; 2] {
        [
            Self::new(sensor, Processing::Nrt),
            Self::new(sensor, Processing::Sp),
        ]
    }
}

impl fmt::Display for FirmsSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FirmsSource {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (sensor, processing) = match s {
            "MODIS_NRT" => (Sensor::ModisTerra, Processing::Nrt),
            "MODIS_SP" => (Sensor::ModisTerra, Processing::Sp),
            "VIIRS_SNPP_NRT" => (Sensor::ViirsSNPP, Processing::Nrt),
            "VIIRS_SNPP_SP" => (Sensor::ViirsSNPP, Processing::Sp),
            "VIIRS_NOAA20_NRT" => (Sensor::ViirsNOAA20, Processing::Nrt),
            "VIIRS_NOAA20_SP" => (Sensor::ViirsNOAA20, Processing::Sp),
            "VIIRS_NOAA21_NRT" => (Sensor::ViirsNOAA21, Processing::Nrt),
            "VIIRS_NOAA21_SP" => (Sensor::ViirsNOAA21, Processing::Sp),
            _ => bail!("unknown FIRMS source identifier: {s}"),
        };

        Ok(Self::new(sensor, processing))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [(&str, Sensor, Processing); 8] = [
        ("MODIS_NRT", Sensor::ModisTerra, Processing::Nrt),
        ("MODIS_SP", Sensor::ModisTerra, Processing::Sp),
        ("VIIRS_SNPP_NRT", Sensor::ViirsSNPP, Processing::Nrt),
        ("VIIRS_SNPP_SP", Sensor::ViirsSNPP, Processing::Sp),
        ("VIIRS_NOAA20_NRT", Sensor::ViirsNOAA20, Processing::Nrt),
        ("VIIRS_NOAA20_SP", Sensor::ViirsNOAA20, Processing::Sp),
        ("VIIRS_NOAA21_NRT", Sensor::ViirsNOAA21, Processing::Nrt),
        ("VIIRS_NOAA21_SP", Sensor::ViirsNOAA21, Processing::Sp),
    ];

    #[test]
    fn every_identifier_parses_and_formats_back_to_itself() {
        for (id, sensor, processing) in ALL {
            let parsed: FirmsSource = id.parse().unwrap();
            assert_eq!(parsed, FirmsSource::new(sensor, processing), "{id}");
            assert_eq!(parsed.as_str(), id);
            assert_eq!(parsed.to_string(), id);
        }
    }

    #[test]
    fn terra_and_aqua_share_one_modis_identifier() {
        for processing in [Processing::Nrt, Processing::Sp] {
            let terra = FirmsSource::new(Sensor::ModisTerra, processing);
            let aqua = FirmsSource::new(Sensor::ModisAqua, processing);
            assert_eq!(terra.as_str(), aqua.as_str());
        }
    }

    #[test]
    fn unknown_identifiers_are_rejected_naming_the_input() {
        // EP: wrong case, padding, empty, a near-miss and an unsupported processing level.
        for bad in ["modis_nrt", " MODIS_NRT", "MODIS_NRT ", "", "MODIS", "VIIRS_SNPP_URT", "MODIS_NRT_SP"] {
            let err = bad.parse::<FirmsSource>().unwrap_err().to_string();
            assert!(err.contains("unknown FIRMS source identifier"), "{err}");
            assert!(err.ends_with(bad), "`{err}` should end with `{bad}`");
        }
    }

    #[test]
    fn all_for_lists_nrt_then_sp_for_the_same_sensor() {
        for sensor in [Sensor::ModisAqua, Sensor::ViirsNOAA21] {
            let [first, second] = FirmsSource::all_for(sensor);
            assert_eq!(first.processing, Processing::Nrt);
            assert_eq!(second.processing, Processing::Sp);
            assert_eq!((first.sensor, second.sensor), (sensor, sensor));
        }
    }
}
