use serde::{Deserialize, Serialize};

pub use crate::calibration::CalibrationBand;
use crate::calibration::default_bands;
use crate::dedup::{MAX_TIME_DIFF_SECS, MIN_OVERLAP_RATIO};
use crate::fleet::{FLEET_RADIUS_KM, MODIS_FLEET_WINDOW_SECS, VIIRS_FLEET_WINDOW_SECS};
use crate::harmonize::{MATCH_RADIUS_KM, MATCH_WINDOW_SECS};
use crate::uncertainty::UncertaintyParams;
use crate::vza::{CORRECTION_A, CORRECTION_B, CORRECTION_C};

/// Upper bound on the cross-sensor match radius; it bounds the H3 candidate search cost.
pub const MAX_MATCH_RADIUS_KM: f64 = 10.0;

/// All tunable thresholds of the harmonization pipeline.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PipelineConfig {
    pub dedup: DedupConfig,
    pub fleet: FleetConfig,
    pub matching: MatchingConfig,
    pub vza: VzaConfig,
    pub calibration: CalibrationConfig,
    #[serde(default)]
    pub uncertainty: UncertaintyParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DedupConfig {
    pub max_time_diff_secs: i64,
    pub min_overlap_ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FleetConfig {
    pub viirs_window_secs: i64,
    pub modis_window_secs: i64,
    pub radius_km: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MatchingConfig {
    pub radius_km: f64,
    pub window_secs: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VzaConfig {
    pub a: f64,
    pub b: f64,
    pub c: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CalibrationConfig {
    pub enabled: bool,
    pub bands: Vec<CalibrationBand>,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            max_time_diff_secs: MAX_TIME_DIFF_SECS,
            min_overlap_ratio: MIN_OVERLAP_RATIO,
        }
    }
}

impl Default for FleetConfig {
    fn default() -> Self {
        Self {
            viirs_window_secs: VIIRS_FLEET_WINDOW_SECS,
            modis_window_secs: MODIS_FLEET_WINDOW_SECS,
            radius_km: FLEET_RADIUS_KM,
        }
    }
}

impl Default for MatchingConfig {
    fn default() -> Self {
        Self {
            radius_km: MATCH_RADIUS_KM,
            window_secs: MATCH_WINDOW_SECS,
        }
    }
}

impl Default for VzaConfig {
    fn default() -> Self {
        Self {
            a: CORRECTION_A,
            b: CORRECTION_B,
            c: CORRECTION_C,
        }
    }
}

impl Default for CalibrationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bands: default_bands(),
        }
    }
}

impl PipelineConfig {
    /// Checks every field and names the offending one in the error.
    pub fn validate(&self) -> Result<(), String> {
        check_secs("dedup.max_time_diff_secs", self.dedup.max_time_diff_secs)?;
        let ratio = self.dedup.min_overlap_ratio;
        if !(0.0..=1.0).contains(&ratio) {
            return Err(format!(
                "dedup.min_overlap_ratio must be within 0.0..=1.0, got {ratio}"
            ));
        }
        check_secs("fleet.viirs_window_secs", self.fleet.viirs_window_secs)?;
        check_secs("fleet.modis_window_secs", self.fleet.modis_window_secs)?;
        check_radius("fleet.radius_km", self.fleet.radius_km)?;
        check_radius("matching.radius_km", self.matching.radius_km)?;
        if self.matching.radius_km > MAX_MATCH_RADIUS_KM {
            return Err(format!(
                "matching.radius_km must be <= {MAX_MATCH_RADIUS_KM}, got {}",
                self.matching.radius_km
            ));
        }
        check_secs("matching.window_secs", self.matching.window_secs)?;
        for (name, value) in [
            ("vza.a", self.vza.a),
            ("vza.b", self.vza.b),
            ("vza.c", self.vza.c),
        ] {
            if !value.is_finite() {
                return Err(format!("{name} must be finite, got {value}"));
            }
        }
        validate_calibration(&self.calibration)?;
        self.uncertainty.validate()?;
        Ok(())
    }
}

