use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    Router,
    extract::{Query, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::get,
};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use tokio::sync::{OnceCell, Semaphore};
use tokio::task::JoinSet;

use pyrosync_core::calendar::compute_anomalies;
use pyrosync_core::harmonize::{harmonize, harmonize_with_links};
use pyrosync_core::models::{
    CalendarCell, Confidence, DayNight, HarmonizedHotspot, Hotspot, HotspotSource, Sensor,
};
use pyrosync_firms::archive::{ArchiveClient, ArchiveError, archive_slug, scan_local_archive};
use pyrosync_firms::parse::{parse_csv, retain_wildfires};
use pyrosync_firms::{FirmsSource, Processing};

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
    index: OnceCell<Arc<ArchiveIndex>>,
    coverage: OnceCell<Arc<Coverage>>,
    series: Mutex<HashMap<String, Arc<OnceCell<Arc<SeriesResponse>>>>>,
    years: Mutex<YearCache>,
    /// Each slot holds one parsed year plus its harmonize working set until its stats are
    /// done, so this bounds both CPU use and peak memory.
    load_slots: Arc<Semaphore>,
}

impl ArchiveState {
    pub fn new(dir: PathBuf, limits: ArchiveLimits) -> Self {
        let cpus = std::thread::available_parallelism().map_or(4, |n| n.get());
        Self {
            client: Arc::new(ArchiveClient::offline(dir)),
            limits,
            index: OnceCell::new(),
            coverage: OnceCell::new(),
            series: Mutex::new(HashMap::new()),
            years: Mutex::new(YearCache::default()),
            load_slots: Arc::new(Semaphore::new((cpus / 2).clamp(2, 8))),
        }
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
                "Region too large for this server: ~{} detections / {:.0} MB of archive CSV; limit {:.0} MB (ARCHIVE_MAX_REGION_MB)",
                approx_count(region.bytes / BYTES_PER_ROW_ESTIMATE),
                megabytes(region.bytes),
                megabytes(self.limits.max_region_bytes),
            ),
        ))
    }
}

/// Sub-router mounted under `/api` for the offline archive endpoints (`/regions`, `/series`, `/points`).
pub fn api_routes(state: Arc<ArchiveState>) -> Router {
    let timeout = state.limits.request_timeout;
    Router::new()
        .route("/regions", get(regions))
        .route("/series", get(series))
        .route("/points", get(points))
        .layer(middleware::from_fn_with_state(timeout, request_timeout))
        .with_state(state)
}

async fn request_timeout(State(limit): State<Duration>, request: Request, next: Next) -> Response {
    match tokio::time::timeout(limit, next.run(request)).await {
        Ok(response) => response,
        Err(_) => ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            format!(
                "archive request timed out after {} s (ARCHIVE_REQUEST_TIMEOUT_SECS)",
                limit.as_secs()
            ),
        )
        .into_response(),
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

pub struct ApiError {
    status: StatusCode,
    message: String,
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
    /// Total size of the region's archive CSV files on disk.
    bytes: u64,
}

struct ArchiveIndex {
    regions: BTreeMap<String, Arc<RegionInfo>>,
    /// Every archive file per year, across all countries.
    modis_files: BTreeMap<i32, Vec<PathBuf>>,
    viirs_files: BTreeMap<i32, Vec<PathBuf>>,
}

fn build_index(dir: &Path) -> std::io::Result<ArchiveIndex> {
    let modis_slug = archive_slug(Sensor::ModisTerra);
    let viirs_slug = archive_slug(Sensor::ViirsSNPP);
    let mut regions: BTreeMap<String, Arc<RegionInfo>> = BTreeMap::new();
    let mut modis_files: BTreeMap<i32, Vec<PathBuf>> = BTreeMap::new();
    let mut viirs_files: BTreeMap<i32, Vec<PathBuf>> = BTreeMap::new();

    for file in scan_local_archive(dir)? {
        let is_modis = Some(file.slug.as_str()) == modis_slug;
        if !is_modis && Some(file.slug.as_str()) != viirs_slug {
            continue;
        }
        let bytes = std::fs::metadata(&file.path)?.len();
        let region = Arc::make_mut(regions.entry(file.country.clone()).or_insert_with(|| {
            Arc::new(RegionInfo {
                name: file.country.replace('_', " "),
                id: file.country.clone(),
                modis_years: BTreeSet::new(),
                viirs_years: BTreeSet::new(),
                bytes: 0,
            })
        }));
        region.bytes += bytes;
        let (years, files) = if is_modis {
            (&mut region.modis_years, &mut modis_files)
        } else {
            (&mut region.viirs_years, &mut viirs_files)
        };
        years.insert(file.year);
        files.entry(file.year).or_default().push(file.path);
    }

    Ok(ArchiveIndex {
        regions,
        modis_files,
        viirs_files,
    })
}

async fn load_index(state: &ArchiveState) -> Result<Arc<ArchiveIndex>, ApiError> {
    state
        .index
        .get_or_try_init(|| async {
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
                "scanned FIRMS archive"
            );
            Ok(Arc::new(index))
        })
        .await
        .cloned()
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
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
}

