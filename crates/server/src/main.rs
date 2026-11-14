mod archive_api;
#[cfg(test)]
mod integration_tests;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    Router,
    extract::{OriginalUri, Query, State},
    http::{HeaderName, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Json, Response},
    routing::get,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, Semaphore};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

use pyrosync_core::effort::EffortFactor;
use pyrosync_core::models::{
    BBox, CalendarCell, Confidence, DayNight, HarmonizedHotspot, Hotspot, HotspotSource, Sensor,
};
use pyrosync_firms::archive::{ArchiveClient, archive_years, filter_bbox};
use pyrosync_firms::{DayRange, FirmsClient, FirmsSource};

const DEFAULT_CACHE_BUDGET_MB: usize = 128;
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 300;
const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const ARCHIVE_FETCH_CONCURRENCY: usize = 4;
// NRT (`days`) responses update continuously as new satellite passes arrive, so keep their TTL short.
const CACHE_TTL_NRT_SECS: u64 = 600;
// Archive (`start`/`end` and `/api/calendar`) responses cover finalized science-quality data that no longer changes.
const CACHE_TTL_ARCHIVE_SECS: u64 = 21_600;
const X_CACHE_HEADER: HeaderName = HeaderName::from_static("x-cache");

// MODIS Terra and Aqua share the same `modis` archive CSVs (tagged per-row by `satellite`).
const ARCHIVE_FETCH_SENSORS: [Sensor; 3] =
    [Sensor::ModisTerra, Sensor::ViirsSNPP, Sensor::ViirsNOAA20];
const ARCHIVE_AVAILABLE_SENSORS: [Sensor; 4] = [
    Sensor::ModisTerra,
    Sensor::ModisAqua,
    Sensor::ViirsSNPP,
    Sensor::ViirsNOAA20,
];
// NOAA-21 is omitted because FIRMS has no `viirs-jpss2` country archive.
const ARCHIVE_UNAVAILABLE_SENSORS: [Sensor; 1] = [Sensor::ViirsNOAA21];
const ARCHIVE_COVERAGE_NOTE: &str = "The FIRMS country archive packages MODIS, VIIRS SNPP and VIIRS NOAA-20; NOAA-21 is not included, so this calendar covers a smaller fleet than the near-real-time endpoints from 2023 onward.";

#[derive(Clone)]
struct AppState {
    firms: Option<Arc<FirmsClient>>,
    archive: Option<Arc<ArchiveClient>>,
    cache: Arc<RwLock<ResponseCache>>,
    request_timeout: Option<Duration>,
}

struct CacheEntry {
    body: Vec<u8>,
    inserted_at: Instant,
    ttl: Duration,
    seq: u64,
}

enum CacheLookup {
    Hit(Vec<u8>),
    Expired,
    Miss,
}

struct ResponseCache {
    budget_bytes: usize,
    total_bytes: usize,
    next_seq: u64,
    entries: HashMap<String, CacheEntry>,
    // Track (inserted_at, seq) in a BTreeMap so eviction drops the oldest entry in O(log N)
    // even when tests inject explicit timestamps.
    order: BTreeMap<(Instant, u64), String>,
}

impl ResponseCache {
    fn new(budget_bytes: usize) -> Self {
        Self {
            budget_bytes,
            total_bytes: 0,
            next_seq: 0,
            entries: HashMap::new(),
            order: BTreeMap::new(),
        }
    }

    fn can_fit(&self, size: usize) -> bool {
        self.budget_bytes > 0 && size <= self.budget_bytes
    }

    fn peek_at(&self, key: &str, now: Instant) -> CacheLookup {
        if self.budget_bytes == 0 {
            return CacheLookup::Miss;
        }
        let Some(entry) = self.entries.get(key) else {
            return CacheLookup::Miss;
        };
        if now.saturating_duration_since(entry.inserted_at) >= entry.ttl {
            CacheLookup::Expired
        } else {
            CacheLookup::Hit(entry.body.clone())
        }
    }

    fn remove_entry(&mut self, key: &str) {
        if let Some(removed) = self.entries.remove(key) {
            self.total_bytes = self.total_bytes.saturating_sub(removed.body.len());
            self.order.remove(&(removed.inserted_at, removed.seq));
        }
    }

    fn get_at(&mut self, key: &str, now: Instant) -> Option<Vec<u8>> {
        match self.peek_at(key, now) {
            CacheLookup::Hit(body) => Some(body),
            CacheLookup::Expired => {
                self.remove_entry(key);
                None
            }
            CacheLookup::Miss => None,
        }
    }

    fn insert_at(&mut self, key: String, body: Vec<u8>, ttl: Duration, now: Instant) {
        if !self.can_fit(body.len()) {
            return;
        }

        // Remove any prior entry for this key before enforcing the budget.
        self.remove_entry(&key);

        let entry_len = body.len();
        while self.total_bytes.saturating_add(entry_len) > self.budget_bytes {
            let Some((_, oldest_key)) = self.order.pop_first() else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest_key) {
                self.total_bytes = self.total_bytes.saturating_sub(removed.body.len());
            }
        }

        let seq = self.next_seq;
        self.next_seq += 1;
        self.total_bytes += entry_len;
        self.order.insert((now, seq), key.clone());
        self.entries.insert(
            key,
            CacheEntry {
                body,
                inserted_at: now,
                ttl,
                seq,
            },
        );
    }
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

#[derive(Deserialize)]
struct CalendarParams {
    country: String,
    from: Option<i32>,
    to: Option<i32>,
    west: Option<f64>,
    south: Option<f64>,
    east: Option<f64>,
    north: Option<f64>,
}

#[derive(Serialize)]
struct CalendarYears {
    requested_from: i32,
    requested_to: i32,
    effective_from: i32,
    effective_to: i32,
}

#[derive(Serialize)]
struct ArchiveCoverage {
    sensors_available: Vec<Sensor>,
    sensors_unavailable: Vec<Sensor>,
    note: &'static str,
}

