use crate::config::VzaConfig;
use crate::models::{DayNight, Hotspot, Sensor};

const EARTH_RADIUS_KM: f64 = 6371.0;
const MODIS_ALTITUDE_KM: f64 = 705.0;
const MODIS_NADIR_SCAN_KM: f64 = 1.0;
const MODIS_MAX_SCAN_ANGLE_DEG: f64 = 55.0;

/// Along-scan pixel size on a spherical Earth for a satellite scan angle `theta_s_rad`.
///
/// Uses the spherical-Earth MODIS geolocation footprint relation with scale factor
/// `C = p0 * R / h` calibrated to match `MODIS_NADIR_SCAN_KM` at nadir (`theta_s = 0`).
fn scan_size_km(theta_s_rad: f64) -> Option<f64> {
    let cos_t = theta_s_rad.cos();
    if !cos_t.is_finite() || cos_t <= 0.0 {
        return None;
    }

    let orbit_radius_km = EARTH_RADIUS_KM + MODIS_ALTITUDE_KM;
    let sin_t = theta_s_rad.sin();
    let radicand = EARTH_RADIUS_KM.powi(2) - (orbit_radius_km * sin_t).powi(2);
    if !radicand.is_finite() || radicand <= 0.0 {
        return None;
    }

    // Calibrate the spherical-Earth projection scale so dx(0) equals MODIS_NADIR_SCAN_KM.
    let scale_c = MODIS_NADIR_SCAN_KM * EARTH_RADIUS_KM / MODIS_ALTITUDE_KM;
    let dx = scale_c * ((orbit_radius_km * cos_t) / radicand.sqrt() - 1.0);
    if !dx.is_finite() || dx <= 0.0 {
        return None;
    }

    Some(dx)
}

fn theta_s_to_vza_deg(theta_s_rad: f64) -> Option<f64> {
    let sin_vza = ((EARTH_RADIUS_KM + MODIS_ALTITUDE_KM) / EARTH_RADIUS_KM) * theta_s_rad.sin();
    if !sin_vza.is_finite() || sin_vza.abs() > 1.0 {
        return None;
    }
    Some(sin_vza.asin().to_degrees())
}

/// Derive the viewing zenith angle in degrees from the FIRMS along-scan pixel size.
pub fn vza_from_scan(scan_km: f64) -> Option<f64> {
    if !scan_km.is_finite() || scan_km <= 0.0 {
        return None;
    }
    if scan_km <= MODIS_NADIR_SCAN_KM {
        return Some(0.0);
    }

    let max_theta_s = MODIS_MAX_SCAN_ANGLE_DEG.to_radians();
    let max_scan_km = scan_size_km(max_theta_s)?;
    // Saturate oversized scan records to the edge of the MODIS swath rather than dropping
    // VZA correction for the entire detection.
    if scan_km >= max_scan_km {
        return theta_s_to_vza_deg(max_theta_s);
    }

    // Spherical-Earth pixel expansion is monotonic over [0, 55°], so bisection avoids
    // fragile closed-form quartic inversion and stays bounded in precision.
    let mut low = 0.0_f64;
    let mut high = max_theta_s;
    for _ in 0..50 {
        if high - low < 1e-9 {
            break;
        }
        let mid = 0.5 * (low + high);
        match scan_size_km(mid) {
            Some(s) if s < scan_km => low = mid,
            Some(_) => high = mid,
            None => return None,
        }
    }

    theta_s_to_vza_deg(0.5 * (low + high))
}

