use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

use pyrosync_core::effort::EffortFactor;
use pyrosync_core::models::{
    BBox, CalendarCell, Confidence, DayNight, HarmonizedHotspot, Hotspot, HotspotSource, Sensor,
};
use pyrosync_firms::{DayRange, FirmsClient, FirmsSource};

#[derive(Clone)]
struct AppState {
    firms: Option<Arc<FirmsClient>>,
}

#[derive(Deserialize)]
struct HarmonizeParams {
    west: f64,
    south: f64,
    east: f64,
    north: f64,
    days: Option<u32>,
    start: Option<String>,
    end: Option<String>,
    include_raw: Option<bool>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct AvailabilityEntry {
    source: &'static str,
    min_date: String,
    max_date: String,
}

#[derive(Serialize)]
struct ReportResponse {
    per_source: Vec<(String, usize)>,
    failures: Vec<(String, String)>,
    transactions_spent: u32,
    uncovered: Vec<(String, String)>,
}

#[derive(Serialize)]
struct RawHotspot {
    lat: f64,
    lon: f64,
    frp: f64,
    sensor: Sensor,
    acquired: DateTime<Utc>,
    daynight: DayNight,
}

impl From<&Hotspot> for RawHotspot {
    fn from(h: &Hotspot) -> Self {
        Self {
            lat: h.lat,
            lon: h.lon,
            frp: h.frp,
            sensor: h.sensor,
            acquired: h.acquired,
            daynight: h.daynight,
        }
    }
}

#[derive(Serialize)]
struct RawResponse {
    modis_count: usize,
    viirs_count: usize,
    total_raw: usize,
    // Omit raw points unless requested because serializing them roughly doubles large multi-month payloads.
    #[serde(skip_serializing_if = "Option::is_none")]
    hotspots: Option<Vec<RawHotspot>>,
}

#[derive(Serialize)]
struct EffortResponse {
    reliable: bool,
    reason: Option<String>,
    factors: Vec<EffortFactor>,
}

#[derive(Serialize)]
struct SourceBreakdown {
    modis_only: usize,
    viirs_only: usize,
    matched: usize,
}

#[derive(Serialize)]
struct CompareCounts {
    raw_total: usize,
    raw_modis: usize,
    raw_viirs: usize,
    harmonized_total: usize,
    removed_total: usize,
    removed_pct: f64,
    by_source: SourceBreakdown,
}

#[derive(Serialize)]
struct CompareFrp {
    raw_sum: f64,
    harmonized_raw_sum: f64,
    harmonized_corrected_sum: f64,
    reduction_pct: f64,
}

#[derive(Serialize)]
struct PlatformCombination {
    platforms: Vec<Sensor>,
    records: usize,
}

#[derive(Serialize)]
struct CompareMerging {
    records_with_multiple_observations: usize,
    observations_merged: usize,
    max_observations_in_one_record: u16,
    platform_combinations: Vec<PlatformCombination>,
}

#[derive(Serialize)]
struct CompareVza {
    records_with_correction: usize,
    mean_factor: Option<f64>,
    max_factor: Option<f64>,
    mean_frp_reduction_pct: Option<f64>,
}

#[derive(Serialize)]
struct CompareResponse {
    counts: CompareCounts,
    frp: CompareFrp,
    merging: CompareMerging,
    vza: CompareVza,
    warnings: Vec<String>,
    report: ReportResponse,
}

#[derive(Serialize)]
struct StatsTotals {
    raw_detections: usize,
    harmonized_records: usize,
    calendar_months: usize,
    distinct_platforms: usize,
}

#[derive(Serialize)]
struct SensorBreakdownEntry {
    sensor: Sensor,
    firms_source: &'static str,
    raw_detections: usize,
    share_pct: f64,
}

#[derive(Serialize)]
struct ConfidenceBreakdown {
    low: usize,
    nominal: usize,
    high: usize,
}

#[derive(Serialize)]
struct DayNightBreakdown {
    day: usize,
    night: usize,
}

#[derive(Serialize)]
struct StatsFrp {
    harmonized_sum: f64,
    harmonized_mean: f64,
    harmonized_max: f64,
    harmonized_median: f64,
}

#[derive(Serialize)]
struct StatsTemporal {
    first_detection: Option<DateTime<Utc>>,
    last_detection: Option<DateTime<Utc>>,
    months_covered: usize,
}

#[derive(Serialize)]
struct StatsPipeline {
    transactions_spent: u32,
    sources_queried: usize,
    sources_failed: usize,
}

#[derive(Serialize)]
struct StatsResponse {
    totals: StatsTotals,
    sensor_breakdown: Vec<SensorBreakdownEntry>,
    confidence_breakdown: ConfidenceBreakdown,
    daynight_breakdown: DayNightBreakdown,
    frp: StatsFrp,
    temporal: StatsTemporal,
    pipeline: StatsPipeline,
    warnings: Vec<String>,
    report: ReportResponse,
}

struct PipelineRun {
    #[allow(dead_code)] // Retained for upcoming /api/compare and /api/stats endpoints.
    bbox: BBox,
    modis_count: usize,
    viirs_count: usize,
    // Keep filtered raw detections even when include_raw=false so /api/compare and /api/stats can reuse this helper.
    raw: Vec<Hotspot>,
    harmonized: Vec<HarmonizedHotspot>,
    calendar: Vec<CalendarCell>,
    report: ReportResponse,
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

enum TimeWindow {
    Days(DayRange),
    Range { start: NaiveDate, end: NaiveDate },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("pyrosync=info".parse()?))
        .init();

