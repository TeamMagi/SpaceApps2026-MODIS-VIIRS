use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    Router,
    body::Bytes,
    extract::{Query, Request, State},
    http::{HeaderName, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::get,
};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use tokio::sync::{OnceCell, Semaphore};
use tokio::task::JoinSet;

use pyrosync_core::calendar::{compute_anomalies, mean_frp_with_uncertainty};
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::density::{ALLOWED_CELL_DEGREES, aggregate_density};
use pyrosync_core::effort::{SwathGapBasis, TropicalSwathGap, flag_tropical_swath_gap};
use pyrosync_core::events::{
    EventClustering, EventParams, FireEvent, NoDetectionDay, cluster_events, no_detection_days,
};
use pyrosync_core::fusion::{FusionModel, apply_fusion_model};
use pyrosync_core::harmonize::harmonize_with_links_masked;
use pyrosync_core::models::{
    CalendarCell, Confidence, DayNight, HarmonizedHotspot, Hotspot, HotspotSource, Sensor,
};
use pyrosync_core::static_mask::StaticMask;
use pyrosync_firms::archive::{ArchiveClient, ArchiveError, archive_slug, scan_local_archive};
use pyrosync_firms::parse::{parse_csv, retain_wildfires};
use pyrosync_firms::{FirmsSource, Processing};

use crate::derived_cache::{ArchiveFileStamp, CoverageStamp, DerivedCache, SeriesStamp};
use crate::{ConfidenceFilter, MinConfidence};

const X_SERIES_SOURCE_HEADER: HeaderName = HeaderName::from_static("x-series-source");
pub const DEFAULT_PRECOMPUTE_CONCURRENCY: usize = 2;

const DEFAULT_POINTS_LIMIT: usize = 4000;
const MAX_POINTS_LIMIT: usize = 20_000;
/// Upper bound on filtered rows kept in the year cache (only /api/points fills it).
const YEAR_CACHE_MAX_ROWS: usize = 4_000_000;
/// A sensor counts as covering a year when its archive coverage spans Jan 15 .. Dec 15.
const PARTIAL_START_DAY: u32 = 15;
const PARTIAL_END_DAY: u32 = 15;
/// Iran, the largest region after Russia, is 225 MB of CSV; its /api/series takes 0.7 s and
/// peaks at ~230 MB (release build). Russian_Federation (1936 MB) passed 2.1 GB within two
/// seconds. 512 MB admits every other country with more than 2x headroom and rejects Russia.
const DEFAULT_MAX_REGION_MB: u64 = 512;
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 90;
/// Mean archive CSV row length (Turkey: 66.9 MB over 857 k rows). Turns file sizes into a
/// detection estimate without parsing.
const BYTES_PER_ROW_ESTIMATE: u64 = 78;

#[derive(Debug, Clone, Copy)]
pub struct ArchiveLimits {
    pub max_region_bytes: u64,
    pub request_timeout: Duration,
}

impl ArchiveLimits {
    /// Reads `ARCHIVE_MAX_REGION_MB` and `ARCHIVE_REQUEST_TIMEOUT_SECS`, falling back to the defaults.
    pub fn from_env() -> Self {
        let read = |name: &str, default: u64| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .filter(|&v| v > 0)
                .unwrap_or(default)
        };
        Self {
            max_region_bytes: read("ARCHIVE_MAX_REGION_MB", DEFAULT_MAX_REGION_MB) * 1_000_000,
            request_timeout: Duration::from_secs(read(
                "ARCHIVE_REQUEST_TIMEOUT_SECS",
                DEFAULT_REQUEST_TIMEOUT_SECS,
            )),
        }
    }
}

pub struct ArchiveState {
    client: Arc<ArchiveClient>,
    limits: ArchiveLimits,
    config: Arc<PipelineConfig>,
    derived_cache: Arc<DerivedCache>,
    bundled_cache: Arc<DerivedCache>,
    index: OnceCell<Arc<ArchiveIndex>>,
    coverage: OnceCell<Arc<Coverage>>,
    #[cfg(test)]
    coverage_scans: std::sync::atomic::AtomicUsize,
    series: Mutex<HashMap<String, Arc<OnceCell<Bytes>>>>,
    years: Mutex<YearCache>,
    /// Each slot holds one parsed year plus its harmonize working set until its stats are
    /// done, so this bounds both CPU use and peak memory.
    load_slots: Arc<Semaphore>,
}

impl ArchiveState {
    pub fn new(dir: PathBuf, limits: ArchiveLimits, config: Arc<PipelineConfig>) -> Self {
        let cpus = std::thread::available_parallelism().map_or(4, |n| n.get());
        Self {
            client: Arc::new(ArchiveClient::offline(dir)),
            limits,
            config,
            derived_cache: Arc::new(DerivedCache::from_env()),
            bundled_cache: Arc::new(DerivedCache::bundled_from_env()),
            index: OnceCell::new(),
            coverage: OnceCell::new(),
            #[cfg(test)]
            coverage_scans: std::sync::atomic::AtomicUsize::new(0),
            series: Mutex::new(HashMap::new()),
            years: Mutex::new(YearCache::default()),
            load_slots: Arc::new(Semaphore::new((cpus / 2).clamp(2, 8))),
        }
    }

    #[cfg(test)]
    pub fn with_derived_cache_dir(mut self, dir: PathBuf) -> Self {
        self.derived_cache = Arc::new(DerivedCache::new(dir));
        self
    }

    #[cfg(test)]
    pub fn with_bundled_cache_dir(mut self, dir: PathBuf) -> Self {
        self.bundled_cache = Arc::new(DerivedCache::read_only(dir));
        self
    }

    #[cfg(test)]
    pub fn coverage_scans(&self) -> usize {
        self.coverage_scans.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn with_year_concurrency(mut self, concurrency: usize) -> Self {
        self.load_slots = Arc::new(Semaphore::new(concurrency.max(1)));
        self
    }

    pub fn dir(&self) -> &Path {
        self.client.cache_dir()
    }

    /// Scans the archive and the sensor coverage ahead of the first request.
    pub async fn warm(&self) {
        if let Err(err) = load_coverage(self).await {
            tracing::warn!(error = %err.message, "could not precompute archive sensor coverage");
        }
    }

    fn admit(&self, region: &RegionInfo) -> Result<(), ApiError> {
        if region.bytes <= self.limits.max_region_bytes {
            return Ok(());
        }
        Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Region too large for this server: ~{} detections / {:.0} MB of archive CSV; limit {:.0} MB (ARCHIVE_MAX_REGION_MB). Run `make precompute` to precompute large regions offline.",
                approx_count(region.bytes / BYTES_PER_ROW_ESTIMATE),
                megabytes(region.bytes),
                megabytes(self.limits.max_region_bytes),
            ),
        ))
    }

    fn admit_year(&self, region: &RegionInfo, year: i32) -> Result<(), ApiError> {
        let year_bytes: u64 = region
            .files
            .iter()
            .filter(|f| archive_stamp_year(f) == Some(year))
            .map(|f| f.bytes)
            .sum();
        if year_bytes <= self.limits.max_region_bytes {
            return Ok(());
        }
        Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Region year {year} too large for this server: ~{} detections / {:.0} MB of archive CSV; limit {:.0} MB (ARCHIVE_MAX_REGION_MB).",
                approx_count(year_bytes / BYTES_PER_ROW_ESTIMATE),
                megabytes(year_bytes),
                megabytes(self.limits.max_region_bytes),
            ),
        ))
    }
}

/// Extracts `<year>` from an [`ArchiveFileStamp::path`] formatted as `<slug>/<year>/<slug>_<year>_<Country>.csv`.
fn archive_stamp_year(stamp: &ArchiveFileStamp) -> Option<i32> {
    stamp.path.split('/').nth(1)?.parse::<i32>().ok()
}

/// Sub-router mounted under `/api` for the offline archive endpoints (`/regions`, `/series`, `/points`).
pub fn api_routes(state: Arc<ArchiveState>) -> Router {
    let timeout = state.limits.request_timeout;
    Router::new()
        .route("/regions", get(regions))
        .route("/series", get(series))
        .route("/points", get(points))
        .route("/density", get(density))
        .layer(middleware::from_fn_with_state(timeout, request_timeout))
        .with_state(state)
}

async fn request_timeout(State(limit): State<Duration>, request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    match tokio::time::timeout(limit, next.run(request)).await {
        Ok(response) => response,
        Err(_) => {
            tracing::warn!(
                path = %path,
                timeout_secs = limit.as_secs_f64(),
                "archive request timed out"
            );
            ApiError::new(
                StatusCode::GATEWAY_TIMEOUT,
                format!(
                    "archive request timed out after {} s (ARCHIVE_REQUEST_TIMEOUT_SECS)",
                    limit.as_secs()
                ),
            )
            .into_response()
        }
    }
}

fn megabytes(bytes: u64) -> f64 {
    bytes as f64 / 1_000_000.0
}

fn approx_count(n: u64) -> String {
    match n {
        1_000_000.. => format!("{:.1}M", n as f64 / 1_000_000.0),
        1_000.. => format!("{}k", n / 1_000),
        _ => n.to_string(),
    }
}

// ---------- errors ----------

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
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

impl From<ArchiveError> for ApiError {
    fn from(err: ArchiveError) -> Self {
        let status = match &err {
            ArchiveError::NotCached { .. }
            | ArchiveError::NotFound { .. }
            | ArchiveError::UnsupportedYear { .. } => StatusCode::NOT_FOUND,
            ArchiveError::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
                StatusCode::NOT_FOUND
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::new(status, err.to_string())
    }
}

fn join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("archive worker failed: {err}"),
    )
}

// ---------- archive index and sensor coverage ----------

#[derive(Clone)]
struct RegionInfo {
    id: String,
    name: String,
    modis_years: BTreeSet<i32>,
    viirs_years: BTreeSet<i32>,
    noaa20_years: BTreeSet<i32>,
    /// Total size of the region's archive CSV files on disk.
    bytes: u64,
    files: Vec<ArchiveFileStamp>,
}

struct ArchiveIndex {
    regions: BTreeMap<String, Arc<RegionInfo>>,
    /// Every archive file per year, across all countries.
    modis_files: BTreeMap<i32, Vec<PathBuf>>,
    viirs_files: BTreeMap<i32, Vec<PathBuf>>,
    noaa20_files: BTreeMap<i32, Vec<PathBuf>>,
}

#[derive(Clone, Copy)]
enum ArchiveSensorKind {
    Modis,
    ViirsSnpp,
    ViirsNoaa20,
}

fn build_index(dir: &Path) -> std::io::Result<ArchiveIndex> {
    let modis_slug = archive_slug(Sensor::ModisTerra);
    let viirs_slug = archive_slug(Sensor::ViirsSNPP);
    let noaa20_slug = archive_slug(Sensor::ViirsNOAA20);
    let mut regions: BTreeMap<String, Arc<RegionInfo>> = BTreeMap::new();
    let mut modis_files: BTreeMap<i32, Vec<PathBuf>> = BTreeMap::new();
    let mut viirs_files: BTreeMap<i32, Vec<PathBuf>> = BTreeMap::new();
    let mut noaa20_files: BTreeMap<i32, Vec<PathBuf>> = BTreeMap::new();

    for file in scan_local_archive(dir)? {
        let slug = Some(file.slug.as_str());
        let kind = if slug == modis_slug {
            ArchiveSensorKind::Modis
        } else if slug == viirs_slug {
            ArchiveSensorKind::ViirsSnpp
        } else if slug == noaa20_slug {
            ArchiveSensorKind::ViirsNoaa20
        } else {
            continue;
        };
        let meta = std::fs::metadata(&file.path)?;
        let bytes = meta.len();
        let stamp_file = ArchiveFileStamp {
            path: format!(
                "{}/{}/{}_{}_{}.csv",
                file.slug, file.year, file.slug, file.year, file.country
            ),
            bytes,
        };
        let region = Arc::make_mut(regions.entry(file.country.clone()).or_insert_with(|| {
            Arc::new(RegionInfo {
                name: file.country.replace('_', " "),
                id: file.country.clone(),
                modis_years: BTreeSet::new(),
                viirs_years: BTreeSet::new(),
                noaa20_years: BTreeSet::new(),
                bytes: 0,
                files: Vec::new(),
            })
        }));
        region.bytes += bytes;
        region.files.push(stamp_file);
        let (years, files) = match kind {
            ArchiveSensorKind::Modis => (&mut region.modis_years, &mut modis_files),
            ArchiveSensorKind::ViirsSnpp => (&mut region.viirs_years, &mut viirs_files),
            ArchiveSensorKind::ViirsNoaa20 => (&mut region.noaa20_years, &mut noaa20_files),
        };
        years.insert(file.year);
        files.entry(file.year).or_default().push(file.path);
    }

    for region in regions.values_mut() {
        Arc::make_mut(region)
            .files
            .sort_by(|a, b| a.path.cmp(&b.path));
    }

    Ok(ArchiveIndex {
        regions,
        modis_files,
        viirs_files,
        noaa20_files,
    })
}

async fn load_index(state: &ArchiveState) -> Result<Arc<ArchiveIndex>, ApiError> {
    state
        .index
        .get_or_try_init(|| async {
            let started = Instant::now();
            let dir = state.dir().to_path_buf();
            let index = tokio::task::spawn_blocking(move || build_index(&dir))
                .await
                .map_err(join_error)?
                .map_err(|err| {
                    ApiError::new(
                        StatusCode::SERVICE_UNAVAILABLE,
                        format!("FIRMS archive is not available: {err}"),
                    )
                })?;
            tracing::info!(
                regions = index.regions.len(),
                dir = %state.dir().display(),
                elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                "scanned FIRMS archive"
            );
            Ok(Arc::new(index))
        })
        .await
        .cloned()
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
struct DateSpan {
    first: NaiveDate,
    last: NaiveDate,
}

impl DateSpan {
    fn covers_year(&self, year: i32) -> bool {
        self.first <= ymd(year, 1, PARTIAL_START_DAY) && self.last >= ymd(year, 12, PARTIAL_END_DAY)
    }

    fn overlaps_month(&self, year: i32, month: u32) -> bool {
        let next = if month == 12 {
            ymd(year + 1, 1, 1)
        } else {
            ymd(year, month + 1, 1)
        };
        self.first < next && self.last >= ymd(year, month, 1)
    }

    fn union(self, other: Self) -> Self {
        Self {
            first: self.first.min(other.first),
            last: self.last.max(other.last),
        }
    }
}

fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid calendar date")
}

/// Observation period of each sensor in the whole archive: the first and last `acq_date` over
/// every row of every country, all fire types included.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq)]
struct Coverage {
    modis: Option<DateSpan>,
    viirs: Option<DateSpan>,
    #[serde(skip)]
    snpp: Option<DateSpan>,
    #[serde(skip)]
    noaa20: Option<DateSpan>,
}

fn scan_coverage(index: &ArchiveIndex) -> std::io::Result<Coverage> {
    let modis = sensor_span(&index.modis_files)?;
    let snpp = sensor_span(&index.viirs_files)?;
    let noaa20 = sensor_span(&index.noaa20_files)?;
    let viirs = match (snpp, noaa20) {
        (Some(a), Some(b)) => Some(a.union(b)),
        (a, b) => a.or(b),
    };
    Ok(Coverage {
        modis,
        viirs,
        snpp,
        noaa20,
    })
}

/// Files are filed by acquisition year, so the extremes over all rows sit in the first and
/// last year that has any row; only those years are read.
fn sensor_span(files: &BTreeMap<i32, Vec<PathBuf>>) -> std::io::Result<Option<DateSpan>> {
    let mut first = None;
    for paths in files.values() {
        if let Some(span) = files_date_span(paths)? {
            first = Some(span.first);
            break;
        }
    }
    let mut last = None;
    for paths in files.values().rev() {
        if let Some(span) = files_date_span(paths)? {
            last = Some(span.last);
            break;
        }
    }
    Ok(first.zip(last).map(|(first, last)| DateSpan { first, last }))
}

fn files_date_span(paths: &[PathBuf]) -> std::io::Result<Option<DateSpan>> {
    let mut span: Option<DateSpan> = None;
    for path in paths {
        if let Some(file_span) = file_date_span(path)? {
            span = Some(span.map_or(file_span, |s| s.union(file_span)));
        }
    }
    Ok(span)
}

/// Streams one CSV and returns the min/max of its `acq_date` column without parsing rows.
fn file_date_span(path: &Path) -> std::io::Result<Option<DateSpan>> {
    let mut reader = std::io::BufReader::with_capacity(1 << 16, std::fs::File::open(path)?);
    let mut line = Vec::new();
    if reader.read_until(b'\n', &mut line)? == 0 {
        return Ok(None);
    }
    let Some(col) = line
        .trim_ascii_end()
        .split(|&b| b == b',')
        .position(|h| h == b"acq_date")
    else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} has no acq_date column", path.display()),
        ));
    };

    // ISO dates order the same as strings, so the raw bytes are compared directly.
    let mut range: Option<([u8; 10], [u8; 10])> = None;
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        let Some(date) = line
            .trim_ascii_end()
            .split(|&b| b == b',')
            .nth(col)
            .and_then(|field| <[u8; 10]>::try_from(field).ok())
        else {
            continue;
        };
        range = Some(match range {
            None => (date, date),
            Some((min, max)) => (min.min(date), max.max(date)),
        });
    }

    let parse = |raw: [u8; 10]| {
        std::str::from_utf8(&raw)
            .ok()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
    };
    Ok(range.and_then(|(min, max)| {
        Some(DateSpan {
            first: parse(min)?,
            last: parse(max)?,
        })
    }))
}

/// Persistent disk representation of [`Coverage`], preserving `snpp` and `noaa20` which are
/// skipped in `/api/series` JSON responses.
#[derive(Serialize, Deserialize)]
struct CachedCoverage {
    modis: Option<DateSpan>,
    viirs: Option<DateSpan>,
    snpp: Option<DateSpan>,
    noaa20: Option<DateSpan>,
}

impl From<&Coverage> for CachedCoverage {
    fn from(c: &Coverage) -> Self {
        Self {
            modis: c.modis,
            viirs: c.viirs,
            snpp: c.snpp,
            noaa20: c.noaa20,
        }
    }
}

impl From<CachedCoverage> for Coverage {
    fn from(c: CachedCoverage) -> Self {
        Self {
            modis: c.modis,
            viirs: c.viirs,
            snpp: c.snpp,
            noaa20: c.noaa20,
        }
    }
}

fn coverage_stamp(index: &ArchiveIndex) -> CoverageStamp {
    CoverageStamp::from_files(index.regions.values().flat_map(|r| r.files.iter()))
}

fn serialize_coverage(coverage: &Coverage) -> Vec<u8> {
    serde_json::to_vec_pretty(&CachedCoverage::from(coverage))
        .expect("Coverage serialization cannot fail")
}

fn deserialize_coverage(bytes: &[u8], source: &str) -> Option<Coverage> {
    match serde_json::from_slice::<CachedCoverage>(bytes) {
        Ok(cached) => Some(Coverage::from(cached)),
        Err(err) => {
            tracing::warn!(
                source,
                error = %err,
                "corrupt derived coverage JSON; ignoring cache"
            );
            None
        }
    }
}

async fn load_coverage(state: &ArchiveState) -> Result<Arc<Coverage>, ApiError> {
    let index = load_index(state).await?;
    state
        .coverage
        .get_or_try_init(|| async move {
            let started = Instant::now();
            let stamp = coverage_stamp(&index);
            let cache = state.derived_cache.clone();
            let bundled = state.bundled_cache.clone();
            let check_stamp = stamp.clone();

            if let Some((coverage, source)) = tokio::task::spawn_blocking(move || {
                if let Some(bytes) = cache.read_coverage(&check_stamp)
                    && let Some(coverage) = deserialize_coverage(&bytes, "disk")
                {
                    return Some((coverage, "disk"));
                }
                if let Some(bytes) = bundled.read_coverage(&check_stamp)
                    && let Some(coverage) = deserialize_coverage(&bytes, "bundled")
                {
                    return Some((coverage, "bundled"));
                }
                None
            })
            .await
            .map_err(join_error)?
            {
                tracing::info!(
                    ?coverage,
                    source,
                    elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                    "loaded sensor coverage from cache"
                );
                return Ok(Arc::new(coverage));
            }

            #[cfg(test)]
            state
                .coverage_scans
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            let cache = state.derived_cache.clone();
            let coverage = tokio::task::spawn_blocking(move || {
                let coverage = scan_coverage(&index)?;
                let bytes = serialize_coverage(&coverage);
                let _ = cache.write_coverage(&stamp, &bytes);
                Ok::<_, std::io::Error>(coverage)
            })
            .await
            .map_err(join_error)?
            .map_err(|err| {
                ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to read sensor coverage from the archive: {err}"),
                )
            })?;
            tracing::info!(
                ?coverage,
                elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                "computed sensor coverage from the archive"
            );
            Ok(Arc::new(coverage))
        })
        .await
        .cloned()
}

// ---------- regions ----------