/// Compute the MODIS viewing zenith angle (VZA) correction factor R_rel(θ).
///
/// Off-nadir MODIS pixel expansion raises the detection threshold and attenuates observed
/// pixel FRP relative to 375 m VIIRS at the MODIS footprint scale. This uses the quadratic
/// polynomial:
///   R_rel(θ) = a·θ² + b·θ + c
/// where θ is VZA in radians (Zheng et al. 2026, Eq. 9 & Fig. 4, R² ≥ 98.18%), normalized at
/// nadir (`c = 1.0`).
///
/// VIIRS FRP is stable across the swath and needs no correction (`None`).
/// MODIS detections use the explicit `vza` when present and otherwise fall
/// back to deriving it from `scan`, returning `None` if neither is valid.
pub fn vza_factor(hotspot: &Hotspot) -> Option<f64> {
    vza_factor_with(hotspot, &VzaConfig::default())
}

/// Same as [`vza_factor`] with configurable polynomial coefficients.
pub fn vza_factor_with(hotspot: &Hotspot, config: &VzaConfig) -> Option<f64> {
    if hotspot.sensor.is_viirs() {
        return None;
    }

    let vza_deg = hotspot.vza.or_else(|| vza_from_scan(hotspot.scan))?;
    Some(correction_ratio_with(vza_deg.to_radians(), config))
}

/// Apply a precomputed VZA correction factor R_rel(θ) to a raw FRP value without
/// mutating the source hotspot.
///
/// Zheng et al. (2026) Eq. 9 defines `FRP_adj = R(θ) × FRP_MODIS` on the MODIS pixel
/// footprint scale, so transforming MODIS FRP toward the VIIRS scale multiplies by `ratio`.
pub fn apply_vza_correction(raw_frp: f64, factor: Option<f64>) -> f64 {
    match factor {
        Some(ratio) if ratio > 0.0 => raw_frp * ratio,
        _ => raw_frp,
    }
}

/// Normalized polynomial correction ratio R_rel(θ) = a·θ² + b·θ + c (θ in radians).
///
/// Derived from Zheng et al. (2026) Fig. 4 (`R(θ) = 0.24θ² + 0.02θ + 1.19` for Aqua MYD14,
/// footprint-scale VIIRS/MODIS FRP ratio), divided by 1.19 so `R_rel(0) = 1.0`. The nadir
/// level offset (1.19) is measured separately in the pre-2012 completeness factor `k_c` so it
/// is not double-counted. Terra has no separate fit (Zheng et al. §5) and reuses this curve
/// as a geometric proxy, reflected in `modis_scale_sigma`.
pub(crate) const CORRECTION_A: f64 = 0.2017;
pub(crate) const CORRECTION_B: f64 = 0.0168;
pub(crate) const CORRECTION_C: f64 = 1.0;

fn correction_ratio_with(theta_rad: f64, config: &VzaConfig) -> f64 {
    let r = config.a * theta_rad.powi(2) + config.b * theta_rad + config.c;
    r.max(1.0)
}

/// Pixel footprint area `A_p = scan_km * track_km` in km^2.
///
/// Returns `None` unless both dimensions are finite and strictly positive.
pub fn pixel_area_km2(scan_km: f64, track_km: f64) -> Option<f64> {
    if !scan_km.is_finite() || scan_km <= 0.0 || !track_km.is_finite() || track_km <= 0.0 {
        return None;
    }
    let area = scan_km * track_km;
    area.is_finite().then_some(area)
}

/// Analytic pixel detection-limit and logistic-slope parameters for a sensor family and period.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectionLimitCoefficients {
    /// Area-proportional threshold slope `lambda` (MW/km^2).
    pub lambda: f64,
    /// Constant threshold offset `mu` (MW).
    pub mu: f64,
    /// Asymptotic logistic slope `r0` (1/MW).
    pub r0: f64,
    /// Area-scaled logistic slope term `r1` (km^2/MW).
    pub r1: f64,
    /// Baseline false-omission/cutoff parameter `c0`.
    pub c0: f64,
}