fn scan_coverage(index: &ArchiveIndex) -> std::io::Result<Coverage> {
    Ok(Coverage {
        modis: sensor_span(&index.modis_files)?,
        viirs: sensor_span(&index.viirs_files)?,
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

async fn load_coverage(state: &ArchiveState) -> Result<Arc<Coverage>, ApiError> {
    let index = load_index(state).await?;
    state
        .coverage
        .get_or_try_init(|| async move {
            let started = Instant::now();
            let coverage = tokio::task::spawn_blocking(move || scan_coverage(&index))
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
    size_mb: f64,
    too_large: bool,
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
    let mut entries: Vec<RegionEntry> = index
        .regions
        .values()
        .map(|r| RegionEntry {
            id: r.id.clone(),
            name: r.name.clone(),
            modis_years: year_span(&r.modis_years),
            viirs_years: year_span(&r.viirs_years),
            size_mb: round_mb(r.bytes),
            too_large: r.bytes > state.limits.max_region_bytes,
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
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
    let viirs = async {
        if region.viirs_years.contains(&year) {
            load_sensor_year(client.clone(), Sensor::ViirsSNPP, region.id.clone(), year).await
        } else {
            Ok(Vec::new())
        }
    };
    let (modis, viirs) = tokio::try_join!(modis, viirs)?;
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
    modis_only: usize,
    viirs_only: usize,
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
struct MonthEntry {
    year: i32,
    month: u32,
    raw_modis: usize,
    raw_viirs: usize,
    raw_total: usize,
    harmonized: usize,
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
fn year_stats(year: i32, data: YearData, observed: &[DateSpan]) -> YearStats {
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
            Some(MonthEntry {
                year,
                month,
                raw_modis,
                raw_viirs,
                raw_total: raw_modis + raw_viirs,
                // Each month is harmonized on its own, so matches and interscan duplicates
                // that straddle a month boundary are not merged; the monthly harmonized counts
                // need not add up to the yearly figure, which is harmonized over the whole year.
                harmonized: harmonize(modis, viirs).len(),
                anomaly_raw: None,
                anomaly_harmonized: None,
            })
        })
        .collect();

    let modis_span = date_span(&data.modis);
    let viirs_span = date_span(&data.viirs);
    let (raw_modis, raw_viirs) = (data.modis.len(), data.viirs.len());
    let linked = harmonize_with_links(data.modis, data.viirs);
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
        modis_only: count_source(HotspotSource::ModisOnly),
        viirs_only: count_source(HotspotSource::ViirsOnly),
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
            anomaly: None,
        })
        .collect();
    compute_anomalies(&mut cells);
    cells.into_iter().map(|c| c.anomaly).collect()
}

fn change_pct(from: usize, to: usize) -> Option<f64> {
    (from > 0).then(|| (to as f64 - from as f64) / from as f64 * 100.0)
}

async fn build_series(
    state: Arc<ArchiveState>,
    region: Arc<RegionInfo>,
    coverage: Arc<Coverage>,
) -> Result<SeriesResponse, ApiError> {
    let years: BTreeSet<i32> = region
        .modis_years
        .union(&region.viirs_years)
        .copied()
        .collect();

    let mut tasks = JoinSet::new();
    for year in years {
        let state = state.clone();
        let region = region.clone();
        let coverage = coverage.clone();
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
                (region.viirs_years.contains(&year), coverage.viirs),
            ]
            .into_iter()
            .filter_map(|(contributes, span)| span.filter(|_| contributes))
            .collect();
            tokio::task::spawn_blocking(move || {
                let _slot = slot;
                year_stats(year, data, &observed)
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

    let cliff = region.viirs_years.first().and_then(|&to_year| {
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
            sensors: ["MODIS Terra+Aqua (archive)", "VIIRS S-NPP (archive)"],
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

async fn series(
    State(state): State<Arc<ArchiveState>>,
    Query(params): Query<SeriesParams>,
) -> Result<Response, ApiError> {
    let started = Instant::now();
    let region = find_region(&state, params.region.as_deref()).await?;
    state.admit(&region)?;
    let coverage = load_coverage(&state).await?;
    let cell = state
        .series
        .lock()
        .expect("series cache lock")
        .entry(region.id.clone())
        .or_default()
        .clone();

    let mut computed = false;
    let response = cell
        .get_or_try_init(|| async {
            computed = true;
            build_series(state.clone(), region.clone(), coverage)
                .await
                .map(Arc::new)
        })
        .await?
        .clone();

    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    if computed {
        tracing::info!(region = %region.id, elapsed_ms, "computed /api/series (first call, archive read + pipeline)");
    } else {
        tracing::debug!(region = %region.id, elapsed_ms, "served /api/series from cache");
    }
    Ok(Json(&*response).into_response())
}

// ---------- points ----------

#[derive(Deserialize)]
struct PointsParams {
    region: Option<String>,
    year: Option<String>,
    month: Option<String>,
    limit: Option<String>,
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
    #[serde(flatten)]
    hotspot: HarmonizedHotspot,
}

#[derive(Serialize)]
struct Stages {
    raw_modis: usize,
    raw_viirs: usize,
    interscan_removed: usize,
    fleet_merged: usize,
    matched_pairs: usize,
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

/// Deterministic subset that keeps links consistent: every `k`-th harmonized output (in time
/// order) with all raw rows linked to it, plus every `k`-th interscan-removed raw row. `k` is
/// the smallest step, starting from ceil(raw / limit), whose subset fits in `limit` raw rows.
/// Returns (k, kept output mask, kept raw mask).
fn sample_links(
    links: &[Option<usize>],
    outputs: usize,
    limit: usize,
) -> (usize, Vec<bool>, Vec<bool>) {
    if links.len() <= limit {
        return (1, vec![true; outputs], vec![true; links.len()]);
    }
    let mut k = links.len().div_ceil(limit.max(1));
    loop {
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
        if keep_raw.iter().filter(|&&kept| kept).count() <= limit {
            return (k, keep_out, keep_raw);
        }
        k += 1;
    }
}

async fn points(
    State(state): State<Arc<ArchiveState>>,
    Query(params): Query<PointsParams>,
) -> Result<Json<PointsResponse>, ApiError> {
    let started = Instant::now();
    let region = find_region(&state, params.region.as_deref()).await?;
    state.admit(&region)?;
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
    let limit = parse_param::<usize>("limit", params.limit.as_deref())?
        .unwrap_or(DEFAULT_POINTS_LIMIT)
        .clamp(1, MAX_POINTS_LIMIT);
    if !region.modis_years.contains(&year) && !region.viirs_years.contains(&year) {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            format!("the archive has no {id} file for {year}"),
        ));
    }

    let slot = state
        .load_slots
        .clone()
        .acquire_owned()
        .await
        .expect("semaphore is never closed");
    let data = load_year(&state, &region, year).await?;
    let response = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        let (modis, viirs) = match month {
            Some(m) => (in_month(&data.modis, m), in_month(&data.viirs, m)),
            None => Arc::try_unwrap(data).map_or_else(
                |shared| (shared.modis.clone(), shared.viirs.clone()),
                |owned| (owned.modis, owned.viirs),
            ),
        };

        let raw_points: Vec<RawPoint> = modis
            .iter()
            .map(|h| RawPoint::from_hotspot(h, "MODIS"))
            .chain(viirs.iter().map(|h| RawPoint::from_hotspot(h, "VIIRS")))
            .collect();
        let bbox = extent(raw_points.iter().map(|p| (p.lat, p.lon)));
        let (raw_modis, raw_viirs) = (modis.len(), viirs.len());
        let linked = harmonize_with_links(modis, viirs);

        let links: Vec<Option<usize>> = linked
            .modis_links
            .iter()
            .chain(&linked.viirs_links)
            .copied()
            .collect();

        let (k, keep_out, keep_raw) = sample_links(&links, linked.hotspots.len(), limit);

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
            interscan_removed: linked.interscan_removed,
            fleet_merged: linked.fleet_merged,
            matched_pairs: linked.matched_pairs,
            harmonized: linked.hotspots.len(),
        };
        let harmonized: Vec<HarmonizedPoint> = linked
            .hotspots
            .into_iter()
            .enumerate()
            .filter(|(i, _)| keep_out[*i])
            .map(|(i, hotspot)| HarmonizedPoint {
                id: format!("h{i}"),
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
        let (header, sat) = if slug == "modis" {
            (MODIS_HEADER, "Terra,MODIS,80,6.2")
        } else {
            (VIIRS_HEADER, "N,VIIRS,n,2")
        };
        let mut csv = format!("{header}\r\n");
        for (lat, date, ty) in rows {
            csv.push_str(&format!("{lat},32.0,320,0.5,0.5,{date},1200,{sat},290,10,D,{ty}\r\n"));
        }
        let path = dir.join(slug).join(year.to_string());
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join(format!("{slug}_{year}_{country}.csv")), csv).unwrap();
    }

    fn state(dir: PathBuf, max_region_bytes: u64) -> Arc<ArchiveState> {
        Arc::new(ArchiveState::new(
            dir,
            ArchiveLimits {
                max_region_bytes,
                request_timeout: Duration::from_secs(30),
            },
        ))
    }

    async fn series_json(state: &Arc<ArchiveState>, region: &str) -> Result<serde_json::Value, ApiError> {
        let response = series(
            State(state.clone()),
            Query(SeriesParams {
                region: Some(region.to_string()),
            }),
        )
        .await?;
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
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
}
