use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{NaiveDate, Utc};
use pyrosync_core::models::{BBox, Hotspot, Sensor};
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use tokio::sync::OnceCell;
use tokio::task::JoinSet;

use crate::area_cache::AreaCache;
use crate::availability::Availability;
use crate::day_range::{DayRange, date_windows, recent_window};
use crate::limiter::RateLimiter;
use crate::parse::parse_csv;
use crate::source::FirmsSource;

pub const FIRMS_API_ROOT: &str = "https://firms.modaps.eosdis.nasa.gov";
const AREA_CSV_PATH: &str = "/api/area/csv";

const USER_AGENT: &str = "pyrosync/0.1 (+https://github.com/TeamMagi)";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_CONCURRENCY: usize = 4;
const MAX_RETRIES: usize = 2;
const MAX_ERROR_BODY_CHARS: usize = 200;

pub struct FirmsClient {
    http: reqwest::Client,
    api_key: String,
    api_root: String,
    limiter: Arc<RateLimiter>,
    availability: OnceCell<Availability>,
    concurrency: usize,
    area_cache: Option<Arc<AreaCache>>,
}

impl std::fmt::Debug for FirmsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FirmsClient")
            .field("api_key", &"***")
            .field("concurrency", &self.concurrency)
            .finish_non_exhaustive()
    }
}

/// What a multi-source, multi-window fetch actually managed to retrieve.
#[derive(Debug, Default)]
pub struct FetchReport {
    pub per_source: Vec<(String, usize)>,
    pub failures: Vec<(String, String)>,
    pub transactions_spent: u32,
    /// Date sub-ranges the caller asked for that NO source could serve. Must be populated:
    /// a silently short answer is the failure mode this whole rewrite exists to prevent.
    pub uncovered: Vec<(NaiveDate, NaiveDate)>,
}

#[derive(Clone)]
struct RequestCtx {
    http: reqwest::Client,
    api_key: String,
    api_root: String,
    limiter: Arc<RateLimiter>,
    area_cache: Option<Arc<AreaCache>>,
}

#[derive(Clone, Copy)]
enum SensorGroup {
    Modis,
    Viirs,
}

struct WindowTask {
    group: SensorGroup,
    source: FirmsSource,
    bbox_str: String,
    start: NaiveDate,
    days: DayRange,
}

impl FirmsClient {
    pub fn new(api_key: String) -> anyhow::Result<Self> {
        Self::with_concurrency(api_key, DEFAULT_CONCURRENCY)
    }

    pub fn with_concurrency(api_key: String, concurrency: usize) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()?;

