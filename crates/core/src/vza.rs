use crate::models::Hotspot;

/// Correct MODIS FRP for viewing zenith angle (VZA) dependency.
///
/// MODIS FRP increases up to 300% from nadir to scan edge due to pixel
/// expansion. This correction uses a quadratic polynomial model:
///   R(θ) = a·θ² + b·θ + c
/// where θ is VZA in radians (Zheng et al. 2026, R² ≥ 98.18%).
///
/// VIIRS FRP is stable across the swath and needs no correction.
pub fn correct_modis_frp(hotspot: &mut Hotspot) {
    if hotspot.sensor.is_viirs() {
        return;
    }

    let Some(vza_deg) = hotspot.vza else {
        return;
    };

    let theta = vza_deg.to_radians();
    let ratio = correction_ratio(theta);

    if ratio > 0.0 {
        hotspot.frp /= ratio;
    }
}

/// Apply VZA correction to a batch of MODIS hotspots.
pub fn correct_batch(hotspots: &mut [Hotspot]) {
    for h in hotspots.iter_mut() {
        correct_modis_frp(h);
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
}