fn validate_calibration(cal: &CalibrationConfig) -> Result<(), String> {
    if cal.enabled && cal.bands.is_empty() {
        return Err("calibration.bands must not be empty when calibration is enabled".to_string());
    }
    let mut prev_max: Option<f64> = None;
    for (i, band) in cal.bands.iter().enumerate() {
        check_abs_lat(&format!("calibration.bands[{i}].min_abs_lat"), band.min_abs_lat)?;
        check_abs_lat(&format!("calibration.bands[{i}].max_abs_lat"), band.max_abs_lat)?;
        if band.min_abs_lat > band.max_abs_lat {
            return Err(format!(
                "calibration.bands[{i}].max_abs_lat ({}) must be >= min_abs_lat ({})",
                band.max_abs_lat, band.min_abs_lat
            ));
        }
        if !band.ratio.is_finite() || band.ratio <= 0.0 {
            return Err(format!(
                "calibration.bands[{i}].ratio must be finite and > 0, got {}",
                band.ratio
            ));
        }
        if !band.ratio_sigma.is_finite() || band.ratio_sigma < 0.0 {
            return Err(format!(
                "calibration.bands[{i}].ratio_sigma must be finite and >= 0, got {}",
                band.ratio_sigma
            ));
        }
        if let Some(prev) = prev_max {
            if band.min_abs_lat < prev {
                return Err(format!(
                    "calibration.bands[{i}].min_abs_lat ({}) overlaps previous max_abs_lat ({prev})",
                    band.min_abs_lat
                ));
            }
            if band.max_abs_lat <= prev {
                return Err(format!(
                    "calibration.bands[{i}].max_abs_lat ({}) must be strictly greater than previous ({prev})",
                    band.max_abs_lat
                ));
            }
        }
        prev_max = Some(band.max_abs_lat);
    }
    Ok(())
}

fn check_abs_lat(name: &str, value: f64) -> Result<(), String> {
    if !value.is_finite() || !(0.0..=90.0).contains(&value) {
        return Err(format!("{name} must be within 0.0..=90.0, got {value}"));
    }
    Ok(())
}

fn check_secs(name: &str, value: i64) -> Result<(), String> {
    if value < 0 {
        return Err(format!("{name} must be >= 0, got {value}"));
    }
    Ok(())
}