#[derive(Serialize)]
struct RegionsResponse {
    source: &'static str,
    max_region_mb: f64,
    regions: Vec<RegionEntry>,
}

#[derive(Serialize)]
struct RegionEntry {
    id: String,
    name: String,
    modis_years: Option<[i32; 2]>,
    viirs_years: Option<[i32; 2]>,
    noaa20_years: Option<[i32; 2]>,
    size_mb: f64,
    too_large: bool,
    precomputed: bool,
}

fn year_span(years: &BTreeSet<i32>) -> Option<[i32; 2]> {
    Some([*years.first()?, *years.last()?])
}

fn round_mb(bytes: u64) -> f64 {
    (megabytes(bytes) * 10.0).round() / 10.0
}

async fn find_region(state: &ArchiveState, region: Option<&str>) -> Result<Arc<RegionInfo>, ApiError> {
    let id = region
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "missing 'region' parameter"))?;
    let index = load_index(state).await?;
    index.regions.get(id).cloned().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            format!("unknown region '{id}'; see /api/regions"),
        )
    })
}

async fn regions(State(state): State<Arc<ArchiveState>>) -> Result<Json<RegionsResponse>, ApiError> {
    let index = load_index(&state).await?;
    let in_memory: HashSet<String> = {
        let series = state.series.lock().expect("series cache lock");
        series
            .iter()
            .filter(|(_, cell)| cell.initialized())
            .map(|(id, _)| id.clone())
            .collect()
    };
    let cache = state.derived_cache.clone();
    let bundled = state.bundled_cache.clone();
    let config = state.config.clone();
    let max_region_bytes = state.limits.max_region_bytes;
    let entries = tokio::task::spawn_blocking(move || {
        let mut entries: Vec<RegionEntry> = index
            .regions
            .values()
            .map(|r| {
                let stamp = SeriesStamp::for_region(&config, &r.files);
                let precomputed = cache.has_valid_series(&r.id, &stamp)
                    || bundled.has_valid_series(&r.id, &stamp);
                let series_available = precomputed || in_memory.contains(&r.id);
                RegionEntry {
                    id: r.id.clone(),
                    name: r.name.clone(),
                    modis_years: year_span(&r.modis_years),
                    viirs_years: year_span(&r.viirs_years),
                    noaa20_years: year_span(&r.noaa20_years),
                    size_mb: round_mb(r.bytes),
                    too_large: r.bytes > max_region_bytes && !series_available,
                    precomputed,
                }
            })
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    })
    .await
    .map_err(join_error)?;

    Ok(Json(RegionsResponse {
        source: "FIRMS archive country files",
        max_region_mb: round_mb(state.limits.max_region_bytes),
        regions: entries,
    }))
}

// ---------- year data and cache ----------

struct YearData {
    modis: Vec<Hotspot>,
    viirs: Vec<Hotspot>,
}

impl YearData {
    fn rows(&self) -> usize {
        self.modis.len() + self.viirs.len()
    }
}

#[derive(Default)]
struct YearCache {
    entries: HashMap<(String, i32), (Arc<YearData>, u64)>,
    rows: usize,
    tick: u64,
}

impl YearCache {
    fn get(&mut self, country: &str, year: i32) -> Option<Arc<YearData>> {
        self.tick += 1;
        let tick = self.tick;
        let entry = self.entries.get_mut(&(country.to_string(), year))?;
        entry.1 = tick;
        Some(entry.0.clone())
    }

    fn insert(&mut self, country: &str, year: i32, data: Arc<YearData>) {
        let rows = data.rows();
        if rows > YEAR_CACHE_MAX_ROWS {
            return;
        }
        self.tick += 1;
        if let Some((old, _)) = self
            .entries
            .insert((country.to_string(), year), (data, self.tick))
        {
            self.rows -= old.rows();
        }
        self.rows += rows;
        while self.rows > YEAR_CACHE_MAX_ROWS {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some((evicted, _)) = self.entries.remove(&oldest) {
                self.rows -= evicted.rows();
            }
        }
    }
}

async fn load_sensor_year(
    client: Arc<ArchiveClient>,
    sensor: Sensor,
    country: String,
    year: i32,
) -> Result<Vec<Hotspot>, ApiError> {
    let csv = client.fetch_country_year_csv(sensor, &country, year).await?;
    tokio::task::spawn_blocking(move || {
        let source = FirmsSource::new(sensor, Processing::Sp);
        let mut rows = parse_csv(&csv, source.as_str()).map_err(|err| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to parse {} {year} {country}: {err:#}", source.as_str()),
            )
        })?;
        drop(csv);
        let unfiltered = rows.len();
        retain_wildfires(&mut rows);
        tracing::debug!(
            country = %country,
            year,
            source = source.as_str(),
            rows = unfiltered,
            type0 = rows.len(),
            "parsed archive file"
        );
        Ok(rows)
    })
    .await
    .map_err(join_error)?
}

async fn read_year(client: &Arc<ArchiveClient>, region: &RegionInfo, year: i32) -> Result<YearData, ApiError> {
    let modis = async {
        if region.modis_years.contains(&year) {
            load_sensor_year(client.clone(), Sensor::ModisTerra, region.id.clone(), year).await
        } else {
            Ok(Vec::new())
        }
    };
    let snpp = async {
        if region.viirs_years.contains(&year) {
            load_sensor_year(client.clone(), Sensor::ViirsSNPP, region.id.clone(), year).await
        } else {
            Ok(Vec::new())
        }
    };
    let noaa20 = async {
        if region.noaa20_years.contains(&year) {
            load_sensor_year(client.clone(), Sensor::ViirsNOAA20, region.id.clone(), year).await
        } else {
            Ok(Vec::new())
        }
    };
    let (modis, mut viirs, mut noaa20) = tokio::try_join!(modis, snpp, noaa20)?;
    viirs.append(&mut noaa20);
    Ok(YearData { modis, viirs })
}

/// Cached year for /api/points. The caller must hold a `load_slots` permit.
async fn load_year(state: &ArchiveState, region: &RegionInfo, year: i32) -> Result<Arc<YearData>, ApiError> {
    if let Some(cached) = state.years.lock().expect("year cache lock").get(&region.id, year) {
        return Ok(cached);
    }
    let data = Arc::new(read_year(&state.client, region, year).await?);
    state
        .years
        .lock()
        .expect("year cache lock")
        .insert(&region.id, year, data.clone());
    Ok(data)
}

// ---------- series ----------

#[derive(Serialize)]
struct SeriesFilters {
    fire_type: [u8; 1],
    confidence: &'static str,
    area: String,
    sensors: [&'static str; 2],
}

#[derive(Serialize)]
struct YearEntry {
    year: i32,
    raw_modis: usize,
    raw_viirs: usize,
    raw_total: usize,
    harmonized: usize,
    matched: usize,
    viirs_aggregated: usize,
    modis_only: usize,
    viirs_only: usize,
    static_masked: usize,
    sama_removed: usize,
    interscan_removed: usize,
    fleet_merged: usize,
    first_date: Option<NaiveDate>,
    last_date: Option<NaiveDate>,
    /// A sensor contributing to this year did not observe all of it (see [`DateSpan::covers_year`]).
    partial: bool,
    modis_first_date: Option<NaiveDate>,
    modis_last_date: Option<NaiveDate>,
    viirs_first_date: Option<NaiveDate>,
    viirs_last_date: Option<NaiveDate>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(Deserialize))]
struct MonthEntry {
    year: i32,
    month: u32,
    raw_modis: usize,
    raw_viirs: usize,
    raw_total: usize,
    harmonized: usize,
    mean_frp: Option<f64>,
    mean_frp_lower: Option<f64>,
    mean_frp_upper: Option<f64>,
    anomaly_raw: Option<f64>,
    anomaly_harmonized: Option<f64>,
}

#[derive(Serialize)]
struct Cliff {
    from_year: i32,
    to_year: i32,
    raw_from: usize,
    raw_to: usize,
    raw_change_pct: Option<f64>,
    harmonized_from: usize,
    harmonized_to: usize,
    harmonized_change_pct: Option<f64>,
}

#[derive(Serialize)]
struct SeriesResponse {
    region: String,
    filters: SeriesFilters,
    coverage: Coverage,
    years: Vec<YearEntry>,
    months: Vec<MonthEntry>,
    cliff: Option<Cliff>,
}

struct YearStats {
    entry: YearEntry,
    months: Vec<MonthEntry>,
}

fn date_span(rows: &[Hotspot]) -> Option<(NaiveDate, NaiveDate)> {
    let first = rows.iter().map(|h| h.acquired).min()?.date_naive();
    let last = rows.iter().map(|h| h.acquired).max()?.date_naive();
    Some((first, last))
}

fn in_month(rows: &[Hotspot], month: u32) -> Vec<Hotspot> {
    rows.iter()
        .filter(|h| h.acquired.month() == month)
        .cloned()
        .collect()
}

/// `observed` holds the archive coverage of every sensor that has a file for this year.
#[cfg(test)]
fn year_stats(year: i32, data: YearData, observed: &[DateSpan]) -> YearStats {
    year_stats_with(year, data, observed, &PipelineConfig::default())
}

fn year_stats_with(
    year: i32,
    data: YearData,
    observed: &[DateSpan],
    config: &PipelineConfig,
) -> YearStats {
    // The static mask is judged on the whole year and shared with the monthly runs, which alone
    // could never reach its month threshold.
    let mask = config
        .static_mask
        .enabled
        .then(|| StaticMask::from_detections(data.modis.iter().chain(&data.viirs), &config.static_mask));
    // Every month a contributing sensor observed gets an entry, zero fires included, so quiet
    // months count in the anomaly baselines; months outside coverage are left out. A month with
    // rows is always kept, so the raw monthly counts add up to the yearly totals.
    let months: Vec<MonthEntry> = (1..=12)
        .filter_map(|month| {
            let modis = in_month(&data.modis, month);
            let viirs = in_month(&data.viirs, month);
            let covered = observed.iter().any(|s| s.overlaps_month(year, month));
            if !covered && modis.is_empty() && viirs.is_empty() {
                return None;
            }
            let (raw_modis, raw_viirs) = (modis.len(), viirs.len());
            // Each month is harmonized on its own, so matches and interscan duplicates
            // that straddle a month boundary are not merged; the monthly harmonized counts
            // need not add up to the yearly figure, which is harmonized over the whole year.
            let harmonized_hotspots =
                harmonize_with_links_masked(modis, viirs, config, mask.as_ref()).hotspots;
            let (mean_frp, mean_frp_lower, mean_frp_upper) =
                match mean_frp_with_uncertainty(&harmonized_hotspots, &config.uncertainty) {
                    Some((mean, lower, upper)) => (Some(mean), Some(lower), Some(upper)),
                    None => (None, None, None),
                };
            Some(MonthEntry {
                year,
                month,
                raw_modis,
                raw_viirs,
                raw_total: raw_modis + raw_viirs,
                harmonized: harmonized_hotspots.len(),
                mean_frp,
                mean_frp_lower,
                mean_frp_upper,
                anomaly_raw: None,
                anomaly_harmonized: None,
            })
        })
        .collect();

    let modis_span = date_span(&data.modis);
    let viirs_span = date_span(&data.viirs);
    let (raw_modis, raw_viirs) = (data.modis.len(), data.viirs.len());
    let linked = harmonize_with_links_masked(data.modis, data.viirs, config, mask.as_ref());
    let count_source = |source: HotspotSource| {
        linked
            .hotspots
            .iter()
            .filter(|h| h.source == source)
            .count()
    };

    let entry = YearEntry {
        year,
        raw_modis,
        raw_viirs,
        raw_total: raw_modis + raw_viirs,
        harmonized: linked.hotspots.len(),
        matched: count_source(HotspotSource::Matched),
        viirs_aggregated: linked.viirs_aggregated,
        modis_only: count_source(HotspotSource::ModisOnly),
        viirs_only: count_source(HotspotSource::ViirsOnly),
        static_masked: linked.static_masked,
        sama_removed: linked.sama_removed,
        interscan_removed: linked.interscan_removed,
        fleet_merged: linked.fleet_merged,
        first_date: [modis_span, viirs_span].iter().flatten().map(|s| s.0).min(),
        last_date: [modis_span, viirs_span].iter().flatten().map(|s| s.1).max(),
        partial: observed.iter().any(|s| !s.covers_year(year)),
        modis_first_date: modis_span.map(|s| s.0),
        modis_last_date: modis_span.map(|s| s.1),
        viirs_first_date: viirs_span.map(|s| s.0),
        viirs_last_date: viirs_span.map(|s| s.1),
    };

    YearStats { entry, months }
}

fn anomalies(months: &[MonthEntry], count: impl Fn(&MonthEntry) -> usize) -> Vec<Option<f64>> {
    let mut cells: Vec<CalendarCell> = months
        .iter()
        .map(|m| CalendarCell {
            year: m.year,
            month: m.month,
            count: count(m) as u64,
            count_normalized: Some(count(m) as f64),
            effort_omega: None,
            mean_frp: 0.0,
            mean_frp_raw: 0.0,
            mean_frp_lower: None,
            mean_frp_upper: None,
            total_frp: 0.0,
            total_frp_lower: None,
            total_frp_upper: None,
            frp_per_detection_eq: None,
            anomaly: None,
            sfai: None,
            quality_flags: Vec::new(),
            conformal: None,
        })
        .collect();
    compute_anomalies(&mut cells);
    cells.into_iter().map(|c| c.anomaly).collect()
}

fn change_pct(from: usize, to: usize) -> Option<f64> {
    (from > 0).then(|| (to as f64 - from as f64) / from as f64 * 100.0)
}

fn region_years(region: &RegionInfo) -> BTreeSet<i32> {
    region
        .modis_years
        .union(&region.viirs_years)
        .copied()
        .chain(region.noaa20_years.iter().copied())
        .collect()
}

async fn build_series(
    state: Arc<ArchiveState>,
    region: Arc<RegionInfo>,
    coverage: Arc<Coverage>,
) -> Result<SeriesResponse, ApiError> {
    let years = region_years(&region);

    let mut tasks = JoinSet::new();
    for year in years {
        let state = state.clone();
        let region = region.clone();
        let coverage = coverage.clone();
        let config = state.config.clone();
        tasks.spawn(async move {
            let slot = state
                .load_slots
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore is never closed");
            // Read straight from disk rather than through the year cache, so the rows can be
            // moved into the pipeline and nothing stays resident after the series is built.
            let data = read_year(&state.client, &region, year).await?;
            let observed: Vec<DateSpan> = [
                (region.modis_years.contains(&year), coverage.modis),
                (region.viirs_years.contains(&year), coverage.snpp),
                (region.noaa20_years.contains(&year), coverage.noaa20),
            ]
            .into_iter()
            .filter_map(|(contributes, span)| span.filter(|_| contributes))
            .collect();
            tokio::task::spawn_blocking(move || {
                let _slot = slot;
                year_stats_with(year, data, &observed, &config)
            })
            .await
            .map_err(join_error)
        });
    }

    let mut stats: BTreeMap<i32, YearStats> = BTreeMap::new();
    while let Some(joined) = tasks.join_next().await {
        let stat = joined.map_err(join_error)??;
        stats.insert(stat.entry.year, stat);
    }

    let mut year_entries = Vec::with_capacity(stats.len());
    let mut months = Vec::new();
    for (_, stat) in stats {
        year_entries.push(stat.entry);
        months.extend(stat.months);
    }

    let raw_anomaly = anomalies(&months, |m| m.raw_total);
    let harmonized_anomaly = anomalies(&months, |m| m.harmonized);
    for (i, month) in months.iter_mut().enumerate() {
        month.anomaly_raw = raw_anomaly[i];
        month.anomaly_harmonized = harmonized_anomaly[i];
    }

    let cliff = region
        .viirs_years
        .iter()
        .chain(&region.noaa20_years)
        .min()
        .copied()
        .and_then(|to_year| {
            let from = year_entries.iter().find(|y| y.year == to_year - 1)?;
            let to = year_entries.iter().find(|y| y.year == to_year)?;
            Some(Cliff {
                from_year: from.year,
                to_year: to.year,
                raw_from: from.raw_total,
                raw_to: to.raw_total,
                raw_change_pct: change_pct(from.raw_total, to.raw_total),
                harmonized_from: from.harmonized,
                harmonized_to: to.harmonized,
                harmonized_change_pct: change_pct(from.harmonized, to.harmonized),
            })
        });

    Ok(SeriesResponse {
        region: region.id.clone(),
        filters: SeriesFilters {
            fire_type: [0],
            confidence: "all",
            area: format!("FIRMS country archive file: {}", region.id),
            sensors: ["MODIS Terra+Aqua (archive)", "VIIRS S-NPP+NOAA-20 (archive)"],
        },
        coverage: *coverage,
        years: year_entries,
        months,
        cliff,
    })
}

#[derive(Deserialize)]
struct SeriesParams {
    region: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SeriesSource {
    Memory,
    Disk,
    Bundled,
    Computed,
}

impl SeriesSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Disk => "disk",
            Self::Bundled => "bundled",
            Self::Computed => "computed",
        }
    }
}

fn serialize_series(response: &SeriesResponse) -> Result<Bytes, ApiError> {
    serde_json::to_vec(response).map(Bytes::from).map_err(|err| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to serialize /api/series response: {err}"),
        )
    })
}

async fn series(
    State(state): State<Arc<ArchiveState>>,
    Query(params): Query<SeriesParams>,
) -> Result<Response, ApiError> {
    let started = Instant::now();
    let region = find_region(&state, params.region.as_deref()).await?;
    let cell = state
        .series
        .lock()
        .expect("series cache lock")
        .entry(region.id.clone())
        .or_default()
        .clone();

    let mut source = SeriesSource::Memory;
    let body = cell
        .get_or_try_init(|| async {
            let stamp = SeriesStamp::for_region(&state.config, &region.files);
            let cache = state.derived_cache.clone();
            let bundled = state.bundled_cache.clone();
            let region_id = region.id.clone();
            let check_stamp = stamp.clone();
            if let Some((bytes, hit_source)) = tokio::task::spawn_blocking(move || {
                if let Some(bytes) = cache.read_series(&region_id, &check_stamp) {
                    Some((bytes, SeriesSource::Disk))
                } else {
                    bundled
                        .read_series(&region_id, &check_stamp)
                        .map(|bytes| (bytes, SeriesSource::Bundled))
                }
            })
            .await
            .map_err(join_error)?
            {
                source = hit_source;
                return Ok::<_, ApiError>(bytes);
            }

            state.admit(&region)?;
            let coverage = load_coverage(&state).await?;
            let response = build_series(state.clone(), region.clone(), coverage).await?;
            let bytes = serialize_series(&response)?;

            let cache = state.derived_cache.clone();
            let region_id = region.id.clone();
            let bytes_for_disk = bytes.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let _ = cache.write_series(&region_id, &stamp, &bytes_for_disk);
            })
            .await;

            source = SeriesSource::Computed;
            Ok(bytes)
        })
        .await?
        .clone();

    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    match source {
        SeriesSource::Computed => tracing::info!(
            region = %region.id,
            elapsed_ms,
            "computed /api/series (archive read + pipeline)"
        ),
        SeriesSource::Disk => tracing::info!(
            region = %region.id,
            elapsed_ms,
            "served /api/series from disk cache"
        ),
        SeriesSource::Bundled => tracing::info!(
            region = %region.id,
            elapsed_ms,
            "served /api/series from bundled cache"
        ),
        SeriesSource::Memory => tracing::debug!(
            region = %region.id,
            elapsed_ms,
            "served /api/series from memory cache"
        ),
    }

    let mut response = (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        body,
    )
        .into_response();
    response.headers_mut().insert(
        X_SERIES_SOURCE_HEADER,
        HeaderValue::from_static(source.as_str()),
    );
    Ok(response)
}

// ---------- precompute ----------

#[derive(Debug, Clone)]
pub struct PrecomputeOptions {
    pub regions: Vec<String>,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrecomputeOutcome {
    Computed,
    Skipped,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct PrecomputeRegionResult {
    pub outcome: PrecomputeOutcome,
}

#[derive(Debug, Clone)]
pub struct PrecomputeReport {
    pub results: Vec<PrecomputeRegionResult>,
    pub total_elapsed: Duration,
}

impl PrecomputeReport {
    pub fn computed(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r.outcome, PrecomputeOutcome::Computed))
            .count()
    }

    pub fn skipped(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r.outcome, PrecomputeOutcome::Skipped))
            .count()
    }

    pub fn failed(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r.outcome, PrecomputeOutcome::Failed(_)))
            .count()
    }
}