// Coefficients are Tables 1-2 of Sofiev & Kouznetsov (2026), GMD 19, 10123. With Eq. 5
// (D_p = lambda * A_p + mu) D_p is the 50 % detection mid-point of the sigmoid, not a hard cutoff.
// The "typical" 5.7/3.7 MW (MODIS) and 3.7/0.7 MW (VIIRS) values in the paper's Conclusions are a
// different, empirically fitted statistic and are not reproduced by Eq. 5 with these tables,
// so the code uses the equation and the tables only.
const MODIS_DAY_LIMIT_COEFFS: DetectionLimitCoefficients = DetectionLimitCoefficients {
    lambda: 4.44,
    mu: 0.52,
    r0: 0.07,
    r1: 1.26,
    c0: 0.045,
};

const MODIS_NIGHT_LIMIT_COEFFS: DetectionLimitCoefficients = DetectionLimitCoefficients {
    lambda: 4.43,
    mu: 1.01,
    r0: 0.11,
    r1: 1.06,
    c0: 0.045,
};

const VIIRS_DAY_LIMIT_COEFFS: DetectionLimitCoefficients = DetectionLimitCoefficients {
    lambda: 6.17,
    mu: 1.44,
    r0: 0.61,
    r1: 0.21,
    c0: 0.05,
};

const VIIRS_NIGHT_LIMIT_COEFFS: DetectionLimitCoefficients = DetectionLimitCoefficients {
    lambda: 1.38,
    mu: 0.35,
    r0: 2.12,
    r1: 1.02,
    c0: 0.05,
};

/// Return the analytic detection-limit coefficients for the sensor's family (MODIS or VIIRS)
/// and day/night period.
pub fn detection_limit_coefficients(
    sensor: Sensor,
    daynight: DayNight,
) -> DetectionLimitCoefficients {
    match (sensor.is_modis(), daynight) {
        (true, DayNight::Day) => MODIS_DAY_LIMIT_COEFFS,
        (true, DayNight::Night) => MODIS_NIGHT_LIMIT_COEFFS,
        (false, DayNight::Day) => VIIRS_DAY_LIMIT_COEFFS,
        (false, DayNight::Night) => VIIRS_NIGHT_LIMIT_COEFFS,
    }
}

/// Analytic 50% FRP detection limit `D_p = lambda * A_p + mu` in MW for pixel area `area_km2`.
pub fn detection_limit_mw(coeffs: &DetectionLimitCoefficients, area_km2: f64) -> Option<f64> {
    if !area_km2.is_finite() || area_km2 <= 0.0 {
        return None;
    }
    Some(coeffs.lambda * area_km2 + coeffs.mu)
}

/// Logistic detection-probability slope `r_p = r0 + r1 / A_p` in 1/MW (taking `A_0 = 0`).
pub fn detection_slope(coeffs: &DetectionLimitCoefficients, area_km2: f64) -> Option<f64> {
    if !area_km2.is_finite() || area_km2 <= 0.0 {
        return None;
    }
    Some(coeffs.r0 + coeffs.r1 / area_km2)
}

/// Analytic 50% FRP detection limit in MW derived from a hotspot's footprint, sensor, and period.
pub fn hotspot_detection_limit_mw(hotspot: &Hotspot) -> Option<f64> {
    let area_km2 = pixel_area_km2(hotspot.scan, hotspot.track)?;
    let coeffs = detection_limit_coefficients(hotspot.sensor, hotspot.daynight);
    detection_limit_mw(&coeffs, area_km2)
}

