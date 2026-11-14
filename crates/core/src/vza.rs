use crate::models::Hotspot;

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

/// Compute the MODIS viewing zenith angle (VZA) correction factor R(θ).
///
/// MODIS FRP increases up to 300% from nadir to scan edge due to pixel
/// expansion. This correction uses a quadratic polynomial model:
///   R(θ) = a·θ² + b·θ + c
/// where θ is VZA in radians (Zheng et al. 2026, R² ≥ 98.18%).
///
/// VIIRS FRP is stable across the swath and needs no correction (`None`).
/// MODIS detections use the explicit `vza` when present and otherwise fall
/// back to deriving it from `scan`, returning `None` if neither is valid.
pub fn vza_factor(hotspot: &Hotspot) -> Option<f64> {
    if hotspot.sensor.is_viirs() {
        return None;
    }

    let vza_deg = hotspot.vza.or_else(|| vza_from_scan(hotspot.scan))?;
    Some(correction_ratio(vza_deg.to_radians()))
}

/// Apply a precomputed VZA correction factor to a raw FRP value without
/// mutating the source hotspot.
pub fn apply_vza_correction(raw_frp: f64, factor: Option<f64>) -> f64 {
    match factor {
        Some(ratio) if ratio > 0.0 => raw_frp / ratio,
        _ => raw_frp,
    }
}

/// Polynomial correction ratio R(θ) = a·θ² + b·θ + c
///
/// These coefficients are derived from VIIRS/MODIS FRP ratios
/// binned at 1° VZA intervals. Placeholder values — will be
/// refined with real overlap-period data.
fn correction_ratio(theta_rad: f64) -> f64 {
    const A: f64 = 0.85;
    const B: f64 = 0.12;
    const C: f64 = 1.0;

    let r = A * theta_rad.powi(2) + B * theta_rad + C;
    r.max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, DayNight, FireType, Sensor};
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
    fn apply_vza_correction_divides_when_factor_present() {
        let corrected = apply_vza_correction(100.0, Some(2.0));
        assert!((corrected - 50.0).abs() < f64::EPSILON);
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
}