    // Keep the server bootable without an API key so health checks and static assets stay live.
    let firms = match std::env::var("FIRMS_API_KEY") {
        Ok(key) if !key.trim().is_empty() => match FirmsClient::new(key) {
            Ok(client) => Some(Arc::new(client)),
            Err(err) => {
                tracing::error!("Failed to initialize FIRMS client: {err}");
                None
            }
        },
        _ => {
            tracing::warn!("FIRMS_API_KEY is not configured; FIRMS endpoints will return 500");
            None
        }
    };

    let state = AppState { firms };
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "web/dist".to_string());
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/harmonize", get(harmonize))
        .route("/api/availability", get(availability))
        .route("/api/compare", get(compare))
        .route("/api/stats", get(stats))
        .with_state(state)
        .fallback_service(ServeDir::new(&static_dir))
        .layer(CorsLayer::permissive());

    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".to_string());
    let addr = format!("0.0.0.0:{port}");
    tracing::info!("PyroSync server listening on {addr}, static files from {static_dir}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn availability(
    State(state): State<AppState>,
) -> Result<Json<Vec<AvailabilityEntry>>, ApiError> {
    let client = require_firms_client(&state)?;
    let avail = client
        .availability()
        .await
        .map_err(|e| ApiError::bad_gateway(format!("Failed to fetch FIRMS availability: {e}")))?;

    // MODIS Terra and Aqua share the same FIRMS source IDs, so probe Terra only to avoid duplicates.
    const SENSORS: [Sensor; 4] = [
        Sensor::ModisTerra,
        Sensor::ViirsSNPP,
        Sensor::ViirsNOAA20,
        Sensor::ViirsNOAA21,
    ];

    let mut entries = Vec::new();
    for sensor in SENSORS {
        for source in FirmsSource::all_for(sensor) {
            if let Some((min_date, max_date)) = avail.range_for(source) {
                entries.push(AvailabilityEntry {
                    source: source.as_str(),
                    min_date: min_date.format("%Y-%m-%d").to_string(),
                    max_date: max_date.format("%Y-%m-%d").to_string(),
                });
            }
        }
    }
    entries.sort_by_key(|entry| entry.source);

    Ok(Json(entries))
}

async fn run_pipeline(state: &AppState, params: &HarmonizeParams) -> Result<PipelineRun, ApiError> {
    let bbox = parse_bbox(params)?;
    let window = parse_time_window(params)?;
    let client = require_firms_client(state)?;

    let (mut modis, mut viirs, report) = match window {
        TimeWindow::Days(days) => client.fetch_all(&bbox, days).await,
        TimeWindow::Range { start, end } => client.fetch_all_range(&bbox, start, end).await,
    };

    if modis.is_empty() && viirs.is_empty() && !report.failures.is_empty() {
        let details = report
            .failures
            .iter()
            .map(|(source, err)| format!("{source}: {err}"))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ApiError::bad_gateway(format!(
            "Upstream FIRMS request failed: {details}"
        )));
    }

    pyrosync_firms::parse::retain_wildfires(&mut modis);
    pyrosync_firms::parse::retain_wildfires(&mut viirs);

    let modis_count = modis.len();
    let viirs_count = viirs.len();

    let mut raw = Vec::with_capacity(modis_count + viirs_count);
    raw.extend(modis.iter().cloned());
    raw.extend(viirs.iter().cloned());

    let harmonized = pyrosync_core::harmonize::harmonize(modis, viirs);
    let calendar = pyrosync_core::calendar::build_calendar(&harmonized);

    let report_json = ReportResponse {
        per_source: report.per_source,
        failures: report.failures,
        transactions_spent: report.transactions_spent,
        uncovered: report
            .uncovered
            .into_iter()
            .map(|(start, end)| {
                (
                    start.format("%Y-%m-%d").to_string(),
                    end.format("%Y-%m-%d").to_string(),
                )
            })
            .collect(),
    };

    Ok(PipelineRun {
        bbox,
        modis_count,
        viirs_count,
        raw,
        harmonized,
        calendar,
        report: report_json,
    })
}