async fn precompute_one_region(
    state: &Arc<ArchiveState>,
    region: &Arc<RegionInfo>,
    coverage: &Arc<Coverage>,
    force: bool,
) -> Result<(PrecomputeOutcome, Option<&'static str>), String> {
    let stamp = SeriesStamp::for_region(&state.config, &region.files);
    if !force {
        let cache = state.derived_cache.clone();
        let bundled = state.bundled_cache.clone();
        let region_id = region.id.clone();
        let check_stamp = stamp.clone();
        let hit = tokio::task::spawn_blocking(move || {
            if cache.read_series(&region_id, &check_stamp).is_some() {
                Some("disk")
            } else if bundled.read_series(&region_id, &check_stamp).is_some() {
                Some("bundled")
            } else {
                None
            }
        })
        .await
        .map_err(|err| format!("cache check task failed: {err}"))?;
        if let Some(layer) = hit {
            return Ok((PrecomputeOutcome::Skipped, Some(layer)));
        }
    }

    let series_resp = build_series(state.clone(), region.clone(), coverage.clone())
        .await
        .map_err(|err| err.message)?;
    let bytes = serialize_series(&series_resp).map_err(|err| err.message)?;

    let cache = state.derived_cache.clone();
    let region_id = region.id.clone();
    tokio::task::spawn_blocking(move || cache.write_series(&region_id, &stamp, &bytes))
        .await
        .map_err(|err| format!("cache write task failed: {err}"))?
        .map_err(|err| format!("failed to write derived cache: {err}"))?;

    Ok((PrecomputeOutcome::Computed, None))
}

/// Precomputes `/api/series` responses into the disk cache for the selected regions
/// (or all indexed regions when `opts.regions` is empty), ignoring `ARCHIVE_MAX_REGION_MB`.
pub async fn precompute_series(
    state: Arc<ArchiveState>,
    opts: &PrecomputeOptions,
) -> Result<PrecomputeReport, ApiError> {
    let total_started = Instant::now();
    let index = load_index(&state).await?;
    let coverage = if opts.force {
        let started = Instant::now();
        #[cfg(test)]
        state
            .coverage_scans
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let stamp = coverage_stamp(&index);
        let scan_index = index.clone();
        let cache = state.derived_cache.clone();
        let coverage = tokio::task::spawn_blocking(move || {
            let coverage = scan_coverage(&scan_index).map_err(|err| {
                ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to read sensor coverage from the archive: {err}"),
                )
            })?;
            let bytes = serialize_coverage(&coverage);
            cache.write_coverage(&stamp, &bytes).map_err(|err| {
                ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to write derived coverage cache: {err}"),
                )
            })?;
            Ok::<_, ApiError>(Arc::new(coverage))
        })
        .await
        .map_err(join_error)??;
        let _ = state.coverage.set(coverage.clone());
        tracing::info!(
            ?coverage,
            elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
            "computed sensor coverage from the archive"
        );
        coverage
    } else {
        load_coverage(&state).await?
    };

    let targets: Vec<String> = if opts.regions.is_empty() {
        index.regions.keys().cloned().collect()
    } else {
        let mut seen = BTreeSet::new();
        opts.regions
            .iter()
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .collect()
    };

    let total = targets.len();
    let mut results = Vec::with_capacity(total);

    // Process regions sequentially to bound peak memory (large countries hold millions of rows per year).
    for (idx, region_id) in targets.into_iter().enumerate() {
        let pos = format!("{}/{total}", idx + 1);
        let region_started = Instant::now();
        let Some(region) = index.regions.get(&region_id) else {
            let err = format!("unknown region '{region_id}'");
            tracing::error!(
                target: "pyrosync::precompute",
                progress = %pos,
                region = %region_id,
                years = 0,
                elapsed_s = 0.0,
                outcome = "failed",
                error = %err,
                "precompute region"
            );
            results.push(PrecomputeRegionResult {
                outcome: PrecomputeOutcome::Failed(err),
            });
            continue;
        };

        let years = region_years(region).len();
        let (outcome, skipped_from) =
            match precompute_one_region(&state, region, &coverage, opts.force).await {
                Ok(res) => res,
                Err(err) => (PrecomputeOutcome::Failed(err), None),
            };
        let elapsed = region_started.elapsed();
        let elapsed_s = (elapsed.as_secs_f64() * 100.0).round() / 100.0;

        match &outcome {
            PrecomputeOutcome::Computed => tracing::info!(
                target: "pyrosync::precompute",
                progress = %pos,
                region = %region.id,
                years,
                elapsed_s,
                outcome = "computed",
                "precompute region"
            ),
            PrecomputeOutcome::Skipped => tracing::info!(
                target: "pyrosync::precompute",
                progress = %pos,
                region = %region.id,
                years,
                elapsed_s,
                outcome = "skipped",
                source = skipped_from.unwrap_or("unknown"),
                "precompute region"
            ),
            PrecomputeOutcome::Failed(err) => tracing::error!(
                target: "pyrosync::precompute",
                progress = %pos,
                region = %region.id,
                years,
                elapsed_s,
                outcome = "failed",
                error = %err,
                "precompute region"
            ),
        }

        results.push(PrecomputeRegionResult { outcome });
    }

    let report = PrecomputeReport {
        results,
        total_elapsed: total_started.elapsed(),
    };
    let total_s = (report.total_elapsed.as_secs_f64() * 100.0).round() / 100.0;
    tracing::info!(
        target: "pyrosync::precompute",
        total,
        computed = report.computed(),
        skipped = report.skipped(),
        failed = report.failed(),
        elapsed_s = total_s,
        "precompute summary"
    );

    Ok(report)
}

// ---------- points ----------

#[derive(Deserialize, Default, Clone)]
struct PointsParams {
    region: Option<String>,
    year: Option<String>,
    month: Option<String>,
    limit: Option<String>,
    dedup_time_secs: Option<String>,
    dedup_overlap: Option<String>,
    match_radius_km: Option<String>,
    match_window_secs: Option<String>,
    fleet_radius_km: Option<String>,
    min_confidence: Option<String>,
    fusion_model: Option<String>,
}

#[derive(Serialize)]
struct RawPoint {
    id: String,
    lat: f64,
    lon: f64,
    sensor: &'static str,
    frp: f64,
    confidence: Confidence,
    acquired: chrono::DateTime<chrono::Utc>,
    daynight: DayNight,
    harmonized_id: Option<String>,
}

impl RawPoint {
    /// `id` and `harmonized_id` are filled in once the row is known to be kept.
    fn from_hotspot(h: &Hotspot, sensor: &'static str) -> Self {
        Self {
            id: String::new(),
            lat: h.lat,
            lon: h.lon,
            sensor,
            frp: h.frp,
            confidence: h.confidence,
            acquired: h.acquired,
            daynight: h.daynight,
            harmonized_id: None,
        }
    }
}

#[derive(Serialize)]
struct HarmonizedPoint {
    id: String,
    /// Fire event the detection belongs to; `None` for watch points.
    event_id: Option<usize>,
    #[serde(flatten)]
    hotspot: HarmonizedHotspot,
}

#[derive(Serialize)]
struct FusionComparison {
    model: FusionModel,
    baseline_count: usize,
    baseline_total_frp: f64,
    count: usize,
    total_frp: f64,
    dropped_below_modis_limit: usize,
}

fn total_frp(hotspots: &[HarmonizedHotspot]) -> f64 {
    hotspots
        .iter()
        .map(|h| h.frp_corrected)
        .filter(|v| v.is_finite())
        .sum()
}

#[derive(Serialize)]
struct Stages {
    raw_modis: usize,
    raw_viirs: usize,
    static_masked: usize,
    sama_removed: usize,
    interscan_removed: usize,
    fleet_merged: usize,
    matched_pairs: usize,
    viirs_aggregated: usize,
    harmonized: usize,
}

#[derive(Serialize, Debug, PartialEq)]
struct Extent {
    west: f64,
    south: f64,
    east: f64,
    north: f64,
}

#[derive(Serialize)]
struct EventParamsMeta {
    eps_km: f64,
    min_samples: usize,
    max_day_gap: i64,
}

/// Events describe the FULL harmonized set of the requested period, not the sampled subset
/// returned in `harmonized`.
#[derive(Serialize)]
struct EventsMeta {
    params: EventParamsMeta,
    count: usize,
    watch_points: usize,
    items: Vec<FireEvent>,
    items_truncated: bool,
}

const MAX_EVENT_ITEMS: usize = 1000;

fn events_meta(clustering: &EventClustering, params: &EventParams) -> EventsMeta {
    let count = clustering.events.len();
    let mut items = clustering.events.clone();
    let items_truncated = count > MAX_EVENT_ITEMS;
    if items_truncated {
        items.sort_by(|a, b| b.detections.cmp(&a.detections).then(a.id.cmp(&b.id)));
        items.truncate(MAX_EVENT_ITEMS);
        items.sort_by_key(|e| e.id);
    }
    EventsMeta {
        params: EventParamsMeta {
            eps_km: params.eps_km,
            min_samples: params.min_samples,
            max_day_gap: params.max_day_gap,
        },
        count,
        watch_points: clustering.watch_points,
        items,
        items_truncated,
    }
}

fn month_bounds(year: i32, month: Option<u32>) -> (NaiveDate, NaiveDate) {
    match month {
        Some(m) => {
            let first = ymd(year, m, 1);
            let next = if m == 12 { ymd(year + 1, 1, 1) } else { ymd(year, m + 1, 1) };
            (first, next.pred_opt().expect("date has a predecessor"))
        }
        None => (ymd(year, 1, 1), ymd(year, 12, 31)),
    }
}

type DayRange = Option<(NaiveDate, NaiveDate)>;

/// Per-family archive coverage for this region and year. A sensor counts only when the region
/// has a file for the year; its date span is the global archive span of that sensor (region
/// files carry no first/last dates of their own). VIIRS is the hull of SNPP and NOAA-20.
fn family_coverage(region: &RegionInfo, year: i32, coverage: &Coverage) -> (DayRange, DayRange) {
    let span = |has: bool, s: Option<DateSpan>| s.filter(|_| has).map(|s| (s.first, s.last));
    let modis = span(region.modis_years.contains(&year), coverage.modis);
    let viirs = match (
        span(region.viirs_years.contains(&year), coverage.snpp),
        span(region.noaa20_years.contains(&year), coverage.noaa20),
    ) {
        (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.max(b.1))),
        (a, b) => a.or(b),
    };
    (modis, viirs)
}

#[derive(Serialize)]
struct PointsResponse {
    region: String,
    year: i32,
    month: Option<u32>,
    truncated: bool,
    limit: usize,
    /// 1 when complete; otherwise every `sample_every`-th harmonized output is kept.
    sample_every: usize,
    raw: Vec<RawPoint>,
    harmonized: Vec<HarmonizedPoint>,
    stages: Stages,
    bbox: Option<Extent>,
    filters: ConfidenceFilter,
    /// Effort metadata for the region. The `/api/series` body is precomputed without any
    /// geometry, so the flag is computed here at serve time from the year's detection extent.
    tropical_swath_gap: Option<TropicalSwathGap>,
    events: EventsMeta,
    no_detection_days: Vec<NoDetectionDay>,
    config: PipelineConfig,
    config_overridden: bool,
    fusion_model: FusionModel,
    fusion_comparison: Option<FusionComparison>,
}

fn parse_param<T: std::str::FromStr>(
    name: &str,
    value: Option<&str>,
) -> Result<Option<T>, ApiError> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| {
            v.parse::<T>().map_err(|_| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!("invalid '{name}' value '{v}'"),
                )
            })
        })
        .transpose()
}

fn extent(rows: impl Iterator<Item = (f64, f64)>) -> Option<Extent> {
    rows.fold(None, |acc: Option<Extent>, (lat, lon)| {
        Some(match acc {
            None => Extent {
                west: lon,
                south: lat,
                east: lon,
                north: lat,
            },
            Some(e) => Extent {
                west: e.west.min(lon),
                south: e.south.min(lat),
                east: e.east.max(lon),
                north: e.north.max(lat),
            },
        })
    })
}

fn build_stride_masks(
    links: &[Option<usize>],
    outputs: usize,
    k: usize,
) -> (usize, Vec<bool>, Vec<bool>) {
    let keep_out: Vec<bool> = (0..outputs).map(|i| i.is_multiple_of(k)).collect();
    let mut removed_rank = 0usize;
    let keep_raw: Vec<bool> = links
        .iter()
        .map(|link| match link {
            Some(h) => keep_out[*h],
            None => {
                removed_rank += 1;
                (removed_rank - 1).is_multiple_of(k)
            }
        })
        .collect();
    (k, keep_out, keep_raw)
}

/// Deterministic subset that keeps links consistent: every `k`-th harmonized output (in time
/// order) with all raw rows linked to it, plus every `k`-th interscan-removed raw row. `k` is
/// the smallest step, starting from ceil(raw / limit), whose subset fits in `limit` raw rows.
/// When no stride can fit within `limit` (e.g. output 0 alone exceeds `limit`), falls back to a
/// greedy time-ordered selection within budget. Returns (k, kept output mask, kept raw mask).
fn sample_links(
    links: &[Option<usize>],
    outputs: usize,
    limit: usize,
) -> (usize, Vec<bool>, Vec<bool>) {
    if links.len() <= limit {
        return (1, vec![true; outputs], vec![true; links.len()]);
    }

    let mut obs_counts = vec![0usize; outputs];
    let mut removed_count = 0usize;
    for link in links {
        match link {
            Some(h) => obs_counts[*h] += 1,
            None => removed_count += 1,
        }
    }

    // Index 0 of outputs and the first `None` row are selected for every stride `k`, so no
    // stride can fit if their combined rows already exceed `limit`.
    let min_kept = obs_counts.first().copied().unwrap_or(0) + usize::from(removed_count > 0);
    let k_start = links.len().div_ceil(limit.max(1));
    if min_kept <= limit {
        let max_k = outputs.max(removed_count).max(1);
        let mut k = k_start;
        loop {
            let mut count = removed_count.div_ceil(k);
            for i in (0..outputs).step_by(k) {
                count += obs_counts[i];
                if count > limit {
                    break;
                }
            }
            if count <= limit {
                return build_stride_masks(links, outputs, k);
            }
            if k >= max_k {
                break;
            }
            k += 1;
        }
    }

    let mut remaining = limit;
    let mut keep_out = vec![false; outputs];
    for (h, &n_obs) in obs_counts.iter().enumerate() {
        if n_obs > 0 && n_obs <= remaining {
            keep_out[h] = true;
            remaining -= n_obs;
        }
    }
    let keep_raw: Vec<bool> = links
        .iter()
        .map(|link| match link {
            Some(h) => keep_out[*h],
            None if remaining > 0 => {
                remaining -= 1;
                true
            }
            None => false,
        })
        .collect();
    (k_start.max(2), keep_out, keep_raw)
}

fn resolve_points_config(
    base: &PipelineConfig,
    params: &PointsParams,
) -> Result<(PipelineConfig, bool), ApiError> {
    let mut config = base.clone();
    let mut overridden = false;
    if let Some(v) = parse_param::<i64>("dedup_time_secs", params.dedup_time_secs.as_deref())? {
        config.dedup.max_time_diff_secs = v;
        overridden = true;
    }
    if let Some(v) = parse_param::<f64>("dedup_overlap", params.dedup_overlap.as_deref())? {
        config.dedup.min_overlap_ratio = v;
        overridden = true;
    }
    if let Some(v) = parse_param::<f64>("match_radius_km", params.match_radius_km.as_deref())? {
        config.matching.radius_km = v;
        overridden = true;
    }
    if let Some(v) = parse_param::<i64>("match_window_secs", params.match_window_secs.as_deref())? {
        config.matching.window_secs = v;
        overridden = true;
    }
    if let Some(v) = parse_param::<f64>("fleet_radius_km", params.fleet_radius_km.as_deref())? {
        config.fleet.radius_km = v;
        overridden = true;
    }
    config
        .validate()
        .map_err(|msg| ApiError::new(StatusCode::BAD_REQUEST, msg))?;
    Ok((config, overridden))
}

/// Validated request shared by `/api/points` and the archive mode of `/api/export`.
struct PeriodRequest {
    region: Arc<RegionInfo>,
    year: i32,
    month: Option<u32>,
    config: PipelineConfig,
    config_overridden: bool,
    min_confidence: MinConfidence,
    fusion_model: FusionModel,
}

/// `own_checks` runs after the region, year and month checks and before the threshold
/// overrides, so callers validate their own parameters in the order `/api/points` always has.
async fn resolve_period(
    state: &ArchiveState,
    params: &PointsParams,
    own_checks: impl FnOnce() -> Result<(), ApiError>,
) -> Result<PeriodRequest, ApiError> {
    let region = find_region(state, params.region.as_deref()).await?;
    let id = region.id.clone();

    let year: i32 = parse_param("year", params.year.as_deref())?
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "missing 'year' parameter"))?;
    let month: Option<u32> = parse_param("month", params.month.as_deref())?;
    if month.is_some_and(|m| !(1..=12).contains(&m)) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "'month' must be within 1..=12",
        ));
    }
    own_checks()?;
    let (config, config_overridden) = resolve_points_config(&state.config, params)?;
    let min_confidence = MinConfidence::parse(params.min_confidence.as_deref())
        .map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?;
    let fusion_model = match params.fusion_model.as_deref().map(str::trim) {
        None | Some("") => FusionModel::default(),
        Some(v) => v
            .parse::<FusionModel>()
            .map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?,
    };
    if !region.modis_years.contains(&year)
        && !region.viirs_years.contains(&year)
        && !region.noaa20_years.contains(&year)
    {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            format!("the archive has no {id} file for {year}"),
        ));
    }
    state.admit_year(&region, year)?;
    Ok(PeriodRequest {
        region,
        year,
        month,
        config,
        config_overridden,
        min_confidence,
        fusion_model,
    })
}

async fn points(
    State(state): State<Arc<ArchiveState>>,
    Query(params): Query<PointsParams>,
) -> Result<Json<PointsResponse>, ApiError> {
    let started = Instant::now();
    let mut limit = DEFAULT_POINTS_LIMIT;
    let PeriodRequest {
        region,
        year,
        month,
        config,
        config_overridden,
        min_confidence,
        fusion_model,
    } = resolve_period(&state, &params, || {
        limit = match parse_param::<usize>("limit", params.limit.as_deref())? {
            Some(0) => {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "'limit' must be at least 1",
                ));
            }
            Some(n) => n.min(MAX_POINTS_LIMIT),
            None => DEFAULT_POINTS_LIMIT,
        };
        Ok(())
    })
    .await?;
    let id = region.id.clone();

    let slot = state
        .load_slots
        .clone()
        .acquire_owned()
        .await
        .expect("semaphore is never closed");
    let data = load_year(&state, &region, year).await?;
    let coverage = load_coverage(&state).await?;
    let response = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        // Judged on the whole year so a single-month request masks the same cells as /api/series.
        let mask = config.static_mask.enabled.then(|| {
            StaticMask::from_detections(data.modis.iter().chain(&data.viirs), &config.static_mask)
        });
        // Measured on the whole year so the flag stays put while the user steps through months.
        let tropical_swath_gap = extent(
            data.modis
                .iter()
                .chain(&data.viirs)
                .map(|h| (h.lat, h.lon)),
        )
        .and_then(|e| {
            flag_tropical_swath_gap(
                e.south,
                e.north,
                SwathGapBasis::Detections,
                config.uncertainty.tropical_swath_gap_compensation,
            )
        });
        let (mut modis, mut viirs) = match month {
            Some(m) => (in_month(&data.modis, m), in_month(&data.viirs, m)),
            None => Arc::try_unwrap(data).map_or_else(
                |shared| (shared.modis.clone(), shared.viirs.clone()),
                |owned| (owned.modis, owned.viirs),
            ),
        };
        // Filtered before harmonization so dropped detections never reach dedup or matching.
        let excluded_detections = min_confidence.retain(&mut modis) + min_confidence.retain(&mut viirs);

        let raw_points: Vec<RawPoint> = modis
            .iter()
            .map(|h| RawPoint::from_hotspot(h, "MODIS"))
            .chain(viirs.iter().map(|h| RawPoint::from_hotspot(h, "VIIRS")))
            .collect();
        let bbox = extent(raw_points.iter().map(|p| (p.lat, p.lon)));
        let (raw_modis, raw_viirs) = (modis.len(), viirs.len());
        let modis_days: BTreeSet<NaiveDate> =
            modis.iter().map(|h| h.acquired.date_naive()).collect();
        let viirs_days: BTreeSet<NaiveDate> =
            viirs.iter().map(|h| h.acquired.date_naive()).collect();
        let fusion_inputs = (fusion_model != FusionModel::ViirsPreferred)
            .then(|| (modis.clone(), viirs.clone()));
        let mut linked = harmonize_with_links_masked(modis, viirs, &config, mask.as_ref());
        let (hotspots, links, fusion_comparison) = match fusion_inputs {
            None => {
                let links: Vec<Option<usize>> = linked
                    .modis_links
                    .iter()
                    .chain(&linked.viirs_links)
                    .copied()
                    .collect();
                (std::mem::take(&mut linked.hotspots), links, None)
            }
            Some((fusion_modis, fusion_viirs)) => {
                let fused = apply_fusion_model(
                    &linked,
                    &fusion_modis,
                    &fusion_viirs,
                    fusion_model,
                    &config,
                );
                let links: Vec<Option<usize>> = linked
                    .modis_links
                    .iter()
                    .chain(&linked.viirs_links)
                    .map(|link| link.and_then(|o| fused.index_map[o]))
                    .collect();
                let comparison = FusionComparison {
                    model: fusion_model,
                    baseline_count: linked.hotspots.len(),
                    baseline_total_frp: total_frp(&linked.hotspots),
                    count: fused.hotspots.len(),
                    total_frp: total_frp(&fused.hotspots),
                    dropped_below_modis_limit: fused.dropped_below_modis_limit,
                };
                (fused.hotspots, links, Some(comparison))
            }
        };

        let event_params = EventParams::default();
        let clustering = cluster_events(&hotspots, &event_params);
        let events = events_meta(&clustering, &event_params);
        let (modis_cov, viirs_cov) = family_coverage(&region, year, &coverage);
        let gaps = no_detection_days(
            &modis_days,
            &viirs_days,
            month_bounds(year, month),
            modis_cov,
            viirs_cov,
        );

        let (k, keep_out, keep_raw) = sample_links(&links, hotspots.len(), limit);

        let raw: Vec<RawPoint> = raw_points
            .into_iter()
            .zip(&links)
            .enumerate()
            .filter(|(i, _)| keep_raw[*i])
            .map(|(i, (mut point, link))| {
                point.id = format!("r{i}");
                point.harmonized_id = link.map(|o| format!("h{o}"));
                point
            })
            .collect();

        let stages = Stages {
            raw_modis,
            raw_viirs,
            static_masked: linked.static_masked,
            sama_removed: linked.sama_removed,
            interscan_removed: linked.interscan_removed,
            fleet_merged: linked.fleet_merged,
            matched_pairs: linked.matched_pairs,
            viirs_aggregated: linked.viirs_aggregated,
            harmonized: hotspots.len(),
        };
        let harmonized: Vec<HarmonizedPoint> = hotspots
            .into_iter()
            .enumerate()
            .filter(|(i, _)| keep_out[*i])
            .map(|(i, hotspot)| HarmonizedPoint {
                id: format!("h{i}"),
                event_id: clustering.labels[i],
                hotspot,
            })
            .collect();

        PointsResponse {
            region: id,
            year,
            month,
            truncated: k > 1,
            limit,
            sample_every: k,
            raw,
            harmonized,
            stages,
            bbox,
            filters: ConfidenceFilter {
                min_confidence,
                excluded_detections,
            },
            tropical_swath_gap,
            events,
            no_detection_days: gaps,
            config,
            config_overridden,
            fusion_model,
            fusion_comparison,
        }
    })
    .await
    .map_err(join_error)?;

    tracing::debug!(
        region = %response.region,
        year,
        month,
        raw = response.raw.len(),
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "served /api/points"
    );
    Ok(Json(response))
}

