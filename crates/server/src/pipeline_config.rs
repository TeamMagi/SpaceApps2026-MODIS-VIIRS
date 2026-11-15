use std::path::Path;
use std::str::FromStr;

use anyhow::Context;
use pyrosync_core::config::PipelineConfig;

const DEFAULT_CONFIG_FILE: &str = "pyrosync.toml";

/// Builds the effective config: defaults, then the TOML file, then environment overrides.
///
/// An explicit `path` must exist; without one, `pyrosync.toml` in the working directory is used
/// only if present. `env` is injected so tests do not touch process-global state.
pub fn load(
    path: Option<&Path>,
    env: impl Fn(&str) -> Option<String>,
) -> anyhow::Result<PipelineConfig> {
    let mut config = match path {
        Some(path) => read_file(path)?,
        None if Path::new(DEFAULT_CONFIG_FILE).is_file() => read_file(Path::new(DEFAULT_CONFIG_FILE))?,
        None => PipelineConfig::default(),
    };

    override_from_env(&mut config, &env)?;
    config
        .validate()
        .map_err(anyhow::Error::msg)
        .context("invalid pipeline configuration")?;
    Ok(config)
}

fn read_file(path: &Path) -> anyhow::Result<PipelineConfig> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read pipeline config {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("cannot parse pipeline config {}", path.display()))
}

fn override_from_env(
    config: &mut PipelineConfig,
    env: &impl Fn(&str) -> Option<String>,
) -> anyhow::Result<()> {
    set(env, "PYROSYNC_DEDUP_MAX_TIME_DIFF_SECS", &mut config.dedup.max_time_diff_secs)?;
    set(env, "PYROSYNC_DEDUP_MIN_OVERLAP_RATIO", &mut config.dedup.min_overlap_ratio)?;
    set(env, "PYROSYNC_FLEET_VIIRS_WINDOW_SECS", &mut config.fleet.viirs_window_secs)?;
    set(env, "PYROSYNC_FLEET_MODIS_WINDOW_SECS", &mut config.fleet.modis_window_secs)?;
    set(env, "PYROSYNC_FLEET_RADIUS_KM", &mut config.fleet.radius_km)?;
    set(env, "PYROSYNC_MATCH_RADIUS_KM", &mut config.matching.radius_km)?;
    set(env, "PYROSYNC_MATCH_WINDOW_SECS", &mut config.matching.window_secs)?;
    set(env, "PYROSYNC_VZA_A", &mut config.vza.a)?;
    set(env, "PYROSYNC_VZA_B", &mut config.vza.b)?;
    set(env, "PYROSYNC_VZA_C", &mut config.vza.c)?;
    set(env, "PYROSYNC_STATIC_MASK_ENABLED", &mut config.static_mask.enabled)?;
    set(
        env,
        "PYROSYNC_STATIC_MASK_MIN_ACTIVE_MONTHS",
        &mut config.static_mask.min_active_months,
    )?;
    set(env, "PYROSYNC_SAMA_ENABLED", &mut config.sama.enabled)?;
    Ok(())
}