        Ok(Self {
            http,
            api_key,
            api_root: FIRMS_API_ROOT.to_string(),
            limiter: Arc::new(RateLimiter::firms_default(concurrency)),
            availability: OnceCell::new(),
            concurrency: concurrency.max(1),
            area_cache: None,
        })
    }

    /// Points every request at `api_root` instead of the FIRMS host, e.g. a local mock in tests.
    pub fn with_api_root(mut self, api_root: impl Into<String>) -> Self {
        self.api_root = api_root.into().trim_end_matches('/').to_string();
        self
    }

    /// Windows already bought are served from disk and cost no transactions.
    pub fn with_area_cache(mut self, cache: AreaCache) -> Self {
        self.area_cache = Some(Arc::new(cache));
        self
    }

    /// Lazily fetched once and cached; every date-routing decision needs it.
    pub async fn availability(&self) -> anyhow::Result<&Availability> {
        self.availability
            .get_or_try_init(|| async {
                let fetched = match Availability::fetch_text(&self.http, &self.api_root, &self.api_key).await {
                    Ok(text) => Availability::from_csv(&text).map(|avail| (avail, text)),
                    Err(err) => Err(err),
                };
                match fetched {
                    Ok((avail, text)) => {
                        if let Some(cache) = &self.area_cache {
                            cache.store_availability(&text).await;
                        }
                        Ok(avail)
                    }
                    Err(err) => {
                        if let Some(cache) = &self.area_cache
                            && let Some(avail) = cache.load_availability().await
                        {
                            tracing::warn!(
                                "FIRMS availability fetch failed; using the cached availability table"
                            );
                            return Ok(avail);
                        }
                        Err(anyhow::anyhow!(
                            "{}",
                            redact_key(&format!("{err:#}"), &self.api_key)
                        ))
                    }
                }
            })
            .await
    }

    /// One request for one window. Checks status, sniffs the body, returns raw validated CSV.
    pub async fn fetch_window_csv(
        &self,
        source: FirmsSource,
        bbox: &BBox,
        start: NaiveDate,
        days: DayRange,
    ) -> anyhow::Result<String> {
        let (result, _spent) = self
            .request_ctx()
            .fetch_window_csv_counted(source, &bbox.to_firms_string(), start, days)
            .await;
        result
    }

    /// One request for one window. Checks status, sniffs the body, returns parsed hotspots.
    pub async fn fetch_window(
        &self,
        source: FirmsSource,
        bbox: &BBox,
        start: NaiveDate,
        days: DayRange,
    ) -> anyhow::Result<Vec<Hotspot>> {
        let (result, _spent) = self
            .request_ctx()
            .fetch_window_counted(source, &bbox.to_firms_string(), start, days)
            .await;
        result
    }

    /// Last `days` days ending today (UTC) for one sensor, routed to NRT or SP by availability.
    pub async fn fetch_recent(
        &self,
        sensor: Sensor,
        bbox: &BBox,
        days: DayRange,
    ) -> anyhow::Result<Vec<Hotspot>> {
        let today = Utc::now().date_naive();
        let (start, _actual_days) = recent_window(today, days);
        let avail = self.availability().await?;
        let spans = avail.plan(sensor, start, today);
        if spans.is_empty() {
            anyhow::bail!("no FIRMS source available for {sensor:?} in {start}..={today}");
        }

        let mut hotspots = Vec::new();
        for (source, span_start, span_end) in spans {
            for (win_start, win_days) in date_windows(span_start, span_end) {
                let mut batch = self.fetch_window(source, bbox, win_start, win_days).await?;
                hotspots.append(&mut batch);
            }
        }
        Ok(hotspots)
    }

    /// Inclusive [start, end] for one sensor: plan() picks the sources, date_windows() chops each
    /// span into <=5-day windows, the limiter paces them. Per-window failures are collected, not
    /// fatal. Spans that no source covers go into FetchReport::uncovered.
    pub async fn fetch_range(
        &self,
        sensor: Sensor,
        bbox: &BBox,
        start: NaiveDate,
        end: NaiveDate,
    ) -> (Vec<Hotspot>, FetchReport) {
        let mut report = FetchReport::default();
        if start > end {
            return (Vec::new(), report);
        }

        let avail = match self.availability().await {
            Ok(a) => a,
            Err(err) => {
                report.failures.push((
                    format!("{sensor:?}"),
                    redact_key(&format!("{err:#}"), &self.api_key),
                ));
                report.uncovered.push((start, end));
                return (Vec::new(), report);
            }
        };

        let spans = avail.plan(sensor, start, end);
        let covered: Vec<(NaiveDate, NaiveDate)> = spans.iter().map(|&(_, s, e)| (s, e)).collect();
        report.uncovered = compute_uncovered(start, end, &covered);

        let bbox_str = bbox.to_firms_string();
        let mut tasks = Vec::new();
        for (source, span_start, span_end) in spans {
            for (win_start, win_days) in date_windows(span_start, span_end) {
                tasks.push(WindowTask {
                    group: SensorGroup::Modis,
                    source,
                    bbox_str: bbox_str.clone(),
                    start: win_start,
                    days: win_days,
                });
            }
        }

        let (mut hotspots, _) = self.run_windows(tasks, &mut report).await;
        hotspots.shrink_to_fit();
        (hotspots, report)
    }

    /// All five sensors over [start, end]. Returns (modis, viirs, report) where viirs merges
    /// S-NPP + NOAA-20 + NOAA-21. A sensor that fails entirely must NOT abort the others.
    pub async fn fetch_all_range(
        &self,
        bbox: &BBox,
        start: NaiveDate,
        end: NaiveDate,
    ) -> (Vec<Hotspot>, Vec<Hotspot>, FetchReport) {
        let mut report = FetchReport::default();
        if start > end {
            return (Vec::new(), Vec::new(), report);
        }

        let avail = match self.availability().await {
            Ok(a) => a,
            Err(err) => {
                report.failures.push((
                    "availability".to_string(),
                    redact_key(&format!("{err:#}"), &self.api_key),
                ));
                report.uncovered.push((start, end));
                return (Vec::new(), Vec::new(), report);
            }
        };

        // MODIS_NRT/MODIS_SP each cover Terra and Aqua together, so ModisTerra stands in
        // for all of MODIS here; adding ModisAqua would fetch the same source twice.
        let sensors = [
            (Sensor::ModisTerra, SensorGroup::Modis),
            (Sensor::ViirsSNPP, SensorGroup::Viirs),
            (Sensor::ViirsNOAA20, SensorGroup::Viirs),
            (Sensor::ViirsNOAA21, SensorGroup::Viirs),
        ];

        let bbox_str = bbox.to_firms_string();
        let mut all_covered = Vec::new();
        let mut tasks = Vec::new();

        for (sensor, group) in sensors {
            let spans = avail.plan(sensor, start, end);
            for (source, span_start, span_end) in spans {
                all_covered.push((span_start, span_end));
                for (win_start, win_days) in date_windows(span_start, span_end) {
                    tasks.push(WindowTask {
                        group,
                        source,
                        bbox_str: bbox_str.clone(),
                        start: win_start,
                        days: win_days,
                    });
                }
            }
        }

        report.uncovered = compute_uncovered(start, end, &all_covered);
        let (modis, viirs) = self.run_windows(tasks, &mut report).await;
        (modis, viirs, report)
    }

    /// Fetches exactly the given (source, window) pairs, with no availability lookup. Measurement
    /// tools use it to pin SP sources and to skip windows before a sensor's first day.
    /// Returns (modis, viirs, report); `uncovered` stays empty because the caller chose the windows.
    pub async fn fetch_source_windows(
        &self,
        bbox: &BBox,
        windows: &[(FirmsSource, NaiveDate, DayRange)],
    ) -> (Vec<Hotspot>, Vec<Hotspot>, FetchReport) {
        let bbox_str = bbox.to_firms_string();
        let tasks = windows
            .iter()
            .map(|&(source, start, days)| WindowTask {
                group: if source.sensor.is_modis() {
                    SensorGroup::Modis
                } else {
                    SensorGroup::Viirs
                },
                source,
                bbox_str: bbox_str.clone(),
                start,
                days,
            })
            .collect();
        let mut report = FetchReport::default();
        let (modis, viirs) = self.run_windows(tasks, &mut report).await;
        (modis, viirs, report)
    }

    /// Backwards-compatible convenience kept for the existing server handler:
    /// last `days` days, all sensors, partial failures tolerated.
    pub async fn fetch_all(
        &self,
        bbox: &BBox,
        days: DayRange,
    ) -> (Vec<Hotspot>, Vec<Hotspot>, FetchReport) {
        let today = Utc::now().date_naive();
        let (start, _actual_days) = recent_window(today, days);
        self.fetch_all_range(bbox, start, today).await
    }

    fn request_ctx(&self) -> RequestCtx {
        RequestCtx {
            http: self.http.clone(),
            api_key: self.api_key.clone(),
            api_root: self.api_root.clone(),
            limiter: Arc::clone(&self.limiter),
            area_cache: self.area_cache.clone(),
        }
    }

    async fn run_windows(
        &self,
        tasks: Vec<WindowTask>,
        report: &mut FetchReport,
    ) -> (Vec<Hotspot>, Vec<Hotspot>) {
        let mut modis = Vec::new();
        let mut viirs = Vec::new();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();

        let mut join_set = JoinSet::new();
        let mut pending = tasks.into_iter();
        let max_in_flight = self.concurrency;

        for task in pending.by_ref().take(max_in_flight) {
            let ctx = self.request_ctx();
            join_set.spawn(async move {
                let (res, spent) = ctx
                    .fetch_window_counted(task.source, &task.bbox_str, task.start, task.days)
                    .await;
                (task, res, spent)
            });
        }

        while let Some(joined) = join_set.join_next().await {
            if let Some(next_task) = pending.next() {
                let ctx = self.request_ctx();
                join_set.spawn(async move {
                    let (res, spent) = ctx
                        .fetch_window_counted(
                            next_task.source,
                            &next_task.bbox_str,
                            next_task.start,
                            next_task.days,
                        )
                        .await;
                    (next_task, res, spent)
                });
            }

            match joined {
                Ok((task, res, spent)) => {
                    report.transactions_spent += spent;
                    let source_id = task.source.as_str().to_string();
                    match res {
                        Ok(mut hotspots) => {
                            *counts.entry(source_id).or_insert(0) += hotspots.len();
                            match task.group {
                                SensorGroup::Modis => modis.append(&mut hotspots),
                                SensorGroup::Viirs => viirs.append(&mut hotspots),
                            }
                        }
                        Err(err) => {
                            report.failures.push((
                                source_id,
                                format!(
                                    "{} ({}d): {}",
                                    task.start.format("%Y-%m-%d"),
                                    task.days.get(),
                                    redact_key(&format!("{err:#}"), &self.api_key)
                                ),
                            ));
                        }
                    }
                }
                Err(join_err) => {
                    report
                        .failures
                        .push(("task".to_string(), format!("window task panicked: {join_err}")));
                }
            }
        }

        report.per_source.extend(counts);
        (modis, viirs)
    }
}