fn check_radius(name: &str, value: f64) -> Result<(), String> {
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{name} must be finite and > 0, got {value}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_documented_values_and_is_valid() {
        let c = PipelineConfig::default();
        assert_eq!(c.dedup.max_time_diff_secs, 8);
        assert_eq!(c.dedup.min_overlap_ratio, 0.30);
        assert_eq!(c.fleet.viirs_window_secs, 3600);
        assert_eq!(c.fleet.modis_window_secs, 10_800);
        assert_eq!(c.fleet.radius_km, 2.0);
        assert_eq!(c.matching.radius_km, 2.0);
        assert_eq!(c.matching.window_secs, 10_800);
        assert_eq!((c.vza.a, c.vza.b, c.vza.c), (0.2017, 0.0168, 1.0));
        assert!(c.calibration.enabled);
        assert_eq!(c.calibration.bands.len(), 3);
        assert_eq!(c.calibration.bands[0].ratio, 1.11);
        assert_eq!(c.calibration.bands[1].ratio, 1.18);
        assert_eq!(c.calibration.bands[2].ratio, 1.28);
        assert_eq!(c.uncertainty, UncertaintyParams::default());
        assert_eq!(c.validate(), Ok(()));
    }

    fn rejected(edit: impl FnOnce(&mut PipelineConfig), field: &str) {
        let mut c = PipelineConfig::default();
        edit(&mut c);
        let err = c.validate().expect_err("config should be rejected");
        assert!(err.contains(field), "`{err}` does not name `{field}`");
    }

    #[test]
    fn validate_rejects_bad_values() {
        rejected(|c| c.matching.window_secs = -1, "matching.window_secs");
        rejected(|c| c.fleet.viirs_window_secs = -5, "fleet.viirs_window_secs");
        rejected(|c| c.matching.radius_km = 0.0, "matching.radius_km");
        rejected(|c| c.fleet.radius_km = f64::NAN, "fleet.radius_km");
        rejected(|c| c.dedup.min_overlap_ratio = 1.5, "dedup.min_overlap_ratio");
        rejected(|c| c.dedup.min_overlap_ratio = f64::NAN, "dedup.min_overlap_ratio");
        rejected(|c| c.vza.b = f64::NAN, "vza.b");
        rejected(|c| c.vza.a = f64::INFINITY, "vza.a");
        rejected(|c| c.matching.radius_km = 10.5, "matching.radius_km");
        rejected(|c| c.calibration.bands[0].ratio = 0.0, "calibration.bands[0].ratio");
        rejected(|c| c.calibration.bands[1].ratio = -0.5, "calibration.bands[1].ratio");
        rejected(|c| c.calibration.bands[0].ratio = f64::NAN, "calibration.bands[0].ratio");
        rejected(
            |c| c.calibration.bands[0].ratio_sigma = -0.1,
            "calibration.bands[0].ratio_sigma",
        );
        rejected(
            |c| c.calibration.bands[2].max_abs_lat = 95.0,
            "calibration.bands[2].max_abs_lat",
        );
        rejected(
            |c| c.calibration.bands[0].min_abs_lat = -1.0,
            "calibration.bands[0].min_abs_lat",
        );
        rejected(
            |c| {
                c.calibration.bands[1].min_abs_lat = 20.0;
            },
            "calibration.bands[1].min_abs_lat",
        );
        rejected(
            |c| {
                c.calibration.bands[1].min_abs_lat = 30.0;
                c.calibration.bands[1].max_abs_lat = 30.0;
            },
            "calibration.bands[1].max_abs_lat",
        );
        rejected(
            |c| c.calibration.bands.clear(),
            "calibration.bands",
        );
        rejected(|c| c.uncertainty.z = 0.0, "uncertainty.z");
        rejected(
            |c| c.uncertainty.modis_sensor_sigma = -0.1,
            "uncertainty.modis_sensor_sigma",
        );
        rejected(
            |c| c.uncertainty.modis_scale_sigma = -0.1,
            "uncertainty.modis_scale_sigma",
        );
    }

    #[test]
    fn validate_accepts_upper_radius_bound() {
        let mut c = PipelineConfig::default();
        c.matching.radius_km = MAX_MATCH_RADIUS_KM;
        assert_eq!(c.validate(), Ok(()));
    }

    #[test]
    fn empty_object_deserializes_to_default() {
        let c: PipelineConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c, PipelineConfig::default());
    }

    #[test]
    fn partial_section_keeps_other_defaults() {
        let c: PipelineConfig = serde_json::from_str(r#"{"matching":{"window_secs":600}}"#).unwrap();
        assert_eq!(c.matching.window_secs, 600);
        assert_eq!(c.matching.radius_km, 2.0);
        assert_eq!(c.dedup, DedupConfig::default());
        assert_eq!(c.calibration, CalibrationConfig::default());
    }

    #[test]
    fn custom_calibration_bands_deserialize_and_validate() {
        let json = r#"{
            "calibration": {
                "enabled": true,
                "bands": [
                    {"min_abs_lat": 0.0, "max_abs_lat": 45.0, "ratio": 1.1, "ratio_sigma": 0.05},
                    {"min_abs_lat": 50.0, "max_abs_lat": 80.0, "ratio": 0.9, "ratio_sigma": 0.1}
                ]
            }
        }"#;
        let c: PipelineConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.validate(), Ok(()));
        assert_eq!(c.calibration.bands.len(), 2);
        assert_eq!(c.calibration.bands[0].ratio, 1.1);
        assert_eq!(c.calibration.bands[1].max_abs_lat, 80.0);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(serde_json::from_str::<PipelineConfig>(r#"{"bogus":1}"#).is_err());
        assert!(serde_json::from_str::<PipelineConfig>(r#"{"dedup":{"bogus":1}}"#).is_err());
        assert!(serde_json::from_str::<PipelineConfig>(r#"{"calibration":{"bogus":1}}"#).is_err());
        assert!(
            serde_json::from_str::<PipelineConfig>(
                r#"{"calibration":{"bands":[{"min_abs_lat":0.0,"max_abs_lat":30.0,"ratio":1.0,"ratio_sigma":0.0,"bogus":1}]}}"#
            )
            .is_err()
        );
    }

    fn accepted(edit: impl FnOnce(&mut PipelineConfig)) {
        let mut c = PipelineConfig::default();
        edit(&mut c);
        assert_eq!(c.validate(), Ok(()));
    }

    #[test]
    fn second_fields_reject_negative_and_accept_zero_and_one() {
        // 3-value BVA around the `>= 0` limit of every seconds field.
        type Setter = fn(&mut PipelineConfig, i64);
        let fields: [(&str, Setter); 4] = [
            ("dedup.max_time_diff_secs", |c, v| c.dedup.max_time_diff_secs = v),
            ("fleet.viirs_window_secs", |c, v| c.fleet.viirs_window_secs = v),
            ("fleet.modis_window_secs", |c, v| c.fleet.modis_window_secs = v),
            ("matching.window_secs", |c, v| c.matching.window_secs = v),
        ];
        for (name, set) in fields {
            rejected(|c| set(c, -1), name);
            accepted(|c| set(c, 0));
            accepted(|c| set(c, 1));
        }
    }

    #[test]
    fn overlap_ratio_boundaries_are_inclusive_zero_and_one() {
        accepted(|c| c.dedup.min_overlap_ratio = 0.0);
        accepted(|c| c.dedup.min_overlap_ratio = 1.0);
        rejected(|c| c.dedup.min_overlap_ratio = -f64::EPSILON, "dedup.min_overlap_ratio");
        rejected(|c| c.dedup.min_overlap_ratio = 1.0 + f64::EPSILON, "dedup.min_overlap_ratio");
        rejected(|c| c.dedup.min_overlap_ratio = f64::INFINITY, "dedup.min_overlap_ratio");
    }

    #[test]
    fn radius_limits_matching_is_capped_but_fleet_is_not() {
        accepted(|c| c.matching.radius_km = f64::MIN_POSITIVE);
        accepted(|c| c.matching.radius_km = MAX_MATCH_RADIUS_KM - 0.001);
        rejected(|c| c.matching.radius_km = MAX_MATCH_RADIUS_KM + 0.001, "matching.radius_km");
        rejected(|c| c.matching.radius_km = f64::INFINITY, "matching.radius_km");
        rejected(|c| c.fleet.radius_km = -1.0, "fleet.radius_km");
        rejected(|c| c.fleet.radius_km = 0.0, "fleet.radius_km");
        accepted(|c| c.fleet.radius_km = 1000.0);
    }

    #[test]
    fn vza_coefficients_each_independently_must_be_finite() {
        // Equivalence partitioning and boundary values of each independent check; negatives and zero are allowed.
        rejected(|c| c.vza.c = f64::NEG_INFINITY, "vza.c");
        rejected(|c| c.vza.a = f64::NAN, "vza.a");
        rejected(|c| c.vza.b = f64::INFINITY, "vza.b");
        accepted(|c| (c.vza.a, c.vza.b, c.vza.c) = (-1.0, 0.0, 0.0));
    }

    #[test]
    fn calibration_latitude_bounds_are_inclusive_zero_to_ninety() {
        let one_band = |min: f64, max: f64| {
            move |c: &mut PipelineConfig| {
                c.calibration.bands = vec![CalibrationBand {
                    min_abs_lat: min,
                    max_abs_lat: max,
                    ratio: 1.0,
                    ratio_sigma: 0.0,
                }];
            }
        };
        accepted(one_band(0.0, 90.0));
        accepted(one_band(90.0, 90.0));
        rejected(one_band(0.0, 90.0 + 1e-9), "bands[0].max_abs_lat");
        rejected(one_band(-1e-9, 10.0), "bands[0].min_abs_lat");
        rejected(one_band(f64::NAN, 10.0), "bands[0].min_abs_lat");
        // min > max by the smallest margin flips only that condition.
        rejected(one_band(10.0 + 1e-9, 10.0), "must be >= min_abs_lat");
    }

    #[test]
    fn calibration_bands_may_touch_but_not_overlap_or_repeat_max() {
        let band = |min: f64, max: f64| CalibrationBand {
            min_abs_lat: min,
            max_abs_lat: max,
            ratio: 1.1,
            ratio_sigma: 0.1,
        };
        let mut c = PipelineConfig::default();
        c.calibration.bands = vec![band(0.0, 30.0), band(30.0, 50.0)];
        assert_eq!(c.validate(), Ok(()));
        c.calibration.bands = vec![band(0.0, 30.0), band(29.999, 50.0)];
        assert!(c.validate().unwrap_err().contains("overlaps previous"));
        c.calibration.bands = vec![band(0.0, 30.0), band(30.0, 30.0)];
        assert!(c.validate().unwrap_err().contains("strictly greater"));
    }

    #[test]
    fn ratio_sigma_zero_is_valid_and_empty_bands_allowed_when_disabled() {
        accepted(|c| c.calibration.bands[0].ratio_sigma = 0.0);
        rejected(|c| c.calibration.bands[0].ratio_sigma = f64::INFINITY, "ratio_sigma");
        accepted(|c| {
            c.calibration.enabled = false;
            c.calibration.bands.clear();
        });
    }

    #[test]
    fn malformed_json_and_wrong_types_give_descriptive_errors() {
        let cases = [
            (r#"{"matching":{"radius_km":"two"}}"#, "string"),
            (r#"{"dedup":{"max_time_diff_secs":1.5}}"#, "floating point"),
            (r#"{"calibration":{"enabled":"yes"}}"#, "boolean"),
            (r#"{"bogus":1}"#, "bogus"),
            (r#"{"matching":"#, "EOF"),
        ];
        for (json, needle) in cases {
            let err = serde_json::from_str::<PipelineConfig>(json)
                .expect_err(json)
                .to_string();
            assert!(err.contains(needle), "`{err}` lacks `{needle}` for {json}");
        }
    }

    #[test]
    fn serialization_round_trip_preserves_config() {
        let mut c = PipelineConfig::default();
        c.matching.window_secs = 600;
        c.calibration.enabled = false;
        let back: PipelineConfig = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
        assert_eq!(back.validate(), Ok(()));
    }
}