fn set<T>(
    env: &impl Fn(&str) -> Option<String>,
    name: &str,
    target: &mut T,
) -> anyhow::Result<()>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    if let Some(raw) = env(name) {
        *target = raw
            .trim()
            .parse()
            .with_context(|| format!("{name}: cannot parse {raw:?}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    struct TempToml(PathBuf);

    impl TempToml {
        fn new(tag: &str, content: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "pyrosync-config-test-{}-{tag}.toml",
                std::process::id()
            ));
            std::fs::write(&path, content).expect("write temp config");
            Self(path)
        }
    }

    impl Drop for TempToml {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn no_file_and_no_env_gives_default() {
        // The test working directory is the crate root, which holds no pyrosync.toml.
        assert_eq!(load(None, no_env).unwrap(), PipelineConfig::default());
    }

    #[test]
    fn example_file_equals_defaults() {
        let example: PipelineConfig =
            toml::from_str(include_str!("../../../pyrosync.example.toml")).unwrap();
        assert_eq!(example, PipelineConfig::default());
    }

    #[test]
    fn partial_toml_overrides_only_named_keys() {
        let file = TempToml::new("partial", "[matching]\nwindow_secs = 600\n\n[vza]\nb = 0.5\n");
        let config = load(Some(&file.0), no_env).unwrap();
        let mut expected = PipelineConfig::default();
        expected.matching.window_secs = 600;
        expected.vza.b = 0.5;
        assert_eq!(config, expected);
    }

    #[test]
    fn env_beats_toml() {
        let file = TempToml::new("env", "[matching]\nradius_km = 3.0\nwindow_secs = 600\n");
        let env = |name: &str| (name == "PYROSYNC_MATCH_RADIUS_KM").then(|| "4.5".to_string());
        let config = load(Some(&file.0), env).unwrap();
        assert_eq!(config.matching.radius_km, 4.5);
        assert_eq!(config.matching.window_secs, 600);
    }

    #[test]
    fn unknown_toml_key_is_an_error() {
        let file = TempToml::new("unknown", "[matching]\nradius = 3.0\n");
        assert!(load(Some(&file.0), no_env).is_err());
    }

    #[test]
    fn invalid_value_names_the_field() {
        let file = TempToml::new("invalid", "[matching]\nradius_km = 50.0\n");
        let err = format!("{:#}", load(Some(&file.0), no_env).unwrap_err());
        assert!(err.contains("matching.radius_km"), "{err}");
    }

    #[test]
    fn explicit_missing_path_is_an_error() {
        let missing = std::env::temp_dir().join("pyrosync-config-test-does-not-exist.toml");
        assert!(load(Some(&missing), no_env).is_err());
    }

    #[test]
    fn toml_round_trip_preserves_config() {
        let mut config = PipelineConfig::default();
        config.static_mask.min_active_months = 11;
        config.sama.enabled = true;
        config.sama.north = 0.0;
        let text = toml::to_string(&config).unwrap();
        assert_eq!(toml::from_str::<PipelineConfig>(&text).unwrap(), config);
        let default_text = toml::to_string(&PipelineConfig::default()).unwrap();
        assert_eq!(
            toml::from_str::<PipelineConfig>(&default_text).unwrap(),
            PipelineConfig::default()
        );
    }

    #[test]
    fn prefilters_toggle_independently_from_toml_and_env() {
        let file = TempToml::new("prefilters", "[static_mask]\nenabled = false\n\n[sama]\nenabled = true\n");
        let config = load(Some(&file.0), no_env).unwrap();
        assert!(!config.static_mask.enabled);
        assert!(config.sama.enabled);
        assert_eq!(config.static_mask.min_active_months, PipelineConfig::default().static_mask.min_active_months);

        let env = |name: &str| match name {
            "PYROSYNC_STATIC_MASK_ENABLED" => Some("true".to_string()),
            "PYROSYNC_STATIC_MASK_MIN_ACTIVE_MONTHS" => Some("12".to_string()),
            "PYROSYNC_SAMA_ENABLED" => Some("false".to_string()),
            _ => None,
        };
        let config = load(Some(&file.0), env).unwrap();
        assert!(config.static_mask.enabled);
        assert_eq!(config.static_mask.min_active_months, 12);
        assert!(!config.sama.enabled);
    }

    #[test]
    fn invalid_static_mask_months_from_env_names_the_field() {
        let env = |name: &str| (name == "PYROSYNC_STATIC_MASK_MIN_ACTIVE_MONTHS").then(|| "13".to_string());
        let err = format!("{:#}", load(None, env).unwrap_err());
        assert!(err.contains("static_mask.min_active_months"), "{err}");
    }

    #[test]
    fn unparsable_env_value_names_the_variable() {
        let env = |name: &str| (name == "PYROSYNC_VZA_A").then(|| "abc".to_string());
        let err = format!("{:#}", load(None, env).unwrap_err());
        assert!(err.contains("PYROSYNC_VZA_A"), "{err}");
    }
}