impl RequestCtx {
    async fn fetch_window_csv_counted(
        &self,
        source: FirmsSource,
        bbox_str: &str,
        start: NaiveDate,
        days: DayRange,
    ) -> (anyhow::Result<String>, u32) {
        let source_str = source.as_str();
        let url = format_firms_url(
            &self.api_root,
            &self.api_key,
            source_str,
            bbox_str,
            start,
            days,
        );
        let safe_url = format_firms_url(&self.api_root, "***", source_str, bbox_str, start, days);
        let cost = u32::from(days.get());
        let mut spent = 0u32;

        if let Some(cache) = &self.area_cache
            && let Some(body) = cache.load(source, bbox_str, start, days).await
        {
            return (Ok(body), 0);
        }

        for attempt in 0..=MAX_RETRIES {
            let guard = self.limiter.acquire(cost).await;
            spent += cost;

            let resp = match self.http.get(&url).send().await {
                Ok(r) => r,
                Err(err) => {
                    drop(guard);
                    let safe_err = redact_key(&format!("{err:#}"), &self.api_key);
                    return (
                        Err(anyhow::anyhow!(
                            "FIRMS request to {safe_url} failed: {safe_err}"
                        )),
                        spent,
                    );
                }
            };

            let status = resp.status();
            let retry_after = parse_retry_after(resp.headers());
            let is_retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();

            if is_retryable && attempt < MAX_RETRIES {
                // Drop both response and limiter permit before sleeping so other windows can proceed.
                drop(resp);
                drop(guard);
                let backoff = Duration::from_secs(1u64 << (2 * attempt));
                let delay = backoff.saturating_add(retry_after);
                tracing::warn!(
                    source = source_str,
                    url = %safe_url,
                    status = %status,
                    attempt = attempt + 1,
                    delay_secs = delay.as_secs(),
                    "transient FIRMS HTTP error, retrying"
                );
                tokio::time::sleep(delay).await;
                continue;
            }

            let body = match resp.text().await {
                Ok(text) => {
                    drop(guard);
                    text
                }
                Err(err) => {
                    drop(guard);
                    let safe_err = redact_key(&format!("{err:#}"), &self.api_key);
                    return (
                        Err(anyhow::anyhow!(
                            "failed reading FIRMS response from {safe_url} (HTTP {status}): {safe_err}"
                        )),
                        spent,
                    );
                }
            };

            let validated = validate_firms_csv(status, body, &safe_url, &self.api_key);
            if let Ok(csv) = &validated
                && let Some(cache) = &self.area_cache
            {
                cache.store(source, bbox_str, start, days, csv).await;
            }
            return (validated, spent);
        }

        unreachable!("retry loop always exits by MAX_RETRIES")
    }

    async fn fetch_window_counted(
        &self,
        source: FirmsSource,
        bbox_str: &str,
        start: NaiveDate,
        days: DayRange,
    ) -> (anyhow::Result<Vec<Hotspot>>, u32) {
        let (csv_res, spent) = self
            .fetch_window_csv_counted(source, bbox_str, start, days)
            .await;
        let safe_url = format_firms_url(
            &self.api_root,
            "***",
            source.as_str(),
            bbox_str,
            start,
            days,
        );
        let parsed = csv_res.and_then(|body| {
            parse_validated_csv(&body, source.as_str(), &safe_url, &self.api_key)
        });
        (parsed, spent)
    }
}

fn format_firms_url(
    api_root: &str,
    api_key: &str,
    source: &str,
    bbox_str: &str,
    start: NaiveDate,
    days: DayRange,
) -> String {
    format!(
        "{api_root}{AREA_CSV_PATH}/{api_key}/{source}/{bbox_str}/{}/{}",
        days.get(),
        start.format("%Y-%m-%d")
    )
}

fn validate_firms_csv(
    status: StatusCode,
    body: String,
    safe_url: &str,
    api_key: &str,
) -> anyhow::Result<String> {
    if !status.is_success() {
        let snippet = redact_key(&truncate_chars(body.trim(), MAX_ERROR_BODY_CHARS), api_key);
        anyhow::bail!("FIRMS request to {safe_url} failed with HTTP {status}: {snippet}");
    }

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let first_line = trimmed.lines().next().unwrap_or("");
    if !first_line.contains("latitude") {
        let snippet = redact_key(&truncate_chars(trimmed, MAX_ERROR_BODY_CHARS), api_key);
        anyhow::bail!(
            "FIRMS returned non-CSV response (HTTP {status}) from {safe_url}: {snippet}"
        );
    }

    Ok(body)
}