#[derive(Serialize)]
struct CalendarCounts {
    raw_detections: usize,
    harmonized_records: usize,
}

#[derive(Serialize)]
struct SensorYearFetch {
    sensor: Sensor,
    year: i32,
    detections: usize,
}

#[derive(Serialize)]
struct SensorYearFailure {
    sensor: Sensor,
    year: i32,
    error: String,
}

#[derive(Serialize)]
struct ArchiveFetchSummary {
    per_sensor_year: Vec<SensorYearFetch>,
    failures: Vec<SensorYearFailure>,
}

#[derive(Serialize)]
struct CalendarResponse {
    country: String,
    years: CalendarYears,
    bbox: Option<BBox>,
    coverage: ArchiveCoverage,
    counts: CalendarCounts,
    calendar: Vec<CalendarCell>,
    effort: EffortResponse,
    fetch: ArchiveFetchSummary,
    warnings: Vec<String>,
}

struct ValidatedCalendarQuery {
    country: String,
    years: CalendarYears,
    clamped: bool,
    archive_min_year: i32,
    archive_max_year: i32,
    bbox: Option<BBox>,
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

#[derive(Debug)]
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

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn gateway_timeout(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
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

fn parse_cache_budget_bytes(raw: Option<&str>) -> usize {
    let default_bytes = DEFAULT_CACHE_BUDGET_MB * 1024 * 1024;
    let Some(raw) = raw else {
        return default_bytes;
    };
    let trimmed = raw.trim();
    match trimmed.parse::<usize>() {
        Ok(0) => 0,
        Ok(mb) => mb.checked_mul(1024 * 1024).unwrap_or_else(|| {
            tracing::warn!(
                "CACHE_BUDGET_MB value '{trimmed}' overflows usize; falling back to {DEFAULT_CACHE_BUDGET_MB} MB"
            );
            default_bytes
        }),
        Err(_) => {
            tracing::warn!(
                "Invalid CACHE_BUDGET_MB value '{trimmed}'; falling back to {DEFAULT_CACHE_BUDGET_MB} MB"
            );
            default_bytes
        }
    }
}

fn parse_request_timeout(raw: Option<&str>) -> Option<Duration> {
    let Some(raw) = raw else {
        return Some(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS));
    };
    let trimmed = raw.trim();
    match trimmed.parse::<u64>() {
        Ok(0) => None,
        Ok(secs) => Some(Duration::from_secs(secs)),
        Err(_) => {
            tracing::warn!(
                "Invalid REQUEST_TIMEOUT_SECS value '{trimmed}'; falling back to {DEFAULT_REQUEST_TIMEOUT_SECS}s"
            );
            Some(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS))
        }
    }
}

fn parse_cors_origin(raw: &str) -> Option<HeaderValue> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let header_value = match HeaderValue::from_str(trimmed) {
        Ok(val) => val,
        Err(err) => {
            tracing::error!("Ignoring invalid CORS origin '{trimmed}': {err}");
            return None;
        }
    };
    let uri = match trimmed.parse::<Uri>() {
        Ok(uri) => uri,
        Err(err) => {
            tracing::error!("Ignoring invalid CORS origin '{trimmed}': {err}");
            return None;
        }
    };
    if uri.scheme_str().is_none() || uri.authority().is_none() {
        tracing::error!("Ignoring invalid CORS origin '{trimmed}': missing scheme or host");
        return None;
    }
    Some(header_value)
}

fn build_cors_layer(raw: Option<&str>) -> CorsLayer {
    let Some(raw) = raw else {
        tracing::warn!("CORS is permissive; set CORS_ALLOWED_ORIGINS in production");
        return CorsLayer::permissive();
    };

    let origins: Vec<HeaderValue> = raw.split(',').filter_map(parse_cors_origin).collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET])
        .allow_headers([header::CONTENT_TYPE])
}

async fn api_not_found(OriginalUri(uri): OriginalUri) -> ApiError {
    ApiError::not_found(format!("Unknown API endpoint: {}", uri.path()))
}

