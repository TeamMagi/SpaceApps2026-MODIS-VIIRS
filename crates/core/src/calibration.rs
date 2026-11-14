use serde::{Deserialize, Serialize};

use crate::config::CalibrationConfig;

/// Latitude-band MODIS->VIIRS FRP calibration ratio (`VIIRS / MODIS`) and uncertainty.
///
/// Based on Li et al. (2018), "Comparison of Fire Radiative Power Estimates From VIIRS
/// and MODIS Observations" (JGR: Atmospheres, doi:10.1029/2017JD027823).
///
/// Limitation: Li et al. (2018) also reports that in large boreal fire clusters (>1000 MW),
/// MODIS FRP can exceed VIIRS FRP by up to 47% due to smoke attenuation in VIIRS M13.
/// That cluster-scale adjustment is intentionally NOT applied here because per-detection
/// fire-cluster FRP is not available during hotspot-level harmonization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationBand {
    pub min_abs_lat: f64,
    pub max_abs_lat: f64,
    pub ratio: f64,
    /// Derived spread estimate from the paper's reported ranges (NOT a fitted sigma
    /// reported in Li et al. 2018); used downstream for uncertainty intervals.
    pub ratio_sigma: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    pub ratio: f64,
    pub ratio_sigma: f64,
}

pub(crate) fn default_bands() -> Vec<CalibrationBand> {
    vec![
        // Tropics (30 S - 30 N): VIIRS FRP averages 16% larger than MODIS.
        // ratio_sigma = 0.13 is a derived estimate from the <13% spread across tropical
        // and broadleaf forest ecosystems vs ~1% in savanna/cropland (not a reported sigma).
        CalibrationBand {
            min_abs_lat: 0.0,
            max_abs_lat: 30.0,
            ratio: 1.16,
            ratio_sigma: 0.13,
        },
        // Mid-latitudes (30 N - 50 N): VIIRS FRP up to 3% smaller than MODIS.
        // ratio_sigma = 0.03 is a derived estimate from that 3% bound (not a reported sigma).
        CalibrationBand {
            min_abs_lat: 30.0,
            max_abs_lat: 50.0,
            ratio: 0.97,
            ratio_sigma: 0.03,
        },
        // High latitudes / boreal (55 N - 70 N): VIIRS FRP up to 18% smaller than MODIS.
        // ratio_sigma = 0.18 is a derived estimate from that 18% bound (not a reported sigma).
        CalibrationBand {
            min_abs_lat: 55.0,
            max_abs_lat: 70.0,
            ratio: 0.82,
            ratio_sigma: 0.18,
        },
    ]
}

/// Returns the MODIS->VIIRS FRP calibration factor for `lat` under `config`.
///
/// Uses `|lat|` (hemispheric symmetry): Li et al. (2018) reports mid- and high-latitude
/// ratios only for the Northern Hemisphere and gives no separate Southern Hemisphere
/// mid/high-latitude values, so we mirror by absolute latitude across hemispheres.
/// Transition gaps between adjacent bands (e.g., 50-55 deg) are linearly interpolated,
/// latitudes beyond the last band (>70 deg) clamp to the last band, and invalid/NaN
/// latitudes or disabled configs return the identity calibration (`ratio = 1.0`, `ratio_sigma = 0.0`).
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

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn band_selection_and_interpolation_match_spec() {
        let cfg = CalibrationConfig::default();

        let eq = calibration_for(0.0, &cfg);
        assert!(approx_eq(eq.ratio, 1.16));
        assert!(approx_eq(eq.ratio_sigma, 0.13));

        let south_tropics = calibration_for(-10.0, &cfg);
        assert!(approx_eq(south_tropics.ratio, 1.16));
        assert!(approx_eq(south_tropics.ratio_sigma, 0.13));

        let mid = calibration_for(40.0, &cfg);
        assert!(approx_eq(mid.ratio, 0.97));
        assert!(approx_eq(mid.ratio_sigma, 0.03));

        let gap_mid = calibration_for(52.5, &cfg);
        assert!(approx_eq(gap_mid.ratio, 0.5 * (0.97 + 0.82)));
        assert!(approx_eq(gap_mid.ratio_sigma, 0.5 * (0.03 + 0.18)));

        let boreal = calibration_for(60.0, &cfg);
        assert!(approx_eq(boreal.ratio, 0.82));
        assert!(approx_eq(boreal.ratio_sigma, 0.18));

        let polar = calibration_for(80.0, &cfg);
        assert!(approx_eq(polar.ratio, 0.82));
        assert!(approx_eq(polar.ratio_sigma, 0.18));

        let south_polar = calibration_for(-80.0, &cfg);
        assert!(approx_eq(south_polar.ratio, 0.82));
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
        assert!(approx_eq(calibration_for(30.0, &cfg).ratio, 1.16));
        assert!(approx_eq(calibration_for(30.0001, &cfg).ratio, 0.97));
        assert!(approx_eq(calibration_for(50.0, &cfg).ratio, 0.97));
        assert!(approx_eq(calibration_for(55.0, &cfg).ratio, 0.82));
        assert!(approx_eq(calibration_for(70.0, &cfg).ratio, 0.82));

        let r51 = calibration_for(51.0, &cfg).ratio;
        let r52 = calibration_for(52.5, &cfg).ratio;
        let r54 = calibration_for(54.0, &cfg).ratio;
        assert!(0.97 > r51 && r51 > r52 && r52 > r54 && r54 > 0.82);
    }
}
