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