fn build_router(
    state: AppState,
    archive_api: Option<Arc<archive_api::ArchiveState>>,
    static_dir: &str,
    cors: CorsLayer,
) -> Router {
    let mut api_router = Router::new()
        .route("/health", get(health))
        .route("/harmonize", get(harmonize))
        .route("/availability", get(availability))
        .route("/compare", get(compare))
        .route("/stats", get(stats))
        .route("/calendar", get(calendar))
        .with_state(state);

    if let Some(archive_api) = archive_api {
        api_router = api_router.merge(archive_api::api_routes(archive_api));
    }

    let api_router = api_router.fallback(api_not_found);

    Router::new()
        .nest("/api", api_router)
        .fallback_service(ServeDir::new(static_dir))
        .layer(cors)
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

    let archive_cache_dir = std::env::var("ARCHIVE_CACHE_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("FIRMS_ARCHIVE_DIR")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR));
    let archive_api = Arc::new(archive_api::ArchiveState::new(
        archive_cache_dir.clone(),
        archive_api::ArchiveLimits::from_env(),
    ));
    if archive_cache_dir.is_dir() {
        let warm_state = archive_api.clone();
        tokio::spawn(async move { warm_state.warm().await });
    }
    let archive = match ArchiveClient::new(archive_cache_dir.clone()) {
        Ok(client) => Some(Arc::new(client)),
        Err(err) => {
            tracing::error!(
                "Failed to initialize FIRMS archive client at {}: {err}",
                archive_cache_dir.display()
            );
            None
        }
    };

    let cache_budget_bytes = parse_cache_budget_bytes(std::env::var("CACHE_BUDGET_MB").ok().as_deref());
    let request_timeout = parse_request_timeout(std::env::var("REQUEST_TIMEOUT_SECS").ok().as_deref());
    let cors_layer = build_cors_layer(std::env::var("CORS_ALLOWED_ORIGINS").ok().as_deref());

    let state = AppState {
        firms,
        archive,
        cache: Arc::new(RwLock::new(ResponseCache::new(cache_budget_bytes))),
        request_timeout,
    };
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "web/dist".to_string());
    let app = build_router(state, Some(archive_api), &static_dir, cors_layer);

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

fn build_warnings_from_counts(
    failure_count: usize,
    singular: &str,
    plural: &str,
    uncovered: &[(String, String)],
) -> (Option<String>, Vec<String>) {
    // Effort normalization infers each month's active fleet from retrieved detections, so upstream
    // fetch failures can drop a satellite and silently apply the wrong Ω factor.
    let reason = (failure_count > 0).then(|| {
        let noun = if failure_count == 1 { singular } else { plural };
        format!(
            "{failure_count} {noun} failed; missing source data may undercount monthly platform configurations and produce inaccurate effort_omega values."
        )
    });

    let mut warnings = Vec::new();
    if let Some(msg) = &reason {
        warnings.push(msg.clone());
    }
    if !uncovered.is_empty() {
        let ranges = uncovered
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

fn build_warnings(report: &ReportResponse) -> (Option<String>, Vec<String>) {
    build_warnings_from_counts(
        report.failures.len(),
        "FIRMS request",
        "FIRMS requests",
        &report.uncovered,
    )
}

fn build_effort_and_warnings_from_counts(
    harmonized: &[HarmonizedHotspot],
    failure_count: usize,
    singular: &str,
    plural: &str,
    uncovered: &[(String, String)],
) -> (EffortResponse, Vec<String>) {
    let (reason, warnings) =
        build_warnings_from_counts(failure_count, singular, plural, uncovered);
    let effort = EffortResponse {
        reliable: reason.is_none(),
        reason,
        factors: pyrosync_core::effort::estimate_effort(harmonized),
    };

    (effort, warnings)
}

fn build_effort_and_warnings(
    harmonized: &[HarmonizedHotspot],
    report: &ReportResponse,
) -> (EffortResponse, Vec<String>) {
    build_effort_and_warnings_from_counts(
        harmonized,
        report.failures.len(),
        "FIRMS request",
        "FIRMS requests",
        &report.uncovered,
    )
}

fn normalize_zero(v: f64) -> f64 {
    if v == 0.0 { 0.0 } else { v }
}

fn build_cache_key(route: &str, bbox: &BBox, window: &TimeWindow, include_raw: bool) -> String {
    let window_part = match window {
        TimeWindow::Days(days) => format!("days={}", days.get()),
        TimeWindow::Range { start, end } => format!("start={start}&end={end}"),
    };
    format!(
        "{route}|{},{},{},{}|{window_part}|raw={include_raw}",
        normalize_zero(bbox.west),
        normalize_zero(bbox.south),
        normalize_zero(bbox.east),
        normalize_zero(bbox.north),
    )
}

fn build_calendar_cache_key(
    country: &str,
    from: i32,
    to: i32,
    bbox: Option<&BBox>,
) -> String {
    let bbox_part = match bbox {
        Some(b) => format!(
            "{},{},{},{}",
            normalize_zero(b.west),
            normalize_zero(b.south),
            normalize_zero(b.east),
            normalize_zero(b.north),
        ),
        None => "none".to_string(),
    };
    format!("calendar|{country}|from={from}&to={to}|{bbox_part}")
}

fn cache_ttl_for_window(window: &TimeWindow) -> Duration {
    match window {
        TimeWindow::Days(_) => Duration::from_secs(CACHE_TTL_NRT_SECS),
        TimeWindow::Range { .. } => Duration::from_secs(CACHE_TTL_ARCHIVE_SECS),
    }
}

fn json_bytes_response(body: Vec<u8>, cache_status: &'static str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (X_CACHE_HEADER, cache_status),
        ],
        body,
    )
        .into_response()
}

async fn get_cached(cache: &RwLock<ResponseCache>, key: &str) -> Option<Vec<u8>> {
    let now = Instant::now();
    let lookup = {
        let guard = cache.read().await;
        guard.peek_at(key, now)
    };
    match lookup {
        CacheLookup::Hit(body) => Some(body),
        CacheLookup::Miss => None,
        CacheLookup::Expired => {
            let mut guard = cache.write().await;
            // Re-check under the write lock in case another task refreshed the entry.
            guard.get_at(key, now)
        }
    }
}

async fn put_cached(cache: &RwLock<ResponseCache>, key: String, body: &[u8], ttl: Duration) {
    // Check budget under read lock first so disabled or oversized payloads avoid cloning.
    {
        let guard = cache.read().await;
        if !guard.can_fit(body.len()) {
            return;
        }
    }
    let mut guard = cache.write().await;
    guard.insert_at(key, body.to_vec(), ttl, Instant::now());
}

async fn run_pipeline_with_timeout(
    state: &AppState,
    params: &HarmonizeParams,
) -> Result<PipelineRun, ApiError> {
    match state.request_timeout {
        Some(timeout) => match tokio::time::timeout(timeout, run_pipeline(state, params)).await {
            Ok(result) => result,
            Err(_) => Err(ApiError::gateway_timeout(format!(
                "Request timed out after {}s; long date ranges can require hundreds of FIRMS requests. Increase REQUEST_TIMEOUT_SECS or narrow the date range.",
                timeout.as_secs()
            ))),
        },
        None => run_pipeline(state, params).await,
    }
}

async fn execute_cached_pipeline<T, F>(
    state: &AppState,
    route: &str,
    params: &HarmonizeParams,
    build_payload: F,
) -> Result<Response, ApiError>
where
    T: Serialize,
    F: FnOnce(PipelineRun, bool) -> T,
{
    // Validate parameters before consulting the cache so invalid requests never probe cache entries.
    let bbox = parse_bbox(params)?;
    let window = parse_time_window(params)?;
    let include_raw = params.include_raw.unwrap_or(false);
    let key = build_cache_key(route, &bbox, &window, include_raw);

    if let Some(cached_body) = get_cached(&state.cache, &key).await {
        return Ok(json_bytes_response(cached_body, "HIT"));
    }

    let run = run_pipeline_with_timeout(state, params).await?;
    let payload = build_payload(run, include_raw);
    let body = serde_json::to_vec(&payload)
        .map_err(|e| ApiError::internal(format!("Failed to serialize response: {e}")))?;

    put_cached(&state.cache, key, &body, cache_ttl_for_window(&window)).await;
    Ok(json_bytes_response(body, "MISS"))
}

async fn harmonize(
    State(state): State<AppState>,
    Query(params): Query<HarmonizeParams>,
) -> Result<Response, ApiError> {
    execute_cached_pipeline(&state, "harmonize", &params, |run, include_raw| {
        let (effort, warnings) = build_effort_and_warnings(&run.harmonized, &run.report);
        let raw = RawResponse {
            modis_count: run.modis_count,
            viirs_count: run.viirs_count,
            total_raw: run.modis_count + run.viirs_count,
            hotspots: include_raw.then(|| run.raw.iter().map(RawHotspot::from).collect()),
        };

        serde_json::json!({
            "raw": raw,
            "harmonized": {
                "count": run.harmonized.len(),
                "hotspots": run.harmonized,
            },
            "calendar": run.calendar,
            "effort": effort,
            "warnings": warnings,
            "report": run.report,
        })
    })
    .await
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
) -> Result<Response, ApiError> {
    execute_cached_pipeline(&state, "compare", &params, |run, _| {
        build_compare_response(run)
    })
    .await
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
) -> Result<Response, ApiError> {
    execute_cached_pipeline(&state, "stats", &params, |run, _| {
        build_stats_response(run)
    })
    .await
}

fn archive_year_bounds() -> (i32, i32) {
    let mut min_year = i32::MAX;
    let mut max_year = i32::MIN;
    for sensor in ARCHIVE_AVAILABLE_SENSORS {
        if let Some(range) = archive_years(sensor) {
            min_year = min_year.min(*range.start());
            max_year = max_year.max(*range.end());
        }
    }
    (min_year, max_year)
}

fn archive_coverage() -> ArchiveCoverage {
    ArchiveCoverage {
        sensors_available: ARCHIVE_AVAILABLE_SENSORS.to_vec(),
        sensors_unavailable: ARCHIVE_UNAVAILABLE_SENSORS.to_vec(),
        note: ARCHIVE_COVERAGE_NOTE,
    }
}

fn parse_calendar_params(params: &CalendarParams) -> Result<ValidatedCalendarQuery, ApiError> {
    if params.country.is_empty() {
        return Err(ApiError::bad_request(
            "Parameter 'country' must not be empty",
        ));
    }
    // Reject path separators and shell metacharacters because `country` is interpolated into archive URLs and cache paths.
    if !params
        .country
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ApiError::bad_request(
            "Parameter 'country' may contain only ASCII letters, digits, '_' and '-'",
        ));
    }

    let (archive_min_year, archive_max_year) = archive_year_bounds();
    let requested_from = params.from.unwrap_or(archive_min_year);
    let requested_to = params.to.unwrap_or(archive_max_year);
    if requested_from > requested_to {
        return Err(ApiError::bad_request(format!(
            "'from' year ({requested_from}) must not be greater than 'to' year ({requested_to})"
        )));
    }

    let effective_from = requested_from.max(archive_min_year);
    let effective_to = requested_to.min(archive_max_year);
    let clamped = effective_from != requested_from || effective_to != requested_to;
    let bbox = parse_optional_bbox(params)?;

    Ok(ValidatedCalendarQuery {
        country: params.country.clone(),
        years: CalendarYears {
            requested_from,
            requested_to,
            effective_from,
            effective_to,
        },
        clamped,
        archive_min_year,
        archive_max_year,
        bbox,
    })
}