#[cfg(test)]
fn correction_ratio(theta_rad: f64) -> f64 {
    correction_ratio_with(theta_rad, &VzaConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, FireType};
    use chrono::Utc;

    fn sample_hotspot(sensor: Sensor, frp: f64, vza: Option<f64>) -> Hotspot {
        Hotspot {
            lat: 39.0,
            lon: 32.0,
            brightness: 330.0,
            frp,
            confidence: Confidence::Nominal,
            sensor,
            acquired: Utc::now(),
            scan: 1.0,
            track: 1.0,
            satellite: String::new(),
            daynight: DayNight::Day,
            vza,
            fire_type: FireType::VegetationFire,
        }
    }

    #[test]
    fn nadir_has_no_correction() {
        let r = correction_ratio(0.0);
        assert!((r - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn correction_ratio_at_one_radian_matches_normalized_zheng_fit() {
        let r = correction_ratio(1.0);
        assert!((r - 1.2185).abs() < 1e-12, "expected 1.2185, got {r}");
    }

    #[test]
    fn large_vza_has_larger_correction() {
        let nadir = correction_ratio(0.0);
        let edge = correction_ratio(60.0_f64.to_radians());
        assert!(edge > nadir);
    }

    #[test]
    fn vza_factor_only_applies_to_modis_with_vza() {
        let modis_with_vza = sample_hotspot(Sensor::ModisTerra, 100.0, Some(45.0));
        let factor = vza_factor(&modis_with_vza);
        assert!(factor.is_some_and(|r| r > 1.0));

        let mut modis_without_vza_or_scan = sample_hotspot(Sensor::ModisAqua, 100.0, None);
        modis_without_vza_or_scan.scan = 0.0;
        assert_eq!(vza_factor(&modis_without_vza_or_scan), None);

        let viirs_with_vza = sample_hotspot(Sensor::ViirsSNPP, 100.0, Some(45.0));
        assert_eq!(vza_factor(&viirs_with_vza), None);
    }

    #[test]
    fn apply_vza_correction_multiplies_when_factor_present() {
        let corrected = apply_vza_correction(100.0, Some(2.0));
        assert!((corrected - 200.0).abs() < f64::EPSILON);
        assert!((apply_vza_correction(100.0, None) - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn vza_from_scan_nadir_is_zero() {
        let vza = vza_from_scan(1.0).expect("nadir scan should return Some");
        assert!(vza.abs() < 1e-9, "expected ~0 deg at nadir, got {vza}");
    }

    #[test]
    fn vza_from_scan_increases_monotonically_with_scan() {
        let v1 = vza_from_scan(1.2).unwrap();
        let v2 = vza_from_scan(1.7).unwrap();
        let v3 = vza_from_scan(2.2).unwrap();
        assert!(v1 < v2 && v2 < v3, "expected {v1} < {v2} < {v3}");
    }

    #[test]
    fn vza_from_scan_real_modis_edge_sample_is_plausible() {
        let vza = vza_from_scan(2.21).expect("real scan 2.21 should resolve");
        assert!(
            (30.0..70.0).contains(&vza),
            "expected VZA in [30, 70) deg for scan=2.21, got {vza}"
        );
    }

    #[test]
    fn vza_from_scan_rejects_invalid_inputs() {
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(vza_from_scan(bad), None, "expected None for {bad}");
        }
    }

    #[test]
    fn vza_from_scan_saturates_oversized_scan() {
        let saturated = vza_from_scan(50.0).expect("oversized scan should saturate");
        assert!(
            (60.0..70.0).contains(&saturated),
            "expected saturated VZA near swath edge, got {saturated}"
        );
        assert_eq!(vza_from_scan(50.0), vza_from_scan(100.0));
    }

    #[test]
    fn vza_factor_derives_from_scan_when_vza_missing() {
        let mut modis = sample_hotspot(Sensor::ModisTerra, 100.0, None);
        modis.scan = 2.21;
        let factor = vza_factor(&modis).expect("expected factor derived from scan");
        assert!(factor > 1.0, "expected factor > 1.0, got {factor}");
    }

    #[test]
    fn vza_factor_prefers_explicit_vza_over_scan() {
        let mut modis = sample_hotspot(Sensor::ModisAqua, 100.0, Some(45.0));
        modis.scan = 2.21;
        let factor = vza_factor(&modis).unwrap();
        let expected = correction_ratio(45.0_f64.to_radians());
        assert!((factor - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn scan_size_km_at_nadir_matches_p0() {
        let size = scan_size_km(0.0).expect("nadir scan angle should return Some");
        assert!(
            (size - 1.0).abs() < 1e-6,
            "expected 1.0 km at nadir, got {size}"
        );
    }

    #[test]
    fn scan_size_km_at_swath_edge_matches_modis_spec() {
        let size = scan_size_km(55.0_f64.to_radians()).expect("55 deg should return Some");
        assert!(
            (size - 4.833).abs() < 0.01,
            "expected ~4.833 km at 55 deg, got {size}"
        );
    }

    #[test]
    fn theta_s_to_vza_deg_at_swath_edge_matches_modis_spec() {
        let vza = theta_s_to_vza_deg(55.0_f64.to_radians()).expect("55 deg should return Some");
        assert!(
            (vza - 65.4).abs() < 0.1,
            "expected ~65.4 deg VZA at 55 deg scan angle, got {vza}"
        );
    }

    #[test]
    fn scan_size_km_is_strictly_monotonic_across_swath() {
        const STEPS: usize = 25;
        let mut prev = scan_size_km(0.0).expect("0 deg should return Some");
        for i in 1..=STEPS {
            let theta_deg = MODIS_MAX_SCAN_ANGLE_DEG * (i as f64) / (STEPS as f64);
            let curr = scan_size_km(theta_deg.to_radians())
                .unwrap_or_else(|| panic!("expected Some at {theta_deg} deg"));
            assert!(
                curr > prev,
                "expected monotonic increase at {theta_deg} deg: prev={prev}, curr={curr}"
            );
            prev = curr;
        }
    }

    #[test]
    fn vza_from_scan_real_modis_sample_avoids_overestimation_regression() {
        let vza = vza_from_scan(2.21).expect("scan=2.21 should resolve");
        assert!(
            (40.0..=52.0).contains(&vza),
            "expected VZA in [40, 52] deg for scan=2.21, got {vza}"
        );
    }

    #[test]
    fn vza_from_scan_inverts_scan_size_km_round_trip() {
        for theta_deg in [0.0_f64, 10.0, 20.0, 30.0, 40.0, 45.0, 50.0, 55.0] {
            let theta_rad = theta_deg.to_radians();
            let scan_km = scan_size_km(theta_rad).expect("valid scan angle");
            let recovered_vza = vza_from_scan(scan_km).expect("round-trip VZA");
            let expected_vza = theta_s_to_vza_deg(theta_rad).expect("expected VZA");
            assert!(
                (recovered_vza - expected_vza).abs() < 0.01,
                "round-trip mismatch at theta_s={theta_deg} deg: recovered={recovered_vza}, expected={expected_vza}"
            );
        }
    }

    #[test]
    fn correction_ratio_never_decreases_from_nadir_to_swath_edge() {
        for d in 0..=65 {
            let prev = correction_ratio(f64::from(d).to_radians());
            let next = correction_ratio(f64::from(d + 1).to_radians());
            assert!(
                next >= prev,
                "ratio decreased from {d} to {} deg: prev={prev}, next={next}",
                d + 1
            );
        }
    }

    #[test]
    fn apply_vza_correction_ignores_non_positive_factor() {
        assert!((apply_vza_correction(100.0, Some(0.0)) - 100.0).abs() < 1e-9);
        assert!((apply_vza_correction(100.0, Some(-2.0)) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn vza_from_scan_is_never_negative() {
        for i in 0..=55 {
            let scan = 0.5 + 0.1 * f64::from(i);
            let vza =
                vza_from_scan(scan).unwrap_or_else(|| panic!("expected Some for scan {scan}"));
            assert!(vza >= 0.0, "negative VZA {vza} for scan {scan}");
        }
    }

    #[test]
    fn pixel_area_km2_computes_product_and_rejects_invalid() {
        let unit = pixel_area_km2(1.0, 1.0).expect("1x1 km should be valid");
        assert!((unit - 1.0).abs() < 1e-12);

        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(pixel_area_km2(bad, 1.0), None, "expected None for scan={bad}");
            assert_eq!(
                pixel_area_km2(1.0, bad),
                None,
                "expected None for track={bad}"
            );
        }
    }

    #[test]
    fn modis_day_detection_limit_nadir_and_edge() {
        let coeffs = detection_limit_coefficients(Sensor::ModisTerra, DayNight::Day);
        assert_eq!(
            coeffs,
            detection_limit_coefficients(Sensor::ModisAqua, DayNight::Day)
        );

        let nadir = detection_limit_mw(&coeffs, 1.0).expect("valid nadir area");
        // Eq. 5 with Table 1 gives 4.96 MW at 1.0 km^2; the paper's Conclusions quote ~5.7 MW
        // as a typical value, which is a different statistic (not an Eq. 5 output).
        assert!((nadir - 4.96).abs() < 1e-9, "expected 4.96 MW, got {nadir}");

        let edge = detection_limit_mw(&coeffs, 9.66).expect("valid edge area");
        assert!((edge - 43.41).abs() < 0.005, "expected 43.41 MW, got {edge}");
    }

    #[test]
    fn modis_night_detection_limit_edge() {
        let coeffs = detection_limit_coefficients(Sensor::ModisAqua, DayNight::Night);
        let edge = detection_limit_mw(&coeffs, 9.66).expect("valid edge area");
        assert!((edge - 43.80).abs() < 0.005, "expected 43.80 MW, got {edge}");
    }

    #[test]
    fn coefficients_match_published_tables_1_and_2() {
        let modis_day = [4.44, 0.52, 0.07, 1.26, 0.045];
        let modis_night = [4.43, 1.01, 0.11, 1.06, 0.045];
        let viirs_day = [6.17, 1.44, 0.61, 0.21, 0.05];
        let viirs_night = [1.38, 0.35, 2.12, 1.02, 0.05];
        let table = [
            (Sensor::ModisTerra, DayNight::Day, modis_day),
            (Sensor::ModisAqua, DayNight::Day, modis_day),
            (Sensor::ModisTerra, DayNight::Night, modis_night),
            (Sensor::ModisAqua, DayNight::Night, modis_night),
            (Sensor::ViirsSNPP, DayNight::Day, viirs_day),
            (Sensor::ViirsNOAA20, DayNight::Day, viirs_day),
            (Sensor::ViirsNOAA21, DayNight::Day, viirs_day),
            (Sensor::ViirsSNPP, DayNight::Night, viirs_night),
            (Sensor::ViirsNOAA20, DayNight::Night, viirs_night),
            (Sensor::ViirsNOAA21, DayNight::Night, viirs_night),
        ];
        for (sensor, daynight, [lambda, mu, r0, r1, c0]) in table {
            let c = detection_limit_coefficients(sensor, daynight);
            assert_eq!(
                (c.lambda, c.mu, c.r0, c.r1, c.c0),
                (lambda, mu, r0, r1, c0),
                "{sensor:?} {daynight:?}"
            );
        }
    }

    #[test]
    fn reproduces_paper_section_5_3_nine_km2_example() {
        // Sofiev & Kouznetsov (2026) Sect. 5.3: 9 km^2 corresponds to 40.5-40.9 MW (day-night).
        let day = detection_limit_coefficients(Sensor::ModisAqua, DayNight::Day);
        let night = detection_limit_coefficients(Sensor::ModisAqua, DayNight::Night);
        assert!((detection_limit_mw(&day, 9.0).unwrap() - 40.48).abs() < 1e-9);
        assert!((detection_limit_mw(&night, 9.0).unwrap() - 40.88).abs() < 1e-9);
    }

    #[test]
    fn viirs_nadir_detection_limits_match_formula_and_ordering() {
        let viirs_nadir_area = pixel_area_km2(0.375, 0.375).expect("valid VIIRS nadir pixel");

        let viirs_day = detection_limit_coefficients(Sensor::ViirsSNPP, DayNight::Day);
        assert_eq!(
            viirs_day,
            detection_limit_coefficients(Sensor::ViirsNOAA20, DayNight::Day)
        );
        assert_eq!(
            viirs_day,
            detection_limit_coefficients(Sensor::ViirsNOAA21, DayNight::Day)
        );
        let viirs_day_nadir = detection_limit_mw(&viirs_day, viirs_nadir_area).unwrap();
        let expected_viirs_day = 6.17 * 0.140625 + 1.44;
        assert!(
            (viirs_day_nadir - expected_viirs_day).abs() < 1e-9,
            "expected {expected_viirs_day} MW, got {viirs_day_nadir}"
        );

        let modis_day = detection_limit_coefficients(Sensor::ModisTerra, DayNight::Day);
        let modis_day_nadir = detection_limit_mw(&modis_day, 1.0).unwrap();
        assert!(
            viirs_day_nadir < modis_day_nadir,
            "expected VIIRS day nadir ({viirs_day_nadir}) < MODIS day nadir ({modis_day_nadir})"
        );

        let viirs_night = detection_limit_coefficients(Sensor::ViirsNOAA20, DayNight::Night);
        let viirs_night_nadir = detection_limit_mw(&viirs_night, viirs_nadir_area).unwrap();
        let expected_viirs_night = 1.38 * 0.140625 + 0.35;
        assert!(
            (viirs_night_nadir - expected_viirs_night).abs() < 1e-9,
            "expected {expected_viirs_night} MW, got {viirs_night_nadir}"
        );
        assert!(
            viirs_night_nadir < 1.0,
            "expected VIIRS night nadir < 1.0 MW, got {viirs_night_nadir}"
        );
    }

    #[test]
    fn detection_limit_increases_and_slope_decreases_with_area() {
        let rows = [
            (Sensor::ModisTerra, DayNight::Day),
            (Sensor::ModisAqua, DayNight::Night),
            (Sensor::ViirsSNPP, DayNight::Day),
            (Sensor::ViirsNOAA21, DayNight::Night),
        ];
        let areas = [0.1, 0.140625, 1.0, 4.0, 9.66];

        for (sensor, daynight) in rows {
            let coeffs = detection_limit_coefficients(sensor, daynight);
            for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
                assert_eq!(detection_limit_mw(&coeffs, bad), None);
                assert_eq!(detection_slope(&coeffs, bad), None);
            }

            let mut prev_dp = detection_limit_mw(&coeffs, areas[0]).unwrap();
            let mut prev_rp = detection_slope(&coeffs, areas[0]).unwrap();
            assert!(prev_rp > coeffs.r0);

            for &area in &areas[1..] {
                let dp = detection_limit_mw(&coeffs, area).unwrap();
                let rp = detection_slope(&coeffs, area).unwrap();
                assert!(dp > prev_dp, "D_p not strictly increasing at area={area}");
                assert!(rp < prev_rp, "r_p not strictly decreasing at area={area}");
                assert!(rp > coeffs.r0, "r_p must exceed r0 at area={area}");
                prev_dp = dp;
                prev_rp = rp;
            }
        }
    }

    #[test]
    fn hotspot_detection_limit_mw_compares_nadir_and_edge_and_rejects_zero_scan() {
        let nadir = sample_hotspot(Sensor::ModisTerra, 100.0, None);
        let mut edge = sample_hotspot(Sensor::ModisTerra, 100.0, None);
        edge.scan = 4.83;
        edge.track = 2.0;

        let nadir_dp = hotspot_detection_limit_mw(&nadir).expect("valid nadir hotspot");
        let edge_dp = hotspot_detection_limit_mw(&edge).expect("valid edge hotspot");
        assert!(
            edge_dp > nadir_dp,
            "expected edge limit ({edge_dp}) > nadir limit ({nadir_dp})"
        );

        let mut bad = sample_hotspot(Sensor::ModisAqua, 100.0, None);
        bad.scan = 0.0;
        assert_eq!(hotspot_detection_limit_mw(&bad), None);
    }
}
