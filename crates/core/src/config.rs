use serde::{Deserialize, Serialize};

pub use crate::calibration::{BiomeCompleteness, CalibrationBand, CalibrationMode, PowerLawBudget};
use crate::calibration::{default_bands, default_biome_table, default_power_law};
use crate::dedup::{MAX_TIME_DIFF_SECS, MIN_OVERLAP_RATIO};
use crate::fleet::{FLEET_RADIUS_KM, MODIS_FLEET_WINDOW_SECS, VIIRS_FLEET_WINDOW_SECS};
use crate::harmonize::{MATCH_RADIUS_KM, MATCH_WINDOW_SECS, SIMULTANEOUS_WINDOW_SECS};
use crate::sama::{SAMA_EAST, SAMA_NORTH, SAMA_SOUTH, SAMA_WEST};
use crate::static_mask::MIN_ACTIVE_MONTHS;
use crate::uncertainty::UncertaintyParams;
use crate::vza::{CORRECTION_A, CORRECTION_B, CORRECTION_C};

/// Upper bound on the cross-sensor match radius; it bounds the H3 candidate search cost.
pub const MAX_MATCH_RADIUS_KM: f64 = 10.0;

/// Upper bound on the fleet merge radius, for the same reason: the H3 k-ring grows with the
/// square of the radius, so an unbounded value (e.g. `fleet_radius_km=1e5` on a request) would
/// ask for a ring of ~10^11 cells.
pub const MAX_FLEET_RADIUS_KM: f64 = 10.0;

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
    pub static_mask: StaticMaskConfig,
    pub sama: SamaConfig,
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
    pub simultaneous_window_secs: i64,
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
    /// `biome` tries the biome chain first; `lat_band` uses only `bands`.
    pub mode: CalibrationMode,
    pub bands: Vec<CalibrationBand>,
    pub biome_table: Vec<BiomeCompleteness>,
    pub power_law: Vec<PowerLawBudget>,
}

/// Persistent hot-cell (gas flare / industry) mask, see [`crate::static_mask`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StaticMaskConfig {
    pub enabled: bool,
    /// Distinct months with a detection that mask a cell for that calendar year.
    pub min_active_months: u8,
}

/// South Atlantic Magnetic Anomaly nighttime VIIRS filter, see [`crate::sama`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SamaConfig {
    pub enabled: bool,
    pub west: f64,
    pub east: f64,
    pub south: f64,
    pub north: f64,
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
            simultaneous_window_secs: SIMULTANEOUS_WINDOW_SECS,
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
            mode: CalibrationMode::default(),
            bands: default_bands(),
            biome_table: default_biome_table(),
            power_law: default_power_law(),
        }
    }
}

impl Default for StaticMaskConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_active_months: MIN_ACTIVE_MONTHS,
        }
    }
}