fn build_calendar_response(
    query: &ValidatedCalendarQuery,
    raw_detections: usize,
    harmonized: &[HarmonizedHotspot],
    calendar: Vec<CalendarCell>,
    per_sensor_year: Vec<SensorYearFetch>,
    failures: Vec<SensorYearFailure>,
) -> CalendarResponse {
    let (effort, mut warnings) = build_effort_and_warnings_from_counts(
        harmonized,
        failures.len(),
        "FIRMS archive download",
        "FIRMS archive downloads",
        &[],
    );

    if query.clamped {
        if query.years.effective_from > query.years.effective_to {
            warnings.push(format!(
                "Requested years {}..={} fall outside FIRMS country archive coverage ({}..={}); no archive data is available for this range.",
                query.years.requested_from,
                query.years.requested_to,
                query.archive_min_year,
                query.archive_max_year,
            ));
        } else {
            warnings.push(format!(
                "Requested years {}..={} were clamped to FIRMS country archive coverage ({}..={}).",
                query.years.requested_from,
                query.years.requested_to,
                query.years.effective_from,
                query.years.effective_to,
            ));
        }
    }

    let coverage = archive_coverage();
    warnings.push(coverage.note.to_string());

    CalendarResponse {
        country: query.country.clone(),
        years: CalendarYears {
            requested_from: query.years.requested_from,
            requested_to: query.years.requested_to,
            effective_from: query.years.effective_from,
            effective_to: query.years.effective_to,
        },
        bbox: query.bbox.clone(),
        coverage,
        counts: CalendarCounts {
            raw_detections,
            harmonized_records: harmonized.len(),
        },
        calendar,
        effort,
        fetch: ArchiveFetchSummary {
            per_sensor_year,
            failures,
        },
        warnings,
    }
}