fn parse_validated_csv(
    body: &str,
    source: &str,
    safe_url: &str,
    api_key: &str,
) -> anyhow::Result<Vec<Hotspot>> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut lines = trimmed.lines();
    let _first_line = lines.next();
    if lines.all(|line| line.trim().is_empty()) {
        return Ok(Vec::new());
    }

    parse_csv(body, source).map_err(|err| {
        anyhow::anyhow!(
            "failed parsing FIRMS CSV from {safe_url}: {}",
            redact_key(&format!("{err:#}"), api_key)
        )
    })
}

fn compute_uncovered(
    start: NaiveDate,
    end: NaiveDate,
    covered: &[(NaiveDate, NaiveDate)],
) -> Vec<(NaiveDate, NaiveDate)> {
    if start > end {
        return Vec::new();
    }

    let mut intervals: Vec<(NaiveDate, NaiveDate)> = covered
        .iter()
        .filter_map(|&(s, e)| {
            let cs = s.max(start);
            let ce = e.min(end);
            (cs <= ce).then_some((cs, ce))
        })
        .collect();
    intervals.sort_by_key(|&(s, e)| (s, e));

    let mut uncovered = Vec::new();
    let mut cursor = start;

    for (s, e) in intervals {
        if s > cursor
            && let Some(gap_end) = s.pred_opt()
        {
            uncovered.push((cursor, gap_end));
        }
        match e.succ_opt() {
            Some(next_day) if next_day > cursor => cursor = next_day,
            Some(_) => {}
            None => return uncovered,
        }
        if cursor > end {
            return uncovered;
        }
    }

    if cursor <= end {
        uncovered.push((cursor, end));
    }
    uncovered
}

