use serde::{Deserialize, Serialize};

use crate::biome::{Biome, biome_at};
use crate::config::CalibrationConfig;
use crate::models::{DayNight, Sensor};
use crate::vza::{
    DetectionLimitCoefficients, detection_limit_coefficients, detection_limit_mw, detection_slope,
    modis_max_scan_angle_rad, modis_pixel_area_km2,
};

/// Latitude-band pre-VIIRS completeness factor `k_c` (`ΣVIIRS / Σ(MODIS × R_rel(θ))`) and uncertainty.
///
/// Measured from our archive (`calibration_grid_check.rs`, Aqua MODIS vs S-NPP 375 m VIIRS,
/// daytime, Δt ≤ 20 min, 1°×1° cell-days; 2019 fit, 2020 hold-out). Accounts for small fires
/// missed by MODIS's 1 km pixel threshold that VIIRS detects; applied ONLY to monthly `total_frp`
/// in pre-2012 cells with no VIIRS detections, never per-detection.
///
/// Consistent with literature: Li et al. (2020) reports MODIS underestimates African total FRP by
/// ~26%, and Sofiev & Kouznetsov (2026) reports ~38% global underestimation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationBand {
    pub min_abs_lat: f64,
    pub max_abs_lat: f64,
    /// Completeness multiplier `k_c` applied to pre-VIIRS monthly total FRP.
    pub ratio: f64,
    /// Measured 1σ spread of `k_c` across grid-days, propagated as systematic uncertainty.
    pub ratio_sigma: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    pub ratio: f64,
    pub ratio_sigma: f64,
}

pub(crate) fn default_bands() -> Vec<CalibrationBand> {
    vec![
        // Tropics (0 - 30 deg): k_c = 1.11 (sigma = 0.08).
        CalibrationBand {
            min_abs_lat: 0.0,
            max_abs_lat: 30.0,
            ratio: 1.11,
            ratio_sigma: 0.08,
        },
        // Mid-latitudes (30 - 50 deg): k_c = 1.18 (sigma = 0.15, small sample).
        CalibrationBand {
            min_abs_lat: 30.0,
            max_abs_lat: 50.0,
            ratio: 1.18,
            ratio_sigma: 0.15,
        },
        // High latitudes / boreal (55 - 70 deg): k_c = 1.28 (sigma = 0.11).
        CalibrationBand {
            min_abs_lat: 55.0,
            max_abs_lat: 70.0,
            ratio: 1.28,
            ratio_sigma: 0.11,
        },
    ]
}

/// Returns the pre-VIIRS completeness factor `k_c` and its uncertainty `σ` for `lat` under `config`.
///
/// Uses `|lat|` (hemispheric symmetry) across bands. Transition gaps between adjacent bands
/// (50-55 deg) are linearly interpolated, latitudes beyond the last band (>70 deg) clamp to the
/// last band, and invalid/NaN latitudes or disabled configs return the identity (`ratio = 1.0`,
/// `ratio_sigma = 0.0`).
pub fn calibration_for(lat: f64, config: &CalibrationConfig) -> Calibration {
    if !config.enabled || !lat.is_finite() || lat.abs() > 90.0 || config.bands.is_empty() {
        return Calibration {
            ratio: 1.0,
            ratio_sigma: 0.0,
        };
    }

    let abs_lat = lat.abs();
    let bands = &config.bands;

    if abs_lat < bands[0].min_abs_lat {
        return Calibration {
            ratio: bands[0].ratio,
            ratio_sigma: bands[0].ratio_sigma,
        };
    }

    for (i, band) in bands.iter().enumerate() {
        if abs_lat <= band.max_abs_lat {
            if i > 0 && abs_lat < band.min_abs_lat {
                let prev = &bands[i - 1];
                let span = band.min_abs_lat - prev.max_abs_lat;
                if span > 0.0 {
                    let t = (abs_lat - prev.max_abs_lat) / span;
                    return Calibration {
                        ratio: prev.ratio + t * (band.ratio - prev.ratio),
                        ratio_sigma: prev.ratio_sigma + t * (band.ratio_sigma - prev.ratio_sigma),
                    };
                }
            }
            return Calibration {
                ratio: band.ratio,
                ratio_sigma: band.ratio_sigma,
            };
        }
    }

    let last = &bands[bands.len() - 1];
    Calibration {
        ratio: last.ratio,
        ratio_sigma: last.ratio_sigma,
    }
}

/// Which completeness model `k_c` comes from. `Biome` falls back to `LatBand` wherever no
/// biome-specific estimate exists, so the old behaviour is always the floor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationMode {
    #[default]
    Biome,
    LatBand,
}

/// Measured completeness `ΣVIIRS / Σ(MODIS × R_rel)` for one stratum. `month = None` pools all
/// months and `daynight = None` pools day and night; a set `month` requires a set `daynight`.
///
/// `ratio` may be below 1: a stratum where the VIIRS total lies below the VZA-corrected MODIS
/// total is a legitimate measurement and is applied as measured (validation only demands finite
/// and > 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiomeCompleteness {
    pub biome: Biome,
    #[serde(default)]
    pub month: Option<u8>,
    #[serde(default)]
    pub daynight: Option<DayNight>,
    pub ratio: f64,
    /// For measured rows: the standard error of the ratio of sums over the `n` cell-days (delta
    /// method). It is NOT comparable to [`CalibrationBand::ratio_sigma`], which is the spread of
    /// `k_c` across cell-days: the standard error shrinks with `n` and therefore understates the
    /// cell-level uncertainty (the tool reports that spread as `sd_hg`, typically 1.5-5, but it is
    /// not ingested here). The calendar propagates this value as given.
    pub ratio_sigma: f64,
    /// Number of paired cell-days behind the estimate.
    pub n: u32,
}

/// Small-fire budget model for one biome: fire FRP density `f(P) ∝ P^(-alpha)` up to `p_max_mw`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerLawBudget {
    pub biome: Biome,
    pub alpha: f64,
    /// 1σ uncertainty of `alpha`, propagated into the factor's sigma.
    #[serde(default = "default_alpha_sigma")]
    pub alpha_sigma: f64,
    /// Largest per-fire FRP of the distribution (MW).
    pub p_max_mw: f64,
}

// No source gives an uncertainty for alpha = 1.31 ([15, Fig. 6] only reports r² = 0.989), so this
// is an explicit assumption, not a measurement.
const DEFAULT_ALPHA_SIGMA: f64 = 0.1;
const VIIRS_NADIR_AREA_KM2: f64 = 0.375 * 0.375;
// The integrand P^(2-alpha) * p(P) vanishes at small P because the observation operator is 0
// there (sigmoid below c0), so the result does not depend on this floor (tested).
const P_MIN_MW: f64 = 0.05;
// Fixed Simpson grids (even interval counts): log-spaced in P, uniform in scan angle.
const P_GRID_INTERVALS: usize = 2000;
const SCAN_GRID_INTERVALS: usize = 40;

fn default_alpha_sigma() -> f64 {
    DEFAULT_ALPHA_SIGMA
}

// The power-law level stays in the chain as an experiment hook but ships with no parameters. It
// brackets the measured savanna completeness (day 1.19, night 1.32) without predicting it: the
// literature alpha = 1.31 gives k ~ 1.03-1.04 (too low), while the alpha ~ 2.2 and p_max ~ 190 MW
// fitted to our own VIIRS data give ~1.6 day / ~2.5 night (too high). Biomes without a measured
// row therefore fall straight to the latitude band.
pub(crate) fn default_power_law() -> Vec<PowerLawBudget> {
    Vec::new()
}