async fn run_calendar_pipeline(
    archive: &Arc<ArchiveClient>,
    query: &ValidatedCalendarQuery,
) -> Result<CalendarResponse, ApiError> {
    let mut modis = Vec::new();
    let mut viirs = Vec::new();
    let mut per_sensor_year = Vec::new();
    let mut failures = Vec::new();

    if query.years.effective_from <= query.years.effective_to {
        let semaphore = Arc::new(Semaphore::new(ARCHIVE_FETCH_CONCURRENCY));
        let mut handles = Vec::new();

        for year in query.years.effective_from..=query.years.effective_to {
            for sensor in ARCHIVE_FETCH_SENSORS {
                if !archive.covers(sensor, year) {
                    continue;
                }
                let permit = Arc::clone(&semaphore)
                    .acquire_owned()
                    .await
                    .map_err(|e| ApiError::internal(format!("Archive worker semaphore closed: {e}")))?;
                let archive = Arc::clone(archive);
                let country = query.country.clone();
                let bbox = query.bbox.clone();

                let handle = tokio::spawn(async move {
                    let _permit = permit;
                    let mut hotspots = archive
                        .fetch_country_year(sensor, &country, year)
                        .await
                        .map_err(|err| err.to_string())?;
                    // Filter per country-year before merging decades of detections to bound peak memory.
                    pyrosync_firms::parse::retain_wildfires(&mut hotspots);
                    if let Some(ref bbox) = bbox {
                        hotspots = filter_bbox(hotspots, bbox);
                    }
                    Ok::<Vec<Hotspot>, String>(hotspots)
                });
                handles.push((sensor, year, handle));
            }
        }

        for (sensor, year, handle) in handles {
            match handle.await {
                Ok(Ok(mut hotspots)) => {
                    per_sensor_year.push(SensorYearFetch {
                        sensor,
                        year,
                        detections: hotspots.len(),
                    });
                    if sensor.is_modis() {
                        modis.append(&mut hotspots);
                    } else {
                        viirs.append(&mut hotspots);
                    }
                }
                Ok(Err(error)) => {
                    failures.push(SensorYearFailure {
                        sensor,
                        year,
                        error,
                    });
                }
                Err(join_err) => {
                    failures.push(SensorYearFailure {
                        sensor,
                        year,
                        error: join_err.to_string(),
                    });
                }
            }
        }
    }

    let raw_detections = modis.len() + viirs.len();
    let harmonized = pyrosync_core::harmonize::harmonize(modis, viirs);
    let calendar_cells = pyrosync_core::calendar::build_calendar(&harmonized);

    Ok(build_calendar_response(
        query,
        raw_detections,
        &harmonized,
        calendar_cells,
        per_sensor_year,
        failures,
    ))
}

async fn run_calendar_with_timeout(
    state: &AppState,
    query: &ValidatedCalendarQuery,
) -> Result<CalendarResponse, ApiError> {
    let archive = require_archive_client(state)?;
    match state.request_timeout {
        Some(timeout) => match tokio::time::timeout(timeout, run_calendar_pipeline(archive, query)).await {
            Ok(result) => result,
            Err(_) => Err(ApiError::gateway_timeout(format!(
                "Request timed out after {}s; cold-cache FIRMS country archive downloads can take several minutes for large countries or multi-year ranges. Increase REQUEST_TIMEOUT_SECS or narrow the year range.",
                timeout.as_secs()
            ))),
        },
        None => run_calendar_pipeline(archive, query).await,
    }
}

async fn calendar(
    State(state): State<AppState>,
    Query(params): Query<CalendarParams>,
) -> Result<Response, ApiError> {
    let query = parse_calendar_params(&params)?;
    let key = build_calendar_cache_key(
        &query.country,
        query.years.requested_from,
        query.years.requested_to,
        query.bbox.as_ref(),
    );

    if let Some(cached_body) = get_cached(&state.cache, &key).await {
        return Ok(json_bytes_response(cached_body, "HIT"));
    }

    // Archive downloads do not use a FIRMS API key, so this endpoint works without `require_firms_client`.
    let response = run_calendar_with_timeout(&state, &query).await?;
    let body = serde_json::to_vec(&response)
        .map_err(|e| ApiError::internal(format!("Failed to serialize response: {e}")))?;

    put_cached(
        &state.cache,
        key,
        &body,
        Duration::from_secs(CACHE_TTL_ARCHIVE_SECS),
    )
    .await;
    Ok(json_bytes_response(body, "MISS"))
}

fn require_firms_client(state: &AppState) -> Result<&FirmsClient, ApiError> {
    state
        .firms
        .as_deref()
        .ok_or_else(|| ApiError::internal("FIRMS_API_KEY is not configured"))
}

fn require_archive_client(state: &AppState) -> Result<&Arc<ArchiveClient>, ApiError> {
    state
        .archive
        .as_ref()
        .ok_or_else(|| ApiError::internal("FIRMS archive cache directory is unavailable"))
}

fn validate_bbox(west: f64, south: f64, east: f64, north: f64) -> Result<BBox, ApiError> {
    if !(-180.0..=180.0).contains(&west) || !(-180.0..=180.0).contains(&east) {
        return Err(ApiError::bad_request(
            "Longitude coordinates ('west', 'east') must be within [-180, 180]",
        ));
    }
    if !(-90.0..=90.0).contains(&south) || !(-90.0..=90.0).contains(&north) {
        return Err(ApiError::bad_request(
            "Latitude coordinates ('south', 'north') must be within [-90, 90]",
        ));
    }
    if west > east {
        return Err(ApiError::bad_request(
            "'west' must be less than or equal to 'east'",
        ));
    }
    if south > north {
        return Err(ApiError::bad_request(
            "'south' must be less than or equal to 'north'",
        ));
    }

    Ok(BBox {
        west,
        south,
        east,
        north,
    })
}

fn parse_bbox(params: &HarmonizeParams) -> Result<BBox, ApiError> {
    validate_bbox(params.west, params.south, params.east, params.north)
}