impl Default for SamaConfig {
    fn default() -> Self {
        Self {
            // Off by default: Collection 2 already strips the SAMA low-confidence pixels
            // (VIIRS C2 375 m User Guide v1.2, Sec. 3.1), and in the archive nighttime VIIRS rows
            // inside the box are 22-36% of all VIIRS rows (Brazil, Bolivia, Paraguay, Argentina,
            // 2018-2020) yet no more isolated than daytime or out-of-box nighttime rows
            // (1.0-2.6% vs 3.9-7.2% by day and 3.7% in Mexico), so dropping them removes fires.
            enabled: false,
            west: SAMA_WEST,
            east: SAMA_EAST,
            south: SAMA_SOUTH,
            north: SAMA_NORTH,
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
        if self.fleet.radius_km > MAX_FLEET_RADIUS_KM {
            return Err(format!(
                "fleet.radius_km must be <= {MAX_FLEET_RADIUS_KM}, got {}",
                self.fleet.radius_km
            ));
        }
        check_radius("matching.radius_km", self.matching.radius_km)?;
        if self.matching.radius_km > MAX_MATCH_RADIUS_KM {
            return Err(format!(
                "matching.radius_km must be <= {MAX_MATCH_RADIUS_KM}, got {}",
                self.matching.radius_km
            ));
        }
        check_secs("matching.window_secs", self.matching.window_secs)?;
        check_secs(
            "matching.simultaneous_window_secs",
            self.matching.simultaneous_window_secs,
        )?;
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
        validate_static_mask(&self.static_mask)?;
        validate_sama(&self.sama)?;
        Ok(())
    }
}

fn validate_static_mask(mask: &StaticMaskConfig) -> Result<(), String> {
    if !(1..=12).contains(&mask.min_active_months) {
        return Err(format!(
            "static_mask.min_active_months must be within 1..=12, got {}",
            mask.min_active_months
        ));
    }
    Ok(())
}

fn validate_sama(sama: &SamaConfig) -> Result<(), String> {
    for (name, value, limit) in [
        ("sama.west", sama.west, 180.0),
        ("sama.east", sama.east, 180.0),
        ("sama.south", sama.south, 90.0),
        ("sama.north", sama.north, 90.0),
    ] {
        if !value.is_finite() || !(-limit..=limit).contains(&value) {
            return Err(format!("{name} must be within -{limit}..={limit}, got {value}"));
        }
    }
    if sama.west > sama.east {
        return Err(format!(
            "sama.east ({}) must be >= sama.west ({})",
            sama.east, sama.west
        ));
    }
    if sama.south > sama.north {
        return Err(format!(
            "sama.north ({}) must be >= sama.south ({})",
            sama.north, sama.south
        ));
    }
    Ok(())
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
    for (i, e) in cal.biome_table.iter().enumerate() {
        let name = format!("calibration.biome_table[{i}]");
        if let Some(m) = e.month {
            if !(1..=12).contains(&m) {
                return Err(format!("{name}.month must be within 1..=12, got {m}"));
            }
            if e.daynight.is_none() {
                return Err(format!("{name}.daynight must be set when month is set"));
            }
        }
        if !e.ratio.is_finite() || e.ratio <= 0.0 {
            return Err(format!("{name}.ratio must be finite and > 0, got {}", e.ratio));
        }
        if !e.ratio_sigma.is_finite() || e.ratio_sigma < 0.0 {
            return Err(format!("{name}.ratio_sigma must be finite and >= 0, got {}", e.ratio_sigma));
        }
    }
    for (i, b) in cal.power_law.iter().enumerate() {
        let name = format!("calibration.power_law[{i}]");
        if !b.alpha.is_finite() || b.alpha <= 1.0 {
            return Err(format!("{name}.alpha must be finite and > 1, got {}", b.alpha));
        }
        if !b.alpha_sigma.is_finite() || b.alpha_sigma < 0.0 {
            return Err(format!("{name}.alpha_sigma must be finite and >= 0, got {}", b.alpha_sigma));
        }
        if !b.p_max_mw.is_finite() || b.p_max_mw <= 0.0 {
            return Err(format!("{name}.p_max_mw must be finite and > 0, got {}", b.p_max_mw));
        }
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
        assert_eq!(c.matching.simultaneous_window_secs, 1_200);
        assert_eq!((c.vza.a, c.vza.b, c.vza.c), (0.2017, 0.0168, 1.0));
        assert!(c.calibration.enabled);
        assert_eq!(c.calibration.bands.len(), 3);
        assert_eq!(c.calibration.bands[0].ratio, 1.11);
        assert_eq!(c.calibration.bands[1].ratio, 1.18);
        assert_eq!(c.calibration.bands[2].ratio, 1.28);
        assert_eq!(c.uncertainty, UncertaintyParams::default());
        assert!(c.static_mask.enabled);
        assert_eq!(c.static_mask.min_active_months, 10);
        assert!(!c.sama.enabled);
        assert_eq!((c.sama.west, c.sama.east, c.sama.south, c.sama.north), (-110.0, 11.0, -55.0, 7.0));
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
        rejected(|c| c.static_mask.min_active_months = 0, "static_mask.min_active_months");
        rejected(|c| c.static_mask.min_active_months = 13, "static_mask.min_active_months");
        rejected(|c| c.sama.west = f64::NAN, "sama.west");
        rejected(|c| c.sama.north = 91.0, "sama.north");
        rejected(|c| c.sama.east = -120.0, "sama.east");
        rejected(|c| c.sama.south = 10.0, "sama.north");
    }

    #[test]
    fn static_mask_month_bounds_are_inclusive_one_to_twelve() {
        accepted(|c| c.static_mask.min_active_months = 1);
        accepted(|c| c.static_mask.min_active_months = 12);
        // A disabled mask is still validated so switching it on later cannot expose a bad value.
        rejected(
            |c| {
                c.static_mask.enabled = false;
                c.static_mask.min_active_months = 0;
            },
            "static_mask.min_active_months",
        );
    }

    #[test]
    fn prefilters_switch_independently_from_json() {
        let c: PipelineConfig = serde_json::from_str(r#"{"sama":{"enabled":true}}"#).unwrap();
        assert!(c.sama.enabled);
        assert_eq!(c.sama.west, SamaConfig::default().west);
        assert_eq!(c.static_mask, StaticMaskConfig::default());
        let c: PipelineConfig = serde_json::from_str(r#"{"static_mask":{"enabled":false}}"#).unwrap();
        assert!(!c.static_mask.enabled);
        assert_eq!(c.sama, SamaConfig::default());
        assert!(serde_json::from_str::<PipelineConfig>(r#"{"sama":{"bogus":1}}"#).is_err());
        assert!(serde_json::from_str::<PipelineConfig>(r#"{"static_mask":{"min_years":3}}"#).is_err());
    }

    #[test]
    fn biome_calibration_fields_are_validated_and_named() {
        use crate::biome::Biome;
        use crate::models::DayNight;
        let row = |month: Option<u8>, daynight: Option<DayNight>, ratio: f64, sigma: f64| BiomeCompleteness {
            biome: Biome::Savanna,
            month,
            daynight,
            ratio,
            ratio_sigma: sigma,
            n: 10,
        };
        let table = |e: BiomeCompleteness| move |c: &mut PipelineConfig| c.calibration.biome_table = vec![e.clone()];
        accepted(table(row(Some(12), Some(DayNight::Day), 1.2, 0.0)));
        accepted(table(row(None, None, 1.2, 0.1)));
        rejected(table(row(Some(0), Some(DayNight::Day), 1.2, 0.1)), "calibration.biome_table[0].month");
        rejected(table(row(Some(13), Some(DayNight::Day), 1.2, 0.1)), "calibration.biome_table[0].month");
        rejected(table(row(Some(5), None, 1.2, 0.1)), "calibration.biome_table[0].daynight");
        rejected(table(row(None, None, 0.0, 0.1)), "calibration.biome_table[0].ratio");
        // A measured ratio below 1 is legitimate (VIIRS total below the corrected MODIS total).
        accepted(table(row(Some(3), Some(DayNight::Day), 0.946, 0.034)));
        accepted(table(row(None, None, 0.5, 0.0)));
        rejected(table(row(None, None, f64::NAN, 0.1)), "calibration.biome_table[0].ratio");
        rejected(table(row(None, None, 1.2, -0.1)), "calibration.biome_table[0].ratio_sigma");

        let law = |alpha: f64, alpha_sigma: f64, p_max_mw: f64| {
            move |c: &mut PipelineConfig| {
                c.calibration.power_law = vec![PowerLawBudget {
                    biome: Biome::Forest,
                    alpha,
                    alpha_sigma,
                    p_max_mw,
                }]
            }
        };
        accepted(law(2.0, 0.0, 500.0));
        rejected(law(1.0, 0.1, 500.0), "calibration.power_law[0].alpha");
        rejected(law(f64::INFINITY, 0.1, 500.0), "calibration.power_law[0].alpha");
        rejected(law(1.5, -1.0, 500.0), "calibration.power_law[0].alpha_sigma");
        rejected(law(1.5, 0.1, 0.0), "calibration.power_law[0].p_max_mw");
    }

    #[test]
    fn default_calibration_round_trips_through_json_and_old_json_still_loads() {
        let c = PipelineConfig::default();
        assert_eq!(c.calibration.mode, CalibrationMode::Biome);
        assert!(c.calibration.power_law.is_empty());
        assert_eq!(c.calibration.biome_table.len(), 109);
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<PipelineConfig>(&json).unwrap(), c);
        assert!(json.contains(r#""mode":"biome""#));
        // A config written before K14 (no mode/table/power_law) still deserializes.
        let old: PipelineConfig = serde_json::from_str(r#"{"calibration":{"enabled":true}}"#).unwrap();
        assert_eq!(old.calibration, CalibrationConfig::default());
        let lat: PipelineConfig = serde_json::from_str(r#"{"calibration":{"mode":"lat_band"}}"#).unwrap();
        assert_eq!(lat.calibration.mode, CalibrationMode::LatBand);
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
        let fields: [(&str, Setter); 5] = [
            ("dedup.max_time_diff_secs", |c, v| c.dedup.max_time_diff_secs = v),
            ("fleet.viirs_window_secs", |c, v| c.fleet.viirs_window_secs = v),
            ("fleet.modis_window_secs", |c, v| c.fleet.modis_window_secs = v),
            ("matching.window_secs", |c, v| c.matching.window_secs = v),
            ("matching.simultaneous_window_secs", |c, v| {
                c.matching.simultaneous_window_secs = v
            }),
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
    fn radius_limits_matching_and_fleet_are_both_capped() {
        accepted(|c| c.matching.radius_km = f64::MIN_POSITIVE);
        accepted(|c| c.matching.radius_km = MAX_MATCH_RADIUS_KM - 0.001);
        rejected(|c| c.matching.radius_km = MAX_MATCH_RADIUS_KM + 0.001, "matching.radius_km");
        rejected(|c| c.matching.radius_km = f64::INFINITY, "matching.radius_km");
        rejected(|c| c.fleet.radius_km = -1.0, "fleet.radius_km");
        rejected(|c| c.fleet.radius_km = 0.0, "fleet.radius_km");
        accepted(|c| c.fleet.radius_km = f64::MIN_POSITIVE);
        accepted(|c| c.fleet.radius_km = MAX_FLEET_RADIUS_KM);
        rejected(|c| c.fleet.radius_km = MAX_FLEET_RADIUS_KM + 0.001, "fleet.radius_km");
        rejected(|c| c.fleet.radius_km = 100_000.0, "fleet.radius_km");
        rejected(|c| c.fleet.radius_km = f64::INFINITY, "fleet.radius_km");
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
        c.static_mask.min_active_months = 10;
        c.sama.enabled = true;
        let back: PipelineConfig = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
        assert_eq!(back.validate(), Ok(()));
    }
}
