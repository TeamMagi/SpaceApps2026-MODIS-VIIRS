use serde::{Deserialize, Serialize};

use crate::config::CalibrationConfig;

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
        };
        assert_eq!(calibration_for(30.0, &cfg).ratio, 1.0);
        assert_eq!(calibration_for(30.0001, &cfg).ratio, 2.0);
    }
}