fn parse_retry_after(headers: &HeaderMap) -> Duration {
    let Some(value) = headers.get(RETRY_AFTER) else {
        return Duration::ZERO;
    };
    let Ok(text) = value.to_str() else {
        return Duration::ZERO;
    };
    match text.trim().parse::<u64>() {
        Ok(secs) => Duration::from_secs(secs),
        Err(_) => Duration::ZERO,
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn redact_key(text: &str, api_key: &str) -> String {
    if api_key.is_empty() {
        return text.to_string();
    }
    text.replace(api_key, "***")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn parse_firms_response(
        status: StatusCode,
        body: &str,
        source: &str,
        safe_url: &str,
        api_key: &str,
    ) -> anyhow::Result<Vec<Hotspot>> {
        let validated = validate_firms_csv(status, body.to_string(), safe_url, api_key)?;
        parse_validated_csv(&validated, source, safe_url, api_key)
    }

    #[test]
    fn splits_multi_week_range_into_max_five_day_windows_and_cost() {
        let windows = date_windows(d(2024, 1, 1), d(2024, 1, 12));
        assert_eq!(windows.len(), 3);
        assert!(windows.iter().all(|(_, days)| days.get() <= DayRange::MAX));
        let total_cost: u32 = windows.iter().map(|(_, days)| u32::from(days.get())).sum();
        assert_eq!(total_cost, 12);
    }

    #[test]
    fn body_sniffer_accepts_empty_and_header_only_csv() {
        let safe_url = "https://firms.modaps.eosdis.nasa.gov/api/area/csv/***/MODIS_SP/-10,10,10,20/1/2024-01-01";
        let empty = parse_firms_response(StatusCode::OK, "", "MODIS_SP", safe_url, "secret");
        assert!(empty.unwrap().is_empty());

        let header_only = "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight\n";
        let res = parse_firms_response(StatusCode::OK, header_only, "MODIS_SP", safe_url, "secret");
        assert!(res.unwrap().is_empty());
    }

    #[test]
    fn csv_path_returns_header_only_csv_as_text() {
        let safe_url = "https://firms.modaps.eosdis.nasa.gov/api/area/csv/***/MODIS_NRT/-10,10,10,20/1/2024-01-01";
        let header_only = "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight\n";
        let csv = validate_firms_csv(StatusCode::OK, header_only.to_string(), safe_url, "secret")
            .unwrap();
        assert_eq!(csv, header_only);
    }

    #[test]
    fn body_sniffer_rejects_200_error_bodies_and_truncates_utf8_safely() {
        let safe_url = "https://firms.modaps.eosdis.nasa.gov/api/area/csv/***/MODIS_SP/-10,10,10,20/1/2024-01-01";
        let err = parse_firms_response(
            StatusCode::OK,
            "Invalid MAP_KEY",
            "MODIS_SP",
            safe_url,
            "secret-map-key",
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Invalid MAP_KEY"), "unexpected error: {msg}");

        let long_utf8 = "🔥".repeat(250);
        let err = parse_firms_response(
            StatusCode::BAD_GATEWAY,
            &long_utf8,
            "MODIS_SP",
            safe_url,
            "secret-map-key",
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("502"));
        assert!(msg.contains(&"🔥".repeat(200)));
        assert!(!msg.contains(&"🔥".repeat(201)));
    }

    #[test]
    fn api_key_is_redacted_from_urls_and_error_messages() {
        let secret = "super-secret-firms-key-12345";
        let safe_url = format_firms_url(
            FIRMS_API_ROOT,
            "***",
            "VIIRS_NOAA21_SP",
            "-125,24,-66,50",
            d(2024, 5, 1),
            DayRange::clamped(5),
        );
        assert!(safe_url.ends_with("/VIIRS_NOAA21_SP/-125,24,-66,50/5/2024-05-01"));
        assert!(!safe_url.contains(secret));

        let err = parse_firms_response(
            StatusCode::OK,
            &format!("Invalid MAP_KEY: {secret}"),
            "VIIRS_NOAA21_SP",
            &safe_url,
            secret,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(!msg.contains(secret), "key leaked in error message: {msg}");
        assert!(msg.contains("***"));
    }

    #[tokio::test]
    async fn cached_window_returns_hotspots_without_spending_transactions() {
        use pyrosync_core::models::Sensor;

        use crate::source::Processing;

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "pyrosync-client-cache-test-{}-{nanos}",
            std::process::id()
        ));
        let cache = AreaCache::new(dir.clone(), Duration::from_secs(600));
        let source = FirmsSource::new(Sensor::ModisTerra, Processing::Sp);
        let bbox = "-10,10,10,20";
        let days = DayRange::clamped(2);
        let csv = concat!(
            "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight,type\n",
            "38.9486,28.1108,308.5,1,1,2011-01-09,1113,Aqua,MODIS,67,6.2,283.8,10.3,D,0\n",
        );
        cache.store(source, bbox, d(2011, 1, 9), days, csv).await;
        cache.store(source, bbox, d(2011, 1, 11), days, "").await;

        let ctx = RequestCtx {
            http: reqwest::Client::new(),
            api_key: "secret".to_string(),
            api_root: FIRMS_API_ROOT.to_string(),
            limiter: Arc::new(RateLimiter::firms_default(1)),
            area_cache: Some(Arc::new(cache)),
        };

        let (res, spent) = ctx
            .fetch_window_counted(source, bbox, d(2011, 1, 9), days)
            .await;
        assert_eq!(res.unwrap().len(), 1);
        assert_eq!(spent, 0);

        let (res, spent) = ctx
            .fetch_window_counted(source, bbox, d(2011, 1, 11), days)
            .await;
        assert!(res.unwrap().is_empty());
        assert_eq!(spent, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compute_uncovered_reports_gaps_accurately() {
        assert_eq!(
            compute_uncovered(d(2020, 1, 1), d(2020, 1, 10), &[]),
            vec![(d(2020, 1, 1), d(2020, 1, 10))]
        );
        assert_eq!(
            compute_uncovered(
                d(2020, 1, 1),
                d(2020, 1, 10),
                &[(d(2020, 1, 3), d(2020, 1, 6))]
            ),
            vec![
                (d(2020, 1, 1), d(2020, 1, 2)),
                (d(2020, 1, 7), d(2020, 1, 10))
            ]
        );
        assert!(
            compute_uncovered(
                d(2020, 1, 1),
                d(2020, 1, 10),
                &[
                    (d(2020, 1, 1), d(2020, 1, 5)),
                    (d(2020, 1, 6), d(2020, 1, 10)),
                ]
            )
            .is_empty()
        );
    }

    use std::sync::Mutex;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::Instant;

    use crate::source::Processing;

    const KEY: &str = "secret-map-key";
    const BBOX: &str = "-10,10,10,20";
    const HEADER: &str = "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight,type\n";
    const ROW: &str = "38.9486,28.1108,308.5,1,1,2020-06-01,1113,Aqua,MODIS,67,6.2,283.8,10.3,D,0\n";
    const AVAIL: &str = "data_id,min_date,max_date\nMODIS_SP,2020-01-01,2020-12-31\nVIIRS_SNPP_SP,2020-01-01,2020-12-31\n";

    type Reply = (u16, Vec<(&'static str, &'static str)>, String);

    fn ok(body: &str) -> Reply {
        (200, Vec::new(), body.to_string())
    }

    fn status(code: u16, body: &str) -> Reply {
        (code, Vec::new(), body.to_string())
    }

    /// Loopback HTTP stub: the handler sees (request index, path) and returns the reply.
    struct Stub {
        root: String,
        paths: Arc<Mutex<Vec<String>>>,
    }

    impl Stub {
        async fn start(handler: impl Fn(usize, &str) -> Reply + Send + Sync + 'static) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let root = format!("http://{}", listener.local_addr().unwrap());
            let paths = Arc::new(Mutex::new(Vec::new()));
            let handler = Arc::new(handler);
            let seen = Arc::clone(&paths);
            tokio::spawn(async move {
                while let Ok((mut sock, _)) = listener.accept().await {
                    let handler = Arc::clone(&handler);
                    let seen = Arc::clone(&seen);
                    tokio::spawn(async move {
                        let mut buf = Vec::new();
                        let mut chunk = [0u8; 1024];
                        while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                            match sock.read(&mut chunk).await {
                                Ok(0) | Err(_) => return,
                                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                            }
                        }
                        let request = String::from_utf8_lossy(&buf).into_owned();
                        let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                        let index = {
                            let mut seen = seen.lock().unwrap();
                            seen.push(path.clone());
                            seen.len() - 1
                        };
                        let (code, headers, body) = handler(index, &path);
                        let mut response = format!(
                            "HTTP/1.1 {code} Stub\r\ncontent-length: {}\r\nconnection: close\r\n",
                            body.len()
                        );
                        for (name, value) in headers {
                            response.push_str(&format!("{name}: {value}\r\n"));
                        }
                        response.push_str("\r\n");
                        response.push_str(&body);
                        let _ = sock.write_all(response.as_bytes()).await;
                        let _ = sock.shutdown().await;
                    });
                }
            });
            Self { root, paths }
        }

        fn hits(&self) -> usize {
            self.paths.lock().unwrap().len()
        }

        fn client(&self) -> FirmsClient {
            FirmsClient::new(KEY.to_string()).unwrap().with_api_root(self.root.clone())
        }
    }

    fn is_availability(path: &str) -> bool {
        path.contains("data_availability")
    }

    fn temp_cache(name: &str) -> (AreaCache, std::path::PathBuf) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "pyrosync-client-test-{name}-{}-{nanos}",
            std::process::id()
        ));
        (AreaCache::new(dir.clone(), Duration::from_secs(600)), dir)
    }

    fn sp() -> FirmsSource {
        FirmsSource::new(Sensor::ModisTerra, Processing::Sp)
    }

    fn bbox() -> BBox {
        BBox { west: -10.0, south: 10.0, east: 10.0, north: 20.0 }
    }

    // ---- availability: Uninitialised -> Initialised, failure keeps it Uninitialised ----

    #[tokio::test(start_paused = true)]
    async fn availability_is_fetched_once_then_served_from_memory() {
        let stub = Stub::start(|_, _| ok(AVAIL)).await;
        let client = stub.client();

        let first = client.availability().await.unwrap().range_for(sp());
        let second = client.availability().await.unwrap().range_for(sp());

        assert_eq!(first, second);
        assert_eq!(stub.hits(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn failed_availability_fetch_is_not_cached_and_the_next_call_retries() {
        let stub = Stub::start(|i, _| if i == 0 { status(500, "boom") } else { ok(AVAIL) }).await;
        let client = stub.client();

        let err = client.availability().await.unwrap_err().to_string();
        assert!(err.contains("500"), "{err}");
        assert!(client.availability().await.unwrap().range_for(sp()).is_some());
        assert_eq!(stub.hits(), 2);
    }

    #[tokio::test(start_paused = true)]
    #[ignore = "BUG-8: availability never refreshed"]
    async fn availability_reflects_newer_firms_data_on_a_later_call() {
        // client.rs `availability: OnceCell<Availability>` is filled once per process.
        let stub = Stub::start(|i, _| {
            if i == 0 {
                ok("data_id,min_date,max_date\nMODIS_NRT,2026-01-01,2026-06-14\n")
            } else {
                ok("data_id,min_date,max_date\nMODIS_NRT,2026-01-01,2026-06-15\n")
            }
        })
        .await;
        let client = stub.client();
        let nrt = FirmsSource::new(Sensor::ModisTerra, Processing::Nrt);

        let before = client.availability().await.unwrap().range_for(nrt).unwrap().1;
        let after = client.availability().await.unwrap().range_for(nrt).unwrap().1;

        assert_ne!(before, after, "FIRMS advanced max_date but the client kept the stale table");
    }

    #[tokio::test(start_paused = true)]
    async fn availability_error_redacts_the_api_key() {
        let stub = Stub::start(|_, _| ok(&format!("Invalid MAP_KEY {KEY}"))).await;
        let err = stub.client().availability().await.unwrap_err().to_string();
        assert!(err.contains("Invalid MAP_KEY") && err.contains("***"), "{err}");
        assert!(!err.contains(KEY), "{err}");
    }

    #[tokio::test(start_paused = true)]
    async fn availability_success_is_persisted_and_failure_falls_back_to_that_file() {
        let (cache, dir) = temp_cache("avail-fallback");
        let online = Stub::start(|_, _| ok(AVAIL)).await;
        online.client().with_area_cache(cache.clone()).availability().await.unwrap();
        assert!(cache.load_availability().await.is_some());

        let offline = Stub::start(|_, _| status(503, "down")).await;
        let client = offline.client().with_area_cache(cache);
        assert!(client.availability().await.unwrap().range_for(sp()).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn availability_failure_without_a_usable_cache_file_is_an_error() {
        let (cache, dir) = temp_cache("avail-no-fallback");
        cache.store_availability("<html>corrupt</html>").await;
        let stub = Stub::start(|_, _| status(404, "nope")).await;
        let result = stub.client().with_area_cache(cache).availability().await.map(|_| ());
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- retry state machine: Attempt(0) -> Attempt(1) -> Attempt(2) -> Done ----

    #[tokio::test(start_paused = true)]
    async fn transient_503_is_retried_once_and_both_attempts_are_billed() {
        let stub = Stub::start(|i, _| if i == 0 { status(503, "busy") } else { ok(&format!("{HEADER}{ROW}")) }).await;
        let ctx = stub.client().request_ctx();
        let start = Instant::now();

        let (res, spent) = ctx.fetch_window_counted(sp(), BBOX, d(2020, 6, 1), DayRange::clamped(2)).await;

        assert_eq!(res.unwrap().len(), 1);
        assert_eq!(stub.hits(), 2);
        assert_eq!(spent, 4);
        assert_eq!(start.elapsed(), Duration::from_secs(1));
    }

    #[tokio::test(start_paused = true)]
    async fn retry_after_header_is_added_to_the_backoff() {
        let stub = Stub::start(|i, _| {
            if i == 0 { (429, vec![("retry-after", "2")], String::new()) } else { ok("") }
        })
        .await;
        let ctx = stub.client().request_ctx();
        let start = Instant::now();

        let (res, spent) = ctx.fetch_window_csv_counted(sp(), BBOX, d(2020, 6, 1), DayRange::clamped(1)).await;

        assert_eq!(res.unwrap(), "");
        assert_eq!(spent, 2);
        assert_eq!(start.elapsed(), Duration::from_secs(3));
    }

    #[tokio::test(start_paused = true)]
    async fn persistent_5xx_exhausts_retries_with_growing_backoff_then_fails() {
        let stub = Stub::start(|_, _| status(502, "bad gateway")).await;
        let ctx = stub.client().request_ctx();
        let start = Instant::now();

        let (res, spent) = ctx.fetch_window_csv_counted(sp(), BBOX, d(2020, 6, 1), DayRange::clamped(3)).await;

        let err = res.unwrap_err().to_string();
        assert!(err.contains("502") && err.contains("bad gateway"), "{err}");
        assert_eq!(stub.hits(), MAX_RETRIES + 1);
        assert_eq!(spent, 9);
        assert_eq!(start.elapsed(), Duration::from_secs(1 + 4));
    }

    #[tokio::test(start_paused = true)]
    async fn client_errors_are_not_retried() {
        for code in [400u16, 401, 404] {
            let stub = Stub::start(move |_, _| status(code, "denied")).await;
            let ctx = stub.client().request_ctx();
            let (res, spent) = ctx.fetch_window_csv_counted(sp(), BBOX, d(2020, 6, 1), DayRange::clamped(1)).await;
            assert!(res.unwrap_err().to_string().contains(&code.to_string()));
            assert_eq!((stub.hits(), spent), (1, 1), "status {code}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn connection_failure_is_reported_with_redacted_url_and_billed_once() {
        let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let root = format!("http://{}", closed.local_addr().unwrap());
        drop(closed);
        let ctx = FirmsClient::new(KEY.to_string()).unwrap().with_api_root(root).request_ctx();

        let (res, spent) = ctx.fetch_window_csv_counted(sp(), BBOX, d(2020, 6, 1), DayRange::clamped(4)).await;

        let err = res.unwrap_err().to_string();
        assert!(err.contains("***") && !err.contains(KEY), "{err}");
        assert_eq!(spent, 4);
    }

    #[tokio::test(start_paused = true)]
    async fn request_url_carries_key_source_bbox_days_and_start_date() {
        let stub = Stub::start(|_, _| ok("")).await;
        let csv = stub
            .client()
            .fetch_window_csv(sp(), &bbox(), d(2020, 6, 1), DayRange::clamped(5))
            .await
            .unwrap();
        assert_eq!(csv, "");
        assert_eq!(
            stub.paths.lock().unwrap()[0],
            format!("/api/area/csv/{KEY}/MODIS_SP/-10,10,10,20/5/2020-06-01")
        );
    }

    // ---- response cache lifecycle through the client ----

    #[tokio::test(start_paused = true)]
    async fn window_walks_miss_stored_hit_without_a_second_request() {
        let (cache, dir) = temp_cache("hit");
        let stub = Stub::start(|_, _| ok(&format!("{HEADER}{ROW}"))).await;
        let ctx = stub.client().with_area_cache(cache).request_ctx();
        let days = DayRange::clamped(2);

        let (first, spent1) = ctx.fetch_window_counted(sp(), BBOX, d(2020, 6, 1), days).await;
        let (second, spent2) = ctx.fetch_window_counted(sp(), BBOX, d(2020, 6, 1), days).await;

        assert_eq!((first.unwrap().len(), second.unwrap().len()), (1, 1));
        assert_eq!((spent1, spent2), (2, 0));
        assert_eq!(stub.hits(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn rejected_and_failed_bodies_are_never_cached() {
        let (cache, dir) = temp_cache("no-poison");
        let stub = Stub::start(|i, _| match i {
            0 => ok("Invalid MAP_KEY"),
            1 => status(404, "missing"),
            _ => ok(&format!("{HEADER}{ROW}")),
        })
        .await;
        let ctx = stub.client().with_area_cache(cache).request_ctx();
        let days = DayRange::clamped(1);

        assert!(ctx.fetch_window_csv_counted(sp(), BBOX, d(2020, 6, 1), days).await.0.is_err());
        assert!(ctx.fetch_window_csv_counted(sp(), BBOX, d(2020, 6, 1), days).await.0.is_err());
        let third = ctx.fetch_window_csv_counted(sp(), BBOX, d(2020, 6, 1), days).await.0;
        assert!(third.unwrap().contains("latitude"));
        assert_eq!(stub.hits(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn empty_valid_response_is_cached_as_a_hit() {
        let (cache, dir) = temp_cache("empty-hit");
        let stub = Stub::start(|_, _| ok("")).await;
        let ctx = stub.client().with_area_cache(cache).request_ctx();
        let days = DayRange::clamped(1);

        ctx.fetch_window_csv_counted(sp(), BBOX, d(2020, 6, 1), days).await.0.unwrap();
        let (again, spent) = ctx.fetch_window_csv_counted(sp(), BBOX, d(2020, 6, 1), days).await;

        assert_eq!(again.unwrap(), "");
        assert_eq!((spent, stub.hits()), (0, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- fetch_range / fetch_all_range ----

    #[tokio::test(start_paused = true)]
    async fn fetch_range_splits_into_windows_and_sums_transactions() {
        let stub = Stub::start(|_, path| {
            if is_availability(path) { ok(AVAIL) } else { ok(&format!("{HEADER}{ROW}")) }
        })
        .await;

        let (hotspots, report) = stub
            .client()
            .fetch_range(Sensor::ModisTerra, &bbox(), d(2020, 6, 1), d(2020, 6, 7))
            .await;

        assert_eq!(hotspots.len(), 2);
        assert_eq!(report.per_source, vec![("MODIS_SP".to_string(), 2)]);
        assert_eq!(report.transactions_spent, 7);
        assert!(report.failures.is_empty() && report.uncovered.is_empty());
        assert_eq!(stub.hits(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn fetch_range_reports_dates_no_source_covers() {
        let stub = Stub::start(|_, path| {
            if is_availability(path) { ok(AVAIL) } else { ok("") }
        })
        .await;

        let (_, report) = stub
            .client()
            .fetch_range(Sensor::ModisTerra, &bbox(), d(2019, 12, 30), d(2020, 1, 2))
            .await;

        assert_eq!(report.uncovered, vec![(d(2019, 12, 30), d(2019, 12, 31))]);
    }

    #[tokio::test(start_paused = true)]
    async fn fetch_range_reversed_dates_do_no_work() {
        let stub = Stub::start(|_, _| ok(AVAIL)).await;
        let (hotspots, report) = stub
            .client()
            .fetch_range(Sensor::ModisTerra, &bbox(), d(2020, 6, 2), d(2020, 6, 1))
            .await;
        assert!(hotspots.is_empty() && report.uncovered.is_empty() && report.failures.is_empty());
        assert_eq!(stub.hits(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn fetch_range_marks_everything_uncovered_when_availability_is_down() {
        let stub = Stub::start(|_, _| status(500, &format!("oops {KEY}"))).await;
        let (hotspots, report) = stub
            .client()
            .fetch_range(Sensor::ViirsSNPP, &bbox(), d(2020, 6, 1), d(2020, 6, 3))
            .await;

        assert!(hotspots.is_empty());
        assert_eq!(report.uncovered, vec![(d(2020, 6, 1), d(2020, 6, 3))]);
        assert_eq!(report.failures.len(), 1);
        assert!(!report.failures[0].1.contains(KEY));
    }

    #[tokio::test(start_paused = true)]
    async fn one_failing_window_is_collected_without_losing_the_others() {
        let stub = Stub::start(|_, path| {
            if is_availability(path) {
                ok(AVAIL)
            } else if path.ends_with("/2020-06-06") {
                status(400, &format!("bad {KEY}"))
            } else {
                ok(&format!("{HEADER}{ROW}"))
            }
        })
        .await;

        let (hotspots, report) = stub
            .client()
            .fetch_range(Sensor::ModisTerra, &bbox(), d(2020, 6, 1), d(2020, 6, 7))
            .await;

        assert_eq!(hotspots.len(), 1);
        assert_eq!(report.failures.len(), 1);
        let (source, message) = &report.failures[0];
        assert_eq!(source, "MODIS_SP");
        assert!(message.starts_with("2020-06-06 (2d)") && !message.contains(KEY), "{message}");
    }

    #[tokio::test(start_paused = true)]
    async fn fetch_all_range_routes_modis_and_viirs_and_tolerates_a_dead_sensor() {
        let stub = Stub::start(|_, path| {
            if is_availability(path) {
                ok(AVAIL)
            } else if path.contains("/MODIS_SP/") {
                ok(&format!("{HEADER}{ROW}"))
            } else {
                ok("")
            }
        })
        .await;

        let (modis, viirs, report) = stub.client().fetch_all_range(&bbox(), d(2020, 6, 1), d(2020, 6, 2)).await;

        assert_eq!(modis.len(), 1);
        assert!(viirs.is_empty());
        assert!(report.failures.is_empty() && report.uncovered.is_empty());
        assert!(report.per_source.contains(&("MODIS_SP".to_string(), 1)));
        assert!(report.per_source.contains(&("VIIRS_SNPP_SP".to_string(), 0)));
        // NOAA-20 and NOAA-21 have no availability rows, so they cost nothing.
        assert_eq!(report.transactions_spent, 4);
    }

    #[tokio::test(start_paused = true)]
    async fn fetch_all_range_reports_availability_failure_and_reversed_range() {
        let stub = Stub::start(|_, _| status(500, "down")).await;
        let client = stub.client();

        let (m, v, report) = client.fetch_all_range(&bbox(), d(2020, 6, 5), d(2020, 6, 1)).await;
        assert!(m.is_empty() && v.is_empty() && report.uncovered.is_empty());
        assert_eq!(stub.hits(), 0);

        let (_, _, report) = client.fetch_all_range(&bbox(), d(2020, 6, 1), d(2020, 6, 5)).await;
        assert_eq!(report.failures[0].0, "availability");
        assert_eq!(report.uncovered, vec![(d(2020, 6, 1), d(2020, 6, 5))]);
    }

    // ---- pure helpers ----

    #[test]
    fn retry_after_parses_seconds_and_ignores_everything_else() {
        let header = |value: &str| {
            let mut map = HeaderMap::new();
            map.insert(RETRY_AFTER, value.parse().unwrap());
            map
        };
        assert_eq!(parse_retry_after(&HeaderMap::new()), Duration::ZERO);
        assert_eq!(parse_retry_after(&header("7")), Duration::from_secs(7));
        assert_eq!(parse_retry_after(&header(" 12 ")), Duration::from_secs(12));
        assert_eq!(parse_retry_after(&header("0")), Duration::ZERO);
        assert_eq!(parse_retry_after(&header("-3")), Duration::ZERO);
        assert_eq!(parse_retry_after(&header("Wed, 21 Oct 2026 07:28:00 GMT")), Duration::ZERO);
    }

    #[test]
    fn redact_key_replaces_every_occurrence_and_tolerates_an_empty_key() {
        assert_eq!(redact_key("a k b k", "k"), "a *** b ***");
        assert_eq!(redact_key("nothing here", "k"), "nothing here");
        assert_eq!(redact_key("unchanged", ""), "unchanged");
        assert_eq!(truncate_chars("abcdef", 3), "abc");
        assert_eq!(truncate_chars("ab", 5), "ab");
    }

    #[test]
    fn compute_uncovered_handles_overlap_disorder_clipping_and_trivial_ranges() {
        let (s, e) = (d(2020, 1, 1), d(2020, 1, 10));
        assert!(compute_uncovered(e, s, &[]).is_empty());
        assert_eq!(compute_uncovered(s, s, &[]), vec![(s, s)]);
        assert!(compute_uncovered(s, s, &[(s, s)]).is_empty());
        // Unsorted, overlapping, and extending past the requested range on both sides.
        let covered = [(d(2020, 1, 6), d(2020, 1, 20)), (d(2019, 12, 1), d(2020, 1, 4)), (d(2020, 1, 3), d(2020, 1, 5))];
        assert!(compute_uncovered(s, e, &covered).is_empty());
        // Intervals entirely outside the range are ignored.
        assert_eq!(compute_uncovered(s, e, &[(d(2021, 1, 1), d(2021, 1, 2))]), vec![(s, e)]);
        // One-day hole between two covered blocks.
        assert_eq!(
            compute_uncovered(s, e, &[(s, d(2020, 1, 4)), (d(2020, 1, 6), e)]),
            vec![(d(2020, 1, 5), d(2020, 1, 5))]
        );
    }

    #[test]
    fn validate_firms_csv_error_path_and_blank_body_edge_cases() {
        let url = "https://x/***";
        let err = validate_firms_csv(StatusCode::INTERNAL_SERVER_ERROR, format!("fail {KEY}"), url, KEY)
            .unwrap_err()
            .to_string();
        assert!(err.contains("500") && !err.contains(KEY), "{err}");
        assert_eq!(validate_firms_csv(StatusCode::OK, "  \n\t".into(), url, KEY).unwrap(), "");
        let err = validate_firms_csv(StatusCode::OK, "lat,lon\n1,2\n".into(), url, KEY).unwrap_err();
        assert!(err.to_string().contains("non-CSV response"));
    }

    #[test]
    fn parse_validated_csv_treats_header_plus_blank_lines_as_empty_and_redacts_parse_errors() {
        let url = "https://x/***";
        let blank_tail = format!("{HEADER}\n   \n\n");
        assert!(parse_validated_csv(&blank_tail, "MODIS_SP", url, KEY).unwrap().is_empty());
        assert!(parse_validated_csv("  ", "MODIS_SP", url, KEY).unwrap().is_empty());

        let broken = format!("{HEADER}not,a,valid,row,{KEY}\n");
        let err = parse_validated_csv(&broken, "MODIS_SP", url, KEY).unwrap_err().to_string();
        assert!(err.starts_with("failed parsing FIRMS CSV"), "{err}");
        assert!(!err.contains(KEY));
    }

    #[test]
    fn builder_normalises_root_zero_concurrency_and_hides_the_key_in_debug() {
        let client = FirmsClient::with_concurrency(KEY.to_string(), 0)
            .unwrap()
            .with_api_root("http://localhost:1234///");
        assert_eq!(client.api_root, "http://localhost:1234");
        assert_eq!(client.concurrency, 1);
        let debug = format!("{client:?}");
        assert!(debug.contains("***") && !debug.contains(KEY));
        assert!(FetchReport::default().failures.is_empty());
    }
}