fn parse_optional_bbox(params: &CalendarParams) -> Result<Option<BBox>, ApiError> {
    match (params.west, params.south, params.east, params.north) {
        (None, None, None, None) => Ok(None),
        (Some(west), Some(south), Some(east), Some(north)) => {
            validate_bbox(west, south, east, north).map(Some)
        }
        _ => Err(ApiError::bad_request(
            "Invalid bounding box: all four of west, south, east, north are required together",
        )),
    }
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

    fn test_state(budget_bytes: usize, request_timeout: Option<Duration>) -> AppState {
        AppState {
            firms: None,
            archive: None,
            cache: Arc::new(RwLock::new(ResponseCache::new(budget_bytes))),
            request_timeout,
        }
    }

    async fn send_http_request(addr: std::net::SocketAddr, request: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect test server");
        stream
            .write_all(request.as_bytes())
            .await
            .expect("write test request");
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .await
            .expect("read test response");
        String::from_utf8(response).expect("utf8 response")
    }

    #[tokio::test]
    async fn handlers_enforce_validation_before_firms_client() {
        let state = test_state(1024 * 1024, Some(Duration::from_secs(300)));
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
            .expect_err("invalid bbox should fail");
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
            .expect_err("missing FIRMS key should return 500");
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn cache_key_distinguishes_include_raw() {
        let bbox = sample_bbox();
        let window = TimeWindow::Days(DayRange::new(2).unwrap());

        let key_false = build_cache_key("harmonize", &bbox, &window, false);
        let key_true = build_cache_key("harmonize", &bbox, &window, true);

        assert_ne!(key_false, key_true);
    }

    #[test]
    fn cache_key_distinguishes_days_and_date_range() {
        let bbox = sample_bbox();
        let days_window = TimeWindow::Days(DayRange::new(3).unwrap());
        let range_window = TimeWindow::Range {
            start: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 7, 3).unwrap(),
        };

        let key_days = build_cache_key("harmonize", &bbox, &days_window, false);
        let key_range = build_cache_key("harmonize", &bbox, &range_window, false);

        assert_ne!(key_days, key_range);
        assert!(key_days.contains("days=3"));
        assert!(key_range.contains("start=2026-07-01&end=2026-07-03"));
    }

    #[test]
    fn cache_evicts_oldest_entries_when_budget_exceeded() {
        let mut cache = ResponseCache::new(10);
        let t0 = Instant::now();
        let ttl = Duration::from_secs(600);

        cache.insert_at("k1".to_string(), vec![1; 4], ttl, t0);
        cache.insert_at(
            "k2".to_string(),
            vec![2; 4],
            ttl,
            t0 + Duration::from_secs(1),
        );
        assert_eq!(cache.total_bytes, 8);

        // Adding 5 bytes requires dropping k1 (4 bytes) so total stays 9 <= 10.
        cache.insert_at(
            "k3".to_string(),
            vec![3; 5],
            ttl,
            t0 + Duration::from_secs(2),
        );

        assert!(cache.total_bytes <= 10);
        assert_eq!(cache.total_bytes, 9);
        assert!(
            cache
                .get_at("k1", t0 + Duration::from_secs(3))
                .is_none()
        );
        assert_eq!(
            cache.get_at("k2", t0 + Duration::from_secs(3)),
            Some(vec![2; 4])
        );
        assert_eq!(
            cache.get_at("k3", t0 + Duration::from_secs(3)),
            Some(vec![3; 5])
        );
    }

    #[test]
    fn cache_skips_entry_larger_than_budget() {
        let mut cache = ResponseCache::new(8);
        let t0 = Instant::now();
        let ttl = Duration::from_secs(600);

        cache.insert_at("small".to_string(), vec![1; 5], ttl, t0);
        cache.insert_at("huge".to_string(), vec![9; 9], ttl, t0 + Duration::from_secs(1));

        assert_eq!(cache.total_bytes, 5);
        assert_eq!(cache.get_at("small", t0 + Duration::from_secs(2)), Some(vec![1; 5]));
        assert!(cache.get_at("huge", t0 + Duration::from_secs(2)).is_none());
    }

    #[test]
    fn cache_expires_entries_on_read_and_evicts_them() {
        let mut cache = ResponseCache::new(1024);
        let t0 = Instant::now();
        let ttl = Duration::from_secs(600);

        cache.insert_at("k1".to_string(), vec![7; 16], ttl, t0);
        assert_eq!(cache.total_bytes, 16);
        assert_eq!(
            cache.get_at("k1", t0 + Duration::from_secs(599)),
            Some(vec![7; 16])
        );

        // Once TTL elapses, reading invalidates and removes the entry, reclaiming its bytes.
        assert!(cache.get_at("k1", t0 + Duration::from_secs(600)).is_none());
        assert_eq!(cache.total_bytes, 0);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn cache_disabled_when_budget_is_zero() {
        let mut cache = ResponseCache::new(0);
        let t0 = Instant::now();
        let ttl = Duration::from_secs(600);

        cache.insert_at("k1".to_string(), vec![1; 4], ttl, t0);
        assert_eq!(cache.total_bytes, 0);
        assert!(cache.entries.is_empty());
        assert!(cache.get_at("k1", t0).is_none());
    }

    #[tokio::test]
    async fn handler_serves_cached_hit_with_x_cache_and_does_not_cache_errors() {
        let state = test_state(1024 * 1024, Some(Duration::from_secs(300)));
        let params = HarmonizeParams {
            west: 26.0,
            south: 36.0,
            east: 45.0,
            north: 42.0,
            days: Some(2),
            start: None,
            end: None,
            include_raw: Some(false),
        };

        // First call fails because FIRMS_API_KEY is missing; errors must never be cached.
        let err = harmonize(State(state.clone()), Query(params))
            .await
            .expect_err("missing key returns 500");
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(state.cache.read().await.entries.len(), 0);

        let bbox = BBox {
            west: 26.0,
            south: 36.0,
            east: 45.0,
            north: 42.0,
        };
        let window = TimeWindow::Days(DayRange::new(2).unwrap());
        let key = build_cache_key("harmonize", &bbox, &window, false);
        put_cached(
            &state.cache,
            key,
            br#"{"cached_payload":true}"#,
            Duration::from_secs(600),
        )
        .await;

        let params = HarmonizeParams {
            west: 26.0,
            south: 36.0,
            east: 45.0,
            north: 42.0,
            days: Some(2),
            start: None,
            end: None,
            include_raw: Some(false),
        };
        let resp = harmonize(State(state), Query(params))
            .await
            .expect("cache hit should succeed without FIRMS client");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
        assert_eq!(
            resp.headers()
                .get(X_CACHE_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some("HIT")
        );
    }

    #[tokio::test]
    async fn router_returns_json_404_for_unknown_api_and_serves_cors_headers() {
        let state = test_state(1024, None);
        let archive_api = Arc::new(archive_api::ArchiveState::new(
            PathBuf::from("nonexistent-archive-dir-for-test"),
            archive_api::ArchiveLimits::from_env(),
        ));
        let cors = build_cors_layer(Some("https://allowed.example, invalid origin, http://localhost:5173"));
        let app = build_router(state, Some(archive_api), "nonexistent-static-dir-for-test", cors);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let api_regions = send_http_request(
            addr,
            "GET /api/regions HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(api_regions.starts_with("HTTP/1.1 503 Service Unavailable\r\n"));
        assert!(api_regions.to_ascii_lowercase().contains("content-type: application/json"));

        let api_404 = send_http_request(
            addr,
            "GET /api/foo HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(api_404.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(api_404.to_ascii_lowercase().contains("content-type: application/json"));
        assert!(api_404.contains(r#"{"error":"Unknown API endpoint: /api/foo"}"#));

        let non_api = send_http_request(
            addr,
            "GET /unknown-spa-route HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(non_api.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(!non_api.contains("Unknown API endpoint"));

        let cors_allowed = send_http_request(
            addr,
            "GET /api/health HTTP/1.1\r\nHost: localhost\r\nOrigin: https://allowed.example\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(cors_allowed.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(
            cors_allowed
                .to_ascii_lowercase()
                .contains("access-control-allow-origin: https://allowed.example")
        );

        let cors_denied = send_http_request(
            addr,
            "GET /api/health HTTP/1.1\r\nHost: localhost\r\nOrigin: https://evil.example\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(
            !cors_denied
                .to_ascii_lowercase()
                .contains("access-control-allow-origin:")
        );

        server.abort();
    }

    #[test]
    fn env_parsers_and_empty_cors_do_not_panic() {
        assert_eq!(parse_cache_budget_bytes(None), 128 * 1024 * 1024);
        assert_eq!(parse_cache_budget_bytes(Some("0")), 0);
        assert_eq!(parse_cache_budget_bytes(Some("64")), 64 * 1024 * 1024);
        assert_eq!(parse_cache_budget_bytes(Some("abc")), 128 * 1024 * 1024);

        assert_eq!(parse_request_timeout(None), Some(Duration::from_secs(300)));
        assert_eq!(parse_request_timeout(Some("0")), None);
        assert_eq!(parse_request_timeout(Some("45")), Some(Duration::from_secs(45)));
        assert_eq!(
            parse_request_timeout(Some("-5")),
            Some(Duration::from_secs(300))
        );

        assert_eq!(
            cache_ttl_for_window(&TimeWindow::Days(DayRange::new(1).unwrap())),
            Duration::from_secs(CACHE_TTL_NRT_SECS)
        );
        assert_eq!(
            cache_ttl_for_window(&TimeWindow::Range {
                start: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                end: NaiveDate::from_ymd_opt(2025, 1, 5).unwrap(),
            }),
            Duration::from_secs(CACHE_TTL_ARCHIVE_SECS)
        );

        // Empty or entirely invalid origin lists must construct safely without panicking.
        let _ = build_cors_layer(None);
        let _ = build_cors_layer(Some(""));
        let _ = build_cors_layer(Some(" , invalid\norigin , not-a-url "));
    }

    fn calendar_params(
        country: &str,
        from: Option<i32>,
        to: Option<i32>,
        bbox: Option<[Option<f64>; 4]>,
    ) -> CalendarParams {
        let [west, south, east, north] = bbox.unwrap_or([None, None, None, None]);
        CalendarParams {
            country: country.to_string(),
            from,
            to,
            west,
            south,
            east,
            north,
        }
    }

    #[test]
    fn calendar_rejects_invalid_country_characters() {
        for bad in ["", "../etc", "Turkey Greece", "Turkey;ls", "Turkey/2020", "Türkiye"] {
            let err = parse_calendar_params(&calendar_params(bad, None, None, None))
                .err()
                .expect("invalid country must return bad_request");
            assert_eq!(err.status, StatusCode::BAD_REQUEST, "country={bad:?}");
        }

        for ok in ["Turkey", "Russian_Federation", "Bosnia-Herzegovina", "Country1"] {
            let parsed = parse_calendar_params(&calendar_params(ok, None, None, None))
                .expect("valid country should pass");
            assert_eq!(parsed.country, ok);
        }
    }

    #[test]
    fn calendar_rejects_from_greater_than_to() {
        let err = parse_calendar_params(&calendar_params("Turkey", Some(2020), Some(2015), None))
            .err()
            .expect("from > to must return bad_request");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);

        let err = parse_calendar_params(&calendar_params("Turkey", Some(2026), None, None))
            .err()
            .expect("from > default to must return bad_request");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn calendar_requires_all_four_bbox_params_together_or_none() {
        let none_bbox = parse_calendar_params(&calendar_params("Turkey", None, None, None))
            .expect("omitting all bbox params should succeed");
        assert!(none_bbox.bbox.is_none());

        for partial in [
            [Some(26.0), None, None, None],
            [Some(26.0), Some(36.0), Some(45.0), None],
            [None, Some(36.0), Some(45.0), Some(42.0)],
        ] {
            let err = parse_calendar_params(&calendar_params("Turkey", None, None, Some(partial)))
                .err()
                .expect("partial bbox must return bad_request");
            assert_eq!(err.status, StatusCode::BAD_REQUEST);
            assert!(err.message.contains("all four of west, south, east, north are required together"));
        }

        let inverted = parse_calendar_params(&calendar_params(
            "Turkey",
            None,
            None,
            Some([Some(45.0), Some(36.0), Some(26.0), Some(42.0)]),
        ))
        .err()
        .expect("west > east must return bad_request");
        assert_eq!(inverted.status, StatusCode::BAD_REQUEST);

        let valid = parse_calendar_params(&calendar_params(
            "Turkey",
            None,
            None,
            Some([Some(26.0), Some(36.0), Some(45.0), Some(42.0)]),
        ))
        .expect("complete valid bbox should pass");
        assert!(valid.bbox.is_some());
    }

    #[test]
    fn calendar_clamps_out_of_range_years_to_archive_coverage() {
        let query = parse_calendar_params(&calendar_params("Turkey", Some(1995), Some(2030), None))
            .expect("out-of-range years should clamp, not fail");
        assert_eq!(query.years.requested_from, 1995);
        assert_eq!(query.years.requested_to, 2030);
        assert_eq!(query.years.effective_from, 2000);
        assert_eq!(query.years.effective_to, 2025);
        assert!(query.clamped);

        let resp = build_calendar_response(&query, 0, &[], Vec::new(), Vec::new(), Vec::new());
        assert!(resp.warnings.iter().any(|w| w.contains("clamped")));

        let upper_only = parse_calendar_params(&calendar_params("Turkey", Some(2015), Some(2030), None))
            .expect("upper out-of-range year should clamp");
        assert_eq!(upper_only.years.effective_from, 2015);
        assert_eq!(upper_only.years.effective_to, 2025);
        assert!(upper_only.clamped);

        let non_overlapping =
            parse_calendar_params(&calendar_params("Turkey", Some(1990), Some(1995), None))
                .expect("completely pre-archive range should not fail");
        assert!(non_overlapping.clamped);
        assert!(non_overlapping.years.effective_from > non_overlapping.years.effective_to);
        let empty_resp = build_calendar_response(
            &non_overlapping,
            0,
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        assert!(empty_resp.calendar.is_empty());
        assert!(empty_resp.warnings.iter().any(|w| w.contains("outside")));
    }

    #[test]
    fn calendar_coverage_includes_noaa20_marks_noaa21_unavailable_and_warns_on_failures() {
        let query = parse_calendar_params(&calendar_params("Turkey", Some(2015), Some(2020), None))
            .expect("valid query");

        let ok_resp = build_calendar_response(&query, 0, &[], Vec::new(), Vec::new(), Vec::new());
        assert_eq!(
            ok_resp.coverage.sensors_available,
            vec![
                Sensor::ModisTerra,
                Sensor::ModisAqua,
                Sensor::ViirsSNPP,
                Sensor::ViirsNOAA20,
            ]
        );
        assert_eq!(ok_resp.coverage.sensors_unavailable, vec![Sensor::ViirsNOAA21]);
        assert!(ok_resp.warnings.iter().any(|w| w == ok_resp.coverage.note));
        assert!(ok_resp.effort.reliable);

        let failed_resp = build_calendar_response(
            &query,
            0,
            &[],
            Vec::new(),
            Vec::new(),
            vec![SensorYearFailure {
                sensor: Sensor::ModisTerra,
                year: 2017,
                error: "http 503".to_string(),
            }],
        );
        assert!(!failed_resp.effort.reliable);
        assert!(failed_resp.effort.reason.is_some());
        assert_eq!(failed_resp.fetch.failures.len(), 1);
        assert!(failed_resp.warnings.len() >= 2);
    }

    #[test]
    fn calendar_cache_key_distinguishes_country_years_and_bbox() {
        let bbox_a = sample_bbox();
        let bbox_b = BBox {
            west: 27.0,
            south: 36.0,
            east: 45.0,
            north: 42.0,
        };

        let k_tr_full = build_calendar_cache_key("Turkey", 2000, 2025, None);
        let k_gr_full = build_calendar_cache_key("Greece", 2000, 2025, None);
        let k_tr_sub = build_calendar_cache_key("Turkey", 2015, 2020, None);
        let k_tr_bbox_a = build_calendar_cache_key("Turkey", 2000, 2025, Some(&bbox_a));
        let k_tr_bbox_b = build_calendar_cache_key("Turkey", 2000, 2025, Some(&bbox_b));

        let keys = [
            &k_tr_full,
            &k_gr_full,
            &k_tr_sub,
            &k_tr_bbox_a,
            &k_tr_bbox_b,
        ];
        let unique: BTreeSet<&String> = keys.into_iter().collect();
        assert_eq!(unique.len(), 5);
    }

    #[tokio::test]
    async fn calendar_handler_serves_cached_hit_without_firms_key() {
        let state = test_state(1024 * 1024, Some(Duration::from_secs(300)));
        assert!(state.firms.is_none());

        let key = build_calendar_cache_key("Turkey", 2000, 2025, None);
        put_cached(
            &state.cache,
            key,
            br#"{"country":"Turkey","calendar":[]}"#,
            Duration::from_secs(CACHE_TTL_ARCHIVE_SECS),
        )
        .await;

        let resp = calendar(
            State(state),
            Query(calendar_params("Turkey", None, None, None)),
        )
        .await
        .expect("cached calendar hit should succeed without FIRMS_API_KEY");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(X_CACHE_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some("HIT")
        );
    }
}