// BEGIN GENERATED BIOME TABLE
// Generated by crates/firms-client/examples/biome_completeness.rs: fit year 2019, 12 regions,
// generated 2026-11-15. Do not hand-edit; rerun the tool and paste its `--emit-rust` output.
// Report: docs/BIYOM_TAMLIK.md. `ratio_sigma` is the standard error of the ratio of sums, see
// [`BiomeCompleteness::ratio_sigma`].
pub(crate) fn default_biome_table() -> Vec<BiomeCompleteness> {
    vec![
        BiomeCompleteness { biome: Biome::Forest, month: None, daynight: None, ratio: 1.091, ratio_sigma: 0.016, n: 8626 },
        BiomeCompleteness { biome: Biome::Forest, month: None, daynight: Some(DayNight::Night), ratio: 1.998, ratio_sigma: 0.049, n: 1861 },
        BiomeCompleteness { biome: Biome::Forest, month: None, daynight: Some(DayNight::Day), ratio: 1.048, ratio_sigma: 0.016, n: 6765 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(1), daynight: Some(DayNight::Day), ratio: 1.176, ratio_sigma: 0.071, n: 237 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(2), daynight: Some(DayNight::Night), ratio: 3.144, ratio_sigma: 0.624, n: 48 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(2), daynight: Some(DayNight::Day), ratio: 1.062, ratio_sigma: 0.039, n: 421 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(3), daynight: Some(DayNight::Night), ratio: 3.168, ratio_sigma: 0.281, n: 178 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(3), daynight: Some(DayNight::Day), ratio: 0.946, ratio_sigma: 0.034, n: 558 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(4), daynight: Some(DayNight::Night), ratio: 2.822, ratio_sigma: 0.225, n: 139 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(4), daynight: Some(DayNight::Day), ratio: 0.914, ratio_sigma: 0.058, n: 451 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(5), daynight: Some(DayNight::Night), ratio: 1.975, ratio_sigma: 0.217, n: 84 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(5), daynight: Some(DayNight::Day), ratio: 1.260, ratio_sigma: 0.077, n: 428 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(6), daynight: Some(DayNight::Night), ratio: 1.403, ratio_sigma: 0.112, n: 95 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(6), daynight: Some(DayNight::Day), ratio: 1.175, ratio_sigma: 0.050, n: 443 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(7), daynight: Some(DayNight::Night), ratio: 2.044, ratio_sigma: 0.093, n: 405 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(7), daynight: Some(DayNight::Day), ratio: 1.158, ratio_sigma: 0.027, n: 1168 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(8), daynight: Some(DayNight::Night), ratio: 2.052, ratio_sigma: 0.068, n: 506 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(8), daynight: Some(DayNight::Day), ratio: 1.030, ratio_sigma: 0.027, n: 1296 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(9), daynight: Some(DayNight::Night), ratio: 1.750, ratio_sigma: 0.062, n: 290 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(9), daynight: Some(DayNight::Day), ratio: 1.096, ratio_sigma: 0.031, n: 851 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(10), daynight: Some(DayNight::Night), ratio: 1.590, ratio_sigma: 0.418, n: 53 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(10), daynight: Some(DayNight::Day), ratio: 1.253, ratio_sigma: 0.071, n: 388 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(11), daynight: Some(DayNight::Day), ratio: 0.923, ratio_sigma: 0.062, n: 251 },
        BiomeCompleteness { biome: Biome::Forest, month: Some(12), daynight: Some(DayNight::Day), ratio: 1.040, ratio_sigma: 0.062, n: 273 },
        BiomeCompleteness { biome: Biome::Shrubland, month: None, daynight: None, ratio: 1.267, ratio_sigma: 0.040, n: 1454 },
        BiomeCompleteness { biome: Biome::Shrubland, month: None, daynight: Some(DayNight::Night), ratio: 1.565, ratio_sigma: 0.075, n: 308 },
        BiomeCompleteness { biome: Biome::Shrubland, month: None, daynight: Some(DayNight::Day), ratio: 1.244, ratio_sigma: 0.042, n: 1146 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(1), daynight: Some(DayNight::Day), ratio: 0.923, ratio_sigma: 0.155, n: 30 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(2), daynight: Some(DayNight::Day), ratio: 1.374, ratio_sigma: 0.183, n: 30 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(3), daynight: Some(DayNight::Day), ratio: 1.145, ratio_sigma: 0.133, n: 53 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(4), daynight: Some(DayNight::Day), ratio: 1.111, ratio_sigma: 0.118, n: 46 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(5), daynight: Some(DayNight::Day), ratio: 1.480, ratio_sigma: 0.183, n: 86 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(6), daynight: Some(DayNight::Day), ratio: 1.127, ratio_sigma: 0.138, n: 110 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(7), daynight: Some(DayNight::Night), ratio: 1.687, ratio_sigma: 0.148, n: 57 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(7), daynight: Some(DayNight::Day), ratio: 1.307, ratio_sigma: 0.092, n: 156 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(8), daynight: Some(DayNight::Night), ratio: 1.959, ratio_sigma: 0.292, n: 62 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(8), daynight: Some(DayNight::Day), ratio: 1.035, ratio_sigma: 0.118, n: 138 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(9), daynight: Some(DayNight::Night), ratio: 1.470, ratio_sigma: 0.114, n: 34 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(9), daynight: Some(DayNight::Day), ratio: 1.269, ratio_sigma: 0.063, n: 138 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(10), daynight: Some(DayNight::Day), ratio: 1.291, ratio_sigma: 0.097, n: 148 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(11), daynight: Some(DayNight::Night), ratio: 1.409, ratio_sigma: 0.141, n: 30 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(11), daynight: Some(DayNight::Day), ratio: 1.487, ratio_sigma: 0.138, n: 143 },
        BiomeCompleteness { biome: Biome::Shrubland, month: Some(12), daynight: Some(DayNight::Day), ratio: 1.302, ratio_sigma: 0.144, n: 68 },
        BiomeCompleteness { biome: Biome::Savanna, month: None, daynight: None, ratio: 1.193, ratio_sigma: 0.008, n: 17468 },
        BiomeCompleteness { biome: Biome::Savanna, month: None, daynight: Some(DayNight::Night), ratio: 1.319, ratio_sigma: 0.037, n: 1658 },
        BiomeCompleteness { biome: Biome::Savanna, month: None, daynight: Some(DayNight::Day), ratio: 1.192, ratio_sigma: 0.008, n: 15810 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(1), daynight: Some(DayNight::Night), ratio: 1.476, ratio_sigma: 0.171, n: 104 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(1), daynight: Some(DayNight::Day), ratio: 1.332, ratio_sigma: 0.029, n: 1141 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(2), daynight: Some(DayNight::Night), ratio: 1.638, ratio_sigma: 0.312, n: 62 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(2), daynight: Some(DayNight::Day), ratio: 1.231, ratio_sigma: 0.055, n: 802 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(3), daynight: Some(DayNight::Night), ratio: 1.112, ratio_sigma: 0.205, n: 38 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(3), daynight: Some(DayNight::Day), ratio: 1.431, ratio_sigma: 0.051, n: 718 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(4), daynight: Some(DayNight::Day), ratio: 1.290, ratio_sigma: 0.067, n: 469 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(5), daynight: Some(DayNight::Night), ratio: 1.532, ratio_sigma: 0.240, n: 51 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(5), daynight: Some(DayNight::Day), ratio: 1.265, ratio_sigma: 0.027, n: 1265 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(6), daynight: Some(DayNight::Night), ratio: 1.114, ratio_sigma: 0.100, n: 124 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(6), daynight: Some(DayNight::Day), ratio: 1.145, ratio_sigma: 0.017, n: 1878 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(7), daynight: Some(DayNight::Night), ratio: 1.099, ratio_sigma: 0.118, n: 184 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(7), daynight: Some(DayNight::Day), ratio: 1.084, ratio_sigma: 0.016, n: 2025 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(8), daynight: Some(DayNight::Night), ratio: 1.471, ratio_sigma: 0.073, n: 324 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(8), daynight: Some(DayNight::Day), ratio: 1.203, ratio_sigma: 0.019, n: 2082 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(9), daynight: Some(DayNight::Night), ratio: 1.403, ratio_sigma: 0.070, n: 372 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(9), daynight: Some(DayNight::Day), ratio: 1.257, ratio_sigma: 0.021, n: 1935 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(10), daynight: Some(DayNight::Night), ratio: 1.422, ratio_sigma: 0.052, n: 205 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(10), daynight: Some(DayNight::Day), ratio: 1.265, ratio_sigma: 0.044, n: 1195 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(11), daynight: Some(DayNight::Night), ratio: 1.090, ratio_sigma: 0.084, n: 81 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(11), daynight: Some(DayNight::Day), ratio: 1.257, ratio_sigma: 0.030, n: 1034 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(12), daynight: Some(DayNight::Night), ratio: 0.949, ratio_sigma: 0.160, n: 92 },
        BiomeCompleteness { biome: Biome::Savanna, month: Some(12), daynight: Some(DayNight::Day), ratio: 1.155, ratio_sigma: 0.024, n: 1266 },
        BiomeCompleteness { biome: Biome::Grassland, month: None, daynight: None, ratio: 1.224, ratio_sigma: 0.031, n: 2146 },
        BiomeCompleteness { biome: Biome::Grassland, month: None, daynight: Some(DayNight::Night), ratio: 1.332, ratio_sigma: 0.141, n: 251 },
        BiomeCompleteness { biome: Biome::Grassland, month: None, daynight: Some(DayNight::Day), ratio: 1.217, ratio_sigma: 0.032, n: 1895 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(1), daynight: Some(DayNight::Day), ratio: 1.291, ratio_sigma: 0.080, n: 180 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(2), daynight: Some(DayNight::Day), ratio: 1.284, ratio_sigma: 0.123, n: 116 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(3), daynight: Some(DayNight::Day), ratio: 1.457, ratio_sigma: 0.149, n: 126 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(4), daynight: Some(DayNight::Day), ratio: 1.219, ratio_sigma: 0.094, n: 127 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(5), daynight: Some(DayNight::Day), ratio: 1.425, ratio_sigma: 0.247, n: 103 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(6), daynight: Some(DayNight::Day), ratio: 0.959, ratio_sigma: 0.099, n: 90 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(7), daynight: Some(DayNight::Night), ratio: 1.525, ratio_sigma: 0.280, n: 64 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(7), daynight: Some(DayNight::Day), ratio: 1.342, ratio_sigma: 0.114, n: 191 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(8), daynight: Some(DayNight::Night), ratio: 1.072, ratio_sigma: 0.231, n: 44 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(8), daynight: Some(DayNight::Day), ratio: 0.976, ratio_sigma: 0.067, n: 146 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(9), daynight: Some(DayNight::Night), ratio: 1.273, ratio_sigma: 0.156, n: 30 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(9), daynight: Some(DayNight::Day), ratio: 1.160, ratio_sigma: 0.068, n: 121 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(10), daynight: Some(DayNight::Day), ratio: 1.036, ratio_sigma: 0.083, n: 185 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(11), daynight: Some(DayNight::Day), ratio: 1.308, ratio_sigma: 0.058, n: 302 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(12), daynight: Some(DayNight::Night), ratio: 1.318, ratio_sigma: 0.230, n: 31 },
        BiomeCompleteness { biome: Biome::Grassland, month: Some(12), daynight: Some(DayNight::Day), ratio: 1.204, ratio_sigma: 0.062, n: 208 },
        BiomeCompleteness { biome: Biome::Cropland, month: None, daynight: None, ratio: 1.325, ratio_sigma: 0.026, n: 4265 },
        BiomeCompleteness { biome: Biome::Cropland, month: None, daynight: Some(DayNight::Night), ratio: 1.487, ratio_sigma: 0.148, n: 313 },
        BiomeCompleteness { biome: Biome::Cropland, month: None, daynight: Some(DayNight::Day), ratio: 1.321, ratio_sigma: 0.026, n: 3952 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(1), daynight: Some(DayNight::Night), ratio: 2.103, ratio_sigma: 0.294, n: 32 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(1), daynight: Some(DayNight::Day), ratio: 1.171, ratio_sigma: 0.064, n: 158 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(2), daynight: Some(DayNight::Night), ratio: 2.592, ratio_sigma: 0.485, n: 40 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(2), daynight: Some(DayNight::Day), ratio: 1.480, ratio_sigma: 0.084, n: 188 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(3), daynight: Some(DayNight::Night), ratio: 1.421, ratio_sigma: 0.343, n: 38 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(3), daynight: Some(DayNight::Day), ratio: 1.331, ratio_sigma: 0.062, n: 318 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(4), daynight: Some(DayNight::Day), ratio: 1.260, ratio_sigma: 0.062, n: 517 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(5), daynight: Some(DayNight::Day), ratio: 1.751, ratio_sigma: 0.113, n: 288 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(6), daynight: Some(DayNight::Day), ratio: 1.462, ratio_sigma: 0.164, n: 116 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(7), daynight: Some(DayNight::Night), ratio: 1.279, ratio_sigma: 0.157, n: 38 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(7), daynight: Some(DayNight::Day), ratio: 1.170, ratio_sigma: 0.095, n: 324 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(8), daynight: Some(DayNight::Night), ratio: 1.794, ratio_sigma: 0.517, n: 40 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(8), daynight: Some(DayNight::Day), ratio: 1.199, ratio_sigma: 0.085, n: 337 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(9), daynight: Some(DayNight::Night), ratio: 0.990, ratio_sigma: 0.170, n: 31 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(9), daynight: Some(DayNight::Day), ratio: 1.148, ratio_sigma: 0.074, n: 462 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(10), daynight: Some(DayNight::Day), ratio: 1.207, ratio_sigma: 0.073, n: 657 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(11), daynight: Some(DayNight::Day), ratio: 1.368, ratio_sigma: 0.067, n: 378 },
        BiomeCompleteness { biome: Biome::Cropland, month: Some(12), daynight: Some(DayNight::Day), ratio: 1.233, ratio_sigma: 0.097, n: 209 },
    ]
}
// END GENERATED BIOME TABLE

