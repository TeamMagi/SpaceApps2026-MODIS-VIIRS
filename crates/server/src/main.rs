mod archive_api;
mod derived_cache;
#[cfg(test)]
mod integration_tests;
mod pipeline_config;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    Router,
    extract::{FromRequestParts, OriginalUri, Query, State, rejection::QueryRejection},
    http::{HeaderName, HeaderValue, Method, StatusCode, Uri, header, request::Parts},
    response::{IntoResponse, Json, Response},
    routing::get,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, Semaphore};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::EnvFilter;

use pyrosync_core::config::PipelineConfig;
use pyrosync_core::effort::{EffortFactor, TropicalSwathGap};
use pyrosync_core::models::{
    BBox, CalendarCell, Confidence, DayNight, HarmonizedHotspot, Hotspot, HotspotSource, Sensor,
};
use pyrosync_firms::archive::{ArchiveClient, ArchiveError, archive_years, filter_bbox};
use pyrosync_firms::{AreaCache, DayRange, FirmsClient, FirmsSource};

const DEFAULT_CACHE_BUDGET_MB: usize = 128;
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 300;
const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const DEFAULT_AREA_CACHE_DIR: &str = ".cache/firms-area";
const ARCHIVE_FETCH_CONCURRENCY: usize = 4;
const MAX_COUNTRY_LEN: usize = 64;
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
const UNCERTAINTY_NOTE: &str = "Harmonized values are model estimates, not ground truth. For MODIS detections, VZA scales raw FRP to MODIS footprint equivalents; VIIRS-era calendar total_frp adds empirically measured pre-VIIRS completeness (k_c) when no VIIRS detections are present in a cell. mean_frp remains native-pixel and is discontinuous at sensor transitions. Default sensor-noise, VZA error, and MODIS scale uncertainty parameters are assumptions, not published values.";

#[derive(Clone)]
struct AppState {
    firms: Option<Arc<FirmsClient>>,
    archive: Option<Arc<ArchiveClient>>,
    cache: Arc<RwLock<ResponseCache>>,
    request_timeout: Option<Duration>,
    config: Arc<PipelineConfig>,
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
    format: Option<String>,
    min_confidence: Option<String>,
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
    min_confidence: Option<String>,
}

#[derive(Clone, Serialize)]
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

/// Lowest FIRMS confidence class a request keeps. `Low` keeps everything (the default).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MinConfidence {
    #[default]
    Low,
    Nominal,
    High,
}

impl MinConfidence {
    pub(crate) fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            None | Some("low") => Ok(Self::Low),
            Some("nominal") => Ok(Self::Nominal),
            Some("high") => Ok(Self::High),
            Some(other) => Err(format!(
                "Invalid 'min_confidence' value '{other}': expected 'low', 'nominal' or 'high'"
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Nominal => "nominal",
            Self::High => "high",
        }
    }

    fn floor(self) -> Confidence {
        match self {
            Self::Low => Confidence::Low,
            Self::Nominal => Confidence::Nominal,
            Self::High => Confidence::High,
        }
    }

    /// Drops detections below the floor and returns how many were dropped.
    pub(crate) fn retain(self, rows: &mut Vec<Hotspot>) -> usize {
        let before = rows.len();
        let floor = self.floor();
        rows.retain(|h| h.confidence >= floor);
        before - rows.len()
    }
}

/// Echo of the confidence filter applied before harmonization, so a filtered response labels itself.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(crate) struct ConfidenceFilter {
    pub(crate) min_confidence: MinConfidence,
    pub(crate) excluded_detections: usize,
}

// FIRMS ships neither the 4 µm optical depth (τ_4) nor the cloud fraction (β), so FRP can only be
// biased low by smoke and cloud: thin smoke attenuates by exp(-τ_4) [16, Eq. 12], cloud hides
// ~22% (MODIS) / ~12% (VIIRS) of fire energy globally [16, Sec. 4.1] and ~7% regionally [23, Eq. 2].
const ATMOSPHERIC_BIAS_LOW_PCT: u8 = 7;
const ATMOSPHERIC_BIAS_HIGH_PCT: u8 = 22;
const ATMOSPHERIC_BIAS_NOTE: &str = "One-sided downward bias, not applied to the numbers: FIRMS carries no smoke optical depth (tau_4) or cloud fraction (beta), so FRP may be underestimated by roughly 7-22%. Thin smoke attenuates FRP by exp(-tau_4); cloud cover hides ~22% (MODIS) and ~12% (VIIRS) of fire energy globally and ~7% in regional inventories. The frp_lower/frp_upper bands do not include this term.";
const ATMOSPHERIC_BIAS_REFS: [&str; 3] = [
    "[16] Observation operator for MODIS and VIIRS FRP (GMD, 2026), Sec. 4.1: cloud loss MODIS 22% / VIIRS 12%",
    "[16] Observation operator for MODIS and VIIRS FRP (GMD, 2026), Eq. 12: thin-smoke attenuation exp(-tau_4)",
    "[23] 3D biomass burning emission inventory for SE/E Asia (2025), Eq. 2: regional cloud-fraction loss ~7%",
];

#[derive(Serialize)]
struct AtmosphericBias {
    direction: &'static str,
    low_pct: u8,
    high_pct: u8,
    applied: bool,
    note: &'static str,
    refs: [&'static str; 3],
}

fn atmospheric_bias() -> AtmosphericBias {
    AtmosphericBias {
        direction: "downward",
        low_pct: ATMOSPHERIC_BIAS_LOW_PCT,
        high_pct: ATMOSPHERIC_BIAS_HIGH_PCT,
        applied: false,
        note: ATMOSPHERIC_BIAS_NOTE,
        refs: ATMOSPHERIC_BIAS_REFS,
    }
}

#[derive(Serialize)]
struct UncertaintyMetadata {
    confidence: f64,
    note: String,
    atmospheric_bias: AtmosphericBias,
}

fn uncertainty_metadata(params: &pyrosync_core::uncertainty::UncertaintyParams) -> UncertaintyMetadata {
    UncertaintyMetadata {
        confidence: pyrosync_core::uncertainty::confidence_from_z(params.z),
        note: UNCERTAINTY_NOTE.to_string(),
        atmospheric_bias: atmospheric_bias(),
    }
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
    uncertainty: UncertaintyMetadata,
    filters: ConfidenceFilter,
    fetch: ArchiveFetchSummary,
    warnings: Vec<String>,
}

#[derive(Clone)]
struct ValidatedCalendarQuery {
    country: String,
    years: CalendarYears,
    clamped: bool,
    archive_min_year: i32,
    archive_max_year: i32,
    bbox: Option<BBox>,
    min_confidence: MinConfidence,
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
    /// `null` outside 30°S-30°N; see `pyrosync_core::effort::TropicalSwathGap`.
    tropical_swath_gap: Option<TropicalSwathGap>,
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
    mean_frp_adjustment_pct: Option<f64>,
}

#[derive(Serialize)]
struct CompareResponse {
    counts: CompareCounts,
    frp: CompareFrp,
    merging: CompareMerging,
    vza: CompareVza,
    filters: ConfidenceFilter,
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
    filters: ConfidenceFilter,
    warnings: Vec<String>,
    report: ReportResponse,
}

struct PipelineRun {
    bbox: BBox,
    modis_count: usize,
    viirs_count: usize,
    // Keep filtered raw detections even when include_raw=false so /api/compare and /api/stats can reuse this helper.
    raw: Vec<Hotspot>,
    harmonized: Vec<HarmonizedHotspot>,
    calendar: Vec<CalendarCell>,
    report: ReportResponse,
    filters: ConfidenceFilter,
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

    fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
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

struct ApiQuery<T>(T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    Query<T>: FromRequestParts<S, Rejection = QueryRejection>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(value)| Self(value))
            .map_err(|err| ApiError::bad_request(err.body_text()))
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

fn parse_area_cache_dir(raw: Option<&str>) -> Option<PathBuf> {
    match raw.map(str::trim) {
        None => Some(PathBuf::from(DEFAULT_AREA_CACHE_DIR)),
        Some("") => None,
        Some(dir) => Some(PathBuf::from(dir)),
    }
}

fn parse_area_cache_nrt_ttl(raw: Option<&str>) -> Duration {
    let default = Duration::from_secs(CACHE_TTL_NRT_SECS);
    let Some(raw) = raw else {
        return default;
    };
    let trimmed = raw.trim();
    match trimmed.parse::<u64>() {
        Ok(secs) => Duration::from_secs(secs),
        Err(_) => {
            tracing::warn!(
                "Invalid AREA_CACHE_NRT_TTL_SECS value '{trimmed}'; falling back to {CACHE_TTL_NRT_SECS}s"
            );
            default
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
        .route("/config", get(config))
        .route("/harmonize", get(harmonize))
        .route("/availability", get(availability))
        .route("/compare", get(compare))
        .route("/stats", get(stats))
        .route("/calendar", get(calendar))
        .route("/export", get(export))
        .with_state(state);

    if let Some(archive_api) = archive_api {
        api_router = api_router.merge(archive_api::api_routes(archive_api));
    }

    let api_router = api_router.fallback(api_not_found);

    let index_path = std::path::Path::new(static_dir).join("index.html");
    if index_path.is_file() {
        Router::new()
            .nest("/api", api_router)
            .fallback_service(ServeDir::new(static_dir).fallback(ServeFile::new(index_path)))
            .layer(cors)
    } else {
        tracing::warn!(
            "SPA fallback disabled: index.html not found at {}",
            index_path.display()
        );
        Router::new()
            .nest("/api", api_router)
            .fallback_service(ServeDir::new(static_dir))
            .layer(cors)
    }
}

const CLI_USAGE: &str = "\
Usage:
  pyrosync-server
  pyrosync-server precompute [--region <id>]... [--concurrency <n>] [--force]";

#[derive(Debug, PartialEq, Eq)]
enum CliCommand {
    Serve,
    Help,
    Precompute {
        regions: Vec<String>,
        concurrency: usize,
        force: bool,
    },
}

fn take_flag_value(
    flag: &str,
    inline: Option<&str>,
    iter: &mut impl Iterator<Item = String>,
) -> Result<String, String> {
    let val = match inline {
        Some(v) => v.to_string(),
        None => iter
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?,
    };
    if val.trim().is_empty() {
        return Err(format!("empty value for {flag}"));
    }
    Ok(val)
}

fn parse_cli(args: impl IntoIterator<Item = String>) -> Result<CliCommand, String> {
    let mut iter = args.into_iter();
    let Some(first) = iter.next() else {
        return Ok(CliCommand::Serve);
    };

    match first.as_str() {
        "-h" | "--help" => {
            if let Some(extra) = iter.next() {
                return Err(format!("unexpected argument '{extra}'"));
            }
            Ok(CliCommand::Help)
        }
        "precompute" => {
            let mut regions = Vec::new();
            let mut concurrency = archive_api::DEFAULT_PRECOMPUTE_CONCURRENCY;
            let mut force = false;

            while let Some(arg) = iter.next() {
                if arg == "-h" || arg == "--help" {
                    return Ok(CliCommand::Help);
                } else if arg == "--force" {
                    force = true;
                } else if arg == "--region" {
                    regions.push(take_flag_value("--region", None, &mut iter)?);
                } else if let Some(val) = arg.strip_prefix("--region=") {
                    regions.push(take_flag_value("--region", Some(val), &mut iter)?);
                } else if arg == "--concurrency" {
                    let raw = take_flag_value("--concurrency", None, &mut iter)?;
                    concurrency = raw
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .filter(|&n| n >= 1)
                        .ok_or_else(|| format!("invalid --concurrency '{raw}': expected integer >= 1"))?;
                } else if let Some(val) = arg.strip_prefix("--concurrency=") {
                    let raw = take_flag_value("--concurrency", Some(val), &mut iter)?;
                    concurrency = raw
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .filter(|&n| n >= 1)
                        .ok_or_else(|| format!("invalid --concurrency '{raw}': expected integer >= 1"))?;
                } else {
                    return Err(format!("unknown argument '{arg}'"));
                }
            }

            Ok(CliCommand::Precompute {
                regions,
                concurrency,
                force,
            })
        }
        other => Err(format!("unknown subcommand or argument '{other}'")),
    }
}

fn archive_cache_dir_from_env() -> PathBuf {
    std::env::var("ARCHIVE_CACHE_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("FIRMS_ARCHIVE_DIR")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR))
}

fn load_pipeline_config() -> anyhow::Result<Arc<PipelineConfig>> {
    // A bad config aborts startup: silently falling back to defaults would change scientific
    // results without anyone noticing.
    let config_path = std::env::var("PYROSYNC_CONFIG")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from);
    let pipeline_config = pipeline_config::load(config_path.as_deref(), |name| {
        std::env::var(name).ok()
    })
    .inspect_err(|err| tracing::error!("Invalid pipeline configuration: {err:#}"))?;
    tracing::info!(config = ?pipeline_config, "Effective pipeline configuration");
    Ok(Arc::new(pipeline_config))
}

async fn run_precompute(
    regions: Vec<String>,
    concurrency: usize,
    force: bool,
) -> std::process::ExitCode {
    let Ok(pipeline_config) = load_pipeline_config() else {
        return std::process::ExitCode::FAILURE;
    };
    let archive_cache_dir = archive_cache_dir_from_env();
    let archive_state = Arc::new(
        archive_api::ArchiveState::new(
            archive_cache_dir,
            archive_api::ArchiveLimits::from_env(),
            pipeline_config,
        )
        .with_year_concurrency(concurrency),
    );

    match archive_api::precompute_series(
        archive_state,
        &archive_api::PrecomputeOptions { regions, force },
    )
    .await
    {
        Ok(report) if report.failed() == 0 => std::process::ExitCode::SUCCESS,
        Ok(_) => std::process::ExitCode::FAILURE,
        Err(err) => {
            tracing::error!(target: "pyrosync::precompute", error = %err, "precompute aborted");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run_server() -> anyhow::Result<()> {
    let pipeline_config = load_pipeline_config()?;

    // Keep the server bootable without an API key so health checks and static assets stay live.
    let firms = match std::env::var("FIRMS_API_KEY") {
        Ok(key) if !key.trim().is_empty() => match FirmsClient::new(key) {
            Ok(mut client) => {
                match parse_area_cache_dir(std::env::var("AREA_CACHE_DIR").ok().as_deref()) {
                    Some(dir) => {
                        let ttl = parse_area_cache_nrt_ttl(
                            std::env::var("AREA_CACHE_NRT_TTL_SECS").ok().as_deref(),
                        );
                        tracing::info!(
                            "FIRMS area cache at {} (NRT TTL {}s)",
                            dir.display(),
                            ttl.as_secs()
                        );
                        client = client.with_area_cache(AreaCache::new(dir, ttl));
                    }
                    None => tracing::info!("FIRMS area cache disabled"),
                }
                Some(Arc::new(client))
            }
            Err(err) => {
                tracing::error!("Failed to initialize FIRMS client: {err}");
                None
            }
        },
        _ => {
            tracing::warn!("FIRMS_API_KEY is not configured; FIRMS endpoints will return 503");
            None
        }
    };

    let archive_cache_dir = archive_cache_dir_from_env();
    let archive_api = Arc::new(archive_api::ArchiveState::new(
        archive_cache_dir.clone(),
        archive_api::ArchiveLimits::from_env(),
        pipeline_config.clone(),
    ));
    if archive_cache_dir.is_dir() {
        let warm_state = archive_api.clone();
        tokio::spawn(async move { warm_state.warm().await });
    }
    // Offline client: the server never downloads archive data on demand; populate via `make data`.
    let archive = Some(Arc::new(ArchiveClient::offline(archive_cache_dir.clone())));

    let cache_budget_bytes = parse_cache_budget_bytes(std::env::var("CACHE_BUDGET_MB").ok().as_deref());
    let request_timeout = parse_request_timeout(std::env::var("REQUEST_TIMEOUT_SECS").ok().as_deref());
    let cors_layer = build_cors_layer(std::env::var("CORS_ALLOWED_ORIGINS").ok().as_deref());

    let state = AppState {
        firms,
        archive,
        cache: Arc::new(RwLock::new(ResponseCache::new(cache_budget_bytes))),
        request_timeout,
        config: pipeline_config,
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

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let command = match parse_cli(std::env::args().skip(1)) {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("error: {err}\n\n{CLI_USAGE}");
            return std::process::ExitCode::from(2);
        }
    };
    if matches!(command, CliCommand::Help) {
        println!("{CLI_USAGE}");
        return std::process::ExitCode::SUCCESS;
    }

    let filter = EnvFilter::from_default_env()
        .add_directive("pyrosync=info".parse().expect("valid tracing directive"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    match command {
        CliCommand::Serve => match run_server().await {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(err) => {
                tracing::error!("Server error: {err:#}");
                std::process::ExitCode::FAILURE
            }
        },
        CliCommand::Precompute {
            regions,
            concurrency,
            force,
        } => run_precompute(regions, concurrency, force).await,
        CliCommand::Help => std::process::ExitCode::SUCCESS,
    }
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn config(State(state): State<AppState>) -> Json<PipelineConfig> {
    Json(state.config.as_ref().clone())
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
    let min_confidence = parse_min_confidence(params.min_confidence.as_deref())?;
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
    let excluded_detections = min_confidence.retain(&mut modis) + min_confidence.retain(&mut viirs);

    let modis_count = modis.len();
    let viirs_count = viirs.len();

    let mut raw = Vec::with_capacity(modis_count + viirs_count);
    raw.extend(modis.iter().cloned());
    raw.extend(viirs.iter().cloned());

    let harmonized = pyrosync_core::harmonize::harmonize_with_config(modis, viirs, &state.config);
    let calendar = pyrosync_core::calendar::build_calendar_with(
        &harmonized,
        &state.config.uncertainty,
        &state.config.calibration,
    );

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
        filters: ConfidenceFilter {
            min_confidence,
            excluded_detections,
        },
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

fn pre_aqua_warning(excluded: usize) -> Option<String> {
    (excluded > 0).then(|| {
        format!(
            "{excluded} months before 2002-07 (Terra-only era) are not effort-comparable with later months and have no normalized count or anomaly."
        )
    })
}

fn build_effort_and_warnings_from_counts(
    harmonized: &[HarmonizedHotspot],
    failure_count: usize,
    singular: &str,
    plural: &str,
    uncovered: &[(String, String)],
    tropical_swath_gap: Option<TropicalSwathGap>,
) -> (EffortResponse, Vec<String>) {
    let (reason, mut warnings) =
        build_warnings_from_counts(failure_count, singular, plural, uncovered);
    let excluded = pyrosync_core::effort::pre_aqua_excluded_months(harmonized).len();
    warnings.extend(pre_aqua_warning(excluded));
    let effort = EffortResponse {
        reliable: reason.is_none(),
        reason,
        factors: pyrosync_core::effort::estimate_effort(harmonized),
        tropical_swath_gap,
    };

    (effort, warnings)
}

fn build_effort_and_warnings(
    harmonized: &[HarmonizedHotspot],
    report: &ReportResponse,
    tropical_swath_gap: Option<TropicalSwathGap>,
) -> (EffortResponse, Vec<String>) {
    build_effort_and_warnings_from_counts(
        harmonized,
        report.failures.len(),
        "FIRMS request",
        "FIRMS requests",
        &report.uncovered,
        tropical_swath_gap,
    )
}

fn normalize_zero(v: f64) -> f64 {
    if v == 0.0 { 0.0 } else { v }
}

fn build_cache_key(
    route: &str,
    bbox: &BBox,
    window: &TimeWindow,
    include_raw: bool,
    min_confidence: MinConfidence,
) -> String {
    let window_part = match window {
        TimeWindow::Days(days) => format!("days={}", days.get()),
        TimeWindow::Range { start, end } => format!("start={start}&end={end}"),
    };
    format!(
        "{route}|{},{},{},{}|{window_part}|raw={include_raw}|conf={}",
        normalize_zero(bbox.west),
        normalize_zero(bbox.south),
        normalize_zero(bbox.east),
        normalize_zero(bbox.north),
        min_confidence.as_str(),
    )
}

fn build_calendar_cache_key(
    country: &str,
    from: i32,
    to: i32,
    bbox: Option<&BBox>,
    min_confidence: MinConfidence,
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
    format!(
        "calendar|{country}|from={from}&to={to}|{bbox_part}|conf={}",
        min_confidence.as_str()
    )
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
    let min_confidence = parse_min_confidence(params.min_confidence.as_deref())?;
    let key = build_cache_key(route, &bbox, &window, include_raw, min_confidence);

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
    ApiQuery(params): ApiQuery<HarmonizeParams>,
) -> Result<Response, ApiError> {
    let uncertainty = uncertainty_metadata(&state.config.uncertainty);
    let uncertainty_params = state.config.uncertainty.clone();
    execute_cached_pipeline(&state, "harmonize", &params, |run, include_raw| {
        let swath_gap = pyrosync_core::calendar::calendar_swath_gap(
            Some(&run.bbox),
            &run.harmonized,
            &uncertainty_params,
        );
        let (effort, warnings) = build_effort_and_warnings(&run.harmonized, &run.report, swath_gap);
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
            "uncertainty": uncertainty,
            "filters": run.filters,
            "warnings": warnings,
            "report": run.report,
        })
    })
    .await
}

#[derive(Clone, Copy)]
enum ExportFormat {
    Csv,
    GeoJson,
}

impl ExportFormat {
    fn parse(value: Option<&str>) -> Result<Self, ApiError> {
        match value.map(str::to_ascii_lowercase).as_deref() {
            None | Some("csv") => Ok(Self::Csv),
            Some("geojson") => Ok(Self::GeoJson),
            Some(other) => Err(ApiError::bad_request(format!(
                "Invalid 'format' value '{other}': expected 'csv' or 'geojson'"
            ))),
        }
    }

    fn route(self) -> &'static str {
        match self {
            Self::Csv => "export-csv",
            Self::GeoJson => "export-geojson",
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Self::Csv => "text/csv; charset=utf-8",
            Self::GeoJson => "application/geo+json",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::GeoJson => "geojson",
        }
    }
}

fn export_response(
    body: Vec<u8>,
    format: ExportFormat,
    bbox: &BBox,
    cache_status: &'static str,
) -> Result<Response, ApiError> {
    let filename = format!(
        "pyrosync_{}_{}_{}_{}_{}.{}",
        bbox.west,
        bbox.south,
        bbox.east,
        bbox.north,
        Utc::now().format("%Y%m%d"),
        format.extension()
    );
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
        .map_err(|e| ApiError::internal(format!("Invalid download filename: {e}")))?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(format.content_type())),
            (header::CONTENT_DISPOSITION, disposition),
            (X_CACHE_HEADER, HeaderValue::from_static(cache_status)),
        ],
        body,
    )
        .into_response())
}

async fn export(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<HarmonizeParams>,
) -> Result<Response, ApiError> {
    // Validate before consulting the cache so invalid requests never probe cache entries.
    let format = ExportFormat::parse(params.format.as_deref())?;
    let bbox = parse_bbox(&params)?;
    let window = parse_time_window(&params)?;
    let min_confidence = parse_min_confidence(params.min_confidence.as_deref())?;
    let key = build_cache_key(format.route(), &bbox, &window, false, min_confidence);

    if let Some(cached_body) = get_cached(&state.cache, &key).await {
        return export_response(cached_body, format, &bbox, "HIT");
    }

    let run = run_pipeline_with_timeout(&state, &params).await?;
    let body = match format {
        ExportFormat::Csv => pyrosync_core::export::to_csv(&run.harmonized),
        ExportFormat::GeoJson => pyrosync_core::export::to_geojson(&run.harmonized),
    }
    .into_bytes();

    put_cached(&state.cache, key, &body, cache_ttl_for_window(&window)).await;
    export_response(body, format, &bbox, "MISS")
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
    let mut vza_adjustment_pct_sum = 0.0_f64;

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
                let vza_only_frp = pyrosync_core::vza::apply_vza_correction(h.frp, h.vza_factor);
                let ratio = vza_only_frp / h.frp;
                vza_reduction_pct_sum += 100.0 * (1.0 - ratio);
                vza_adjustment_pct_sum += 100.0 * (ratio - 1.0);
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
            mean_frp_adjustment_pct: None,
        }
    } else {
        let count_f64 = vza_count as f64;
        CompareVza {
            records_with_correction: vza_count,
            mean_factor: Some(vza_factor_sum / count_f64),
            max_factor: vza_max_factor,
            mean_frp_reduction_pct: Some(vza_reduction_pct_sum / count_f64),
            mean_frp_adjustment_pct: Some(vza_adjustment_pct_sum / count_f64),
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
        filters: run.filters,
        warnings,
        report: run.report,
    }
}

async fn compare(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<HarmonizeParams>,
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
        filters: run.filters,
        warnings,
        report: run.report,
    }
}

async fn stats(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<HarmonizeParams>,
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
    if params.country.len() > MAX_COUNTRY_LEN {
        return Err(ApiError::bad_request(format!(
            "Parameter 'country' must be at most {MAX_COUNTRY_LEN} characters"
        )));
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
    let min_confidence = parse_min_confidence(params.min_confidence.as_deref())?;

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
        min_confidence,
    })
}

fn build_calendar_response(
    query: &ValidatedCalendarQuery,
    raw_detections: usize,
    harmonized: &[HarmonizedHotspot],
    calendar: Vec<CalendarCell>,
    per_sensor_year: Vec<SensorYearFetch>,
    failures: Vec<SensorYearFailure>,
    uncertainty_params: &pyrosync_core::uncertainty::UncertaintyParams,
) -> CalendarResponse {
    let swath_gap = pyrosync_core::calendar::calendar_swath_gap(
        query.bbox.as_ref(),
        harmonized,
        uncertainty_params,
    );
    let (effort, mut warnings) = build_effort_and_warnings_from_counts(
        harmonized,
        failures.len(),
        "FIRMS archive download",
        "FIRMS archive downloads",
        &[],
        swath_gap,
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

    if !failures.is_empty() {
        let missing: Vec<String> = failures
            .iter()
            .map(|f| format!("{:?} {}", f.sensor, f.year))
            .collect();
        warnings.push(format!(
            "Missing local archive data for: {}. Run `make data` to download the FIRMS country archive (~41 GB).",
            missing.join(", ")
        ));
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
        uncertainty: uncertainty_metadata(uncertainty_params),
        // The caller fills in `excluded_detections`, which is only known after the archive reads.
        filters: ConfidenceFilter {
            min_confidence: query.min_confidence,
            excluded_detections: 0,
        },
        fetch: ArchiveFetchSummary {
            per_sensor_year,
            failures,
        },
        warnings,
    }
}

fn format_archive_error(err: ArchiveError) -> String {
    match err {
        ArchiveError::NotCached { path } => format!(
            "missing locally: archive file {} is not in the local archive (read-only client, no download)",
            path.display()
        ),
        other => other.to_string(),
    }
}

enum CountryYearOutcome {
    /// Detections kept and the number dropped by the confidence filter.
    Found(Vec<Hotspot>, usize),
    CompleteNoFile,
}

async fn run_calendar_pipeline(
    archive: &Arc<ArchiveClient>,
    config: &PipelineConfig,
    query: &ValidatedCalendarQuery,
) -> Result<CalendarResponse, ApiError> {
    let mut modis = Vec::new();
    let mut viirs = Vec::new();
    let mut per_sensor_year = Vec::new();
    let mut failures = Vec::new();
    let mut attempted_any = false;
    let mut found_any_file = false;
    let mut excluded_detections = 0usize;

    if query.years.effective_from <= query.years.effective_to {
        let semaphore = Arc::new(Semaphore::new(ARCHIVE_FETCH_CONCURRENCY));
        let mut handles = Vec::new();

        for year in query.years.effective_from..=query.years.effective_to {
            for sensor in ARCHIVE_FETCH_SENSORS {
                if !archive.covers(sensor, year) {
                    continue;
                }
                attempted_any = true;
                let permit = Arc::clone(&semaphore)
                    .acquire_owned()
                    .await
                    .map_err(|e| ApiError::internal(format!("Archive worker semaphore closed: {e}")))?;
                let archive = Arc::clone(archive);
                let country = query.country.clone();
                let bbox = query.bbox.clone();
                let min_confidence = query.min_confidence;

                let handle = tokio::spawn(async move {
                    let _permit = permit;
                    let mut hotspots = match archive.fetch_country_year(sensor, &country, year).await {
                        Ok(hotspots) => hotspots,
                        // A completed sensor-year zip with no CSV for this country means zero detections, not a missing download.
                        Err(ArchiveError::NotCached { .. })
                            if archive.sensor_year_complete(sensor, year) =>
                        {
                            return Ok(CountryYearOutcome::CompleteNoFile);
                        }
                        Err(err) => return Err(format_archive_error(err)),
                    };
                    // Filter per country-year before merging decades of detections to bound peak memory.
                    pyrosync_firms::parse::retain_wildfires(&mut hotspots);
                    if let Some(ref bbox) = bbox {
                        hotspots = filter_bbox(hotspots, bbox);
                    }
                    let excluded = min_confidence.retain(&mut hotspots);
                    Ok::<CountryYearOutcome, String>(CountryYearOutcome::Found(hotspots, excluded))
                });
                handles.push((sensor, year, handle));
            }
        }

        for (sensor, year, handle) in handles {
            match handle.await {
                Ok(Ok(CountryYearOutcome::Found(mut hotspots, excluded))) => {
                    found_any_file = true;
                    excluded_detections += excluded;
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
                Ok(Ok(CountryYearOutcome::CompleteNoFile)) => {
                    per_sensor_year.push(SensorYearFetch {
                        sensor,
                        year,
                        detections: 0,
                    });
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

    if attempted_any && !found_any_file {
        return Err(ApiError::not_found(format!(
            "No local archive data for country '{}'. Historical data is not downloaded by default; run `make data` to download the FIRMS country archive (~41 GB).",
            query.country
        )));
    }

    let config = config.clone();
    let query = query.clone();
    tokio::task::spawn_blocking(move || {
        let raw_detections = modis.len() + viirs.len();
        let harmonized = pyrosync_core::harmonize::harmonize_with_config(modis, viirs, &config);
        let calendar_cells = pyrosync_core::calendar::build_calendar_with(
            &harmonized,
            &config.uncertainty,
            &config.calibration,
        );

        let mut response = build_calendar_response(
            &query,
            raw_detections,
            &harmonized,
            calendar_cells,
            per_sensor_year,
            failures,
            &config.uncertainty,
        );
        response.filters.excluded_detections = excluded_detections;
        response
    })
    .await
    .map_err(|e| ApiError::internal(format!("Calendar worker failed: {e}")))
}

async fn run_calendar_with_timeout(
    state: &AppState,
    query: &ValidatedCalendarQuery,
) -> Result<CalendarResponse, ApiError> {
    let archive = require_archive_client(state)?;
    match state.request_timeout {
        Some(timeout) => match tokio::time::timeout(timeout, run_calendar_pipeline(archive, &state.config, query)).await {
            Ok(result) => result,
            // Dropping the timed-out future cannot cancel a `spawn_blocking` task already in flight;
            // the blocking CPU work continues running in the background until it finishes.
            Err(_) => Err(ApiError::gateway_timeout(format!(
                "Request timed out after {}s; reading and harmonizing large multi-year archives can take several minutes. Increase REQUEST_TIMEOUT_SECS or narrow the year range.",
                timeout.as_secs()
            ))),
        },
        None => run_calendar_pipeline(archive, &state.config, query).await,
    }
}

async fn calendar(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<CalendarParams>,
) -> Result<Response, ApiError> {
    let query = parse_calendar_params(&params)?;
    let key = build_calendar_cache_key(
        &query.country,
        query.years.requested_from,
        query.years.requested_to,
        query.bbox.as_ref(),
        query.min_confidence,
    );

    if let Some(cached_body) = get_cached(&state.cache, &key).await {
        return Ok(json_bytes_response(cached_body, "HIT"));
    }

    // Offline archive reads do not use a FIRMS API key, so this endpoint works without `require_firms_client`.
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
        .ok_or_else(|| ApiError::service_unavailable("FIRMS_API_KEY is not configured"))
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

fn parse_min_confidence(value: Option<&str>) -> Result<MinConfidence, ApiError> {
    MinConfidence::parse(value).map_err(ApiError::bad_request)
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

    #[test]
    fn pre_aqua_warning_names_count_and_is_absent_when_nothing_excluded() {
        assert_eq!(pre_aqua_warning(0), None);
        assert_eq!(
            pre_aqua_warning(18).as_deref(),
            Some("18 months before 2002-07 (Terra-only era) are not effort-comparable with later months and have no normalized count or anomaly.")
        );
    }
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
            frp_lower: args.frp_corrected,
            frp_upper: args.frp_corrected,
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
            filters: ConfidenceFilter::default(),
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
        assert!(resp.vza.mean_frp_adjustment_pct.is_none());
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
            filters: ConfidenceFilter::default(),
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
        assert_eq!(resp.vza.mean_frp_reduction_pct, Some(-100.0));
        assert_eq!(resp.vza.mean_frp_adjustment_pct, Some(100.0));
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
            filters: ConfidenceFilter::default(),
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
            filters: ConfidenceFilter::default(),
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
    async fn config_endpoint_reflects_state_config() {
        let mut state = test_state(1024, None);
        let mut custom = PipelineConfig::default();
        custom.matching.radius_km = 4.5;
        custom.vza.a = 0.0;
        state.config = Arc::new(custom.clone());

        let Json(body) = config(State(state)).await;
        assert_eq!(body, custom);
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["matching"]["radius_km"], 4.5);
        assert_eq!(json["matching"]["window_secs"], 10_800);
    }

    fn test_state(budget_bytes: usize, request_timeout: Option<Duration>) -> AppState {
        AppState {
            firms: None,
            archive: None,
            cache: Arc::new(RwLock::new(ResponseCache::new(budget_bytes))),
            request_timeout,
            config: Arc::new(PipelineConfig::default()),
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
            format: None,
            min_confidence: None,
        };
        let err = compare(State(state.clone()), ApiQuery(bad_bbox))
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
            format: None,
            min_confidence: None,
        };
        let err = stats(State(state), ApiQuery(missing_key))
            .await
            .expect_err("missing FIRMS key should return 503");
        assert_eq!(err.status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn cache_key_distinguishes_include_raw() {
        let bbox = sample_bbox();
        let window = TimeWindow::Days(DayRange::new(2).unwrap());

        let key_false = build_cache_key("harmonize", &bbox, &window, false, MinConfidence::Low);
        let key_true = build_cache_key("harmonize", &bbox, &window, true, MinConfidence::Low);

        assert_ne!(key_false, key_true);
    }

    #[test]
    fn cache_key_distinguishes_min_confidence() {
        let bbox = sample_bbox();
        let window = TimeWindow::Days(DayRange::new(2).unwrap());
        let keys: BTreeSet<String> = [MinConfidence::Low, MinConfidence::Nominal, MinConfidence::High]
            .into_iter()
            .map(|level| build_cache_key("harmonize", &bbox, &window, false, level))
            .collect();
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn min_confidence_parser_defaults_to_low_and_rejects_unknown_values() {
        assert_eq!(MinConfidence::parse(None), Ok(MinConfidence::Low));
        assert_eq!(MinConfidence::parse(Some("low")), Ok(MinConfidence::Low));
        assert_eq!(MinConfidence::parse(Some("Nominal")), Ok(MinConfidence::Nominal));
        assert_eq!(MinConfidence::parse(Some("high")), Ok(MinConfidence::High));
        for bad in ["", "medium", "l", "n"] {
            assert!(MinConfidence::parse(Some(bad)).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn cache_key_distinguishes_days_and_date_range() {
        let bbox = sample_bbox();
        let days_window = TimeWindow::Days(DayRange::new(3).unwrap());
        let range_window = TimeWindow::Range {
            start: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 7, 3).unwrap(),
        };

        let key_days = build_cache_key("harmonize", &bbox, &days_window, false, MinConfidence::Low);
        let key_range = build_cache_key("harmonize", &bbox, &range_window, false, MinConfidence::Low);

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
            format: None,
            min_confidence: None,
        };

        // First call fails because FIRMS_API_KEY is missing; errors must never be cached.
        let err = harmonize(State(state.clone()), ApiQuery(params))
            .await
            .expect_err("missing key returns 503");
        assert_eq!(err.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(state.cache.read().await.entries.len(), 0);

        let bbox = BBox {
            west: 26.0,
            south: 36.0,
            east: 45.0,
            north: 42.0,
        };
        let window = TimeWindow::Days(DayRange::new(2).unwrap());
        let key = build_cache_key("harmonize", &bbox, &window, false, MinConfidence::Low);
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
            format: None,
            min_confidence: None,
        };
        let resp = harmonize(State(state), ApiQuery(params))
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

    fn export_params(format: Option<&str>, west: f64) -> HarmonizeParams {
        HarmonizeParams {
            west,
            south: 36.0,
            east: 45.0,
            north: 42.0,
            days: Some(2),
            start: None,
            end: None,
            include_raw: None,
            format: format.map(str::to_string),
            min_confidence: None,
        }
    }

    #[test]
    fn export_format_parser_accepts_known_values_and_rejects_others() {
        assert!(matches!(ExportFormat::parse(None), Ok(ExportFormat::Csv)));
        assert!(matches!(ExportFormat::parse(Some("csv")), Ok(ExportFormat::Csv)));
        assert!(matches!(ExportFormat::parse(Some("geojson")), Ok(ExportFormat::GeoJson)));
        assert!(matches!(ExportFormat::parse(Some("GeoJSON")), Ok(ExportFormat::GeoJson)));
        let err = ExportFormat::parse(Some("xml")).err().expect("xml rejected");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn export_validates_before_firms_client() {
        let state = test_state(1024 * 1024, Some(Duration::from_secs(300)));
        let err = export(State(state.clone()), ApiQuery(export_params(Some("xml"), 26.0)))
            .await
            .expect_err("unknown format should fail");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);

        let err = export(State(state), ApiQuery(export_params(Some("csv"), 50.0)))
            .await
            .expect_err("invalid bbox should fail");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn export_serves_cached_hit_as_csv_download() {
        let state = test_state(1024 * 1024, Some(Duration::from_secs(300)));
        let bbox = sample_bbox();
        let window = TimeWindow::Days(DayRange::new(2).unwrap());
        let key = build_cache_key("export-csv", &bbox, &window, false, MinConfidence::Low);
        let cached = b"lat,lon\r\n1,2\r\n";
        put_cached(&state.cache, key, cached, Duration::from_secs(600)).await;

        let resp = export(State(state), ApiQuery(export_params(None, 26.0)))
            .await
            .expect("cache hit should succeed without FIRMS client");
        assert_eq!(resp.status(), StatusCode::OK);
        let headers = resp.headers();
        assert_eq!(
            headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()),
            Some("text/csv; charset=utf-8")
        );
        assert_eq!(
            headers.get(X_CACHE_HEADER).and_then(|v| v.to_str().ok()),
            Some("HIT")
        );
        let disposition = headers
            .get(header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .expect("content-disposition");
        assert!(disposition.starts_with("attachment; filename=\"pyrosync_"));
        assert!(disposition.ends_with(".csv\""));
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], cached);
    }

    #[tokio::test]
    async fn router_returns_json_404_for_unknown_api_and_serves_cors_headers() {
        let state = test_state(1024, None);
        let archive_api = Arc::new(archive_api::ArchiveState::new(
            PathBuf::from("nonexistent-archive-dir-for-test"),
            archive_api::ArchiveLimits::from_env(),
            Arc::new(PipelineConfig::default()),
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

    #[tokio::test]
    async fn spa_fallback_serves_index_for_unknown_paths_but_not_for_api() {
        let unique = format!(
            "pyrosync-spa-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after unix epoch")
                .as_nanos()
        );
        let static_dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&static_dir).expect("create temp static dir");
        std::fs::write(
            static_dir.join("index.html"),
            "<html><body>spa-index-marker</body></html>",
        )
        .expect("write index.html");
        std::fs::write(static_dir.join("app.js"), "// spa-js-marker").expect("write app.js");

        let state = test_state(1024, None);
        let archive_api = Arc::new(archive_api::ArchiveState::new(
            PathBuf::from("nonexistent-archive-dir-for-test"),
            archive_api::ArchiveLimits::from_env(),
            Arc::new(PipelineConfig::default()),
        ));
        let cors = build_cors_layer(None);
        let app = build_router(
            state,
            Some(archive_api),
            static_dir.to_str().expect("utf8 temp dir path"),
            cors,
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let spa_route = send_http_request(
            addr,
            "GET /harita HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(spa_route.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(spa_route.contains("spa-index-marker"));

        let js_asset = send_http_request(
            addr,
            "GET /app.js HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(js_asset.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(js_asset.contains("spa-js-marker"));

        let api_404 = send_http_request(
            addr,
            "GET /api/olmayanbirsey HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(api_404.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(api_404.contains("Unknown API endpoint"));

        server.abort();
        std::fs::remove_dir_all(&static_dir).expect("remove temp static dir");
    }

    #[tokio::test]
    async fn missing_static_dir_keeps_plain_404_without_panicking() {
        let state = test_state(1024, None);
        let archive_api = Arc::new(archive_api::ArchiveState::new(
            PathBuf::from("nonexistent-archive-dir-for-test"),
            archive_api::ArchiveLimits::from_env(),
            Arc::new(PipelineConfig::default()),
        ));
        let cors = build_cors_layer(None);
        let app = build_router(
            state,
            Some(archive_api),
            "nonexistent-static-dir-for-test",
            cors,
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let non_api = send_http_request(
            addr,
            "GET /harita HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(non_api.starts_with("HTTP/1.1 404 Not Found\r\n"));

        server.abort();
    }

    #[test]
    fn area_cache_env_parsers_apply_defaults_and_fallbacks() {
        assert_eq!(
            parse_area_cache_dir(None),
            Some(PathBuf::from(".cache/firms-area"))
        );
        assert_eq!(parse_area_cache_dir(Some("  ")), None);
        assert_eq!(parse_area_cache_dir(Some("/data/a")), Some(PathBuf::from("/data/a")));
        assert_eq!(parse_area_cache_nrt_ttl(None), Duration::from_secs(600));
        assert_eq!(parse_area_cache_nrt_ttl(Some("0")), Duration::ZERO);
        assert_eq!(parse_area_cache_nrt_ttl(Some("30")), Duration::from_secs(30));
        assert_eq!(parse_area_cache_nrt_ttl(Some("abc")), Duration::from_secs(600));
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
            min_confidence: None,
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

        let default_uncertainty = pyrosync_core::uncertainty::UncertaintyParams::default();
        let resp = build_calendar_response(
            &query,
            0,
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &default_uncertainty,
        );
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
            &default_uncertainty,
        );
        assert!(empty_resp.calendar.is_empty());
        assert!(empty_resp.warnings.iter().any(|w| w.contains("outside")));
    }

    #[test]
    fn calendar_coverage_includes_noaa20_marks_noaa21_unavailable_and_warns_on_failures() {
        let query = parse_calendar_params(&calendar_params("Turkey", Some(2015), Some(2020), None))
            .expect("valid query");
        let default_uncertainty = pyrosync_core::uncertainty::UncertaintyParams::default();

        let ok_resp = build_calendar_response(
            &query,
            0,
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &default_uncertainty,
        );
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
                error: "missing locally: archive file modis_2017_Turkey.csv is not in the local archive (read-only client, no download)".to_string(),
            }],
            &default_uncertainty,
        );
        assert!(!failed_resp.effort.reliable);
        assert!(failed_resp.effort.reason.is_some());
        assert_eq!(failed_resp.fetch.failures.len(), 1);
        assert!(failed_resp.warnings.len() >= 3);
        assert!(failed_resp
            .warnings
            .iter()
            .any(|w| w.contains("ModisTerra 2017") && w.contains("make data")));
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

        let k_tr_full = build_calendar_cache_key("Turkey", 2000, 2025, None, MinConfidence::Low);
        let k_gr_full = build_calendar_cache_key("Greece", 2000, 2025, None, MinConfidence::Low);
        let k_tr_sub = build_calendar_cache_key("Turkey", 2015, 2020, None, MinConfidence::Low);
        let k_tr_bbox_a = build_calendar_cache_key("Turkey", 2000, 2025, Some(&bbox_a), MinConfidence::Low);
        let k_tr_bbox_b = build_calendar_cache_key("Turkey", 2000, 2025, Some(&bbox_b), MinConfidence::Low);
        let k_tr_nominal = build_calendar_cache_key("Turkey", 2000, 2025, None, MinConfidence::Nominal);

        let keys = [
            &k_tr_full,
            &k_gr_full,
            &k_tr_sub,
            &k_tr_bbox_a,
            &k_tr_bbox_b,
            &k_tr_nominal,
        ];
        let unique: BTreeSet<&String> = keys.into_iter().collect();
        assert_eq!(unique.len(), 6);
    }

    #[tokio::test]
    async fn calendar_handler_serves_cached_hit_without_firms_key() {
        let state = test_state(1024 * 1024, Some(Duration::from_secs(300)));
        assert!(state.firms.is_none());

        let key = build_calendar_cache_key("Turkey", 2000, 2025, None, MinConfidence::Low);
        put_cached(
            &state.cache,
            key,
            br#"{"country":"Turkey","calendar":[]}"#,
            Duration::from_secs(CACHE_TTL_ARCHIVE_SECS),
        )
        .await;

        let resp = calendar(
            State(state),
            ApiQuery(calendar_params("Turkey", None, None, None)),
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

    #[tokio::test]
    async fn calendar_with_empty_archive_returns_404_with_make_data_hint() {
        let dir = tempfile::tempdir().expect("temp dir");
        let archive = Arc::new(ArchiveClient::offline(dir.path().to_path_buf()));
        assert!(archive.is_offline());

        let state = AppState {
            firms: None,
            archive: Some(archive),
            cache: Arc::new(RwLock::new(ResponseCache::new(1024 * 1024))),
            request_timeout: Some(Duration::from_secs(30)),
            config: Arc::new(PipelineConfig::default()),
        };

        let err = calendar(
            State(state.clone()),
            ApiQuery(calendar_params("Turkey", Some(2020), Some(2020), None)),
        )
        .await
        .expect_err("missing local archive must return 404 without downloading");

        assert_eq!(err.status, StatusCode::NOT_FOUND);
        assert!(err.message.contains("Turkey"), "message: {}", err.message);
        assert!(err.message.contains("make data"), "message: {}", err.message);
        assert!(state.cache.read().await.entries.is_empty());
        assert!(
            std::fs::read_dir(dir.path()).unwrap().next().is_none(),
            "offline calendar must not create or download files"
        );
    }

    #[tokio::test]
    async fn calendar_offline_archive_includes_uncertainty_note_and_cell_bounds() {
        let dir = tempfile::tempdir().expect("temp dir");
        let year_dir = dir.path().join("modis").join("2020");
        std::fs::create_dir_all(&year_dir).expect("create archive year dir");
        let csv = "\
latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight,type
39.0000,32.0000,330.0,1.5,1.0,2020-07-01,1000,Terra,MODIS,80,6.1,290.0,50.0,D,0
39.1000,32.1000,328.0,2.0,1.0,2020-07-02,1000,Aqua,MODIS,75,6.1,289.0,60.0,D,0
";
        std::fs::write(year_dir.join("modis_2020_Testland.csv"), csv).expect("write archive csv");

        let archive = Arc::new(ArchiveClient::offline(dir.path().to_path_buf()));
        let state = AppState {
            firms: None,
            archive: Some(archive),
            cache: Arc::new(RwLock::new(ResponseCache::new(1024 * 1024))),
            request_timeout: Some(Duration::from_secs(30)),
            config: Arc::new(PipelineConfig::default()),
        };

        let resp = calendar(
            State(state),
            ApiQuery(calendar_params("Testland", Some(2020), Some(2020), None)),
        )
        .await
        .expect("offline calendar should succeed");
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("read body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("valid json");

        assert_eq!(json["uncertainty"]["note"], UNCERTAINTY_NOTE);
        let conf = json["uncertainty"]["confidence"]
            .as_f64()
            .expect("confidence");
        assert!((conf - 0.682689).abs() < 1e-4, "confidence={conf}");

        let cells = json["calendar"].as_array().expect("calendar cells");
        assert_eq!(cells.len(), 1);
        let cell = &cells[0];
        let mean_frp = cell["mean_frp"].as_f64().expect("mean_frp");
        let lower = cell["mean_frp_lower"]
            .as_f64()
            .expect("mean_frp_lower should be present");
        let upper = cell["mean_frp_upper"]
            .as_f64()
            .expect("mean_frp_upper should be present");
        assert!(lower < mean_frp && mean_frp < upper, "{lower} < {mean_frp} < {upper}");
        println!(
            "sample /api/calendar cell: {}",
            serde_json::to_string_pretty(cell).expect("pretty json")
        );
    }

    #[test]
    fn cli_parser_handles_server_precompute_and_errors() {
        let parse = |args: &[&str]| parse_cli(args.iter().map(|s| (*s).to_string()));
        assert_eq!(parse(&[]), Ok(CliCommand::Serve));
        assert_eq!(parse(&["--help"]), Ok(CliCommand::Help));
        assert_eq!(
            parse(&["precompute"]),
            Ok(CliCommand::Precompute {
                regions: Vec::new(),
                concurrency: archive_api::DEFAULT_PRECOMPUTE_CONCURRENCY,
                force: false,
            })
        );
        assert_eq!(
            parse(&[
                "precompute",
                "--region",
                "Cyprus",
                "--region=Armenia",
                "--concurrency",
                "4",
                "--force",
            ]),
            Ok(CliCommand::Precompute {
                regions: vec!["Cyprus".to_string(), "Armenia".to_string()],
                concurrency: 4,
                force: true,
            })
        );
        assert!(parse(&["unknown-cmd"]).is_err());
        assert!(parse(&["precompute", "--bogus"]).is_err());
        assert!(parse(&["precompute", "--concurrency", "0"]).is_err());
        assert!(parse(&["precompute", "--region"]).is_err());
    }

    async fn oneshot_request(app: &Router, uri: &str) -> (StatusCode, String, Vec<u8>) {
        use tower::ServiceExt;

        let req = axum::http::Request::get(uri)
            .body(axum::body::Body::empty())
            .expect("build request");
        let resp = app.clone().oneshot(req).await.expect("router response");
        let status = resp.status();
        let content_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("read response body")
            .to_vec();
        (status, content_type, body)
    }

    #[tokio::test]
    async fn query_deserialization_errors_return_json_400() {
        let state = test_state(1024, None);
        let app = build_router(state, None, "nonexistent-static-dir-for-test", build_cors_layer(None));

        for uri in [
            "/api/calendar",
            "/api/calendar?country=Malta&from=abc",
            "/api/harmonize?west=abc&south=36&east=45&north=42",
            "/api/harmonize?west=26&south=36&east=45&north=42&days=-1",
            "/api/compare?west=abc&south=36&east=45&north=42",
            "/api/stats?west=abc&south=36&east=45&north=42",
            "/api/export?west=abc&south=36&east=45&north=42",
        ] {
            let (status, content_type, body) = oneshot_request(&app, uri).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "uri={uri}");
            assert!(
                content_type.starts_with("application/json"),
                "uri={uri} content-type={content_type} body={}",
                String::from_utf8_lossy(&body)
            );
            let json: serde_json::Value =
                serde_json::from_slice(&body).expect("error body should be valid JSON");
            assert!(
                json["error"].as_str().is_some_and(|s| !s.is_empty()),
                "uri={uri} json={json}"
            );
        }
    }

    #[tokio::test]
    async fn firms_endpoints_return_503_when_api_key_missing() {
        let state = test_state(1024, None);
        let app = build_router(state, None, "nonexistent-static-dir-for-test", build_cors_layer(None));

        for uri in [
            "/api/harmonize?west=26&south=36&east=45&north=42&days=1",
            "/api/compare?west=26&south=36&east=45&north=42&days=1",
            "/api/stats?west=26&south=36&east=45&north=42&days=1",
            "/api/export?west=26&south=36&east=45&north=42&days=1",
            "/api/availability",
        ] {
            let (status, content_type, body) = oneshot_request(&app, uri).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "uri={uri}");
            assert!(
                content_type.starts_with("application/json"),
                "uri={uri} content-type={content_type}"
            );
            let json: serde_json::Value = serde_json::from_slice(&body).expect("json error");
            assert_eq!(json["error"], "FIRMS_API_KEY is not configured", "uri={uri}");
        }
    }

    #[tokio::test]
    async fn calendar_rejects_country_longer_than_64_chars_without_echoing_value() {
        let max_valid = "a".repeat(64);
        let parsed = parse_calendar_params(&calendar_params(&max_valid, None, None, None))
            .expect("64-char country should be accepted");
        assert_eq!(parsed.country, max_valid);

        let too_long = "a".repeat(65);
        let err = parse_calendar_params(&calendar_params(&too_long, None, None, None))
            .err()
            .expect("65-char country must be rejected");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(!err.message.contains(&too_long));

        let state = test_state(1024, None);
        let app = build_router(state, None, "nonexistent-static-dir-for-test", build_cors_layer(None));
        let ten_kb = "b".repeat(10 * 1024);
        let (status, content_type, body) =
            oneshot_request(&app, &format!("/api/calendar?country={ten_kb}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(content_type.starts_with("application/json"));
        let body_str = String::from_utf8(body).expect("utf8");
        assert!(!body_str.contains(&ten_kb));
    }

    fn write_sample_viirs_csv(dir: &std::path::Path, slug: &str, year: i32, country: &str) {
        let year_dir = dir.join(slug).join(year.to_string());
        std::fs::create_dir_all(&year_dir).expect("create year dir");
        let csv = format!(
            "latitude,longitude,bright_ti4,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_ti5,frp,daynight,type\n\
             60.1000,19.9000,330.0,0.4,0.4,{year}-07-01,1100,N,VIIRS,n,2.0,290.0,12.0,D,0\n"
        );
        std::fs::write(year_dir.join(format!("{slug}_{year}_{country}.csv")), csv)
            .expect("write sample viirs csv");
    }

    #[tokio::test]
    async fn calendar_treats_completed_sensor_year_missing_country_as_zero_detections() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_sample_viirs_csv(dir.path(), "viirs-snpp", 2021, "Aland_Islands");
        write_sample_viirs_csv(dir.path(), "viirs-jpss1", 2021, "Aland_Islands");
        let modis_dir = dir.path().join("modis").join("2021");
        std::fs::create_dir_all(&modis_dir).expect("create modis 2021 dir");
        std::fs::write(modis_dir.join(".complete"), b"").expect("write .complete marker");

        let mut state = test_state(1024 * 1024, Some(Duration::from_secs(30)));
        state.archive = Some(Arc::new(ArchiveClient::offline(dir.path().to_path_buf())));
        let app = build_router(state, None, "nonexistent-static-dir-for-test", build_cors_layer(None));

        let (status, _, body) = oneshot_request(
            &app,
            "/api/calendar?country=Aland_Islands&from=2021&to=2021",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "body={}", String::from_utf8_lossy(&body));
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["fetch"]["failures"], serde_json::json!([]));
        assert_eq!(json["effort"]["reliable"], serde_json::json!(true));
        let warnings = json["warnings"].as_array().expect("warnings");
        assert!(
            !warnings
                .iter()
                .any(|w| w.as_str().unwrap_or("").contains("make data")),
            "unexpected make data warning: {warnings:?}"
        );
        let per_sy = json["fetch"]["per_sensor_year"]
            .as_array()
            .expect("per_sensor_year");
        assert!(
            per_sy
                .iter()
                .any(|e| e["sensor"] == "ModisTerra" && e["year"] == 2021 && e["detections"] == 0),
            "expected ModisTerra 2021 with 0 detections in {per_sy:?}"
        );
    }

    #[tokio::test]
    async fn calendar_still_fails_when_sensor_year_not_marked_complete() {
        let dir = tempfile::tempdir().expect("temp dir");
        write_sample_viirs_csv(dir.path(), "viirs-snpp", 2021, "Aland_Islands");
        write_sample_viirs_csv(dir.path(), "viirs-jpss1", 2021, "Aland_Islands");

        let mut state = test_state(1024 * 1024, Some(Duration::from_secs(30)));
        state.archive = Some(Arc::new(ArchiveClient::offline(dir.path().to_path_buf())));
        let app = build_router(state, None, "nonexistent-static-dir-for-test", build_cors_layer(None));

        let (status, _, body) = oneshot_request(
            &app,
            "/api/calendar?country=Aland_Islands&from=2021&to=2021",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        let failures = json["fetch"]["failures"].as_array().expect("failures");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0]["sensor"], "ModisTerra");
        assert_eq!(json["effort"]["reliable"], serde_json::json!(false));
        let warnings = json["warnings"].as_array().expect("warnings");
        assert!(
            warnings
                .iter()
                .any(|w| w.as_str().unwrap_or("").contains("make data")),
            "expected make data warning: {warnings:?}"
        );
    }

    #[tokio::test]
    async fn calendar_returns_404_when_no_files_found_even_if_all_sensor_years_complete() {
        let dir = tempfile::tempdir().expect("temp dir");
        for slug in ["modis", "viirs-snpp", "viirs-jpss1"] {
            let ydir = dir.path().join(slug).join("2021");
            std::fs::create_dir_all(&ydir).expect("create year dir");
            std::fs::write(ydir.join(".complete"), b"").expect("write .complete marker");
        }

        let mut state = test_state(1024 * 1024, Some(Duration::from_secs(30)));
        state.archive = Some(Arc::new(ArchiveClient::offline(dir.path().to_path_buf())));
        let app = build_router(state, None, "nonexistent-static-dir-for-test", build_cors_layer(None));

        let (status, _, body) =
            oneshot_request(&app, "/api/calendar?country=Atlantis&from=2021&to=2021").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "body={}", String::from_utf8_lossy(&body));
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert!(
            json["error"]
                .as_str()
                .unwrap_or("")
                .contains("No local archive data"),
            "json={json}"
        );
    }

    #[test]
    fn compare_vza_adjustment_reports_both_signed_reduction_and_positive_adjustment() {
        let dt = Utc.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).unwrap();
        let run = PipelineRun {
            bbox: sample_bbox(),
            modis_count: 2,
            viirs_count: 0,
            raw: vec![
                make_raw(Sensor::ModisTerra, 100.0, 10, DayNight::Day),
                make_raw(Sensor::ModisAqua, 100.0, 11, DayNight::Day),
            ],
            harmonized: vec![
                // Nadir MODIS: vza_factor=1.0 -> 0% adjustment.
                make_harmonized(HarmonizedArgs {
                    platforms: vec![Sensor::ModisTerra],
                    source: HotspotSource::ModisOnly,
                    frp: 100.0,
                    frp_corrected: 100.0,
                    vza_factor: Some(1.0),
                    n_obs: 1,
                    confidence: Confidence::Nominal,
                    daynight: DayNight::Day,
                    acquired: dt,
                }),
                // Off-nadir MODIS: vza_factor=2.0 -> +100% VZA adjustment (frp_corrected=200.0).
                make_harmonized(HarmonizedArgs {
                    platforms: vec![Sensor::ModisAqua],
                    source: HotspotSource::ModisOnly,
                    frp: 100.0,
                    frp_corrected: 200.0,
                    vza_factor: Some(2.0),
                    n_obs: 1,
                    confidence: Confidence::Nominal,
                    daynight: DayNight::Day,
                    acquired: dt,
                }),
            ],
            calendar: Vec::new(),
            report: empty_report(),
            filters: ConfidenceFilter::default(),
        };

        let resp = build_compare_response(run);
        let mean_reduction = resp
            .vza
            .mean_frp_reduction_pct
            .expect("mean_frp_reduction_pct should be present");
        let mean_adjustment = resp
            .vza
            .mean_frp_adjustment_pct
            .expect("mean_frp_adjustment_pct should be present");
        // (0% + (-100%)) / 2 = -50% reduction; (0% + 100%) / 2 = +50% adjustment.
        assert!(
            (mean_reduction - (-50.0)).abs() < 1e-9,
            "expected -50.0, got {mean_reduction}"
        );
        assert!(
            (mean_adjustment - 50.0).abs() < 1e-9,
            "expected 50.0, got {mean_adjustment}"
        );
    }
}