// ---------- density ----------

const DEFAULT_DENSITY_CELL_DEG: f64 = 0.25;

#[derive(Deserialize, Default)]
struct DensityParams {
    region: Option<String>,
    year: Option<String>,
    cell: Option<String>,
    min_confidence: Option<String>,
    fusion_model: Option<String>,
}

impl DensityParams {
    fn points_params(&self) -> PointsParams {
        PointsParams {
            region: self.region.clone(),
            year: self.year.clone(),
            min_confidence: self.min_confidence.clone(),
            fusion_model: self.fusion_model.clone(),
            ..PointsParams::default()
        }
    }
}

#[derive(Serialize)]
struct DensityMonth {
    month: u32,
    raw_total: u64,
    harmonized_total: u64,
    /// `[row, col, raw_count, raw_frp, harm_count, harm_frp]`
    cells: Vec<(u32, u32, u64, f64, u64, f64)>,
}

#[derive(Serialize)]
struct DensityMaxResponse {
    raw_count: u64,
    raw_frp: f64,
    harm_count: u64,
    harm_frp: f64,
}

#[derive(Serialize)]
struct DensityResponse {
    region: String,
    year: i32,
    cell_deg: f64,
    grid: &'static str,
    months: Vec<DensityMonth>,
    max: DensityMaxResponse,
    bbox: Option<Extent>,
    covered_months: Vec<u32>,
}

/// Months of `year` that overlap the archive coverage of either sensor family.
fn covered_months(region: &RegionInfo, year: i32, coverage: &Coverage) -> Vec<u32> {
    let (modis, viirs) = family_coverage(region, year, coverage);
    (1..=12)
        .filter(|&m| {
            let (first, last) = month_bounds(year, Some(m));
            [modis, viirs]
                .into_iter()
                .flatten()
                .any(|(from, to)| from <= last && to >= first)
        })
        .collect()
}

async fn density(
    State(state): State<Arc<ArchiveState>>,
    Query(params): Query<DensityParams>,
) -> Result<Json<DensityResponse>, ApiError> {
    let started = Instant::now();
    let mut cell_deg = DEFAULT_DENSITY_CELL_DEG;
    let PeriodRequest {
        region,
        year,
        config,
        min_confidence,
        fusion_model,
        ..
    } = resolve_period(&state, &params.points_params(), || {
        if let Some(cell) = parse_param::<f64>("cell", params.cell.as_deref())? {
            cell_deg = cell;
        }
        if !ALLOWED_CELL_DEGREES.contains(&cell_deg) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "'cell' must be one of 0.1, 0.25, 0.5, 1.0",
            ));
        }
        Ok(())
    })
    .await?;
    let id = region.id.clone();

    let slot = state
        .load_slots
        .clone()
        .acquire_owned()
        .await
        .expect("semaphore is never closed");
    let data = load_year(&state, &region, year).await?;
    let coverage = load_coverage(&state).await?;
    let response = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        // Same input path as /api/points: whole-year static mask, confidence filter before
        // harmonization, then the selected fusion model.
        let mask = config.static_mask.enabled.then(|| {
            StaticMask::from_detections(data.modis.iter().chain(&data.viirs), &config.static_mask)
        });
        let (mut modis, mut viirs) = (data.modis.clone(), data.viirs.clone());
        drop(data);
        min_confidence.retain(&mut modis);
        min_confidence.retain(&mut viirs);
        let bbox = extent(modis.iter().chain(&viirs).map(|h| (h.lat, h.lon)));
        let fusion_inputs = (fusion_model != FusionModel::ViirsPreferred)
            .then(|| (modis.clone(), viirs.clone()));
        let raw: Vec<Hotspot> = modis.iter().chain(&viirs).cloned().collect();
        let mut linked = harmonize_with_links_masked(modis, viirs, &config, mask.as_ref());
        let hotspots = match fusion_inputs {
            None => std::mem::take(&mut linked.hotspots),
            Some((fusion_modis, fusion_viirs)) => {
                apply_fusion_model(&linked, &fusion_modis, &fusion_viirs, fusion_model, &config)
                    .hotspots
            }
        };
        let merged = aggregate_density(&raw, &hotspots, cell_deg);

        DensityResponse {
            region: id,
            year,
            cell_deg,
            grid: "global_latlon",
            months: merged
                .months
                .into_iter()
                .map(|m| DensityMonth {
                    month: m.month,
                    raw_total: m.raw_total,
                    harmonized_total: m.harmonized_total,
                    cells: m
                        .cells
                        .into_iter()
                        .map(|c| {
                            let s = c.stats;
                            (c.row, c.col, s.raw_count, s.raw_frp, s.harm_count, s.harm_frp)
                        })
                        .collect(),
                })
                .collect(),
            max: DensityMaxResponse {
                raw_count: merged.max.raw_count,
                raw_frp: merged.max.raw_frp,
                harm_count: merged.max.harm_count,
                harm_frp: merged.max.harm_frp,
            },
            bbox,
            covered_months: covered_months(&region, year, &coverage),
        }
    })
    .await
    .map_err(join_error)?;

    tracing::debug!(
        region = %response.region,
        year,
        cell_deg,
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "served /api/density"
    );
    Ok(Json(response))
}

// ---------- export (archive mode of /api/export) ----------

/// Guards memory: the export materialises every record of the period in one response body.
const MAX_EXPORT_RECORDS: usize = 500_000;
const CSV_BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Deserialize, Default, Clone)]
struct ArchiveExportParams {
    region: Option<String>,
    year: Option<String>,
    month: Option<String>,
    format: Option<String>,
    view: Option<String>,
    dedup_time_secs: Option<String>,
    dedup_overlap: Option<String>,
    match_radius_km: Option<String>,
    match_window_secs: Option<String>,
    fleet_radius_km: Option<String>,
    min_confidence: Option<String>,
    fusion_model: Option<String>,
}

impl ArchiveExportParams {
    fn points_params(&self) -> PointsParams {
        PointsParams {
            region: self.region.clone(),
            year: self.year.clone(),
            month: self.month.clone(),
            limit: None,
            dedup_time_secs: self.dedup_time_secs.clone(),
            dedup_overlap: self.dedup_overlap.clone(),
            match_radius_km: self.match_radius_km.clone(),
            match_window_secs: self.match_window_secs.clone(),
            fleet_radius_km: self.fleet_radius_km.clone(),
            min_confidence: self.min_confidence.clone(),
            fusion_model: self.fusion_model.clone(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExportView {
    Harmonized,
    Raw,
}

impl ExportView {
    fn parse(value: Option<&str>) -> Result<Self, ApiError> {
        match value.map(str::to_ascii_lowercase).as_deref() {
            None | Some("harmonized") => Ok(Self::Harmonized),
            Some("raw") => Ok(Self::Raw),
            Some(other) => Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("Invalid 'view' value '{other}': expected 'harmonized' or 'raw'"),
            )),
        }
    }
}

/// Lowercase ASCII file-name stem of a region id (`Korea, South` -> `korea_south`).
fn region_slug(id: &str) -> String {
    let mut slug = String::new();
    for c in id.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug = slug.trim_end_matches('_');
    if slug.is_empty() { "region".to_string() } else { slug.to_string() }
}

fn export_filename(
    region_id: &str,
    year: i32,
    month: Option<u32>,
    view: ExportView,
    extension: &str,
) -> String {
    let period = match month {
        Some(m) => format!("{year:04}-{m:02}"),
        None => format!("{year:04}"),
    };
    let suffix = if view == ExportView::Raw { "_raw" } else { "" };
    format!("pyrosync_{}_{period}{suffix}.{extension}", region_slug(region_id))
}

async fn export_archive(
    state: Arc<ArchiveState>,
    params: ArchiveExportParams,
) -> Result<Response, ApiError> {
    let format = crate::ExportFormat::parse(params.format.as_deref())
        .map_err(|e| ApiError::new(e.status, e.message))?;
    let view = ExportView::parse(params.view.as_deref())?;
    let PeriodRequest {
        region,
        year,
        month,
        config,
        min_confidence,
        fusion_model,
        ..
    } = resolve_period(&state, &params.points_params(), || Ok(())).await?;

    let slot = state
        .load_slots
        .clone()
        .acquire_owned()
        .await
        .expect("semaphore is never closed");
    let data = load_year(&state, &region, year).await?;
    let body = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        let mask = config.static_mask.enabled.then(|| {
            StaticMask::from_detections(data.modis.iter().chain(&data.viirs), &config.static_mask)
        });
        let (mut modis, mut viirs) = match month {
            Some(m) => (in_month(&data.modis, m), in_month(&data.viirs, m)),
            None => (data.modis.clone(), data.viirs.clone()),
        };
        drop(data);
        min_confidence.retain(&mut modis);
        min_confidence.retain(&mut viirs);
        if modis.len() + viirs.len() > MAX_EXPORT_RECORDS {
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                format!(
                    "the period holds {} detections, above the export limit of {MAX_EXPORT_RECORDS}; request a single month or raise 'min_confidence'",
                    modis.len() + viirs.len()
                ),
            ));
        }

        let text = if view == ExportView::Raw {
            let raw: Vec<Hotspot> = modis.into_iter().chain(viirs).collect();
            match format {
                crate::ExportFormat::Csv => pyrosync_core::export::raw_to_csv(&raw),
                crate::ExportFormat::GeoJson => pyrosync_core::export::raw_to_geojson(&raw),
            }
        } else {
            let fusion_inputs = (fusion_model != FusionModel::ViirsPreferred)
                .then(|| (modis.clone(), viirs.clone()));
            let mut linked = harmonize_with_links_masked(modis, viirs, &config, mask.as_ref());
            let hotspots = match fusion_inputs {
                None => std::mem::take(&mut linked.hotspots),
                Some((fusion_modis, fusion_viirs)) => {
                    apply_fusion_model(&linked, &fusion_modis, &fusion_viirs, fusion_model, &config)
                        .hotspots
                }
            };
            match format {
                crate::ExportFormat::Csv => pyrosync_core::export::to_csv(&hotspots),
                crate::ExportFormat::GeoJson => pyrosync_core::export::to_geojson(&hotspots),
            }
        };
        let mut body = Vec::with_capacity(text.len() + CSV_BOM.len());
        if matches!(format, crate::ExportFormat::Csv) {
            body.extend_from_slice(CSV_BOM);
        }
        body.extend_from_slice(text.as_bytes());
        Ok(body)
    })
    .await
    .map_err(join_error)??;

    let filename = export_filename(&region.id, year, month, view, format.extension());
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(format.content_type())),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        body,
    )
        .into_response())
}