/// Level of the lookup chain that supplied a completeness factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletenessSource {
    MeasuredMonth,
    MeasuredPooledMonths,
    MeasuredPooledBiome,
    PowerLaw,
    LatBand,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Completeness {
    pub ratio: f64,
    pub ratio_sigma: f64,
    pub source: CompletenessSource,
}

/// `B(a, b) = ∫ P · P^(-alpha) dP` over `[a, b]`: the FRP carried by fires in that range.
fn frp_budget(alpha: f64, a: f64, b: f64) -> f64 {
    let e = 2.0 - alpha;
    if e.abs() < 1e-12 {
        (b / a).ln()
    } else {
        (b.powf(e) - a.powf(e)) / e
    }
}

/// Old sharp-threshold, nadir-only model: `k = 1 + B(d_viirs, d_modis) / B(d_modis, p_max)`.
/// Kept as the reference limit of the current model (steep sigmoid, nadir areas). `None` for
/// degenerate inputs.
pub fn power_law_factor_raw(alpha: f64, d_viirs_mw: f64, d_modis_mw: f64, p_max_mw: f64) -> Option<f64> {
    let finite = [alpha, d_viirs_mw, d_modis_mw, p_max_mw].iter().all(|v| v.is_finite());
    if !finite || alpha <= 1.0 || d_viirs_mw <= 0.0 || d_modis_mw <= d_viirs_mw || p_max_mw <= d_modis_mw {
        return None;
    }
    let seen = frp_budget(alpha, d_modis_mw, p_max_mw);
    let missed = frp_budget(alpha, d_viirs_mw, d_modis_mw);
    let k = 1.0 + missed / seen;
    k.is_finite().then_some(k)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Detection {
    /// Every fire above `D_p` is seen, none below (reference model for tests).
    #[cfg(test)]
    Sharp,
    /// Observation operator of Sofiev & Kouznetsov (2026), Eq. 5-7.
    Logistic,
}

/// `(pixel area km², weight)` pairs; weights sum to 1.
type AreaGrid = Vec<(f64, f64)>;

#[cfg(test)]
fn nadir_modis_areas() -> AreaGrid {
    vec![(1.0, 1.0)]
}

/// MODIS pixel areas averaged uniformly in scan angle over the swath (the pattern is symmetric,
/// so `[0, 55°]` suffices), Simpson weights. Geometry: `vza::modis_pixel_area_km2`.
fn swath_modis_areas() -> AreaGrid {
    let n = SCAN_GRID_INTERVALS;
    let max = modis_max_scan_angle_rad();
    (0..=n)
        .filter_map(|i| {
            let w = if i == 0 || i == n {
                1.0
            } else if i % 2 == 1 {
                4.0
            } else {
                2.0
            };
            modis_pixel_area_km2(max * i as f64 / n as f64).map(|a| (a, w / (3.0 * n as f64)))
        })
        .collect()
}

/// `p(P; A)` of the observation operator, `max(0, sigmoid - c0) / (1 - c0)`.
fn detection_probability(c: &DetectionLimitCoefficients, d: f64, r: f64, p: f64) -> f64 {
    let sigmoid = 1.0 / (1.0 + (-r * (p - d)).exp());
    (sigmoid - c.c0).max(0.0) / (1.0 - c.c0)
}

/// FRP a sensor sees per unit fire density, `∫ P^(1-alpha) p(P; A) dP`, for one pixel area.
fn seen_frp(
    alpha: f64,
    p_min: f64,
    p_max: f64,
    c: &DetectionLimitCoefficients,
    area: f64,
    detection: Detection,
) -> Option<f64> {
    let d = detection_limit_mw(c, area)?;
    match detection {
        #[cfg(test)]
        Detection::Sharp => Some(if d < p_max { frp_budget(alpha, d.max(p_min), p_max) } else { 0.0 }),
        Detection::Logistic => {
            let r = detection_slope(c, area)?;
            // Substitute P = e^u: the integrand becomes P^(2-alpha) p(P) du.
            let (u0, u1) = (p_min.ln(), p_max.ln());
            let n = P_GRID_INTERVALS;
            let h = (u1 - u0) / n as f64;
            let f = |u: f64| {
                let p = u.exp();
                p.powf(2.0 - alpha) * detection_probability(c, d, r, p)
            };
            let mut sum = f(u0) + f(u1);
            for i in 1..n {
                sum += f(u0 + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
            }
            Some(sum * h / 3.0)
        }
    }
}

fn swath_mean_seen(
    alpha: f64,
    p_min: f64,
    p_max: f64,
    c: &DetectionLimitCoefficients,
    areas: &[(f64, f64)],
    detection: Detection,
) -> Option<f64> {
    areas
        .iter()
        .map(|&(a, w)| seen_frp(alpha, p_min, p_max, c, a, detection).map(|v| v * w))
        .sum()
}

/// `k = E_V[Σ FRP seen by VIIRS] / E_M[Σ FRP seen by MODIS]` for `f(P) ∝ P^(-alpha)` on
/// `[p_min, p_max]`, the same ratio of imperfect observations the archive measures. `None` for
/// `alpha <= 1`, non-finite input, or `p_max` not above the MODIS 50 % limit at nadir.
pub(crate) fn power_law_k(
    alpha: f64,
    p_min: f64,
    p_max: f64,
    daynight: DayNight,
    modis_areas: &[(f64, f64)],
    detection: Detection,
) -> Option<f64> {
    if !(alpha.is_finite() && p_min.is_finite() && p_max.is_finite()) || alpha <= 1.0 || p_min <= 0.0 {
        return None;
    }
    let cm = detection_limit_coefficients(Sensor::ModisAqua, daynight);
    let cv = detection_limit_coefficients(Sensor::ViirsSNPP, daynight);
    if p_max <= detection_limit_mw(&cm, 1.0)? {
        return None;
    }
    let viirs = swath_mean_seen(alpha, p_min, p_max, &cv, &[(VIIRS_NADIR_AREA_KM2, 1.0)], detection)?;
    let modis = swath_mean_seen(alpha, p_min, p_max, &cm, modis_areas, detection)?;
    let k = viirs / modis;
    (modis > 0.0 && k.is_finite()).then_some(k)
}

/// Power-law completeness for `budget` at `daynight`: swath-averaged MODIS (Aqua coefficients,
/// 1 km² nadir to ~9.7 km² at ±55°), nadir VIIRS 375 m (VIIRS keeps pixel growth small via its
/// aggregation zones; `vza.rs` has no VIIRS geometry), logistic detection. Sigma is half the
/// spread of `k` at `alpha ± alpha_sigma`. Integrates numerically; per-record callers should use
/// [`PowerLawCache`].
pub fn power_law_factor(budget: &PowerLawBudget, daynight: DayNight) -> Option<Calibration> {
    let areas = swath_modis_areas();
    let k_at = |alpha: f64| power_law_k(alpha, P_MIN_MW, budget.p_max_mw, daynight, &areas, Detection::Logistic);
    let k = k_at(budget.alpha)?;
    let lo = k_at(budget.alpha - budget.alpha_sigma);
    let hi = k_at(budget.alpha + budget.alpha_sigma);
    let sigma = match (lo, hi) {
        (Some(l), Some(h)) => 0.5 * (h - l).abs(),
        (Some(o), None) | (None, Some(o)) => (o - k).abs(),
        (None, None) => 0.0,
    };
    Some(Calibration {
        ratio: k,
        ratio_sigma: if sigma.is_finite() { sigma } else { 0.0 },
    })
}

/// Memo of [`power_law_factor`] per `(biome, day/night)`. Valid for one config: build one per
/// calendar build and pass it down, so the integration runs once per stratum, not per record.
#[derive(Debug, Default)]
pub struct PowerLawCache {
    entries: Vec<(Biome, DayNight, Option<Calibration>)>,
}

impl PowerLawCache {
    fn get(&mut self, config: &CalibrationConfig, biome: Biome, daynight: DayNight) -> Option<Calibration> {
        if let Some(e) = self.entries.iter().find(|e| e.0 == biome && e.1 == daynight) {
            return e.2;
        }
        let value = config
            .power_law
            .iter()
            .find(|b| b.biome == biome)
            .and_then(|b| power_law_factor(b, daynight));
        self.entries.push((biome, daynight, value));
        value
    }
}

fn measured(
    table: &[BiomeCompleteness],
    biome: Biome,
    month: Option<u8>,
    daynight: Option<DayNight>,
) -> Option<Calibration> {
    table
        .iter()
        .find(|e| e.biome == biome && e.month == month && e.daynight == daynight)
        .map(|e| Calibration {
            ratio: e.ratio,
            ratio_sigma: e.ratio_sigma,
        })
}

/// Completeness factor for one MODIS record in a VIIRS-less cell.
///
/// Chain: measured (biome, month, day/night), measured (biome, day/night), measured (biome),
/// power-law budget (only for biomes configured in `power_law`; none by default), latitude band. Non-burnable or unknown biomes and `mode = lat_band` go
/// straight to the latitude band; a disabled config yields the identity.
pub fn completeness_for(lat: f64, lon: f64, month: u32, daynight: DayNight, config: &CalibrationConfig) -> Completeness {
    completeness_for_cached(lat, lon, month, daynight, config, &mut PowerLawCache::default())
}

/// [`completeness_for`] with a caller-held [`PowerLawCache`] (one per config) for per-record use.
pub fn completeness_for_cached(
    lat: f64,
    lon: f64,
    month: u32,
    daynight: DayNight,
    config: &CalibrationConfig,
    cache: &mut PowerLawCache,
) -> Completeness {
    let lat_band = || {
        let c = calibration_for(lat, config);
        Completeness {
            ratio: c.ratio,
            ratio_sigma: c.ratio_sigma,
            source: CompletenessSource::LatBand,
        }
    };
    if !config.enabled || config.mode == CalibrationMode::LatBand {
        return lat_band();
    }
    let Some(biome) = biome_at(lat, lon).filter(|b| *b != Biome::NonBurnable) else {
        return lat_band();
    };
    let table = &config.biome_table;
    let month = u8::try_from(month).ok().filter(|m| (1..=12).contains(m));
    let found = month
        .and_then(|m| measured(table, biome, Some(m), Some(daynight)))
        .map(|c| (c, CompletenessSource::MeasuredMonth))
        .or_else(|| {
            measured(table, biome, None, Some(daynight)).map(|c| (c, CompletenessSource::MeasuredPooledMonths))
        })
        .or_else(|| measured(table, biome, None, None).map(|c| (c, CompletenessSource::MeasuredPooledBiome)))
        .or_else(|| {
            cache
                .get(config, biome, daynight)
                .map(|c| (c, CompletenessSource::PowerLaw))
        });
    match found {
        Some((c, source)) => Completeness {
            ratio: c.ratio,
            ratio_sigma: c.ratio_sigma,
            source,
        },
        None => lat_band(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    const SAVANNA: (f64, f64) = (-13.0, 132.0); // northern Australia
    const FOREST: (f64, f64) = (-3.0, -60.0); // Amazon
    const CROPLAND: (f64, f64) = (49.0, 32.0); // Ukraine
    const SAHARA: (f64, f64) = (23.0, 12.0);

    fn literature_savanna() -> PowerLawBudget {
        PowerLawBudget {
            biome: Biome::Savanna,
            alpha: 1.31,
            alpha_sigma: DEFAULT_ALPHA_SIGMA,
            p_max_mw: 1000.0,
        }
    }

    fn power_law_cfg() -> CalibrationConfig {
        CalibrationConfig {
            biome_table: Vec::new(),
            power_law: vec![literature_savanna()],
            ..Default::default()
        }
    }

    fn entry(biome: Biome, month: Option<u8>, daynight: Option<DayNight>, ratio: f64) -> BiomeCompleteness {
        BiomeCompleteness {
            biome,
            month,
            daynight,
            ratio,
            ratio_sigma: 0.05,
            n: 100,
        }
    }

    fn cfg_with(table: Vec<BiomeCompleteness>) -> CalibrationConfig {
        CalibrationConfig {
            biome_table: table,
            ..Default::default()
        }
    }

    fn at(p: (f64, f64), month: u32, dn: DayNight, cfg: &CalibrationConfig) -> Completeness {
        completeness_for(p.0, p.1, month, dn, cfg)
    }

    #[test]
    fn test_points_have_the_expected_biomes() {
        assert_eq!(biome_at(SAVANNA.0, SAVANNA.1), Some(Biome::Savanna));
        assert_eq!(biome_at(FOREST.0, FOREST.1), Some(Biome::Forest));
        assert_eq!(biome_at(CROPLAND.0, CROPLAND.1), Some(Biome::Cropland));
        assert_eq!(biome_at(SAHARA.0, SAHARA.1), Some(Biome::NonBurnable));
    }

    #[test]
    fn default_power_law_ships_empty() {
        assert!(default_power_law().is_empty());
        assert!(CalibrationConfig::default().power_law.is_empty());
    }

    #[test]
    fn default_biome_table_is_the_measured_one() {
        let table = default_biome_table();
        assert_eq!(table.len(), 109);
        for e in &table {
            assert!(e.ratio.is_finite() && e.ratio > 0.0, "{e:?}");
            assert!(e.ratio_sigma.is_finite() && e.ratio_sigma > 0.0, "{e:?}");
            assert!(e.n >= 30, "{e:?}");
            assert!(e.month.is_none() || e.daynight.is_some(), "{e:?}");
        }
        for (i, a) in table.iter().enumerate() {
            for b in &table[i + 1..] {
                assert!(
                    (a.biome, a.month, a.daynight) != (b.biome, b.month, b.daynight),
                    "duplicate stratum {a:?}"
                );
            }
        }
        for biome in [Biome::Forest, Biome::Shrubland, Biome::Savanna, Biome::Grassland, Biome::Cropland] {
            assert!(measured(&table, biome, None, None).is_some(), "{biome:?}");
            for dn in [DayNight::Day, DayNight::Night] {
                assert!(measured(&table, biome, None, Some(dn)).is_some(), "{biome:?} {dn:?}");
            }
        }
        // Tundra and wetland were not measured (too few paired cell-days).
        assert!(table.iter().all(|e| !matches!(e.biome, Biome::Tundra | Biome::Wetland)));
        // Ratios below 1 are legitimate measurements and the shipped validation accepts them.
        assert!(table.iter().any(|e| e.ratio < 1.0));
        crate::config::PipelineConfig::default().validate().unwrap();
    }

    #[test]
    fn measured_ratio_below_one_is_applied_as_measured() {
        // Forest, March, day: VIIRS total below the VZA-corrected MODIS total.
        let c = at(FOREST, 3, DayNight::Day, &CalibrationConfig::default());
        assert_eq!((c.ratio, c.source), (0.946, CompletenessSource::MeasuredMonth));
        let cfg = cfg_with(vec![entry(Biome::Savanna, Some(7), Some(DayNight::Day), 0.9)]);
        let c = at(SAVANNA, 7, DayNight::Day, &cfg);
        assert_eq!((c.ratio, c.source), (0.9, CompletenessSource::MeasuredMonth));
        let pipeline = crate::config::PipelineConfig {
            calibration: cfg,
            ..Default::default()
        };
        assert!(pipeline.validate().is_ok());
    }

    fn find_biome(wanted: Biome) -> (f64, f64) {
        for lat in -60..=75 {
            for lon in -180..180 {
                let p = (f64::from(lat) + 0.5, f64::from(lon) + 0.5);
                if biome_at(p.0, p.1) == Some(wanted) {
                    return p;
                }
            }
        }
        panic!("no {wanted:?} cell found");
    }

    #[test]
    fn real_table_tells_the_biomes_apart_with_the_expected_sources() {
        let cfg = CalibrationConfig::default();
        let s = at(SAVANNA, 7, DayNight::Day, &cfg);
        let f = at(FOREST, 7, DayNight::Day, &cfg);
        let c = at(CROPLAND, 7, DayNight::Day, &cfg);
        for x in [&s, &f, &c] {
            assert_eq!(x.source, CompletenessSource::MeasuredMonth);
        }
        assert_eq!((s.ratio, s.ratio_sigma), (1.084, 0.016));
        assert_eq!((f.ratio, f.ratio_sigma), (1.158, 0.027));
        assert_eq!((c.ratio, c.ratio_sigma), (1.170, 0.095));
    }

    #[test]
    fn real_table_missing_month_falls_to_pooled_levels() {
        let cfg = CalibrationConfig::default();
        // Savanna April has a day row but no night row: pooled over months for night.
        assert_eq!(at(SAVANNA, 4, DayNight::Day, &cfg).source, CompletenessSource::MeasuredMonth);
        let c = at(SAVANNA, 4, DayNight::Night, &cfg);
        assert_eq!((c.ratio, c.source), (1.319, CompletenessSource::MeasuredPooledMonths));
        // Without the day/night pooled row either, the biome-pooled row answers.
        let mut table = default_biome_table();
        table.retain(|e| !(e.biome == Biome::Savanna && e.daynight == Some(DayNight::Night)));
        let c = at(SAVANNA, 4, DayNight::Night, &cfg_with(table));
        assert_eq!((c.ratio, c.source), (1.193, CompletenessSource::MeasuredPooledBiome));
    }

    #[test]
    fn real_table_unmeasured_and_non_burnable_use_the_latitude_band() {
        let cfg = CalibrationConfig::default();
        for biome in [Biome::Tundra, Biome::Wetland] {
            let p = find_biome(biome);
            let c = at(p, 7, DayNight::Day, &cfg);
            let band = calibration_for(p.0, &cfg);
            assert_eq!(c.source, CompletenessSource::LatBand, "{biome:?}");
            assert_eq!((c.ratio, c.ratio_sigma), (band.ratio, band.ratio_sigma));
        }
        let c = at(SAHARA, 7, DayNight::Day, &cfg);
        assert_eq!(c.source, CompletenessSource::LatBand);
        assert_eq!(c.ratio, calibration_for(SAHARA.0, &cfg).ratio);
    }

    #[test]
    fn default_chain_never_reaches_the_power_law() {
        let cfg = CalibrationConfig::default();
        for p in [SAVANNA, FOREST, CROPLAND] {
            for month in 1..=12 {
                for dn in [DayNight::Day, DayNight::Night] {
                    assert_ne!(at(p, month, dn, &cfg).source, CompletenessSource::PowerLaw);
                }
            }
        }
    }

    #[test]
    fn chain_levels_win_in_order() {
        let (b, dn) = (Biome::Savanna, DayNight::Day);
        let mut table = vec![
            entry(b, Some(7), Some(dn), 1.5),
            entry(b, None, Some(dn), 1.4),
            entry(b, None, None, 1.3),
        ];
        let cfg = cfg_with(table.clone());
        let c = at(SAVANNA, 7, dn, &cfg);
        assert_eq!((c.ratio, c.source), (1.5, CompletenessSource::MeasuredMonth));

        // Another month: pooled over months for the same day/night.
        let c = at(SAVANNA, 8, dn, &cfg);
        assert_eq!((c.ratio, c.source), (1.4, CompletenessSource::MeasuredPooledMonths));

        // Night has no day/night entry: pooled over biome.
        let c = at(SAVANNA, 7, DayNight::Night, &cfg);
        assert_eq!((c.ratio, c.source), (1.3, CompletenessSource::MeasuredPooledBiome));

        // Without the biome-pooled entry a configured power law answers.
        table.pop();
        let cfg = CalibrationConfig {
            biome_table: table,
            ..power_law_cfg()
        };
        let c = at(SAVANNA, 7, DayNight::Night, &cfg);
        assert_eq!(c.source, CompletenessSource::PowerLaw);

        // Without power-law parameters (the default) the latitude band answers.
        let cfg = cfg_with(Vec::new());
        let c = at(SAVANNA, 7, dn, &cfg);
        assert_eq!(c.source, CompletenessSource::LatBand);
        let band = calibration_for(SAVANNA.0, &cfg);
        assert_eq!((c.ratio, c.ratio_sigma), (band.ratio, band.ratio_sigma));
    }

    #[test]
    fn table_entries_of_another_biome_or_month_do_not_leak() {
        let cfg = cfg_with(vec![entry(Biome::Forest, Some(7), Some(DayNight::Day), 1.9)]);
        assert_eq!(at(SAVANNA, 7, DayNight::Day, &cfg).source, CompletenessSource::LatBand);
        assert_eq!(at(FOREST, 8, DayNight::Day, &cfg).source, CompletenessSource::LatBand);
        assert_eq!(at(FOREST, 7, DayNight::Night, &cfg).source, CompletenessSource::LatBand);
        assert_eq!(at(FOREST, 7, DayNight::Day, &cfg).ratio, 1.9);
        // An invalid month never matches a monthly entry.
        assert_eq!(at(FOREST, 0, DayNight::Day, &cfg).source, CompletenessSource::LatBand);
    }

    #[test]
    fn non_burnable_unknown_and_lat_band_mode_equal_calibration_for() {
        let every = vec![
            entry(Biome::Savanna, None, None, 1.9),
            entry(Biome::NonBurnable, None, None, 1.9),
        ];
        let cfg = cfg_with(every);
        let lat_mode = CalibrationConfig {
            mode: CalibrationMode::LatBand,
            ..cfg.clone()
        };
        for (p, c) in [(SAHARA, &cfg), (SAVANNA, &lat_mode), (CROPLAND, &lat_mode), (FOREST, &lat_mode)] {
            let got = at(p, 7, DayNight::Day, c);
            let want = calibration_for(p.0, c);
            assert_eq!(got.source, CompletenessSource::LatBand);
            assert_eq!((got.ratio, got.ratio_sigma), (want.ratio, want.ratio_sigma));
        }
        // Out-of-range longitude has no biome.
        assert_eq!(biome_at(0.0, 200.0), None);
        let got = completeness_for(0.0, 200.0, 7, DayNight::Day, &cfg);
        assert_eq!(got.source, CompletenessSource::LatBand);
        assert_eq!(got.ratio, calibration_for(0.0, &cfg).ratio);
    }

    #[test]
    fn disabled_config_is_the_identity_in_biome_mode() {
        let cfg = CalibrationConfig {
            enabled: false,
            ..Default::default()
        };
        let c = at(SAVANNA, 7, DayNight::Day, &cfg);
        assert_eq!((c.ratio, c.ratio_sigma), (1.0, 0.0));
    }

    #[test]
    fn savanna_forest_and_cropland_are_told_apart() {
        let cfg = cfg_with(vec![
            entry(Biome::Savanna, None, None, 1.6),
            entry(Biome::Cropland, None, None, 1.2),
        ]);
        let s = at(SAVANNA, 7, DayNight::Day, &cfg);
        let c = at(CROPLAND, 7, DayNight::Day, &cfg);
        let f = at(FOREST, 7, DayNight::Day, &cfg);
        assert_eq!((s.ratio, s.source), (1.6, CompletenessSource::MeasuredPooledBiome));
        assert_eq!((c.ratio, c.source), (1.2, CompletenessSource::MeasuredPooledBiome));
        assert_eq!(f.source, CompletenessSource::LatBand);
    }

    #[test]
    fn power_law_matches_hand_computed_values() {
        // alpha = 2 uses the log form: k = 1 + ln(2/1) / ln(4/2) = 2.
        assert!(approx_eq(power_law_factor_raw(2.0, 1.0, 2.0, 4.0).unwrap(), 2.0));
        // alpha = 3: B(a, b) = 1/a - 1/b -> missed 0.5, seen 0.25, k = 3.
        assert!(approx_eq(power_law_factor_raw(3.0, 1.0, 2.0, 4.0).unwrap(), 3.0));
        // alpha = 1.5: B(a, b) = 2 (sqrt b - sqrt a) -> missed 2, seen 4, k = 1.5.
        assert!(approx_eq(power_law_factor_raw(1.5, 1.0, 4.0, 16.0).unwrap(), 1.5));
        // Near alpha = 2 the closed form is continuous with the log form.
        let near = power_law_factor_raw(2.0 + 1e-9, 1.0, 2.0, 4.0).unwrap();
        assert!((near - 2.0).abs() < 1e-6);
    }

    #[test]
    fn power_law_rejects_degenerate_inputs() {
        assert!(power_law_factor_raw(1.31, 2.0, 5.0, 100.0).is_some());
        for (a, dv, dm, pm) in [
            (1.0, 2.0, 5.0, 100.0),
            (0.5, 2.0, 5.0, 100.0),
            (f64::NAN, 2.0, 5.0, 100.0),
            (1.31, 5.0, 5.0, 100.0),
            (1.31, 6.0, 5.0, 100.0),
            (1.31, 0.0, 5.0, 100.0),
            (1.31, 2.0, 5.0, 5.0),
            (1.31, 2.0, 5.0, 4.0),
            (1.31, 2.0, f64::INFINITY, 100.0),
        ] {
            assert_eq!(power_law_factor_raw(a, dv, dm, pm), None, "{a} {dv} {dm} {pm}");
        }
        let budget = PowerLawBudget {
            biome: Biome::Savanna,
            alpha: 1.31,
            alpha_sigma: 0.1,
            p_max_mw: 3.0,
        };
        // p_max below the MODIS limit: no factor, the chain falls through to the band.
        assert_eq!(power_law_factor(&budget, DayNight::Day), None);
        let cfg = CalibrationConfig {
            biome_table: Vec::new(),
            power_law: vec![budget],
            ..Default::default()
        };
        assert_eq!(at(SAVANNA, 7, DayNight::Day, &cfg).source, CompletenessSource::LatBand);
    }

    fn k_of(alpha: f64, p_max: f64, dn: DayNight, areas: &[(f64, f64)], det: Detection) -> f64 {
        power_law_k(alpha, P_MIN_MW, p_max, dn, areas, det).unwrap()
    }

    fn logistic_swath(alpha: f64, p_max: f64, dn: DayNight) -> f64 {
        k_of(alpha, p_max, dn, &swath_modis_areas(), Detection::Logistic)
    }

    #[test]
    fn swath_geometry_runs_from_one_to_about_the_documented_edge_area() {
        let areas = swath_modis_areas();
        assert!((areas.iter().map(|a| a.1).sum::<f64>() - 1.0).abs() < 1e-12);
        assert!((areas[0].0 - 1.0).abs() < 1e-6, "nadir {}", areas[0].0);
        let edge = areas.last().unwrap().0;
        assert!((edge - 9.66).abs() < 0.15, "edge {edge}");
    }

    #[test]
    fn sharp_nadir_limit_reproduces_the_old_closed_form() {
        let d_v: f64 = 6.17 * 0.375 * 0.375 + 1.44;
        let d_m: f64 = 4.44 + 0.52;
        let want = power_law_factor_raw(1.31, d_v, d_m, 1000.0).unwrap();
        let got = k_of(1.31, 1000.0, DayNight::Day, &nadir_modis_areas(), Detection::Sharp);
        assert!((got - want).abs() < 1e-12, "{got} vs {want}");
        // A very steep sigmoid converges to the step; scaling r makes the logistic a step.
        let mut cm = detection_limit_coefficients(Sensor::ModisAqua, DayNight::Day);
        cm.r0 *= 200.0;
        cm.r1 *= 200.0;
        let steep = seen_frp(1.31, P_MIN_MW, 1000.0, &cm, 1.0, Detection::Logistic).unwrap();
        let step = seen_frp(1.31, P_MIN_MW, 1000.0, &cm, 1.0, Detection::Sharp).unwrap();
        // The grid cannot resolve the step exactly; 1 % of the seen budget is enough here.
        assert!((steep / step - 1.0).abs() < 0.01, "{steep} vs {step}");
    }

    #[test]
    fn k_is_at_least_one_and_grows_with_the_modis_edge_area() {
        for dn in [DayNight::Day, DayNight::Night] {
            let nadir = k_of(1.31, 1000.0, dn, &nadir_modis_areas(), Detection::Logistic);
            let swath = logistic_swath(1.31, 1000.0, dn);
            let edge = k_of(1.31, 1000.0, dn, &[(9.66, 1.0)], Detection::Logistic);
            assert!(nadir >= 1.0 && swath >= 1.0, "{nadir} {swath}");
            assert!(nadir < swath && swath < edge, "{nadir} {swath} {edge}");
        }
    }

    #[test]
    fn k_falls_toward_one_as_alpha_decreases() {
        let mut prev = f64::INFINITY;
        for alpha in [2.2, 1.8, 1.5, 1.31, 1.2] {
            let k = logistic_swath(alpha, 1000.0, DayNight::Day);
            assert!(k < prev, "alpha {alpha}: {k} !< {prev}");
            prev = k;
        }
    }

    #[test]
    fn day_and_night_differ() {
        let d = logistic_swath(1.31, 1000.0, DayNight::Day);
        let n = logistic_swath(1.31, 1000.0, DayNight::Night);
        assert!((d - n).abs() > 1e-3, "{d} {n}");
    }

    #[test]
    fn result_does_not_depend_on_p_min_or_grid() {
        let areas = swath_modis_areas();
        for dn in [DayNight::Day, DayNight::Night] {
            let base = power_law_k(1.31, 0.05, 1000.0, dn, &areas, Detection::Logistic).unwrap();
            // Below ~0.1 MW the VIIRS night operator is under c0; a floor of 0.5 MW would cut real detections.
            for p_min in [0.1, 0.02, 0.01] {
                let k = power_law_k(1.31, p_min, 1000.0, dn, &areas, Detection::Logistic).unwrap();
                assert!((k / base - 1.0).abs() < 1e-4, "{dn:?} p_min {p_min}: {k} vs {base}");
            }
        }
    }

    #[test]
    fn model_is_deterministic_bit_for_bit() {
        let budget = literature_savanna();
        let a = power_law_factor(&budget, DayNight::Day).unwrap();
        let b = power_law_factor(&budget, DayNight::Day).unwrap();
        assert_eq!(a.ratio.to_bits(), b.ratio.to_bits());
        assert_eq!(a.ratio_sigma.to_bits(), b.ratio_sigma.to_bits());
    }

    #[test]
    fn literature_savanna_factor_is_finite_with_positive_sigma() {
        let budget = literature_savanna();
        for dn in [DayNight::Day, DayNight::Night] {
            let c = power_law_factor(&budget, dn).unwrap();
            assert!(c.ratio > 1.0 && c.ratio.is_finite());
            assert!(c.ratio_sigma > 0.0 && c.ratio_sigma.is_finite());
            // Sigma is half the spread of k at alpha +- alpha_sigma.
            let lo = logistic_swath(budget.alpha - budget.alpha_sigma, budget.p_max_mw, dn);
            let hi = logistic_swath(budget.alpha + budget.alpha_sigma, budget.p_max_mw, dn);
            assert!((c.ratio_sigma - 0.5 * (hi - lo).abs()).abs() < 1e-12);
        }
    }

    #[test]
    fn alpha_sigma_reaching_one_keeps_sigma_finite() {
        let budget = PowerLawBudget {
            biome: Biome::Savanna,
            alpha: 1.05,
            alpha_sigma: 0.2,
            p_max_mw: 1000.0,
        };
        let c = power_law_factor(&budget, DayNight::Day).unwrap();
        assert!(c.ratio_sigma.is_finite() && c.ratio_sigma >= 0.0);
        let zero = PowerLawBudget {
            alpha_sigma: 0.0,
            ..budget
        };
        assert_eq!(power_law_factor(&zero, DayNight::Day).unwrap().ratio_sigma, 0.0);
    }

    #[test]
    fn cache_returns_the_direct_value_and_fills_once() {
        let cfg = power_law_cfg();
        let direct = power_law_factor(&literature_savanna(), DayNight::Day).unwrap();
        let mut cache = PowerLawCache::default();
        for _ in 0..3 {
            let c = completeness_for_cached(SAVANNA.0, SAVANNA.1, 7, DayNight::Day, &cfg, &mut cache);
            assert_eq!(c.source, CompletenessSource::PowerLaw);
            assert_eq!(c.ratio.to_bits(), direct.ratio.to_bits());
            assert_eq!(c.ratio_sigma.to_bits(), direct.ratio_sigma.to_bits());
        }
        assert_eq!(cache.entries.len(), 1);
        let plain = at(SAVANNA, 7, DayNight::Day, &cfg);
        assert_eq!(plain.ratio.to_bits(), direct.ratio.to_bits());
    }

    #[test]
    fn logistic_k_guards_degenerate_inputs() {
        let areas = nadir_modis_areas();
        let k = |a: f64, pmin: f64, pmax: f64| power_law_k(a, pmin, pmax, DayNight::Day, &areas, Detection::Logistic);
        assert!(k(1.31, 0.05, 1000.0).is_some());
        assert_eq!(k(1.0, 0.05, 1000.0), None);
        assert_eq!(k(0.5, 0.05, 1000.0), None);
        assert_eq!(k(f64::NAN, 0.05, 1000.0), None);
        assert_eq!(k(1.31, 0.05, f64::INFINITY), None);
        assert_eq!(k(1.31, 0.0, 1000.0), None);
        assert_eq!(k(1.31, 0.05, 4.96), None);
    }

    /// Prints the numbers quoted in the docs; run with `-- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_k14_power_law_report() {
        let (nadir, swath) = (nadir_modis_areas(), swath_modis_areas());
        for dn in [DayNight::Day, DayNight::Night] {
            println!("== {dn:?}: alpha p_max | old(sharp nadir) swath+sharp new(swath+logistic)");
            for p_max in [300.0, 1000.0, 3000.0] {
                for alpha in [1.2, 1.31, 1.5, 1.8, 2.2] {
                    let f = |a: &[(f64, f64)], d| power_law_k(alpha, P_MIN_MW, p_max, dn, a, d).unwrap();
                    println!(
                        "{alpha:.2} {p_max:6.0} | {:.4} {:.4} {:.4}",
                        f(&nadir, Detection::Sharp),
                        f(&swath, Detection::Sharp),
                        f(&swath, Detection::Logistic)
                    );
                }
            }
            let nl = power_law_k(1.31, P_MIN_MW, 1000.0, dn, &nadir, Detection::Logistic).unwrap();
            println!("alpha 1.31 nadir+logistic {nl:.4}");
            let c = power_law_factor(&literature_savanna(), dn).unwrap();
            println!("literature savanna {:.4} sigma {:.4}", c.ratio, c.ratio_sigma);
        }
        println!("edge area {:.4}", swath.last().unwrap().0);
        let t = std::time::Instant::now();
        let _ = power_law_factor(&literature_savanna(), DayNight::Day);
        println!("one factor (3 k evaluations): {:?}", t.elapsed());
    }

    #[test]
    fn band_selection_and_interpolation_match_spec() {
        let cfg = CalibrationConfig::default();

        let eq = calibration_for(0.0, &cfg);
        assert!(approx_eq(eq.ratio, 1.11));
        assert!(approx_eq(eq.ratio_sigma, 0.08));

        let south_tropics = calibration_for(-10.0, &cfg);
        assert!(approx_eq(south_tropics.ratio, 1.11));
        assert!(approx_eq(south_tropics.ratio_sigma, 0.08));

        let mid = calibration_for(40.0, &cfg);
        assert!(approx_eq(mid.ratio, 1.18));
        assert!(approx_eq(mid.ratio_sigma, 0.15));

        let gap_mid = calibration_for(52.5, &cfg);
        assert!(approx_eq(gap_mid.ratio, 0.5 * (1.18 + 1.28)));
        assert!(approx_eq(gap_mid.ratio_sigma, 0.5 * (0.15 + 0.11)));

        let boreal = calibration_for(60.0, &cfg);
        assert!(approx_eq(boreal.ratio, 1.28));
        assert!(approx_eq(boreal.ratio_sigma, 0.11));

        let polar = calibration_for(80.0, &cfg);
        assert!(approx_eq(polar.ratio, 1.28));
        assert!(approx_eq(polar.ratio_sigma, 0.11));

        let south_polar = calibration_for(-80.0, &cfg);
        assert!(approx_eq(south_polar.ratio, 1.28));
    }

    #[test]
    fn invalid_and_nan_latitudes_return_identity() {
        let cfg = CalibrationConfig::default();
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 90.1, -90.1] {
            let cal = calibration_for(bad, &cfg);
            assert_eq!(cal.ratio, 1.0, "expected 1.0 for {bad}");
            assert_eq!(cal.ratio_sigma, 0.0, "expected 0.0 for {bad}");
        }
    }

    #[test]
    fn disabled_or_empty_config_returns_identity() {
        let disabled = CalibrationConfig {
            enabled: false,
            ..Default::default()
        };
        assert_eq!(
            calibration_for(10.0, &disabled),
            Calibration {
                ratio: 1.0,
                ratio_sigma: 0.0
            }
        );

        let empty = CalibrationConfig {
            enabled: true,
            bands: Vec::new(),
            ..Default::default()
        };
        assert_eq!(
            calibration_for(10.0, &empty),
            Calibration {
                ratio: 1.0,
                ratio_sigma: 0.0
            }
        );
    }

    #[test]
    fn band_boundaries_and_interpolation_monotonicity() {
        let cfg = CalibrationConfig::default();
        assert!(approx_eq(calibration_for(30.0, &cfg).ratio, 1.11));
        assert!(approx_eq(calibration_for(30.0001, &cfg).ratio, 1.18));
        assert!(approx_eq(calibration_for(50.0, &cfg).ratio, 1.18));
        assert!(approx_eq(calibration_for(55.0, &cfg).ratio, 1.28));
        assert!(approx_eq(calibration_for(70.0, &cfg).ratio, 1.28));

        let r51 = calibration_for(51.0, &cfg).ratio;
        let r52 = calibration_for(52.5, &cfg).ratio;
        let r54 = calibration_for(54.0, &cfg).ratio;
        assert!(1.18 < r51 && r51 < r52 && r52 < r54 && r54 < 1.28);
    }

    fn tuple(lat: f64) -> (f64, f64) {
        let c = calibration_for(lat, &CalibrationConfig::default());
        (c.ratio, c.ratio_sigma)
    }

    #[test]
    fn k_c_is_symmetric_between_hemispheres_on_a_grid() {
        for abs in (0..=90).map(f64::from).chain([29.9, 50.5, 52.5, 54.9]) {
            assert_eq!(tuple(abs), tuple(-abs), "asymmetry at {abs}");
        }
    }

    #[test]
    fn band_edges_use_three_value_bva_around_30_50_55_70() {
        let near = |lat: f64, expected: f64, tol: f64| {
            let r = tuple(lat).0;
            assert!((r - expected).abs() <= tol, "lat {lat}: {r} vs {expected}");
        };
        near(29.999, 1.11, 0.0);
        near(30.0, 1.11, 0.0);
        near(30.001, 1.18, 0.0);
        near(49.999, 1.18, 0.0);
        near(50.0, 1.18, 0.0);
        near(50.001, 1.18, 1e-3); // start of the 50-55 transition: continuous with 1.18
        near(54.999, 1.28, 1e-3); // end of the transition: continuous with 1.28
        near(55.0, 1.28, 0.0);
        near(55.001, 1.28, 0.0);
        near(69.999, 1.28, 0.0);
        near(70.0, 1.28, 0.0);
        near(70.001, 1.28, 0.0);
    }

    #[test]
    fn poles_and_signed_zero_are_valid_latitudes() {
        assert_eq!(tuple(90.0), (1.28, 0.11));
        assert_eq!(tuple(-90.0), (1.28, 0.11));
        assert_eq!(tuple(-0.0), (1.11, 0.08));
    }

    #[test]
    fn transition_interpolates_linearly_for_ratio_and_sigma() {
        for (lat, t) in [(51.0, 0.2), (52.5, 0.5), (54.0, 0.8)] {
            let (ratio, sigma) = tuple(lat);
            assert!(approx_eq(ratio, 1.18 + t * (1.28 - 1.18)), "ratio at {lat}");
            assert!(approx_eq(sigma, 0.15 + t * (0.11 - 0.15)), "sigma at {lat}");
        }
    }

    #[test]
    fn ratio_is_non_decreasing_and_stays_within_band_limits_over_valid_latitudes() {
        let mut prev = 0.0;
        for i in 0..=900 {
            let (ratio, sigma) = tuple(f64::from(i) / 10.0);
            assert!((1.11..=1.28).contains(&ratio), "ratio {ratio} at {i}");
            assert!((0.08..=0.15).contains(&sigma), "sigma {sigma} at {i}");
            assert!(ratio >= prev, "ratio decreased at {i}");
            prev = ratio;
        }
    }

    #[test]
    fn latitude_below_first_band_clamps_to_first_band() {
        let cfg = CalibrationConfig {
            enabled: true,
            bands: vec![CalibrationBand {
                min_abs_lat: 10.0,
                max_abs_lat: 20.0,
                ratio: 1.5,
                ratio_sigma: 0.2,
            }],
            ..Default::default()
        };
        assert_eq!(calibration_for(0.0, &cfg), Calibration { ratio: 1.5, ratio_sigma: 0.2 });
        assert_eq!(calibration_for(85.0, &cfg), Calibration { ratio: 1.5, ratio_sigma: 0.2 });
    }

    #[test]
    fn touching_bands_have_no_interpolation_gap() {
        let band = |min, max, ratio| CalibrationBand {
            min_abs_lat: min,
            max_abs_lat: max,
            ratio,
            ratio_sigma: 0.0,
        };
        let cfg = CalibrationConfig {
            enabled: true,
            bands: vec![band(0.0, 30.0, 1.0), band(30.0, 60.0, 2.0)],
            ..Default::default()
        };
        assert_eq!(calibration_for(30.0, &cfg).ratio, 1.0);
        assert_eq!(calibration_for(30.0001, &cfg).ratio, 2.0);
    }
}
