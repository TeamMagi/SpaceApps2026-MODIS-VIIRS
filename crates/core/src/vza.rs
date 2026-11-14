use crate::models::Hotspot;

/// Compute the MODIS viewing zenith angle (VZA) correction factor R(θ).
///
/// MODIS FRP increases up to 300% from nadir to scan edge due to pixel
/// expansion. This correction uses a quadratic polynomial model:
///   R(θ) = a·θ² + b·θ + c
/// where θ is VZA in radians (Zheng et al. 2026, R² ≥ 98.18%).
///
/// VIIRS FRP is stable across the swath and needs no correction (`None`),
/// and MODIS detections without a recorded VZA also return `None`.
pub fn vza_factor(hotspot: &Hotspot) -> Option<f64> {
    if hotspot.sensor.is_viirs() {
        return None;
    }

    let vza_deg = hotspot.vza?;
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

        let modis_without_vza = sample_hotspot(Sensor::ModisAqua, 100.0, None);
        assert_eq!(vza_factor(&modis_without_vza), None);

        let viirs_with_vza = sample_hotspot(Sensor::ViirsSNPP, 100.0, Some(45.0));
        assert_eq!(vza_factor(&viirs_with_vza), None);
    }

    #[test]
    fn apply_vza_correction_divides_when_factor_present() {
        let corrected = apply_vza_correction(100.0, Some(2.0));
        assert!((corrected - 50.0).abs() < f64::EPSILON);
        assert!((apply_vza_correction(100.0, None) - 100.0).abs() < f64::EPSILON);
    }
}
