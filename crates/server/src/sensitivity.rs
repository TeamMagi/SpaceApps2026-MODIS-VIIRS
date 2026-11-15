//! FireHarmonize-style preview of how the MODIS/VIIRS match outcome reacts to the match radius
//! and time window. The FIRMS data is fetched once and re-matched for every grid cell.

use axum::{Json, extract::State};
use pyrosync_core::models::HotspotSource;
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiQuery, AppState, HarmonizeParams, run_pipeline_with_timeout};

const MAX_GRID_AXIS: usize = 5;
const MAX_WINDOW_MIN: u32 = 1_440;

#[derive(Deserialize)]
pub(crate) struct SensitivityParams {
    west: f64,
    south: f64,
    east: f64,
    north: f64,
    days: Option<u32>,
    start: Option<String>,
    end: Option<String>,
    radius_km: String,
    window_min: String,
}

#[derive(Serialize)]
pub(crate) struct SensitivityCell {
    radius_km: f64,
    window_min: u32,
    matched: usize,
    modis_only: usize,
    viirs_only: usize,
    harmonized_total: usize,
}

#[derive(Serialize)]
pub(crate) struct SensitivityResponse {
    raw_modis: usize,
    raw_viirs: usize,
    radius_km: Vec<f64>,
    window_min: Vec<u32>,
    cells: Vec<SensitivityCell>,
}

fn parse_axis<T>(name: &str, raw: &str) -> Result<Vec<T>, ApiError>
where
    T: std::str::FromStr + PartialOrd + Copy,
{
    let mut values = Vec::new();
    for part in raw.split(',') {
        let value = part.trim().parse::<T>().map_err(|_| {
            ApiError::bad_request(format!("'{name}' contains an invalid value: '{part}'"))
        })?;
        values.push(value);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values.dedup();
    if values.len() > MAX_GRID_AXIS {
        return Err(ApiError::bad_request(format!(
            "'{name}' accepts at most {MAX_GRID_AXIS} distinct values, got {}",
            values.len()
        )));
    }
    Ok(values)
}

fn grid_config(
    base: &pyrosync_core::config::PipelineConfig,
    radius_km: f64,
    window_min: u32,
) -> Result<pyrosync_core::config::PipelineConfig, ApiError> {
    let mut config = base.clone();
    config.matching.radius_km = radius_km;
    config.matching.window_secs = i64::from(window_min) * 60;
    config.validate().map_err(ApiError::bad_request)?;
    Ok(config)
}

pub(crate) async fn sensitivity(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<SensitivityParams>,
) -> Result<Json<SensitivityResponse>, ApiError> {
    let radii = parse_axis::<f64>("radius_km", &params.radius_km)?;
    let windows = parse_axis::<u32>("window_min", &params.window_min)?;
    if let Some(&bad) = windows.iter().find(|&&w| w > MAX_WINDOW_MIN) {
        return Err(ApiError::bad_request(format!(
            "'window_min' must be <= {MAX_WINDOW_MIN}, got {bad}"
        )));
    }
    for &radius in &radii {
        for &window in &windows {
            grid_config(&state.config, radius, window)?;
        }
    }

    let pipeline_params = HarmonizeParams {
        west: params.west,
        south: params.south,
        east: params.east,
        north: params.north,
        days: params.days,
        start: params.start,
        end: params.end,
        include_raw: None,
        format: None,
    };
    let run = run_pipeline_with_timeout(&state, &pipeline_params).await?;
    let modis: Vec<_> = run.raw.iter().filter(|h| h.sensor.is_modis()).cloned().collect();
    let viirs: Vec<_> = run.raw.iter().filter(|h| h.sensor.is_viirs()).cloned().collect();

    let mut cells = Vec::with_capacity(radii.len() * windows.len());
    for &radius_km in &radii {
        for &window_min in &windows {
            let config = grid_config(&state.config, radius_km, window_min)?;
            let harmonized = pyrosync_core::harmonize::harmonize_with_config(
                modis.clone(),
                viirs.clone(),
                &config,
            );
            let count = |source: HotspotSource| harmonized.iter().filter(|h| h.source == source).count();
            cells.push(SensitivityCell {
                radius_km,
                window_min,
                matched: count(HotspotSource::Matched),
                modis_only: count(HotspotSource::ModisOnly),
                viirs_only: count(HotspotSource::ViirsOnly),
                harmonized_total: harmonized.len(),
            });
        }
    }

    Ok(Json(SensitivityResponse {
        raw_modis: run.modis_count,
        raw_viirs: run.viirs_count,
        radius_km: radii,
        window_min: windows,
        cells,
    }))
}