/// Route layer of `/api/export`: a `region` query parameter switches to the archive export,
/// every other request continues to the live FIRMS handler untouched.
pub async fn export_dispatch(
    State(archive): State<Option<Arc<ArchiveState>>>,
    request: Request,
    next: Next,
) -> Response {
    #[derive(Deserialize)]
    struct Probe {
        region: Option<String>,
    }
    let archive_mode = Query::<Probe>::try_from_uri(request.uri())
        .is_ok_and(|Query(probe)| probe.region.is_some());
    if !archive_mode {
        return next.run(request).await;
    }
    let Some(state) = archive else {
        return ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "the archive is not available")
            .into_response();
    };
    let params = match Query::<ArchiveExportParams>::try_from_uri(request.uri()) {
        Ok(Query(params)) => params,
        Err(rejection) => {
            return ApiError::new(StatusCode::BAD_REQUEST, rejection.body_text()).into_response();
        }
    };
    let limit = state.limits.request_timeout;
    match tokio::time::timeout(limit, export_archive(state, params)).await {
        Ok(Ok(response)) => response,
        Ok(Err(err)) => err.into_response(),
        Err(_) => ApiError::new(
            StatusCode::GATEWAY_TIMEOUT,
            format!(
                "archive request timed out after {} s (ARCHIVE_REQUEST_TIMEOUT_SECS)",
                limit.as_secs()
            ),
        )
        .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODIS_HEADER: &str = "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight,type";
    const VIIRS_HEADER: &str = "latitude,longitude,bright_ti4,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_ti5,frp,daynight,type";

    /// Fresh per-test archive directory so parallel tests never share files.
    fn temp_archive(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pyrosync-server-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes one archive file; each row is (latitude, acq_date, fire type).
    fn write_file(dir: &Path, slug: &str, year: i32, country: &str, rows: &[(f64, &str, u8)]) {
        let (header, sat) = match slug {
            "modis" => (MODIS_HEADER, "Terra,MODIS,80,6.2"),
            "viirs-jpss1" => (VIIRS_HEADER, "N20,VIIRS,n,2"),
            _ => (VIIRS_HEADER, "N,VIIRS,n,2"),
        };
        let mut csv = format!("{header}\r\n");
        for (lat, date, ty) in rows {
            csv.push_str(&format!("{lat},32.0,320,0.5,0.5,{date},1200,{sat},290,10,D,{ty}\r\n"));
        }
        let path = dir.join(slug).join(year.to_string());
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join(format!("{slug}_{year}_{country}.csv")), csv).unwrap();
    }

    fn state_with_caches(
        dir: PathBuf,
        derived_dir: PathBuf,
        bundled_dir: PathBuf,
        max_region_bytes: u64,
        config: PipelineConfig,
    ) -> Arc<ArchiveState> {
        Arc::new(
            ArchiveState::new(
                dir,
                ArchiveLimits {
                    max_region_bytes,
                    request_timeout: Duration::from_secs(30),
                },
                Arc::new(config),
            )
            .with_derived_cache_dir(derived_dir)
            .with_bundled_cache_dir(bundled_dir),
        )
    }

    fn state_with_config(
        dir: PathBuf,
        derived_dir: PathBuf,
        max_region_bytes: u64,
        config: PipelineConfig,
    ) -> Arc<ArchiveState> {
        let bundled_dir = dir.join("_bundled_cache");
        state_with_caches(dir, derived_dir, bundled_dir, max_region_bytes, config)
    }

    fn state(dir: PathBuf, max_region_bytes: u64) -> Arc<ArchiveState> {
        let derived_dir = dir.join("_derived_cache");
        state_with_config(dir, derived_dir, max_region_bytes, PipelineConfig::default())
    }

    async fn series_raw(
        state: &Arc<ArchiveState>,
        region: &str,
    ) -> Result<(Option<String>, Bytes), ApiError> {
        let response = series(
            State(state.clone()),
            Query(SeriesParams {
                region: Some(region.to_string()),
            }),
        )
        .await?;
        let source = response
            .headers()
            .get(X_SERIES_SOURCE_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        Ok((source, body))
    }

    async fn series_json(state: &Arc<ArchiveState>, region: &str) -> Result<serde_json::Value, ApiError> {
        let (_, body) = series_raw(state, region).await?;
        Ok(serde_json::from_slice(&body).unwrap())
    }

    fn d(year: i32, month: u32, day: u32) -> NaiveDate {
        ymd(year, month, day)
    }

    #[tokio::test]
    async fn size_guard_rejects_oversized_region_before_parsing() {
        let dir = temp_archive("size-guard");
        write_file(&dir, "modis", 2010, "Small", &[(39.0, "2010-07-01", 0)]);
        let big: Vec<(f64, &str, u8)> = (0..200)
            .map(|i| (30.0 + f64::from(i) * 0.05, "2010-07-01", 0))
            .collect();
        write_file(&dir, "modis", 2010, "Big", &big);
        let small_bytes = std::fs::metadata(dir.join("modis/2010/modis_2010_Small.csv"))
            .unwrap()
            .len();
        let state = state(dir.clone(), small_bytes * 2);

        let Json(listing) = regions(State(state.clone())).await.ok().unwrap();
        let flag = |id: &str| listing.regions.iter().find(|r| r.id == id).unwrap().too_large;
        assert!(flag("Big"));
        assert!(!flag("Small"));
        assert!(listing.regions.iter().all(|r| r.size_mb >= 0.0));

        let Err(err) = series_json(&state, "Big").await else {
            panic!("oversized region must be refused");
        };
        assert_eq!(err.status, StatusCode::PAYLOAD_TOO_LARGE);
        assert!(err.message.starts_with("Region too large for this server"), "{}", err.message);

        let points_err = points(
            State(state.clone()),
            Query(PointsParams {
                region: Some("Big".into()),
                year: Some("2010".into()),
                month: None,
                limit: None,
                ..Default::default()
            }),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(points_err.status, StatusCode::PAYLOAD_TOO_LARGE);

        assert!(series_json(&state, "Small").await.is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn series_fills_zero_months_and_flags_partial_from_sensor_coverage() {
        let dir = temp_archive("coverage");
        write_file(&dir, "modis", 2010, "Land", &[(39.0, "2010-11-05", 0)]);
        write_file(
            &dir,
            "modis",
            2011,
            "Land",
            &[(39.0, "2011-02-01", 0), (39.5, "2011-07-10", 0)],
        );
        write_file(&dir, "modis", 2012, "Land", &[(39.0, "2012-08-01", 0)]);
        write_file(&dir, "viirs-snpp", 2012, "Land", &[(38.0, "2012-03-01", 0)]);
        // Another country's non-vegetation row still extends the MODIS coverage to Dec 31.
        write_file(&dir, "modis", 2012, "Other", &[(40.0, "2012-12-31", 2)]);
        let state = state(dir.clone(), u64::MAX);

        let json = series_json(&state, "Land").await.ok().unwrap();
        assert_eq!(json["coverage"]["modis"]["first"], "2010-11-05");
        assert_eq!(json["coverage"]["modis"]["last"], "2012-12-31");
        assert_eq!(json["coverage"]["viirs"]["first"], "2012-03-01");

        let years = json["years"].as_array().unwrap();
        let partial: Vec<(i64, bool)> = years
            .iter()
            .map(|y| (y["year"].as_i64().unwrap(), y["partial"].as_bool().unwrap()))
            .collect();
        assert_eq!(partial, [(2010, true), (2011, false), (2012, true)]);

        let months = json["months"].as_array().unwrap();
        let of_year = |year: i64| -> Vec<&serde_json::Value> {
            months.iter().filter(|m| m["year"] == year).collect()
        };
        let listed = |year: i64| -> Vec<u64> {
            of_year(year).iter().map(|m| m["month"].as_u64().unwrap()).collect()
        };
        assert_eq!(listed(2010), [11, 12]);
        assert_eq!(listed(2011), (1..=12).collect::<Vec<u64>>());
        assert_eq!(listed(2012), (1..=12).collect::<Vec<u64>>());

        for year in years {
            let sum: u64 = of_year(year["year"].as_i64().unwrap())
                .iter()
                .map(|m| m["raw_total"].as_u64().unwrap())
                .sum();
            assert_eq!(sum, year["raw_total"].as_u64().unwrap());
        }
        assert_eq!(
            of_year(2011).iter().filter(|m| m["raw_total"] == 0).count(),
            10
        );

        // November has three observed years (1, 0, 0), so the zero months form a baseline.
        let nov_2010 = of_year(2010).into_iter().find(|m| m["month"] == 11).unwrap();
        assert!(nov_2010["anomaly_raw"].as_f64().unwrap() > 0.0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn coverage_rules_for_partial_years_and_months() {
        let modis = DateSpan {
            first: d(2000, 11, 1),
            last: d(2025, 12, 31),
        };
        let viirs = DateSpan {
            first: d(2012, 1, 20),
            last: d(2025, 6, 30),
        };
        assert!(!modis.covers_year(2000));
        assert!(modis.covers_year(2001));
        assert!(modis.covers_year(2025));
        assert!(!viirs.covers_year(2012));
        assert!(viirs.covers_year(2013));
        assert!(!viirs.covers_year(2025));

        assert!(!modis.overlaps_month(2000, 10));
        assert!(modis.overlaps_month(2000, 11));
        assert!(viirs.overlaps_month(2012, 1));
        assert!(viirs.overlaps_month(2025, 6));
        assert!(!viirs.overlaps_month(2025, 7));
    }

    #[test]
    fn year_stats_keeps_raw_month_sums_equal_to_year_totals() {
        let rows = |dates: &[&str]| -> Vec<Hotspot> {
            let csv: String = std::iter::once(MODIS_HEADER.to_string())
                .chain(dates.iter().enumerate().map(|(i, date)| {
                    format!("{},32.0,320,1,1,{date},1200,Terra,MODIS,80,6.2,290,10,D,0", 36.0 + i as f64)
                }))
                .collect::<Vec<_>>()
                .join("\n");
            parse_csv(&csv, "MODIS_SP").unwrap()
        };
        let data = YearData {
            modis: rows(&["2015-03-02", "2015-03-20", "2015-08-11"]),
            viirs: Vec::new(),
        };
        let span = DateSpan {
            first: d(2000, 11, 1),
            last: d(2025, 12, 31),
        };
        let stats = year_stats(2015, data, &[span]);
        assert!(!stats.entry.partial);
        assert_eq!(stats.months.len(), 12);
        let sum: usize = stats.months.iter().map(|m| m.raw_total).sum();
        assert_eq!(sum, stats.entry.raw_total);
        assert_eq!(stats.months[2].raw_modis, 2);
    }

    #[test]
    fn year_stats_counts_viirs_pixels_absorbed_into_one_matched_record() {
        let modis = parse_csv(
            &format!("{MODIS_HEADER}
36.0,32.0,320,1,1,2015-08-11,1200,Terra,MODIS,80,6.2,290,10,D,0"),
            "MODIS_SP",
        )
        .unwrap();
        let viirs_rows: String = [36.0, 36.003, 36.006]
            .iter()
            .map(|lat| format!("
{lat},32.0,330,0.375,0.375,2015-08-11,1210,N,VIIRS,n,2,290,5,D,0"))
            .collect();
        let viirs = parse_csv(&format!("{VIIRS_HEADER}{viirs_rows}"), "VIIRS_SNPP_SP").unwrap();
        let span = DateSpan {
            first: d(2000, 11, 1),
            last: d(2025, 12, 31),
        };
        let stats = year_stats(2015, YearData { modis, viirs }, &[span]);
        let e = &stats.entry;
        assert_eq!(e.matched, 1);
        assert_eq!(e.viirs_aggregated, 2);
        assert_eq!(e.viirs_only, 0);
        assert_eq!(
            e.raw_total
                - e.static_masked
                - e.sama_removed
                - e.interscan_removed
                - e.fleet_merged
                - e.matched
                - e.viirs_aggregated,
            e.harmonized
        );
    }

    #[test]
    fn year_stats_reports_static_mask_and_applies_it_to_every_month() {
        // One VIIRS detection at the same spot on the 5th of every month: a year-round source.
        let csv: String = std::iter::once(VIIRS_HEADER.to_string())
            .chain((1..=12).map(|m| {
                format!("31.0,47.5,320,0.5,0.5,2015-{m:02}-05,1200,N,VIIRS,n,2,290,10,N,0")
            }))
            .chain(std::iter::once(
                "36.0,32.0,320,0.5,0.5,2015-08-11,1200,N,VIIRS,n,2,290,10,D,0".to_string(),
            ))
            .collect::<Vec<_>>()
            .join("\n");
        let viirs = parse_csv(&csv, "VIIRS_SNPP_SP").unwrap();
        let span = DateSpan {
            first: d(2012, 1, 20),
            last: d(2025, 12, 31),
        };
        let data = || YearData {
            modis: Vec::new(),
            viirs: viirs.clone(),
        };

        let stats = year_stats(2015, data(), &[span]);
        assert_eq!(stats.entry.static_masked, 12);
        assert_eq!(stats.entry.sama_removed, 0);
        assert_eq!(stats.entry.harmonized, 1);
        assert_eq!(stats.entry.raw_viirs, 13);
        let monthly: usize = stats.months.iter().map(|m| m.harmonized).sum();
        assert_eq!(monthly, 1, "the yearly mask must reach the monthly runs");

        let mut off = PipelineConfig::default();
        off.static_mask.enabled = false;
        let stats = year_stats_with(2015, data(), &[span], &off);
        assert_eq!(stats.entry.static_masked, 0);
        assert_eq!(stats.entry.harmonized, 13);
    }

    #[test]
    fn file_date_span_reads_every_row() {
        let dir = temp_archive("date-span");
        write_file(
            &dir,
            "viirs-snpp",
            2012,
            "X",
            &[(1.0, "2012-05-01", 0), (2.0, "2012-01-20", 3), (3.0, "2012-12-31", 2)],
        );
        let span = file_date_span(&dir.join("viirs-snpp/2012/viirs-snpp_2012_X.csv"))
            .unwrap()
            .unwrap();
        assert_eq!(span.first, d(2012, 1, 20));
        assert_eq!(span.last, d(2012, 12, 31));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sample_links_keeps_everything_under_limit() {
        let links = vec![Some(0), Some(1), None, Some(1)];
        let (k, out, raw) = sample_links(&links, 2, 10);
        assert_eq!(k, 1);
        assert!(out.iter().all(|&o| o));
        assert!(raw.iter().all(|&r| r));
    }

    #[test]
    fn sample_links_keeps_linked_rows_together_and_fits_limit() {
        // 100 outputs, each with two raw rows, plus 20 interscan-removed rows.
        let mut links: Vec<Option<usize>> = (0..200).map(|i| Some(i / 2)).collect();
        links.extend(std::iter::repeat_n(None, 20));
        let (k, out, raw) = sample_links(&links, 100, 50);
        assert!(k >= 5);
        assert!(raw.iter().filter(|&&r| r).count() <= 50);
        for (i, link) in links.iter().enumerate() {
            if let Some(o) = link {
                assert_eq!(raw[i], out[*o], "raw row {i} must follow its output");
            }
        }
        assert!(out[0] && !out[1]);
    }

    fn assert_sample_invariants(
        links: &[Option<usize>],
        outputs: usize,
        limit: usize,
        k: usize,
        out: &[bool],
        raw: &[bool],
    ) {
        assert_eq!(out.len(), outputs);
        assert_eq!(raw.len(), links.len());
        let kept_raw = raw.iter().filter(|&&r| r).count();
        assert!(kept_raw <= limit, "kept {kept_raw} > limit {limit}");
        if links.len() > limit {
            assert!(k > 1, "truncated result must report sample_every > 1, got {k}");
        }
        for (i, link) in links.iter().enumerate() {
            if raw[i] && let Some(h) = link {
                assert!(out[*h], "raw row {i} references dropped output {h}");
            }
        }
    }

    fn run_sample_links_with_timeout(
        links: Vec<Option<usize>>,
        outputs: usize,
        limit: usize,
    ) -> (usize, Vec<bool>, Vec<bool>) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let res = sample_links(&links, outputs, limit);
            let _ = tx.send(res);
        });
        rx.recv_timeout(Duration::from_secs(1))
            .expect("sample_links did not finish within 1s")
    }

    #[test]
    fn sample_links_handles_empty_and_all_none_inputs() {
        let (k, out, raw) = sample_links(&[], 0, 10);
        assert_eq!(k, 1);
        assert!(out.is_empty());
        assert!(raw.is_empty());

        let (k, out, raw) = sample_links(&[], 0, 0);
        assert_eq!(k, 1);
        assert!(out.is_empty());
        assert!(raw.is_empty());

        let all_none = vec![None; 10];
        let (k, out, raw) = sample_links(&all_none, 0, 3);
        assert_sample_invariants(&all_none, 0, 3, k, &out, &raw);
        assert_eq!(raw.iter().filter(|&&r| r).count(), 3);
    }

    #[test]
    fn sample_links_terminates_when_first_output_exceeds_limit_or_limit_is_one() {
        // Output 0 has 2 raw rows, so limit=1 would loop forever without a bound/fallback.
        let links_one = vec![Some(0), Some(0), Some(1)];
        let (k1, out1, raw1) = run_sample_links_with_timeout(links_one.clone(), 2, 1);
        assert_sample_invariants(&links_one, 2, 1, k1, &out1, &raw1);
        assert_eq!(raw1.iter().filter(|&&r| r).count(), 1);
        assert!(!out1[0] && out1[1]);

        // First output has 5 raw rows > limit=2; later output (1 row) and None row fill budget.
        let links_multi = vec![
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(1),
            None,
            None,
        ];
        let (k2, out2, raw2) = run_sample_links_with_timeout(links_multi.clone(), 2, 2);
        assert_sample_invariants(&links_multi, 2, 2, k2, &out2, &raw2);
        assert_eq!(raw2.iter().filter(|&&r| r).count(), 2);
        assert!(!out2[0] && out2[1]);
    }

    #[test]
    fn sample_links_large_input_completes_quickly() {
        let mut links = Vec::with_capacity(200_005);
        links.extend(std::iter::repeat_n(Some(0), 10));
        for i in 1..=100_000 {
            links.push(Some(i));
            links.push(Some(i));
        }
        links.extend(std::iter::repeat_n(None, 5));

        let started = Instant::now();
        let (k, out, raw) = run_sample_links_with_timeout(links.clone(), 100_001, 5);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_sample_invariants(&links, 100_001, 5, k, &out, &raw);
    }

    #[tokio::test]
    async fn points_rejects_zero_limit_and_handles_limit_one_with_merged_hotspot() {
        let dir = temp_archive("points-limit");
        // Co-located MODIS + VIIRS rows harmonize into a single output with 2 raw rows,
        // plus a standalone VIIRS row so greedy fallback can still keep 1 raw row at limit=1.
        write_file(&dir, "modis", 2021, "Malta", &[(35.9, "2021-07-01", 0)]);
        write_file(
            &dir,
            "viirs-snpp",
            2021,
            "Malta",
            &[(35.9, "2021-07-01", 0), (36.0, "2021-07-02", 0)],
        );
        let state = state(dir.clone(), u64::MAX);

        let err = points(
            State(state.clone()),
            Query(PointsParams {
                region: Some("Malta".into()),
                year: Some("2021".into()),
                month: None,
                limit: Some("0".into()),
                ..Default::default()
            }),
        )
        .await
        .err()
        .expect("limit=0 must fail with 400");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert_eq!(err.message, "'limit' must be at least 1");

        let Json(pts) = tokio::time::timeout(
            Duration::from_secs(2),
            points(
                State(state),
                Query(PointsParams {
                    region: Some("Malta".into()),
                    year: Some("2021".into()),
                    month: None,
                    limit: Some("1".into()),
                    ..Default::default()
                }),
            ),
        )
        .await
        .expect("points with limit=1 must not hang")
        .ok()
        .unwrap();
        assert_eq!(pts.limit, 1);
        assert!(pts.truncated);
        assert!(pts.sample_every > 1);
        assert!(pts.raw.len() <= 1);
        let kept_ids: HashSet<&str> = pts.harmonized.iter().map(|h| h.id.as_str()).collect();
        for r in &pts.raw {
            if let Some(hid) = r.harmonized_id.as_deref() {
                assert!(kept_ids.contains(hid), "orphan harmonized_id {hid}");
            }
        }

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn points_reports_events_and_gaps_for_the_full_set_even_when_truncated() {
        let dir = temp_archive("points-events");
        write_file(
            &dir,
            "viirs-snpp",
            2021,
            "Malta",
            &[
                (35.9, "2021-07-01", 0),
                (35.9, "2021-07-02", 0),
                (35.9, "2021-07-03", 0),
                (36.5, "2021-07-20", 0),
            ],
        );
        let state = state(dir.clone(), u64::MAX);
        let Json(pts) = points(
            State(state),
            Query(PointsParams {
                region: Some("Malta".into()),
                year: Some("2021".into()),
                month: None,
                limit: Some("1".into()),
                ..Default::default()
            }),
        )
        .await
        .ok()
        .unwrap();

        assert!(pts.truncated);
        assert!(pts.harmonized.len() < 4);
        assert_eq!(pts.events.count, 1);
        assert_eq!(pts.events.items.len(), 1);
        assert!(!pts.events.items_truncated);
        assert_eq!(pts.events.items[0].detections, 3);
        assert_eq!(pts.events.watch_points, 1);
        assert_eq!(pts.events.params.eps_km, 2.0);
        assert!(pts.harmonized.iter().all(|h| h.event_id.is_none() || h.event_id == Some(0)));

        // VIIRS coverage is 07-01..07-20; 07-01..03 and 07-20 have detections.
        assert_eq!(pts.no_detection_days.len(), 16);
        assert!(pts.no_detection_days.iter().all(|d| d.family == pyrosync_core::uncertainty::SensorFamily::Viirs
            && !d.other_family_detected));
        assert_eq!(pts.no_detection_days[0].date, ymd(2021, 7, 4));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn points_fusion_model_defaults_to_identity_and_reports_comparison_otherwise() {
        let dir = temp_archive("points-fusion");
        write_file(&dir, "modis", 2021, "Malta", &[(35.9, "2021-07-01", 0)]);
        write_file(
            &dir,
            "viirs-snpp",
            2021,
            "Malta",
            &[(35.9, "2021-07-01", 0), (36.5, "2021-07-02", 0)],
        );
        let state = state(dir.clone(), u64::MAX);
        let request = |model: Option<&str>| PointsParams {
            region: Some("Malta".into()),
            year: Some("2021".into()),
            fusion_model: model.map(Into::into),
            ..Default::default()
        };

        let Json(default) = points(State(state.clone()), Query(request(None)))
            .await
            .ok()
            .unwrap();
        assert_eq!(default.fusion_model, FusionModel::ViirsPreferred);
        assert!(default.fusion_comparison.is_none());
        let body = serde_json::to_value(&default).unwrap();
        assert_eq!(body["fusion_model"], "viirs_preferred");
        assert!(body["fusion_comparison"].is_null());

        let Json(calibrated) = points(State(state.clone()), Query(request(Some("modis_calibrated"))))
            .await
            .ok()
            .unwrap();
        let comparison = calibrated.fusion_comparison.as_ref().expect("comparison");
        assert_eq!(calibrated.fusion_model, FusionModel::ModisCalibrated);
        assert!(comparison.count <= comparison.baseline_count);
        assert_eq!(comparison.baseline_count, default.stages.harmonized);
        assert_eq!(calibrated.stages.harmonized, comparison.count);
        assert!(!calibrated.config_overridden);

        let err = points(State(state), Query(request(Some("bogus"))))
            .await
            .err()
            .unwrap();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("fusion_model") && err.message.contains("equal_weight"));

        let _ = std::fs::remove_dir_all(dir);
    }

    async fn export_request(
        state: &Arc<ArchiveState>,
        query: &str,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        use tower::ServiceExt;

        let app = Router::new().route(
            "/export",
            get(|| async { "live" }).route_layer(middleware::from_fn_with_state(
                Some(state.clone()),
                export_dispatch,
            )),
        );
        let request = axum::http::Request::get(format!("/export?{query}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec();
        (status, headers, body)
    }

    fn export_fixture(name: &str) -> (PathBuf, Arc<ArchiveState>) {
        let dir = temp_archive(name);
        write_file(
            &dir,
            "modis",
            2021,
            "Malta",
            &[(35.9, "2021-07-01", 0), (35.95, "2021-08-03", 0)],
        );
        write_file(
            &dir,
            "viirs-snpp",
            2021,
            "Malta",
            &[(35.9, "2021-07-01", 0), (36.5, "2021-07-02", 0)],
        );
        let state = state(dir.clone(), u64::MAX);
        (dir, state)
    }

    fn header_str(headers: &axum::http::HeaderMap, name: header::HeaderName) -> String {
        headers.get(name).unwrap().to_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn export_archive_csv_matches_points_harmonized_count() {
        let (dir, state) = export_fixture("export-csv");
        let Json(points) = points(
            State(state.clone()),
            Query(PointsParams {
                region: Some("Malta".into()),
                year: Some("2021".into()),
                month: Some("7".into()),
                ..Default::default()
            }),
        )
        .await
        .ok()
        .unwrap();

        let (status, headers, body) = export_request(&state, "region=Malta&year=2021&month=7").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(header_str(&headers, header::CONTENT_TYPE), "text/csv; charset=utf-8");
        assert_eq!(
            header_str(&headers, header::CONTENT_DISPOSITION),
            "attachment; filename=\"pyrosync_malta_2021-07.csv\""
        );
        assert!(body.starts_with(CSV_BOM));
        let text = std::str::from_utf8(&body[CSV_BOM.len()..]).unwrap();
        let mut lines = text.trim_end_matches("\r\n").split("\r\n");
        assert_eq!(lines.next(), Some(pyrosync_core::export::CSV_HEADER));
        assert_eq!(lines.count(), points.stages.harmonized);

        let (_, headers, _) = export_request(&state, "region=Malta&year=2021").await;
        assert_eq!(
            header_str(&headers, header::CONTENT_DISPOSITION),
            "attachment; filename=\"pyrosync_malta_2021.csv\""
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn export_archive_geojson_and_raw_views() {
        let (dir, state) = export_fixture("export-geojson-raw");
        let (status, headers, body) =
            export_request(&state, "region=Malta&year=2021&month=7&format=geojson").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(header_str(&headers, header::CONTENT_TYPE), "application/geo+json");
        assert!(!body.starts_with(CSV_BOM));
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["type"], "FeatureCollection");
        let features = parsed["features"].as_array().unwrap();
        assert!(!features.is_empty());
        assert_eq!(features[0]["geometry"]["type"], "Point");

        let (status, headers, body) =
            export_request(&state, "region=Malta&year=2021&month=7&view=raw").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            header_str(&headers, header::CONTENT_DISPOSITION),
            "attachment; filename=\"pyrosync_malta_2021-07_raw.csv\""
        );
        let text = std::str::from_utf8(&body[CSV_BOM.len()..]).unwrap();
        let lines: Vec<&str> = text.trim_end_matches("\r\n").split("\r\n").collect();
        assert_eq!(lines[0], pyrosync_core::export::RAW_CSV_HEADER);
        assert_eq!(lines.len() - 1, 3);

        let (status, headers, body) =
            export_request(&state, "region=Malta&year=2021&view=raw&format=geojson").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            header_str(&headers, header::CONTENT_DISPOSITION),
            "attachment; filename=\"pyrosync_malta_2021_raw.geojson\""
        );
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["features"].as_array().unwrap().len(), 4);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn export_archive_rejects_bad_params_and_keeps_points_errors() {
        let (dir, state) = export_fixture("export-errors");
        for (query, expected) in [
            ("region=Malta&year=2021&format=xml", StatusCode::BAD_REQUEST),
            ("region=Malta&year=2021&view=bogus", StatusCode::BAD_REQUEST),
            ("region=Malta", StatusCode::BAD_REQUEST),
            ("region=Malta&year=2021&month=13", StatusCode::BAD_REQUEST),
            ("region=Malta&year=2021&fusion_model=bogus", StatusCode::BAD_REQUEST),
            ("region=Malta&year=1990", StatusCode::NOT_FOUND),
            ("region=Atlantis&year=2021", StatusCode::NOT_FOUND),
        ] {
            let (status, _, body) = export_request(&state, query).await;
            assert_eq!(status, expected, "query={query}");
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert!(json["error"].as_str().is_some_and(|s| !s.is_empty()), "query={query}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn export_archive_honours_fusion_model_and_live_requests_pass_through() {
        let (dir, state) = export_fixture("export-fusion");
        let rows = |body: &[u8]| {
            std::str::from_utf8(&body[CSV_BOM.len()..])
                .unwrap()
                .trim_end_matches("\r\n")
                .split("\r\n")
                .count()
                - 1
        };
        let (_, _, default) = export_request(&state, "region=Malta&year=2021").await;
        let (status, _, calibrated) =
            export_request(&state, "region=Malta&year=2021&fusion_model=modis_calibrated").await;
        assert_eq!(status, StatusCode::OK);
        assert!(rows(&calibrated) <= rows(&default));

        let (status, _, body) = export_request(&state, "west=26&south=36&east=45&north=42").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"live");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn region_slug_is_lowercase_ascii() {
        assert_eq!(region_slug("Turkey"), "turkey");
        assert_eq!(region_slug("Korea, South"), "korea_south");
        assert_eq!(region_slug("Côte_d_Ivoire"), "c_te_d_ivoire");
        assert_eq!(region_slug("!!"), "region");
    }

    #[test]
    fn events_meta_caps_items_keeping_largest_in_id_order() {
        use pyrosync_core::events::EventKind;
        let at = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;
        let events: Vec<FireEvent> = (0..MAX_EVENT_ITEMS + 5)
            .map(|id| FireEvent {
                id,
                kind: EventKind::Dense,
                detections: if id < 5 { 1 } else { 10 },
                start: at,
                end: at,
                days: 1,
                centroid_lat: 0.0,
                centroid_lon: 0.0,
                bbox: pyrosync_core::models::BBox { west: 0.0, south: 0.0, east: 0.0, north: 0.0 },
                total_frp: 0.0,
            })
            .collect();
        let clustering = EventClustering { labels: Vec::new(), events, watch_points: 7 };
        let meta = events_meta(&clustering, &EventParams::default());
        assert_eq!(meta.count, MAX_EVENT_ITEMS + 5);
        assert!(meta.items_truncated);
        assert_eq!(meta.items.len(), MAX_EVENT_ITEMS);
        assert!(meta.items.iter().all(|e| e.detections == 10));
        assert!(meta.items.windows(2).all(|w| w[0].id < w[1].id));
    }

    #[test]
    fn month_bounds_and_family_coverage() {
        assert_eq!(month_bounds(2021, Some(2)), (ymd(2021, 2, 1), ymd(2021, 2, 28)));
        assert_eq!(month_bounds(2021, Some(12)), (ymd(2021, 12, 1), ymd(2021, 12, 31)));
        assert_eq!(month_bounds(2021, None), (ymd(2021, 1, 1), ymd(2021, 12, 31)));

        let region = RegionInfo {
            id: "X".into(),
            name: "X".into(),
            modis_years: BTreeSet::from([2021]),
            viirs_years: BTreeSet::from([2021]),
            noaa20_years: BTreeSet::new(),
            bytes: 0,
            files: Vec::new(),
        };
        let cov = Coverage {
            modis: Some(DateSpan { first: ymd(2000, 11, 1), last: ymd(2022, 1, 1) }),
            viirs: None,
            snpp: Some(DateSpan { first: ymd(2012, 1, 20), last: ymd(2022, 1, 1) }),
            noaa20: Some(DateSpan { first: ymd(2018, 1, 1), last: ymd(2022, 1, 1) }),
        };
        let (m, v) = family_coverage(&region, 2021, &cov);
        assert_eq!(m, Some((ymd(2000, 11, 1), ymd(2022, 1, 1))));
        assert_eq!(v, Some((ymd(2012, 1, 20), ymd(2022, 1, 1))));
        assert_eq!(family_coverage(&region, 2019, &cov), (None, None));
    }

    #[test]
    fn change_pct_handles_zero_base() {
        assert_eq!(change_pct(0, 5), None);
        assert!((change_pct(18_396, 31_886).unwrap() - 73.331_159).abs() < 1e-3);
    }

    #[test]
    fn extent_covers_all_points() {
        assert_eq!(extent(std::iter::empty()), None);
        let e = extent([(39.0, 30.0), (37.5, 44.0), (41.0, 26.0)].into_iter()).unwrap();
        assert_eq!(
            e,
            Extent {
                west: 26.0,
                south: 37.5,
                east: 44.0,
                north: 41.0
            }
        );
    }

    #[tokio::test]
    async fn noaa20_files_are_indexed_in_regions_series_and_points() {
        let dir = temp_archive("noaa20");
        write_file(&dir, "viirs-snpp", 2020, "Turkey", &[(39.0, "2020-07-01", 0)]);
        write_file(
            &dir,
            "viirs-jpss1",
            2020,
            "Turkey",
            &[(39.0, "2020-07-01", 0), (40.5, "2020-08-15", 0)],
        );
        // NOAA-20-only year for a second region confirms points/series work without SNPP or MODIS.
        write_file(&dir, "viirs-jpss1", 2021, "OnlyN20", &[(38.0, "2021-06-01", 0)]);

        let snpp_bytes = std::fs::metadata(dir.join("viirs-snpp/2020/viirs-snpp_2020_Turkey.csv"))
            .unwrap()
            .len();
        let n20_bytes = std::fs::metadata(dir.join("viirs-jpss1/2020/viirs-jpss1_2020_Turkey.csv"))
            .unwrap()
            .len();
        // Budget admits SNPP alone, but rejects Turkey once NOAA-20 bytes are included.
        let state_tight = state(dir.clone(), snpp_bytes);
        let Json(listing) = regions(State(state_tight)).await.ok().unwrap();
        let turkey = listing.regions.iter().find(|r| r.id == "Turkey").unwrap();
        assert_eq!(turkey.modis_years, None);
        assert_eq!(turkey.viirs_years, Some([2020, 2020]));
        assert_eq!(turkey.noaa20_years, Some([2020, 2020]));
        assert_eq!(turkey.size_mb, round_mb(snpp_bytes + n20_bytes));
        assert!(turkey.too_large);

        let only_n20 = listing.regions.iter().find(|r| r.id == "OnlyN20").unwrap();
        assert_eq!(only_n20.viirs_years, None);
        assert_eq!(only_n20.noaa20_years, Some([2021, 2021]));

        let state_open = state(dir.clone(), u64::MAX);
        let series_tr = series_json(&state_open, "Turkey").await.ok().unwrap();
        let year_2020 = &series_tr["years"][0];
        assert_eq!(year_2020["raw_viirs"], 3);
        // The co-located SNPP + NOAA-20 detection on 2020-07-01 merges in VIIRS fleet consolidation.
        assert_eq!(year_2020["harmonized"], 2);

        let Json(pts) = points(
            State(state_open.clone()),
            Query(PointsParams {
                region: Some("OnlyN20".into()),
                year: Some("2021".into()),
                month: None,
                limit: None,
                ..Default::default()
            }),
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(pts.stages.raw_viirs, 1);
        assert_eq!(pts.stages.harmonized, 1);
        assert_eq!(pts.raw[0].sensor, "VIIRS");
        assert!(pts.harmonized[0].hotspot.platforms.contains(&Sensor::ViirsNOAA20));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn series_disk_cache_roundtrip_serves_identical_bytes() {
        let dir = temp_archive("derived-roundtrip");
        write_file(
            &dir,
            "modis",
            2015,
            "Alpha",
            &[(39.0, "2015-07-01", 0), (39.5, "2015-08-15", 0)],
        );
        write_file(&dir, "viirs-snpp", 2015, "Alpha", &[(39.0, "2015-07-01", 0)]);

        let first_state = state(dir.clone(), u64::MAX);
        let (src1, body1) = series_raw(&first_state, "Alpha").await.ok().unwrap();
        assert_eq!(src1.as_deref(), Some("computed"));

        // New ArchiveState simulates a server restart with empty in-memory OnceCells.
        let second_state = state(dir.clone(), u64::MAX);
        let (src2, body2) = series_raw(&second_state, "Alpha").await.ok().unwrap();
        assert_eq!(src2.as_deref(), Some("disk"));
        assert_eq!(body1, body2);

        let (src3, body3) = series_raw(&second_state, "Alpha").await.ok().unwrap();
        assert_eq!(src3.as_deref(), Some("memory"));
        assert_eq!(body1, body3);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn series_disk_cache_invalidates_on_csv_change_and_config_change() {
        let dir = temp_archive("derived-invalidate");
        let derived = dir.join("_derived_cache");
        write_file(&dir, "modis", 2015, "Beta", &[(39.0, "2015-07-01", 0)]);

        let s1 = state_with_config(
            dir.clone(),
            derived.clone(),
            u64::MAX,
            PipelineConfig::default(),
        );
        let (src1, body1) = series_raw(&s1, "Beta").await.ok().unwrap();
        assert_eq!(src1.as_deref(), Some("computed"));
        let json1: serde_json::Value = serde_json::from_slice(&body1).unwrap();
        assert_eq!(json1["years"][0]["raw_total"], 1);

        // Appending a row changes the CSV byte size, invalidating the file stamp.
        let csv_path = dir.join("modis/2015/modis_2015_Beta.csv");
        let mut contents = std::fs::read_to_string(&csv_path).unwrap();
        contents.push_str("40.5,32.0,320,0.5,0.5,2015-07-02,1200,Terra,MODIS,80,6.2,290,10,D,0\r\n");
        std::fs::write(&csv_path, contents).unwrap();

        let s2 = state_with_config(
            dir.clone(),
            derived.clone(),
            u64::MAX,
            PipelineConfig::default(),
        );
        let (src2, body2) = series_raw(&s2, "Beta").await.ok().unwrap();
        assert_eq!(src2.as_deref(), Some("computed"));
        let json2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
        assert_eq!(json2["years"][0]["raw_total"], 2);

        // Changing PipelineConfig invalidates the config digest in the stamp.
        let mut alt_cfg = PipelineConfig::default();
        alt_cfg.matching.window_secs += 60;
        let s3 = state_with_config(dir.clone(), derived.clone(), u64::MAX, alt_cfg);
        let (src3, _) = series_raw(&s3, "Beta").await.ok().unwrap();
        assert_eq!(src3.as_deref(), Some("computed"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn series_disk_cache_and_memory_bypass_admit_limit() {
        let dir = temp_archive("derived-admit-bypass");
        let big: Vec<(f64, &str, u8)> = (0..100)
            .map(|i| (30.0 + f64::from(i) * 0.05, "2018-07-01", 0))
            .collect();
        write_file(&dir, "modis", 2018, "Huge", &big);

        let tight_state = state(dir.clone(), 100);
        let err = series_raw(&tight_state, "Huge").await.err().unwrap();
        assert_eq!(err.status, StatusCode::PAYLOAD_TOO_LARGE);
        assert!(err.message.contains("make precompute"), "{}", err.message);

        // Precompute ignores max_region_bytes and populates the disk cache.
        let precompute_state = state(dir.clone(), 100);
        let report = precompute_series(
            precompute_state,
            &PrecomputeOptions {
                regions: vec!["Huge".to_string()],
                force: false,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(report.computed(), 1);
        assert_eq!(report.failed(), 0);

        // Now the exact same tight_state serves Huge from disk (and then memory) with 200 OK.
        let (src_disk, body_disk) = series_raw(&tight_state, "Huge").await.ok().unwrap();
        assert_eq!(src_disk.as_deref(), Some("disk"));
        let json: serde_json::Value = serde_json::from_slice(&body_disk).unwrap();
        assert_eq!(json["years"][0]["raw_total"], 100);

        let (src_mem, body_mem) = series_raw(&tight_state, "Huge").await.ok().unwrap();
        assert_eq!(src_mem.as_deref(), Some("memory"));
        assert_eq!(body_disk, body_mem);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn series_succeeds_when_derived_cache_dir_is_unwritable() {
        let dir = temp_archive("derived-unwritable");
        write_file(&dir, "modis", 2015, "Gamma", &[(39.0, "2015-07-01", 0)]);
        // A regular file cannot be created as a directory, simulating an unwritable cache path.
        let blocker_file = dir.join("not-a-directory");
        std::fs::write(&blocker_file, b"occupied").unwrap();

        let s = state_with_config(
            dir.clone(),
            blocker_file,
            u64::MAX,
            PipelineConfig::default(),
        );
        let (src, body) = series_raw(&s, "Gamma").await.ok().unwrap();
        assert_eq!(src.as_deref(), Some("computed"));
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["region"], "Gamma");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn series_ignores_corrupt_disk_cache_and_recomputes() {
        let dir = temp_archive("derived-corrupt");
        let derived = dir.join("_derived_cache");
        write_file(&dir, "modis", 2015, "Delta", &[(39.0, "2015-07-01", 0)]);

        let s1 = state(dir.clone(), u64::MAX);
        let (src1, body1) = series_raw(&s1, "Delta").await.ok().unwrap();
        assert_eq!(src1.as_deref(), Some("computed"));

        // Corrupt the cached JSON body; the checksum mismatch must force recomputation.
        std::fs::write(derived.join("series/Delta.json"), b"{corrupt").unwrap();
        let s2 = state(dir.clone(), u64::MAX);
        let (src2, body2) = series_raw(&s2, "Delta").await.ok().unwrap();
        assert_eq!(src2.as_deref(), Some("computed"));
        assert_eq!(body1, body2);

        // Corrupt the metadata stamp; JSON parse failure must force recomputation.
        std::fs::write(derived.join("series/Delta.meta.json"), b"not-json").unwrap();
        let s3 = state(dir.clone(), u64::MAX);
        let (src3, body3) = series_raw(&s3, "Delta").await.ok().unwrap();
        assert_eq!(src3.as_deref(), Some("computed"));
        assert_eq!(body1, body3);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn precompute_skips_valid_regions_and_recomputes_with_force() {
        let dir = temp_archive("precompute-cmd");
        write_file(&dir, "modis", 2015, "Cyprus", &[(35.0, "2015-07-01", 0)]);
        write_file(&dir, "modis", 2015, "Armenia", &[(40.0, "2015-07-01", 0)]);

        let make_state = || {
            Arc::new(
                ArchiveState::new(
                    dir.clone(),
                    ArchiveLimits {
                        max_region_bytes: u64::MAX,
                        request_timeout: Duration::from_secs(30),
                    },
                    Arc::new(PipelineConfig::default()),
                )
                .with_derived_cache_dir(dir.join("_derived_cache"))
                .with_bundled_cache_dir(dir.join("_bundled_cache"))
                .with_year_concurrency(2),
            )
        };

        let r1 = precompute_series(
            make_state(),
            &PrecomputeOptions {
                regions: Vec::new(),
                force: false,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(r1.computed(), 2);
        assert_eq!(r1.skipped(), 0);
        assert_eq!(r1.failed(), 0);

        let r2 = precompute_series(
            make_state(),
            &PrecomputeOptions {
                regions: vec!["Cyprus".to_string(), "Armenia".to_string()],
                force: false,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(r2.computed(), 0);
        assert_eq!(r2.skipped(), 2);
        assert_eq!(r2.failed(), 0);

        let r3 = precompute_series(
            make_state(),
            &PrecomputeOptions {
                regions: vec!["Cyprus".to_string()],
                force: true,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(r3.computed(), 1);
        assert_eq!(r3.skipped(), 0);
        assert_eq!(r3.failed(), 0);

        let r4 = precompute_series(
            make_state(),
            &PrecomputeOptions {
                regions: vec!["Cyprus".to_string(), "MissingLand".to_string()],
                force: false,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(r4.skipped(), 1);
        assert_eq!(r4.failed(), 1);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn regions_reports_precomputed_and_clears_too_large_until_invalidated() {
        let dir = temp_archive("regions-precomputed");
        write_file(&dir, "modis", 2015, "Small", &[(39.0, "2015-07-01", 0)]);
        let big: Vec<(f64, &str, u8)> = (0..120)
            .map(|i| (30.0 + f64::from(i) * 0.05, "2015-07-01", 0))
            .collect();
        write_file(&dir, "modis", 2015, "Big", &big);

        let small_bytes = std::fs::metadata(dir.join("modis/2015/modis_2015_Small.csv"))
            .unwrap()
            .len();
        let limit = small_bytes * 2;

        fn find_entry<'a>(listing: &'a RegionsResponse, id: &str) -> &'a RegionEntry {
            listing.regions.iter().find(|r| r.id == id).unwrap()
        }

        // Before precompute: Big is too_large and not precomputed; Small is admitted.
        let s1 = state(dir.clone(), limit);
        let Json(list1) = regions(State(s1)).await.ok().unwrap();
        let big1 = find_entry(&list1, "Big");
        assert!(big1.too_large);
        assert!(!big1.precomputed);
        let small1 = find_entry(&list1, "Small");
        assert!(!small1.too_large);
        assert!(!small1.precomputed);

        // Precompute only Big into the disk cache.
        let precompute_state = state(dir.clone(), limit);
        let report = precompute_series(
            precompute_state,
            &PrecomputeOptions {
                regions: vec!["Big".to_string()],
                force: false,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(report.computed(), 1);

        // Fresh ArchiveState has empty memory cache, so /api/regions sees Big via disk metadata only.
        let s2 = state(dir.clone(), limit);
        let Json(list2) = regions(State(s2.clone())).await.ok().unwrap();
        let big2 = find_entry(&list2, "Big");
        assert!(!big2.too_large);
        assert!(big2.precomputed);
        let small2 = find_entry(&list2, "Small");
        assert!(!small2.too_large);
        assert!(!small2.precomputed);

        // Loading /api/series populates memory cache; even if the disk meta disappears,
        // /api/regions still keeps too_large=false while served from memory.
        let (src_mem_load, _) = series_raw(&s2, "Big").await.ok().unwrap();
        assert_eq!(src_mem_load.as_deref(), Some("disk"));
        std::fs::remove_file(dir.join("_derived_cache/series/Big.meta.json")).unwrap();
        let Json(list2_mem) = regions(State(s2)).await.ok().unwrap();
        let big2_mem = find_entry(&list2_mem, "Big");
        assert!(!big2_mem.too_large);
        assert!(!big2_mem.precomputed);

        // Appending a row to Big's CSV invalidates the stamp, restoring too_large=true, precomputed=false.
        let csv_path = dir.join("modis/2015/modis_2015_Big.csv");
        let mut contents = std::fs::read_to_string(&csv_path).unwrap();
        contents.push_str("41.0,32.0,320,0.5,0.5,2015-07-02,1200,Terra,MODIS,80,6.2,290,10,D,0\r\n");
        std::fs::write(&csv_path, contents).unwrap();

        let s3 = state(dir.clone(), limit);
        let Json(list3) = regions(State(s3)).await.ok().unwrap();
        let big3 = find_entry(&list3, "Big");
        assert!(big3.too_large);
        assert!(!big3.precomputed);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn density_validates_params_and_matches_points_totals() {
        let dir = temp_archive("density");
        write_file(
            &dir,
            "modis",
            2021,
            "Malta",
            &[(35.9, "2021-07-01", 0), (35.95, "2021-03-05", 0)],
        );
        write_file(
            &dir,
            "viirs-snpp",
            2021,
            "Malta",
            &[(35.9, "2021-07-01", 0), (36.0, "2021-07-02", 0), (36.3, "2021-12-31", 0)],
        );
        let state = state(dir.clone(), u64::MAX);
        let params = |year: Option<&str>, cell: Option<&str>| {
            Query(DensityParams {
                region: Some("Malta".into()),
                year: year.map(Into::into),
                cell: cell.map(Into::into),
                ..DensityParams::default()
            })
        };

        for bad in ["0.3", "2", "abc", "0"] {
            let err = density(State(state.clone()), params(Some("2021"), Some(bad)))
                .await
                .err()
                .expect("bad cell must fail");
            assert_eq!(err.status, StatusCode::BAD_REQUEST, "cell={bad}");
        }
        let err = density(State(state.clone()), params(None, None))
            .await
            .err()
            .expect("missing year must fail");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);

        let Json(d) = density(State(state.clone()), params(Some("2021"), None))
            .await
            .ok()
            .unwrap();
        assert_eq!(d.cell_deg, 0.25);
        assert_eq!(d.grid, "global_latlon");
        assert_eq!(d.months.len(), 12);
        assert!(d.months.iter().enumerate().all(|(i, m)| m.month == i as u32 + 1));

        let Json(p) = points(
            State(state.clone()),
            Query(PointsParams {
                region: Some("Malta".into()),
                year: Some("2021".into()),
                ..PointsParams::default()
            }),
        )
        .await
        .ok()
        .unwrap();
        let raw_sum: u64 = d.months.iter().map(|m| m.raw_total).sum();
        let harm_sum: u64 = d.months.iter().map(|m| m.harmonized_total).sum();
        assert_eq!(raw_sum as usize, p.stages.raw_modis + p.stages.raw_viirs);
        assert_eq!(harm_sum as usize, p.stages.harmonized);
        assert_eq!(d.bbox, p.bbox);

        let (mut max_rc, mut max_rf, mut max_hc, mut max_hf) = (0, 0.0f64, 0, 0.0f64);
        for m in &d.months {
            assert!(m.cells.windows(2).all(|w| (w[0].0, w[0].1) < (w[1].0, w[1].1)));
            assert_eq!(m.cells.iter().map(|c| c.2).sum::<u64>(), m.raw_total);
            assert_eq!(m.cells.iter().map(|c| c.4).sum::<u64>(), m.harmonized_total);
            for c in &m.cells {
                max_rc = max_rc.max(c.2);
                max_rf = max_rf.max(c.3);
                max_hc = max_hc.max(c.4);
                max_hf = max_hf.max(c.5);
            }
        }
        assert_eq!(
            (d.max.raw_count, d.max.raw_frp, d.max.harm_count, d.max.harm_frp),
            (max_rc, max_rf, max_hc, max_hf)
        );
        assert!(!d.covered_months.is_empty() || d.months.iter().any(|m| m.raw_total > 0));

        // Overrides must flow through the same path as /api/points.
        for (min_confidence, fusion_model) in [("high", None), ("low", Some("modis_calibrated"))] {
            let Json(d) = density(
                State(state.clone()),
                Query(DensityParams {
                    region: Some("Malta".into()),
                    year: Some("2021".into()),
                    min_confidence: Some(min_confidence.into()),
                    fusion_model: fusion_model.map(Into::into),
                    ..DensityParams::default()
                }),
            )
            .await
            .ok()
            .unwrap();
            let Json(p) = points(
                State(state.clone()),
                Query(PointsParams {
                    region: Some("Malta".into()),
                    year: Some("2021".into()),
                    min_confidence: Some(min_confidence.into()),
                    fusion_model: fusion_model.map(Into::into),
                    ..PointsParams::default()
                }),
            )
            .await
            .ok()
            .unwrap();
            assert_eq!(
                d.months.iter().map(|m| m.raw_total).sum::<u64>() as usize,
                p.stages.raw_modis + p.stages.raw_viirs
            );
            assert_eq!(
                d.months.iter().map(|m| m.harmonized_total).sum::<u64>() as usize,
                p.stages.harmonized
            );
        }

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn points_min_confidence_drops_low_rows_before_harmonizing() {
        let dir = temp_archive("points-min-confidence");
        let path = dir.join("viirs-snpp").join("2021");
        std::fs::create_dir_all(&path).unwrap();
        let mut csv = format!("{VIIRS_HEADER}\r\n");
        for (lat, date, conf) in [
            (39.0, "2021-07-01", "n"),
            (39.5, "2021-07-02", "h"),
            (40.0, "2021-07-03", "l"),
            (40.5, "2021-07-04", "l"),
        ] {
            csv.push_str(&format!(
                "{lat},32.0,320,0.5,0.5,{date},1200,N,VIIRS,{conf},2,290,10,D,0\r\n"
            ));
        }
        std::fs::write(path.join("viirs-snpp_2021_Malta.csv"), csv).unwrap();
        let state = state(dir.clone(), u64::MAX);

        let query = |min_confidence: Option<&str>| PointsParams {
            region: Some("Malta".into()),
            year: Some("2021".into()),
            month: None,
            limit: None,
            min_confidence: min_confidence.map(str::to_string),
            ..Default::default()
        };

        let Json(all) = points(State(state.clone()), Query(query(None))).await.ok().unwrap();
        assert_eq!(all.stages.raw_viirs, 4);
        assert_eq!(all.stages.harmonized, 4);
        assert_eq!(all.filters.min_confidence, MinConfidence::Low);
        assert_eq!(all.filters.excluded_detections, 0);

        let Json(nominal) = points(State(state.clone()), Query(query(Some("nominal"))))
            .await
            .ok()
            .unwrap();
        assert_eq!(nominal.stages.raw_viirs, 2);
        assert_eq!(nominal.stages.harmonized, 2);
        assert_eq!(nominal.raw.len(), 2);
        assert!(nominal.raw.iter().all(|p| p.confidence != Confidence::Low));
        assert_eq!(nominal.filters.min_confidence, MinConfidence::Nominal);
        assert_eq!(nominal.filters.excluded_detections, 2);

        let err = points(State(state), Query(query(Some("medium"))))
            .await
            .err()
            .expect("unknown min_confidence must fail with 400");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("min_confidence"), "{}", err.message);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn points_flag_tropical_swath_gap_from_the_whole_year_extent() {
        let dir = temp_archive("points-swath-gap");
        // July sits outside the belt; August reaches into it, so the yearly extent is 25..35.
        write_file(
            &dir,
            "modis",
            2010,
            "Savanna",
            &[(35.0, "2010-07-01", 0), (34.0, "2010-07-02", 0), (25.0, "2010-08-01", 0)],
        );
        write_file(&dir, "modis", 2010, "Steppe", &[(45.0, "2010-07-01", 0)]);
        let state = state(dir.clone(), u64::MAX);
        let query = |region: &str| PointsParams {
            region: Some(region.into()),
            year: Some("2010".into()),
            month: Some("7".into()),
            limit: None,
            ..Default::default()
        };

        let Json(tropical) = points(State(state.clone()), Query(query("Savanna"))).await.ok().unwrap();
        let json = serde_json::to_value(&tropical).unwrap();
        let flag = &json["tropical_swath_gap"];
        assert_eq!(flag["basis"], "detections");
        assert_eq!(flag["missed_frp_low_pct"], 10.7);
        assert_eq!(flag["missed_frp_high_pct"], 12.5);
        assert!(flag["compensation"].is_null());
        let share = flag["belt_share"].as_f64().unwrap();
        let expected = pyrosync_core::effort::tropical_belt_share(25.0, 35.0);
        assert!((share - expected).abs() < 1e-12, "{share} vs {expected}");

        let Json(steppe) = points(State(state), Query(query("Steppe"))).await.ok().unwrap();
        let json = serde_json::to_value(&steppe).unwrap();
        assert!(json["tropical_swath_gap"].is_null());
        assert!(json.as_object().unwrap().contains_key("tropical_swath_gap"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn points_admits_by_requested_year_not_total_region_size() {
        let dir = temp_archive("points-year-admit");
        // Several modest years whose sum exceeds the limit, plus one oversized year.
        let small_rows: Vec<(f64, &str, u8)> = vec![
            (39.0, "2018-07-01", 0),
            (39.1, "2018-07-02", 0),
        ];
        write_file(&dir, "modis", 2018, "Multi", &small_rows);
        write_file(
            &dir,
            "modis",
            2019,
            "Multi",
            &[(39.0, "2019-07-01", 0), (39.1, "2019-07-02", 0)],
        );
        let big_rows: Vec<(f64, &str, u8)> = (0..120)
            .map(|i| (30.0 + f64::from(i) * 0.05, "2020-07-01", 0))
            .collect();
        write_file(&dir, "modis", 2020, "Multi", &big_rows);

        let year_2018_bytes = std::fs::metadata(dir.join("modis/2018/modis_2018_Multi.csv"))
            .unwrap()
            .len();
        // Limit admits 2018 and 2019 individually, while total region bytes (2018+2019+2020)
        // and 2020 alone exceed the limit.
        let state = state(dir.clone(), year_2018_bytes * 2);

        let Json(ok_pts) = points(
            State(state.clone()),
            Query(PointsParams {
                region: Some("Multi".into()),
                year: Some("2018".into()),
                month: None,
                limit: None,
                ..Default::default()
            }),
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(ok_pts.year, 2018);
        assert_eq!(ok_pts.stages.raw_modis, 2);

        let err = points(
            State(state),
            Query(PointsParams {
                region: Some("Multi".into()),
                year: Some("2020".into()),
                month: None,
                limit: None,
                ..Default::default()
            }),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(err.status, StatusCode::PAYLOAD_TOO_LARGE);
        assert!(err.message.contains("2020"), "{}", err.message);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Copies a computed `series/<region>.{json,meta.json}` pair from `src` to `dst`.
    fn copy_series_files(src: &Path, dst: &Path, region: &str) {
        let dst_series = dst.join("series");
        std::fs::create_dir_all(&dst_series).unwrap();
        for suffix in ["json", "meta.json"] {
            let name = format!("{region}.{suffix}");
            std::fs::copy(src.join("series").join(&name), dst_series.join(&name)).unwrap();
        }
    }

    #[tokio::test]
    async fn series_disk_cache_survives_archive_mtime_change_and_invalidates_on_size_change() {
        let dir = temp_archive("mtime-independent");
        write_file(&dir, "modis", 2015, "Cyprus", &[(35.0, "2015-07-01", 0)]);

        let report = precompute_series(
            state(dir.clone(), u64::MAX),
            &PrecomputeOptions {
                regions: vec!["Cyprus".to_string()],
                force: false,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(report.computed(), 1);

        // Changing mtime without changing size or content must keep the stamp valid (machine-independent).
        let csv_path = dir.join("modis/2015/modis_2015_Cyprus.csv");
        let past = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&csv_path)
            .unwrap()
            .set_modified(past)
            .unwrap();

        let s_after_mtime = state(dir.clone(), u64::MAX);
        let (src_mtime, _) = series_raw(&s_after_mtime, "Cyprus").await.ok().unwrap();
        assert_eq!(src_mtime.as_deref(), Some("disk"));

        // Changing byte size must still invalidate the stamp and recompute.
        let mut contents = std::fs::read_to_string(&csv_path).unwrap();
        contents.push_str("35.5,32.0,320,0.5,0.5,2015-07-02,1200,Terra,MODIS,80,6.2,290,10,D,0\r\n");
        std::fs::write(&csv_path, contents).unwrap();

        let s_after_size = state(dir.clone(), u64::MAX);
        let (src_size, body_size) = series_raw(&s_after_size, "Cyprus").await.ok().unwrap();
        assert_eq!(src_size.as_deref(), Some("computed"));
        let json: serde_json::Value = serde_json::from_slice(&body_size).unwrap();
        assert_eq!(json["years"][0]["raw_total"], 2);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn series_serves_from_bundled_without_writing_disk_then_memory() {
        let dir = temp_archive("bundled-hit");
        write_file(&dir, "modis", 2015, "Malta", &[(35.9, "2015-07-01", 0)]);

        let staging_dir = dir.join("_staging");
        let bundled_dir = dir.join("_bundled");
        let writable_dir = dir.join("_writable");

        let staging_state = state_with_caches(
            dir.clone(),
            staging_dir.clone(),
            dir.join("_unused_bundled"),
            u64::MAX,
            PipelineConfig::default(),
        );
        let (src_comp, body_comp) = series_raw(&staging_state, "Malta").await.ok().unwrap();
        assert_eq!(src_comp.as_deref(), Some("computed"));
        copy_series_files(&staging_dir, &bundled_dir, "Malta");

        let s = state_with_caches(
            dir.clone(),
            writable_dir.clone(),
            bundled_dir,
            u64::MAX,
            PipelineConfig::default(),
        );
        let (src_bundled, body_bundled) = series_raw(&s, "Malta").await.ok().unwrap();
        assert_eq!(src_bundled.as_deref(), Some("bundled"));
        assert_eq!(body_bundled, body_comp);
        assert!(
            !writable_dir.exists(),
            "serving from bundled must not create or write to the writable cache"
        );

        let (src_mem, body_mem) = series_raw(&s, "Malta").await.ok().unwrap();
        assert_eq!(src_mem.as_deref(), Some("memory"));
        assert_eq!(body_mem, body_comp);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn series_prefers_writable_disk_over_bundled() {
        let dir = temp_archive("disk-over-bundled");
        write_file(&dir, "modis", 2015, "Crete", &[(35.2, "2015-07-01", 0)]);

        let writable_dir = dir.join("_writable");
        let bundled_dir = dir.join("_bundled");

        let seed_state = state_with_caches(
            dir.clone(),
            writable_dir.clone(),
            dir.join("_unused_bundled"),
            u64::MAX,
            PipelineConfig::default(),
        );
        let (src_seed, _) = series_raw(&seed_state, "Crete").await.ok().unwrap();
        assert_eq!(src_seed.as_deref(), Some("computed"));
        copy_series_files(&writable_dir, &bundled_dir, "Crete");

        let s = state_with_caches(
            dir.clone(),
            writable_dir,
            bundled_dir,
            u64::MAX,
            PipelineConfig::default(),
        );
        let (src, _) = series_raw(&s, "Crete").await.ok().unwrap();
        assert_eq!(src.as_deref(), Some("disk"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn series_ignores_corrupt_bundled_body_without_modifying_bundled_file() {
        let dir = temp_archive("bundled-corrupt");
        write_file(&dir, "modis", 2015, "Rhodes", &[(36.2, "2015-07-01", 0)]);

        let staging_dir = dir.join("_staging");
        let bundled_dir = dir.join("_bundled");
        let writable_dir = dir.join("_writable");

        let staging_state = state_with_caches(
            dir.clone(),
            staging_dir.clone(),
            dir.join("_unused_bundled"),
            u64::MAX,
            PipelineConfig::default(),
        );
        let (_, expected_body) = series_raw(&staging_state, "Rhodes").await.ok().unwrap();
        copy_series_files(&staging_dir, &bundled_dir, "Rhodes");

        let corrupt_bytes = b"{corrupt-bundled";
        let bundled_body_path = bundled_dir.join("series/Rhodes.json");
        std::fs::write(&bundled_body_path, corrupt_bytes).unwrap();

        let s = state_with_caches(
            dir.clone(),
            writable_dir.clone(),
            bundled_dir,
            u64::MAX,
            PipelineConfig::default(),
        );
        let (src, body) = series_raw(&s, "Rhodes").await.ok().unwrap();
        assert_eq!(src.as_deref(), Some("computed"));
        assert_eq!(body, expected_body);
        assert_eq!(std::fs::read(&bundled_body_path).unwrap(), corrupt_bytes);
        assert!(writable_dir.join("series/Rhodes.json").is_file());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn regions_marks_bundled_oversized_region_available() {
        let dir = temp_archive("regions-bundled");
        let big: Vec<(f64, &str, u8)> = (0..120)
            .map(|i| (30.0 + f64::from(i) * 0.05, "2015-07-01", 0))
            .collect();
        write_file(&dir, "modis", 2015, "BigBundled", &big);

        let staging_dir = dir.join("_staging");
        let bundled_dir = dir.join("_bundled");
        let writable_dir = dir.join("_writable");

        let precompute_state = state_with_caches(
            dir.clone(),
            staging_dir.clone(),
            dir.join("_unused_bundled"),
            100,
            PipelineConfig::default(),
        );
        let report = precompute_series(
            precompute_state,
            &PrecomputeOptions {
                regions: vec!["BigBundled".to_string()],
                force: false,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(report.computed(), 1);
        copy_series_files(&staging_dir, &bundled_dir, "BigBundled");

        let s = state_with_caches(
            dir.clone(),
            writable_dir,
            bundled_dir,
            100,
            PipelineConfig::default(),
        );
        let Json(listing) = regions(State(s)).await.ok().unwrap();
        let entry = listing
            .regions
            .iter()
            .find(|r| r.id == "BigBundled")
            .unwrap();
        assert!(!entry.too_large);
        assert!(entry.precomputed);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Copies `coverage.{json,meta.json}` from `src` to `dst`.
    fn copy_coverage_files(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for name in ["coverage.json", "coverage.meta.json"] {
            std::fs::copy(src.join(name), dst.join(name)).unwrap();
        }
    }

    #[tokio::test]
    async fn precompute_skips_valid_bundled_region_and_force_writes_to_disk() {
        let dir = temp_archive("precompute-bundled");
        write_file(&dir, "modis", 2015, "Samos", &[(37.7, "2015-07-01", 0)]);

        let staging_dir = dir.join("_staging");
        let bundled_dir = dir.join("_bundled");
        let writable_dir = dir.join("_writable");

        let r_stage = precompute_series(
            state_with_caches(
                dir.clone(),
                staging_dir.clone(),
                dir.join("_unused_bundled"),
                u64::MAX,
                PipelineConfig::default(),
            ),
            &PrecomputeOptions {
                regions: vec!["Samos".to_string()],
                force: false,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(r_stage.computed(), 1);
        copy_series_files(&staging_dir, &bundled_dir, "Samos");
        copy_coverage_files(&staging_dir, &bundled_dir);

        let r_skip = precompute_series(
            state_with_caches(
                dir.clone(),
                writable_dir.clone(),
                bundled_dir.clone(),
                u64::MAX,
                PipelineConfig::default(),
            ),
            &PrecomputeOptions {
                regions: vec!["Samos".to_string()],
                force: false,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(r_skip.skipped(), 1);
        assert_eq!(r_skip.computed(), 0);
        assert_eq!(r_skip.failed(), 0);
        assert!(!writable_dir.exists());

        let r_force = precompute_series(
            state_with_caches(
                dir.clone(),
                writable_dir.clone(),
                bundled_dir,
                u64::MAX,
                PipelineConfig::default(),
            ),
            &PrecomputeOptions {
                regions: vec!["Samos".to_string()],
                force: true,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(r_force.computed(), 1);
        assert_eq!(r_force.skipped(), 0);
        assert_eq!(r_force.failed(), 0);
        assert!(writable_dir.join("coverage.json").is_file());
        assert!(writable_dir.join("coverage.meta.json").is_file());
        assert!(writable_dir.join("series/Samos.json").is_file());
        assert!(writable_dir.join("series/Samos.meta.json").is_file());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn series_treats_v1_schema_with_mtime_secs_as_stale_and_recomputes() {
        let dir = temp_archive("schema-v1-stale");
        write_file(&dir, "modis", 2015, "Lesbos", &[(39.1, "2015-07-01", 0)]);

        let writable_dir = dir.join("_writable");
        let bundled_dir = dir.join("_bundled");

        let seed_state = state_with_caches(
            dir.clone(),
            writable_dir.clone(),
            bundled_dir.clone(),
            u64::MAX,
            PipelineConfig::default(),
        );
        let (_, real_body) = series_raw(&seed_state, "Lesbos").await.ok().unwrap();

        let csv_path = dir.join("modis/2015/modis_2015_Lesbos.csv");
        let csv_bytes = std::fs::metadata(&csv_path).unwrap().len();
        let v1_meta = serde_json::json!({
            "schema_version": 1,
            "config_hash": crate::derived_cache::config_digest(&PipelineConfig::default()),
            "files": [{
                "path": "modis/2015/modis_2015_Lesbos.csv",
                "bytes": csv_bytes,
                "mtime_secs": 1_700_000_000u64,
            }],
            "body_hash": format!("{:016x}", crate::derived_cache::fnv1a_64(&real_body)),
        });
        let v1_meta_bytes = serde_json::to_vec_pretty(&v1_meta).unwrap();

        std::fs::create_dir_all(bundled_dir.join("series")).unwrap();
        std::fs::write(bundled_dir.join("series/Lesbos.json"), &real_body).unwrap();
        std::fs::write(bundled_dir.join("series/Lesbos.meta.json"), &v1_meta_bytes).unwrap();
        std::fs::write(writable_dir.join("series/Lesbos.meta.json"), &v1_meta_bytes).unwrap();

        let s = state_with_caches(
            dir.clone(),
            writable_dir.clone(),
            bundled_dir,
            u64::MAX,
            PipelineConfig::default(),
        );
        let (src, body) = series_raw(&s, "Lesbos").await.ok().unwrap();
        assert_eq!(src.as_deref(), Some("computed"));
        assert_eq!(body, real_body);

        let refreshed_meta: serde_json::Value = serde_json::from_slice(
            &std::fs::read(writable_dir.join("series/Lesbos.meta.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            refreshed_meta["schema_version"].as_u64(),
            Some(u64::from(crate::derived_cache::DERIVED_SCHEMA_VERSION))
        );
        assert!(refreshed_meta["files"][0].get("mtime_secs").is_none());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn series_treats_v3_schema_as_stale_and_recomputes() {
        let dir = temp_archive("schema-v3-stale");
        write_file(&dir, "modis", 2015, "Chios", &[(38.4, "2015-07-01", 0)]);

        let writable_dir = dir.join("_writable");
        let bundled_dir = dir.join("_bundled");

        let seed_state = state_with_caches(
            dir.clone(),
            writable_dir.clone(),
            bundled_dir.clone(),
            u64::MAX,
            PipelineConfig::default(),
        );
        let (_, real_body) = series_raw(&seed_state, "Chios").await.ok().unwrap();

        let csv_path = dir.join("modis/2015/modis_2015_Chios.csv");
        let csv_bytes = std::fs::metadata(&csv_path).unwrap().len();
        let v3_meta = serde_json::json!({
            "schema_version": 3,
            "config_hash": crate::derived_cache::config_digest(&PipelineConfig::default()),
            "files": [{
                "path": "modis/2015/modis_2015_Chios.csv",
                "bytes": csv_bytes,
            }],
            "body_hash": format!("{:016x}", crate::derived_cache::fnv1a_64(&real_body)),
        });
        let v3_meta_bytes = serde_json::to_vec_pretty(&v3_meta).unwrap();

        std::fs::create_dir_all(bundled_dir.join("series")).unwrap();
        std::fs::write(bundled_dir.join("series/Chios.json"), &real_body).unwrap();
        std::fs::write(bundled_dir.join("series/Chios.meta.json"), &v3_meta_bytes).unwrap();
        std::fs::write(writable_dir.join("series/Chios.meta.json"), &v3_meta_bytes).unwrap();

        let s = state_with_caches(
            dir.clone(),
            writable_dir.clone(),
            bundled_dir,
            u64::MAX,
            PipelineConfig::default(),
        );
        let (src, body) = series_raw(&s, "Chios").await.ok().unwrap();
        assert_eq!(src.as_deref(), Some("computed"));
        assert_eq!(body, real_body);

        let refreshed_meta: serde_json::Value = serde_json::from_slice(
            &std::fs::read(writable_dir.join("series/Chios.meta.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            refreshed_meta["schema_version"].as_u64(),
            Some(u64::from(crate::derived_cache::DERIVED_SCHEMA_VERSION))
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn series_month_mean_frp_and_uncertainty_round_trips() {
        let dir = temp_archive("month-mean-frp");
        write_file(
            &dir,
            "modis",
            2015,
            "FrpLand",
            &[(39.0, "2015-06-01", 0), (39.5, "2015-08-01", 0)],
        );
        write_file(
            &dir,
            "viirs-snpp",
            2015,
            "FrpLand",
            &[(39.0, "2015-06-01", 0)],
        );

        let s1 = state(dir.clone(), u64::MAX);
        let (src1, body1) = series_raw(&s1, "FrpLand").await.ok().unwrap();
        assert_eq!(src1.as_deref(), Some("computed"));

        let s2 = state(dir.clone(), u64::MAX);
        let (src2, body2) = series_raw(&s2, "FrpLand").await.ok().unwrap();
        assert_eq!(src2.as_deref(), Some("disk"));
        assert_eq!(body1, body2);

        let json: serde_json::Value = serde_json::from_slice(&body2).unwrap();
        let months = json["months"].as_array().unwrap();
        let listed: Vec<u64> = months.iter().map(|m| m["month"].as_u64().unwrap()).collect();
        // Uncovered months (1..=5, 9..=12) are omitted; covered empty month 7 stays with None.
        assert_eq!(listed, [6, 7, 8]);

        for m in [6u64, 8u64] {
            let entry = months.iter().find(|e| e["month"] == m).unwrap();
            let mean = entry["mean_frp"].as_f64().expect("mean_frp must be present");
            let lower = entry["mean_frp_lower"].as_f64().expect("mean_frp_lower must be present");
            let upper = entry["mean_frp_upper"].as_f64().expect("mean_frp_upper must be present");
            assert!(0.0 <= lower && lower <= mean && mean <= upper);
        }

        let empty_july = months.iter().find(|e| e["month"] == 7).unwrap();
        assert_eq!(empty_july["harmonized"], 0);
        assert_eq!(empty_july.get("mean_frp"), Some(&serde_json::Value::Null));
        assert_eq!(empty_july.get("mean_frp_lower"), Some(&serde_json::Value::Null));
        assert_eq!(empty_july.get("mean_frp_upper"), Some(&serde_json::Value::Null));

        let round_tripped: Vec<MonthEntry> =
            serde_json::from_value(json["months"].clone()).unwrap();
        let reserialized = serde_json::to_value(&round_tripped).unwrap();
        assert_eq!(reserialized, json["months"]);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn regions_rejects_corrupt_or_missing_body_until_repaired() {
        let dir = temp_archive("regions-corrupt-body");
        let big: Vec<(f64, &str, u8)> = (0..120)
            .map(|i| (30.0 + f64::from(i) * 0.05, "2015-07-01", 0))
            .collect();
        write_file(&dir, "modis", 2015, "BigCorrupt", &big);

        let staging_dir = dir.join("_staging");
        let bundled_dir = dir.join("_bundled");
        let writable_dir = dir.join("_writable");

        let report = precompute_series(
            state_with_caches(
                dir.clone(),
                staging_dir.clone(),
                dir.join("_unused_bundled"),
                100,
                PipelineConfig::default(),
            ),
            &PrecomputeOptions {
                regions: vec!["BigCorrupt".to_string()],
                force: false,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(report.computed(), 1);
        copy_series_files(&staging_dir, &bundled_dir, "BigCorrupt");

        let bundled_body_path = bundled_dir.join("series/BigCorrupt.json");
        let valid_body = std::fs::read(&bundled_body_path).unwrap();

        let s = state_with_caches(
            dir.clone(),
            writable_dir,
            bundled_dir,
            100,
            PipelineConfig::default(),
        );
        let find_big = |listing: &RegionsResponse| -> RegionEntry {
            let e = listing
                .regions
                .iter()
                .find(|r| r.id == "BigCorrupt")
                .unwrap();
            RegionEntry {
                id: e.id.clone(),
                name: e.name.clone(),
                modis_years: e.modis_years,
                viirs_years: e.viirs_years,
                noaa20_years: e.noaa20_years,
                size_mb: e.size_mb,
                too_large: e.too_large,
                precomputed: e.precomputed,
            }
        };

        let Json(initial) = regions(State(s.clone())).await.ok().unwrap();
        let entry = find_big(&initial);
        assert!(entry.precomputed);
        assert!(!entry.too_large);

        // Corrupt 1 byte in the bundled body (same byte length, updated content).
        let mut corrupt_body = valid_body.clone();
        corrupt_body[0] ^= 0x01;
        let bumped_mtime = std::time::SystemTime::now() + Duration::from_secs(10);
        std::fs::write(&bundled_body_path, &corrupt_body).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&bundled_body_path)
            .unwrap()
            .set_modified(bumped_mtime)
            .unwrap();

        let Json(after_corrupt) = regions(State(s.clone())).await.ok().unwrap();
        let entry = find_big(&after_corrupt);
        assert!(!entry.precomputed);
        assert!(entry.too_large);

        // Repair the body; /api/regions must pick it up again.
        let repaired_mtime = bumped_mtime + Duration::from_secs(10);
        std::fs::write(&bundled_body_path, &valid_body).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&bundled_body_path)
            .unwrap()
            .set_modified(repaired_mtime)
            .unwrap();

        let Json(after_repair) = regions(State(s.clone())).await.ok().unwrap();
        let entry = find_big(&after_repair);
        assert!(entry.precomputed);
        assert!(!entry.too_large);

        // Missing body with intact metadata must also report unavailable.
        std::fs::remove_file(&bundled_body_path).unwrap();
        let Json(after_remove) = regions(State(s)).await.ok().unwrap();
        let entry = find_big(&after_remove);
        assert!(!entry.precomputed);
        assert!(entry.too_large);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[derive(Clone, Default)]
    struct LogBuf(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for LogBuf {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogBuf {
        type Writer = Self;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[tokio::test]
    async fn request_timeout_returns_504_and_logs_warning() {
        use tower::ServiceExt;

        let buf = LogBuf::default();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .with_ansi(false)
            .with_writer(buf.clone())
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        let app = Router::new()
            .route(
                "/api/slow",
                get(|| async {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    "ok"
                }),
            )
            .layer(middleware::from_fn_with_state(
                Duration::from_millis(20),
                request_timeout,
            ));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/slow?region=Test")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["error"]
                .as_str()
                .is_some_and(|m| m.contains("timed out")),
            "unexpected error body: {json}"
        );

        let logs = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        assert!(logs.contains("/api/slow"), "missing path in logs: {logs}");
        assert!(logs.contains("WARN"), "missing WARN level in logs: {logs}");
    }

    #[tokio::test]
    async fn points_default_config_echoes_server_config_and_not_overridden() {
        let dir = temp_archive("points-config-default");
        write_file(&dir, "modis", 2021, "Malta", &[(35.9, "2021-07-01", 0)]);

        let mut server_config = PipelineConfig::default();
        server_config.matching.window_secs = 7200;
        let state = state_with_config(
            dir.clone(),
            dir.join("_derived_cache"),
            u64::MAX,
            server_config.clone(),
        );

        let Json(pts) = points(
            State(state),
            Query(PointsParams {
                region: Some("Malta".into()),
                year: Some("2021".into()),
                ..Default::default()
            }),
        )
        .await
        .ok()
        .unwrap();

        assert!(!pts.config_overridden);
        assert_eq!(pts.config, server_config);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn coverage_disk_cache_roundtrip_avoids_rescan_and_preserves_all_sensors() {
        let dir = temp_archive("coverage-disk-roundtrip");
        let derived = dir.join("_derived_cache");
        write_file(&dir, "modis", 2021, "Malta", &[(35.9, "2021-07-01", 0)]);
        write_file(&dir, "viirs-snpp", 2021, "Malta", &[(35.9, "2021-07-02", 0)]);
        write_file(
            &dir,
            "viirs-jpss1",
            2021,
            "Malta",
            &[(36.0, "2021-07-05", 0)],
        );

        let s1 = state_with_config(dir.clone(), derived.clone(), u64::MAX, PipelineConfig::default());
        let cov1 = load_coverage(&s1).await.ok().unwrap();
        assert_eq!(s1.coverage_scans(), 1);
        assert!(derived.join("coverage.json").is_file());
        assert!(derived.join("coverage.meta.json").is_file());
        assert_eq!(
            cov1.snpp,
            Some(DateSpan {
                first: d(2021, 7, 2),
                last: d(2021, 7, 2),
            })
        );
        assert_eq!(
            cov1.noaa20,
            Some(DateSpan {
                first: d(2021, 7, 5),
                last: d(2021, 7, 5),
            })
        );

        // Fresh state simulates a restart: must read coverage from disk without rescanning.
        let s2 = state_with_config(dir.clone(), derived.clone(), u64::MAX, PipelineConfig::default());
        let cov2 = load_coverage(&s2).await.ok().unwrap();
        assert_eq!(s2.coverage_scans(), 0);
        assert_eq!(cov2, cov1);

        // /api/points must also work with the cached coverage (including snpp/noaa20 spans).
        let Json(pts) = points(
            State(s2.clone()),
            Query(PointsParams {
                region: Some("Malta".into()),
                year: Some("2021".into()),
                month: Some("7".into()),
                limit: None,
                ..Default::default()
            }),
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(s2.coverage_scans(), 0);
        assert!(!pts.no_detection_days.is_empty());

        // Coverage does not depend on PipelineConfig, so changing config must keep the cache valid.
        let mut alt_cfg = PipelineConfig::default();
        alt_cfg.matching.window_secs += 60;
        let s3 = state_with_config(dir.clone(), derived, u64::MAX, alt_cfg);
        let cov3 = load_coverage(&s3).await.ok().unwrap();
        assert_eq!(s3.coverage_scans(), 0);
        assert_eq!(cov3, cov1);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn points_threshold_overrides_change_matching_and_echo_effective_config() {
        let dir = temp_archive("points-config-override");
        let modis_dir = dir.join("modis/2021");
        let viirs_dir = dir.join("viirs-snpp/2021");
        std::fs::create_dir_all(&modis_dir).unwrap();
        std::fs::create_dir_all(&viirs_dir).unwrap();
        // Same location, 1 hour apart: matched under default 10800s window, unmatched under 60s.
        std::fs::write(
            modis_dir.join("modis_2021_Malta.csv"),
            format!(
                "{MODIS_HEADER}\r\n35.9,32.0,320,0.5,0.5,2021-07-01,1200,Terra,MODIS,80,6.2,290,10,D,0\r\n"
            ),
        )
        .unwrap();
        std::fs::write(
            viirs_dir.join("viirs-snpp_2021_Malta.csv"),
            format!(
                "{VIIRS_HEADER}\r\n35.9,32.0,320,0.5,0.5,2021-07-01,1300,N,VIIRS,n,2,290,10,D,0\r\n"
            ),
        )
        .unwrap();

        let state = state(dir.clone(), u64::MAX);

        let Json(baseline) = points(
            State(state.clone()),
            Query(PointsParams {
                region: Some("Malta".into()),
                year: Some("2021".into()),
                ..Default::default()
            }),
        )
        .await
        .ok()
        .unwrap();
        assert!(!baseline.config_overridden);
        assert_eq!(baseline.config, PipelineConfig::default());
        assert_eq!(baseline.stages.matched_pairs, 1);
        assert_eq!(baseline.stages.harmonized, 1);

        let Json(narrow) = points(
            State(state.clone()),
            Query(PointsParams {
                region: Some("Malta".into()),
                year: Some("2021".into()),
                match_window_secs: Some("60".into()),
                ..Default::default()
            }),
        )
        .await
        .ok()
        .unwrap();
        assert!(narrow.config_overridden);
        assert_eq!(narrow.config.matching.window_secs, 60);
        assert_eq!(narrow.stages.matched_pairs, 0);
        assert_eq!(narrow.stages.harmonized, 2);

        let Json(all_overrides) = points(
            State(state),
            Query(PointsParams {
                region: Some("Malta".into()),
                year: Some("2021".into()),
                dedup_time_secs: Some("12".into()),
                dedup_overlap: Some("0.5".into()),
                match_radius_km: Some("5.5".into()),
                match_window_secs: Some("120".into()),
                fleet_radius_km: Some("3.5".into()),
                ..Default::default()
            }),
        )
        .await
        .ok()
        .unwrap();
        assert!(all_overrides.config_overridden);
        assert_eq!(all_overrides.config.dedup.max_time_diff_secs, 12);
        assert_eq!(all_overrides.config.dedup.min_overlap_ratio, 0.5);
        assert_eq!(all_overrides.config.matching.radius_km, 5.5);
        assert_eq!(all_overrides.config.matching.window_secs, 120);
        assert_eq!(all_overrides.config.fleet.radius_km, 3.5);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn coverage_reads_from_bundled_without_writing_writable_dir() {
        let dir = temp_archive("coverage-bundled");
        write_file(&dir, "modis", 2020, "Cyprus", &[(35.0, "2020-07-01", 0)]);
        write_file(&dir, "viirs-snpp", 2020, "Cyprus", &[(35.1, "2020-07-10", 0)]);

        let staging_dir = dir.join("_staging");
        let bundled_dir = dir.join("_bundled");
        let writable_dir = dir.join("_writable");

        let staging_state = state_with_caches(
            dir.clone(),
            staging_dir.clone(),
            dir.join("_unused_bundled"),
            u64::MAX,
            PipelineConfig::default(),
        );
        let expected = load_coverage(&staging_state).await.ok().unwrap();
        assert_eq!(staging_state.coverage_scans(), 1);
        copy_coverage_files(&staging_dir, &bundled_dir);

        let s = state_with_caches(
            dir.clone(),
            writable_dir.clone(),
            bundled_dir,
            u64::MAX,
            PipelineConfig::default(),
        );
        let loaded = load_coverage(&s).await.ok().unwrap();
        assert_eq!(s.coverage_scans(), 0);
        assert_eq!(loaded, expected);
        assert!(
            !writable_dir.exists(),
            "reading coverage from bundled must not create or write to the writable cache"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn points_rejects_out_of_range_and_non_numeric_threshold_overrides() {
        let dir = temp_archive("points-config-invalid");
        write_file(&dir, "modis", 2021, "Malta", &[(35.9, "2021-07-01", 0)]);
        let state = state(dir.clone(), u64::MAX);

        let base = PointsParams {
            region: Some("Malta".into()),
            year: Some("2021".into()),
            ..Default::default()
        };

        let range_cases = [
            (
                PointsParams {
                    match_radius_km: Some("50".into()),
                    ..base.clone()
                },
                "matching.radius_km",
            ),
            (
                PointsParams {
                    dedup_overlap: Some("1.5".into()),
                    ..base.clone()
                },
                "dedup.min_overlap_ratio",
            ),
            (
                PointsParams {
                    dedup_time_secs: Some("-1".into()),
                    ..base.clone()
                },
                "dedup.max_time_diff_secs",
            ),
            (
                PointsParams {
                    match_window_secs: Some("-5".into()),
                    ..base.clone()
                },
                "matching.window_secs",
            ),
            (
                PointsParams {
                    fleet_radius_km: Some("-1".into()),
                    ..base.clone()
                },
                "fleet.radius_km",
            ),
        ];

        for (params, field) in range_cases {
            let err = points(State(state.clone()), Query(params))
                .await
                .err()
                .expect("out-of-range threshold must fail with 400");
            assert_eq!(err.status, StatusCode::BAD_REQUEST);
            assert!(
                err.message.contains(field),
                "error `{}` does not name `{field}`",
                err.message
            );
        }

        let parse_cases = [
            PointsParams {
                match_radius_km: Some("abc".into()),
                ..base.clone()
            },
            PointsParams {
                dedup_time_secs: Some("1.5".into()),
                ..base.clone()
            },
            PointsParams {
                dedup_overlap: Some("nope".into()),
                ..base.clone()
            },
            PointsParams {
                match_window_secs: Some("xyz".into()),
                ..base.clone()
            },
            PointsParams {
                fleet_radius_km: Some("bad".into()),
                ..base
            },
        ];

        for params in parse_cases {
            let err = points(State(state.clone()), Query(params))
                .await
                .err()
                .expect("non-numeric threshold must fail with 400");
            assert_eq!(err.status, StatusCode::BAD_REQUEST);
        }

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn coverage_disk_cache_invalidates_on_file_size_change_and_survives_mtime() {
        let dir = temp_archive("coverage-invalidate");
        write_file(&dir, "modis", 2020, "Cyprus", &[(35.0, "2020-07-01", 0)]);

        let s1 = state(dir.clone(), u64::MAX);
        let cov1 = load_coverage(&s1).await.ok().unwrap();
        assert_eq!(s1.coverage_scans(), 1);
        assert_eq!(cov1.modis.unwrap().last, d(2020, 7, 1));

        // Changing mtime without changing size keeps the stamp valid.
        let csv_path = dir.join("modis/2020/modis_2020_Cyprus.csv");
        let past = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&csv_path)
            .unwrap()
            .set_modified(past)
            .unwrap();
        let s_mtime = state(dir.clone(), u64::MAX);
        let _ = load_coverage(&s_mtime).await.ok().unwrap();
        assert_eq!(s_mtime.coverage_scans(), 0);

        // Changing file byte size invalidates the stamp and triggers a rescan.
        let mut contents = std::fs::read_to_string(&csv_path).unwrap();
        contents.push_str("35.2,32.0,320,0.5,0.5,2020-08-15,1200,Terra,MODIS,80,6.2,290,10,D,0\r\n");
        std::fs::write(&csv_path, contents).unwrap();

        let s2 = state(dir.clone(), u64::MAX);
        let cov2 = load_coverage(&s2).await.ok().unwrap();
        assert_eq!(s2.coverage_scans(), 1);
        assert_eq!(cov2.modis.unwrap().last, d(2020, 8, 15));

        // Subsequent restart reads the updated cache without rescanning.
        let s3 = state(dir.clone(), u64::MAX);
        let cov3 = load_coverage(&s3).await.ok().unwrap();
        assert_eq!(s3.coverage_scans(), 0);
        assert_eq!(cov3, cov2);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn coverage_ignores_corrupt_files_and_rescans_without_error() {
        let dir = temp_archive("coverage-corrupt");
        let derived = dir.join("_derived_cache");
        write_file(&dir, "modis", 2020, "Cyprus", &[(35.0, "2020-07-01", 0)]);

        let s1 = state(dir.clone(), u64::MAX);
        let expected = load_coverage(&s1).await.ok().unwrap();
        assert_eq!(s1.coverage_scans(), 1);

        // Corrupt coverage.json -> must fall back to scan with no error and repair disk cache.
        std::fs::write(derived.join("coverage.json"), b"{corrupt").unwrap();
        let s2 = state(dir.clone(), u64::MAX);
        let repaired = load_coverage(&s2).await.ok().unwrap();
        assert_eq!(s2.coverage_scans(), 1);
        assert_eq!(repaired, expected);

        let s3 = state(dir.clone(), u64::MAX);
        let after_repair = load_coverage(&s3).await.ok().unwrap();
        assert_eq!(s3.coverage_scans(), 0);
        assert_eq!(after_repair, expected);

        // Corrupt coverage.meta.json -> must also fall back to scan with no error.
        std::fs::write(derived.join("coverage.meta.json"), b"not-json").unwrap();
        let s4 = state(dir.clone(), u64::MAX);
        let after_meta_corrupt = load_coverage(&s4).await.ok().unwrap();
        assert_eq!(s4.coverage_scans(), 1);
        assert_eq!(after_meta_corrupt, expected);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn precompute_persists_coverage_to_writable_disk() {
        let dir = temp_archive("precompute-coverage");
        let derived = dir.join("_derived_cache");
        write_file(&dir, "modis", 2020, "Cyprus", &[(35.0, "2020-07-01", 0)]);

        let s1 = state(dir.clone(), u64::MAX);
        let report = precompute_series(
            s1.clone(),
            &PrecomputeOptions {
                regions: vec!["Cyprus".to_string()],
                force: false,
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(report.computed(), 1);
        assert_eq!(s1.coverage_scans(), 1);
        assert!(derived.join("coverage.json").is_file());
        assert!(derived.join("coverage.meta.json").is_file());

        let s2 = state(dir.clone(), u64::MAX);
        let _ = load_coverage(&s2).await.ok().unwrap();
        assert_eq!(s2.coverage_scans(), 0);

        let _ = std::fs::remove_dir_all(dir);
    }
}