fn build_warnings(report: &ReportResponse) -> (Option<String>, Vec<String>) {
    // Effort normalization infers each month's active fleet from retrieved detections, so upstream
    // FIRMS failures can drop a satellite and silently apply the wrong Ω factor.
    let failure_count = report.failures.len();
    let reason = (failure_count > 0).then(|| {
        let noun = if failure_count == 1 {
            "FIRMS request"
        } else {
            "FIRMS requests"
        };
        format!(
            "{failure_count} {noun} failed; missing source data may undercount monthly platform configurations and produce inaccurate effort_omega values."
        )
    });

    let mut warnings = Vec::new();
    if let Some(msg) = &reason {
        warnings.push(msg.clone());
    }
    if !report.uncovered.is_empty() {
        let ranges = report
            .uncovered
            .iter()
            .map(|(start, end)| format!("{start}..{end}"))
            .collect::<Vec<_>>()
            .join(", ");
        warnings.push(format!(
            "Requested time range has intervals not covered by any FIRMS source: {ranges}."
        ));
    }

    (reason, warnings)
}

fn build_effort_and_warnings(
    harmonized: &[HarmonizedHotspot],
    report: &ReportResponse,
) -> (EffortResponse, Vec<String>) {
    let (reason, warnings) = build_warnings(report);
    let effort = EffortResponse {
        reliable: reason.is_none(),
        reason,
        factors: pyrosync_core::effort::estimate_effort(harmonized),
    };

    (effort, warnings)
}

async fn harmonize(
    State(state): State<AppState>,
    Query(params): Query<HarmonizeParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let include_raw = params.include_raw.unwrap_or(false);
    let run = run_pipeline(&state, &params).await?;
    let (effort, warnings) = build_effort_and_warnings(&run.harmonized, &run.report);

    let raw = RawResponse {
        modis_count: run.modis_count,
        viirs_count: run.viirs_count,
        total_raw: run.modis_count + run.viirs_count,
        hotspots: include_raw.then(|| run.raw.iter().map(RawHotspot::from).collect()),
    };

    Ok(Json(serde_json::json!({
        "raw": raw,
        "harmonized": {
            "count": run.harmonized.len(),
            "hotspots": run.harmonized,
        },
        "calendar": run.calendar,
        "effort": effort,
        "warnings": warnings,
        "report": run.report,
    })))
}

fn build_compare_response(run: PipelineRun) -> CompareResponse {
    let raw_total = run.modis_count + run.viirs_count;
    let harmonized_total = run.harmonized.len();
    let removed_total = raw_total.saturating_sub(harmonized_total);
    let removed_pct = if raw_total == 0 {
        0.0
    } else {
        (removed_total as f64 / raw_total as f64) * 100.0
    };

    let mut by_source = SourceBreakdown {
        modis_only: 0,
        viirs_only: 0,
        matched: 0,
    };
    let mut harmonized_raw_sum = 0.0_f64;
    let mut harmonized_corrected_sum = 0.0_f64;
    let mut records_with_multiple_observations = 0_usize;
    let mut total_observations = 0_usize;
    let mut max_observations_in_one_record = 0_u16;
    let mut platform_counts: BTreeMap<Vec<Sensor>, usize> = BTreeMap::new();

    let mut vza_count = 0_usize;
    let mut vza_factor_sum = 0.0_f64;
    let mut vza_max_factor: Option<f64> = None;
    let mut vza_reduction_pct_sum = 0.0_f64;

    for h in &run.harmonized {
        match h.source {
            HotspotSource::ModisOnly => by_source.modis_only += 1,
            HotspotSource::ViirsOnly => by_source.viirs_only += 1,
            HotspotSource::Matched => by_source.matched += 1,
        }

        harmonized_raw_sum += h.frp;
        harmonized_corrected_sum += h.frp_corrected;

        if h.n_obs > 1 {
            records_with_multiple_observations += 1;
        }
        total_observations += usize::from(h.n_obs);
        max_observations_in_one_record = max_observations_in_one_record.max(h.n_obs);

        let mut platforms = h.platforms.clone();
        platforms.sort();
        platforms.dedup();
        *platform_counts.entry(platforms).or_insert(0) += 1;

        if let Some(factor) = h.vza_factor {
            vza_count += 1;
            vza_factor_sum += factor;
            vza_max_factor = Some(match vza_max_factor {
                Some(cur) if cur.total_cmp(&factor).is_ge() => cur,
                _ => factor,
            });
            if h.frp > 0.0 {
                vza_reduction_pct_sum += 100.0 * (1.0 - h.frp_corrected / h.frp);
            }
        }
    }

    let raw_sum: f64 = run.raw.iter().map(|h| h.frp).sum();
    let reduction_pct = if raw_sum == 0.0 {
        0.0
    } else {
        100.0 * (1.0 - harmonized_corrected_sum / raw_sum)
    };

    let mut platform_combinations: Vec<PlatformCombination> = platform_counts
        .into_iter()
        .map(|(platforms, records)| PlatformCombination { platforms, records })
        .collect();
    platform_combinations.sort_by(|a, b| {
        b.records
            .cmp(&a.records)
            .then_with(|| a.platforms.cmp(&b.platforms))
    });

    let vza = if vza_count == 0 {
        CompareVza {
            records_with_correction: 0,
            mean_factor: None,
            max_factor: None,
            mean_frp_reduction_pct: None,
        }
    } else {
        let count_f64 = vza_count as f64;
        CompareVza {
            records_with_correction: vza_count,
            mean_factor: Some(vza_factor_sum / count_f64),
            max_factor: vza_max_factor,
            mean_frp_reduction_pct: Some(vza_reduction_pct_sum / count_f64),
        }
    };

    let (_, warnings) = build_warnings(&run.report);

    CompareResponse {
        counts: CompareCounts {
            raw_total,
            raw_modis: run.modis_count,
            raw_viirs: run.viirs_count,
            harmonized_total,
            removed_total,
            removed_pct,
            by_source,
        },
        frp: CompareFrp {
            raw_sum,
            harmonized_raw_sum,
            harmonized_corrected_sum,
            reduction_pct,
        },
        merging: CompareMerging {
            records_with_multiple_observations,
            observations_merged: total_observations.saturating_sub(harmonized_total),
            max_observations_in_one_record,
            platform_combinations,
        },
        vza,
        warnings,
        report: run.report,
    }
}

async fn compare(
    State(state): State<AppState>,
    Query(params): Query<HarmonizeParams>,
) -> Result<Json<CompareResponse>, ApiError> {
    let run = run_pipeline(&state, &params).await?;
    Ok(Json(build_compare_response(run)))
}

fn build_stats_response(run: PipelineRun) -> StatsResponse {
    const ALL_SENSORS: [Sensor; 5] = [
        Sensor::ModisTerra,
        Sensor::ModisAqua,
        Sensor::ViirsSNPP,
        Sensor::ViirsNOAA20,
        Sensor::ViirsNOAA21,
    ];

    let raw_detections = run.raw.len();
    let mut sensor_counts: BTreeMap<Sensor, usize> = BTreeMap::new();
    for h in &run.raw {
        *sensor_counts.entry(h.sensor).or_insert(0) += 1;
    }

    let sensor_breakdown = ALL_SENSORS
        .into_iter()
        .map(|sensor| {
            let count = sensor_counts.get(&sensor).copied().unwrap_or(0);
            let share_pct = if raw_detections == 0 {
                0.0
            } else {
                (count as f64 / raw_detections as f64) * 100.0
            };
            SensorBreakdownEntry {
                sensor,
                firms_source: sensor.firms_source(),
                raw_detections: count,
                share_pct,
            }
        })
        .collect();

    let mut distinct_platforms = BTreeSet::new();
    let mut confidence_breakdown = ConfidenceBreakdown {
        low: 0,
        nominal: 0,
        high: 0,
    };
    let mut daynight_breakdown = DayNightBreakdown { day: 0, night: 0 };
    let mut first_detection: Option<DateTime<Utc>> = None;
    let mut last_detection: Option<DateTime<Utc>> = None;
    let mut frps = Vec::with_capacity(run.harmonized.len());

    for h in &run.harmonized {
        distinct_platforms.extend(h.platforms.iter().copied());

        match h.confidence {
            Confidence::Low => confidence_breakdown.low += 1,
            Confidence::Nominal => confidence_breakdown.nominal += 1,
            Confidence::High => confidence_breakdown.high += 1,
        }

        match h.daynight {
            DayNight::Day => daynight_breakdown.day += 1,
            DayNight::Night => daynight_breakdown.night += 1,
        }

        first_detection = Some(first_detection.map_or(h.acquired, |cur| cur.min(h.acquired)));
        last_detection = Some(last_detection.map_or(h.acquired, |cur| cur.max(h.acquired)));
        frps.push(h.frp_corrected);
    }

    let frp = if frps.is_empty() {
        StatsFrp {
            harmonized_sum: 0.0,
            harmonized_mean: 0.0,
            harmonized_max: 0.0,
            harmonized_median: 0.0,
        }
    } else {
        frps.sort_by(f64::total_cmp);
        let len = frps.len();
        let harmonized_sum: f64 = frps.iter().sum();
        let harmonized_mean = harmonized_sum / len as f64;
        let harmonized_max = frps[len - 1];
        let harmonized_median = if len % 2 == 1 {
            frps[len / 2]
        } else {
            (frps[len / 2 - 1] + frps[len / 2]) / 2.0
        };
        StatsFrp {
            harmonized_sum,
            harmonized_mean,
            harmonized_max,
            harmonized_median,
        }
    };

    let mut queried_sources: BTreeSet<&str> = run
        .report
        .per_source
        .iter()
        .map(|(source, _)| source.as_str())
        .collect();
    for (source, _) in &run.report.failures {
        queried_sources.insert(source.as_str());
    }

    let calendar_months = run.calendar.len();
    let (_, warnings) = build_warnings(&run.report);

    StatsResponse {
        totals: StatsTotals {
            raw_detections,
            harmonized_records: run.harmonized.len(),
            calendar_months,
            distinct_platforms: distinct_platforms.len(),
        },
        sensor_breakdown,
        confidence_breakdown,
        daynight_breakdown,
        frp,
        temporal: StatsTemporal {
            first_detection,
            last_detection,
            months_covered: calendar_months,
        },
        pipeline: StatsPipeline {
            transactions_spent: run.report.transactions_spent,
            sources_queried: queried_sources.len(),
            sources_failed: run.report.failures.len(),
        },
        warnings,
        report: run.report,
    }
}

async fn stats(
    State(state): State<AppState>,
    Query(params): Query<HarmonizeParams>,
) -> Result<Json<StatsResponse>, ApiError> {
    let run = run_pipeline(&state, &params).await?;
    Ok(Json(build_stats_response(run)))
}

fn require_firms_client(state: &AppState) -> Result<&FirmsClient, ApiError> {
    state
        .firms
        .as_deref()
        .ok_or_else(|| ApiError::internal("FIRMS_API_KEY is not configured"))
}

fn parse_bbox(params: &HarmonizeParams) -> Result<BBox, ApiError> {
    if !(-180.0..=180.0).contains(&params.west) || !(-180.0..=180.0).contains(&params.east) {
        return Err(ApiError::bad_request(
            "Longitude coordinates ('west', 'east') must be within [-180, 180]",
        ));
    }
    if !(-90.0..=90.0).contains(&params.south) || !(-90.0..=90.0).contains(&params.north) {
        return Err(ApiError::bad_request(
            "Latitude coordinates ('south', 'north') must be within [-90, 90]",
        ));
    }
    if params.west > params.east {
        return Err(ApiError::bad_request(
            "'west' must be less than or equal to 'east'",
        ));
    }
    if params.south > params.north {
        return Err(ApiError::bad_request(
            "'south' must be less than or equal to 'north'",
        ));
    }

    Ok(BBox {
        west: params.west,
        south: params.south,
        east: params.east,
        north: params.north,
    })
}

fn parse_time_window(params: &HarmonizeParams) -> Result<TimeWindow, ApiError> {
    match (params.days, params.start.as_deref(), params.end.as_deref()) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => Err(ApiError::bad_request(
            "'days' cannot be combined with 'start' or 'end'",
        )),
        (None, Some(_), None) | (None, None, Some(_)) => Err(ApiError::bad_request(
            "Both 'start' and 'end' are required when specifying a date range",
        )),
        (None, Some(start_str), Some(end_str)) => {
            let start = NaiveDate::parse_from_str(start_str, "%Y-%m-%d").map_err(|_| {
                ApiError::bad_request(format!(
                    "Invalid 'start' date '{start_str}': expected YYYY-MM-DD"
                ))
            })?;
            let end = NaiveDate::parse_from_str(end_str, "%Y-%m-%d").map_err(|_| {
                ApiError::bad_request(format!(
                    "Invalid 'end' date '{end_str}': expected YYYY-MM-DD"
                ))
            })?;
            if end < start {
                return Err(ApiError::bad_request(format!(
                    "'end' date ({end}) must not be earlier than 'start' date ({start})"
                )));
            }
            Ok(TimeWindow::Range { start, end })
        }
        (days_opt, None, None) => {
            let days = days_opt.unwrap_or(1);
            let day_range = u8::try_from(days)
                .ok()
                .and_then(|d| DayRange::new(d).ok())
                .ok_or_else(|| {
                    ApiError::bad_request(format!(
                        "Invalid 'days' value {days}: must be in the range 1..=5"
                    ))
                })?;
            Ok(TimeWindow::Days(day_range))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use pyrosync_core::models::FireType;

    fn sample_bbox() -> BBox {
        BBox {
            west: 26.0,
            south: 36.0,
            east: 45.0,
            north: 42.0,
        }
    }

    fn empty_report() -> ReportResponse {
        ReportResponse {
            per_source: Vec::new(),
            failures: Vec::new(),
            transactions_spent: 0,
            uncovered: Vec::new(),
        }
    }

    fn make_raw(sensor: Sensor, frp: f64, hour: u32, daynight: DayNight) -> Hotspot {
        Hotspot {
            lat: 39.0,
            lon: 32.0,
            brightness: 330.0,
            frp,
            confidence: Confidence::Nominal,
            sensor,
            acquired: Utc.with_ymd_and_hms(2026, 7, 15, hour, 0, 0).unwrap(),
            scan: 1.0,
            track: 1.0,
            satellite: String::new(),
            daynight,
            vza: None,
            fire_type: FireType::VegetationFire,
        }
    }

    struct HarmonizedArgs {
        platforms: Vec<Sensor>,
        source: HotspotSource,
        frp: f64,
        frp_corrected: f64,
        vza_factor: Option<f64>,
        n_obs: u16,
        confidence: Confidence,
        daynight: DayNight,
        acquired: DateTime<Utc>,
    }

    fn make_harmonized(args: HarmonizedArgs) -> HarmonizedHotspot {
        HarmonizedHotspot {
            lat: 39.0,
            lon: 32.0,
            frp: args.frp,
            frp_corrected: args.frp_corrected,
            vza_factor: args.vza_factor,
            n_obs: args.n_obs,
            platforms: args.platforms,
            confidence: args.confidence,
            source: args.source,
            acquired: args.acquired,
            daynight: args.daynight,
        }
    }

    #[test]
    fn compare_empty_dataset_returns_zeros_and_nulls() {
        let run = PipelineRun {
            bbox: sample_bbox(),
            modis_count: 0,
            viirs_count: 0,
            raw: Vec::new(),
            harmonized: Vec::new(),
            calendar: Vec::new(),
            report: empty_report(),
        };

        let resp = build_compare_response(run);
        assert_eq!(resp.counts.raw_total, 0);
        assert_eq!(resp.counts.raw_modis, 0);
        assert_eq!(resp.counts.raw_viirs, 0);
        assert_eq!(resp.counts.harmonized_total, 0);
        assert_eq!(resp.counts.removed_total, 0);
        assert_eq!(resp.counts.removed_pct, 0.0);
        assert_eq!(resp.frp.raw_sum, 0.0);
        assert_eq!(resp.frp.reduction_pct, 0.0);
        assert_eq!(resp.merging.records_with_multiple_observations, 0);
        assert_eq!(resp.merging.observations_merged, 0);
        assert_eq!(resp.merging.max_observations_in_one_record, 0);
        assert!(resp.merging.platform_combinations.is_empty());
        assert_eq!(resp.vza.records_with_correction, 0);
        assert!(resp.vza.mean_factor.is_none());
        assert!(resp.vza.max_factor.is_none());
        assert!(resp.vza.mean_frp_reduction_pct.is_none());
        assert!(resp.warnings.is_empty());
    }

    #[test]
    fn compare_metrics_and_deterministic_platform_order() {
        let dt = Utc.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).unwrap();
        let raw = vec![
            make_raw(Sensor::ModisTerra, 100.0, 10, DayNight::Day),
            make_raw(Sensor::ModisAqua, 50.0, 11, DayNight::Day),
            make_raw(Sensor::ViirsSNPP, 40.0, 12, DayNight::Day),
            make_raw(Sensor::ViirsNOAA20, 60.0, 13, DayNight::Night),
            make_raw(Sensor::ViirsNOAA21, 50.0, 14, DayNight::Night),
        ];
        // raw_sum = 300.0; modis = 2, viirs = 3
        let harmonized = vec![
            // Tie on count=1 between [ModisTerra] and [ViirsSNPP, ViirsNOAA20]; ModisTerra < ViirsSNPP
            make_harmonized(HarmonizedArgs {
                platforms: vec![Sensor::ViirsNOAA20, Sensor::ViirsSNPP],
                source: HotspotSource::ViirsOnly,
                frp: 60.0,
                frp_corrected: 60.0,
                vza_factor: None,
                n_obs: 2,
                confidence: Confidence::High,
                daynight: DayNight::Day,
                acquired: dt,
            }),
            make_harmonized(HarmonizedArgs {
                platforms: vec![Sensor::ModisTerra],
                source: HotspotSource::ModisOnly,
                frp: 100.0,
                frp_corrected: 50.0,
                vza_factor: Some(2.0),
                n_obs: 1,
                confidence: Confidence::Nominal,
                daynight: DayNight::Day,
                acquired: dt,
            }),
            make_harmonized(HarmonizedArgs {
                platforms: vec![Sensor::ModisAqua, Sensor::ViirsNOAA21],
                source: HotspotSource::Matched,
                frp: 50.0,
                frp_corrected: 50.0,
                vza_factor: None,
                n_obs: 2,
                confidence: Confidence::High,
                daynight: DayNight::Night,
                acquired: dt,
            }),
            make_harmonized(HarmonizedArgs {
                platforms: vec![Sensor::ModisAqua, Sensor::ViirsNOAA21],
                source: HotspotSource::Matched,
                frp: 40.0,
                frp_corrected: 40.0,
                vza_factor: None,
                n_obs: 3,
                confidence: Confidence::Low,
                daynight: DayNight::Night,
                acquired: dt,
            }),
        ];
        let mut report = empty_report();
        report
            .failures
            .push(("MODIS_NRT".to_string(), "timeout".to_string()));

        let run = PipelineRun {
            bbox: sample_bbox(),
            modis_count: 2,
            viirs_count: 3,
            raw,
            harmonized,
            calendar: Vec::new(),
            report,
        };

        let resp = build_compare_response(run);
        assert_eq!(resp.counts.raw_total, 5);
        assert_eq!(resp.counts.harmonized_total, 4);
        assert_eq!(resp.counts.removed_total, 1);
        assert!((resp.counts.removed_pct - 20.0).abs() < 1e-9);
        assert_eq!(resp.counts.by_source.modis_only, 1);
        assert_eq!(resp.counts.by_source.viirs_only, 1);
        assert_eq!(resp.counts.by_source.matched, 2);

        assert!((resp.frp.raw_sum - 300.0).abs() < 1e-9);
        assert!((resp.frp.harmonized_raw_sum - 250.0).abs() < 1e-9);
        assert!((resp.frp.harmonized_corrected_sum - 200.0).abs() < 1e-9);
        assert!((resp.frp.reduction_pct - (100.0 / 3.0)).abs() < 1e-9);

        assert_eq!(resp.merging.records_with_multiple_observations, 3);
        // total n_obs = 2 + 1 + 2 + 3 = 8; merged = 8 - 4 = 4
        assert_eq!(resp.merging.observations_merged, 4);
        assert_eq!(resp.merging.max_observations_in_one_record, 3);
        assert_eq!(resp.merging.platform_combinations.len(), 3);
        assert_eq!(
            resp.merging.platform_combinations[0].platforms,
            vec![Sensor::ModisAqua, Sensor::ViirsNOAA21]
        );
        assert_eq!(resp.merging.platform_combinations[0].records, 2);
        assert_eq!(
            resp.merging.platform_combinations[1].platforms,
            vec![Sensor::ModisTerra]
        );
        assert_eq!(
            resp.merging.platform_combinations[2].platforms,
            vec![Sensor::ViirsSNPP, Sensor::ViirsNOAA20]
        );

        assert_eq!(resp.vza.records_with_correction, 1);
        assert_eq!(resp.vza.mean_factor, Some(2.0));
        assert_eq!(resp.vza.max_factor, Some(2.0));
        assert_eq!(resp.vza.mean_frp_reduction_pct, Some(50.0));
        assert_eq!(resp.warnings.len(), 1);
    }

    #[test]
    fn stats_empty_dataset_returns_zeros_and_nulls() {
        let run = PipelineRun {
            bbox: sample_bbox(),
            modis_count: 0,
            viirs_count: 0,
            raw: Vec::new(),
            harmonized: Vec::new(),
            calendar: Vec::new(),
            report: empty_report(),
        };

        let resp = build_stats_response(run);
        assert_eq!(resp.totals.raw_detections, 0);
        assert_eq!(resp.totals.harmonized_records, 0);
        assert_eq!(resp.totals.calendar_months, 0);
        assert_eq!(resp.totals.distinct_platforms, 0);
        assert_eq!(resp.sensor_breakdown.len(), 5);
        assert!(
            resp.sensor_breakdown
                .iter()
                .all(|s| s.raw_detections == 0 && s.share_pct == 0.0)
        );
        assert_eq!(resp.frp.harmonized_sum, 0.0);
        assert_eq!(resp.frp.harmonized_mean, 0.0);
        assert_eq!(resp.frp.harmonized_max, 0.0);
        assert_eq!(resp.frp.harmonized_median, 0.0);
        assert!(resp.temporal.first_detection.is_none());
        assert!(resp.temporal.last_detection.is_none());
        assert_eq!(resp.temporal.months_covered, 0);
        assert_eq!(resp.pipeline.sources_queried, 0);
        assert_eq!(resp.pipeline.sources_failed, 0);
    }

    #[test]
    fn stats_metrics_median_and_pipeline() {
        let t1 = Utc.with_ymd_and_hms(2026, 7, 1, 6, 14, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 8, 14, 18, 28, 0).unwrap();
        let raw = vec![
            make_raw(Sensor::ViirsNOAA20, 10.0, 10, DayNight::Day),
            make_raw(Sensor::ViirsNOAA20, 20.0, 11, DayNight::Day),
            make_raw(Sensor::ModisTerra, 30.0, 12, DayNight::Night),
            make_raw(Sensor::ViirsSNPP, 40.0, 13, DayNight::Night),
        ];
        let harmonized = vec![
            make_harmonized(HarmonizedArgs {
                platforms: vec![Sensor::ViirsNOAA20],
                source: HotspotSource::ViirsOnly,
                frp: 10.0,
                frp_corrected: 10.0,
                vza_factor: None,
                n_obs: 1,
                confidence: Confidence::Low,
                daynight: DayNight::Day,
                acquired: t2,
            }),
            make_harmonized(HarmonizedArgs {
                platforms: vec![Sensor::ModisTerra, Sensor::ViirsSNPP],
                source: HotspotSource::Matched,
                frp: 30.0,
                frp_corrected: 40.0,
                vza_factor: None,
                n_obs: 2,
                confidence: Confidence::High,
                daynight: DayNight::Night,
                acquired: t1,
            }),
            make_harmonized(HarmonizedArgs {
                platforms: vec![Sensor::ViirsNOAA20],
                source: HotspotSource::ViirsOnly,
                frp: 20.0,
                frp_corrected: 20.0,
                vza_factor: None,
                n_obs: 1,
                confidence: Confidence::Nominal,
                daynight: DayNight::Day,
                acquired: t1,
            }),
            make_harmonized(HarmonizedArgs {
                platforms: vec![Sensor::ViirsSNPP],
                source: HotspotSource::ViirsOnly,
                frp: 50.0,
                frp_corrected: 50.0,
                vza_factor: None,
                n_obs: 1,
                confidence: Confidence::High,
                daynight: DayNight::Night,
                acquired: t2,
            }),
        ];
        let calendar = pyrosync_core::calendar::build_calendar(&harmonized);
        let report = ReportResponse {
            per_source: vec![
                ("MODIS_NRT".to_string(), 1),
                ("VIIRS_NOAA20_NRT".to_string(), 2),
                ("VIIRS_SNPP_NRT".to_string(), 1),
            ],
            failures: vec![("VIIRS_NOAA21_NRT".to_string(), "502".to_string())],
            transactions_spent: 4,
            uncovered: vec![("2026-07-01".to_string(), "2026-07-02".to_string())],
        };

        let run = PipelineRun {
            bbox: sample_bbox(),
            modis_count: 1,
            viirs_count: 3,
            raw,
            harmonized,
            calendar,
            report,
        };

        let resp = build_stats_response(run);
        assert_eq!(resp.totals.raw_detections, 4);
        assert_eq!(resp.totals.harmonized_records, 4);
        assert_eq!(resp.totals.calendar_months, 2);
        assert_eq!(resp.totals.distinct_platforms, 3);

        assert_eq!(resp.sensor_breakdown[0].sensor, Sensor::ModisTerra);
        assert_eq!(resp.sensor_breakdown[0].firms_source, "MODIS_NRT");
        assert!((resp.sensor_breakdown[0].share_pct - 25.0).abs() < 1e-9);
        assert_eq!(resp.sensor_breakdown[3].sensor, Sensor::ViirsNOAA20);
        assert_eq!(resp.sensor_breakdown[3].raw_detections, 2);
        assert!((resp.sensor_breakdown[3].share_pct - 50.0).abs() < 1e-9);

        assert_eq!(resp.confidence_breakdown.low, 1);
        assert_eq!(resp.confidence_breakdown.nominal, 1);
        assert_eq!(resp.confidence_breakdown.high, 2);
        assert_eq!(resp.daynight_breakdown.day, 2);
        assert_eq!(resp.daynight_breakdown.night, 2);

        // Sorted FRPs: [10.0, 20.0, 40.0, 50.0] -> sum=120, mean=30, max=50, median=30
        assert!((resp.frp.harmonized_sum - 120.0).abs() < 1e-9);
        assert!((resp.frp.harmonized_mean - 30.0).abs() < 1e-9);
        assert!((resp.frp.harmonized_max - 50.0).abs() < 1e-9);
        assert!((resp.frp.harmonized_median - 30.0).abs() < 1e-9);

        assert_eq!(resp.temporal.first_detection, Some(t1));
        assert_eq!(resp.temporal.last_detection, Some(t2));
        assert_eq!(resp.temporal.months_covered, 2);

        assert_eq!(resp.pipeline.transactions_spent, 4);
        assert_eq!(resp.pipeline.sources_queried, 4);
        assert_eq!(resp.pipeline.sources_failed, 1);
        assert_eq!(resp.warnings.len(), 2);
    }

    #[tokio::test]
    async fn handlers_enforce_validation_before_firms_client() {
        let state = AppState { firms: None };
        let bad_bbox = HarmonizeParams {
            west: 50.0,
            south: 36.0,
            east: 45.0,
            north: 42.0,
            days: Some(1),
            start: None,
            end: None,
            include_raw: Some(true),
        };
        let err = compare(State(state.clone()), Query(bad_bbox))
            .await
            .err()
            .expect("invalid bbox should fail");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);

        let missing_key = HarmonizeParams {
            west: 26.0,
            south: 36.0,
            east: 45.0,
            north: 42.0,
            days: Some(1),
            start: None,
            end: None,
            include_raw: Some(true),
        };
        let err = stats(State(state), Query(missing_key))
            .await
            .err()
            .expect("missing FIRMS key should return 500");
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
