use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{NaiveDate, Utc};
use pyrosync_core::models::{BBox, Hotspot, Sensor};
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use tokio::sync::OnceCell;
use tokio::task::JoinSet;

use crate::availability::Availability;
use crate::day_range::{DayRange, date_windows, recent_window};
use crate::limiter::RateLimiter;
use crate::parse::parse_csv;
use crate::source::FirmsSource;

pub const FIRMS_BASE_URL: &str = "https://firms.modaps.eosdis.nasa.gov/api/area/csv";

const USER_AGENT: &str = "pyrosync/0.1 (+https://github.com/TeamMagi)";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_CONCURRENCY: usize = 4;
const MAX_RETRIES: usize = 2;
const MAX_ERROR_BODY_CHARS: usize = 200;

pub struct FirmsClient {
    http: reqwest::Client,
    api_key: String,
    limiter: Arc<RateLimiter>,
    availability: OnceCell<Availability>,
    concurrency: usize,
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
    limiter: Arc<RateLimiter>,
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
            limiter: Arc::new(RateLimiter::firms_default(concurrency)),
            availability: OnceCell::new(),
            concurrency: concurrency.max(1),
        })
    }

    /// Lazily fetched once and cached; every date-routing decision needs it.
    pub async fn availability(&self) -> anyhow::Result<&Availability> {
        self.availability
            .get_or_try_init(|| async {
                Availability::fetch(&self.http, &self.api_key)
                    .await
                    .map_err(|err| {
                        anyhow::anyhow!("{}", redact_key(&format!("{err:#}"), &self.api_key))
                    })
            })
            .await
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
            limiter: Arc::clone(&self.limiter),
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
    async fn fetch_window_counted(
        &self,
        source: FirmsSource,
        bbox_str: &str,
        start: NaiveDate,
        days: DayRange,
    ) -> (anyhow::Result<Vec<Hotspot>>, u32) {
        let source_str = source.as_str();
        let url = format_firms_url(&self.api_key, source_str, bbox_str, start, days);
        let safe_url = format_firms_url("***", source_str, bbox_str, start, days);
        let cost = u32::from(days.get());
        let mut spent = 0u32;

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

            let parsed = parse_firms_response(status, &body, source_str, &safe_url, &self.api_key);
            return (parsed, spent);
        }

        unreachable!("retry loop always exits by MAX_RETRIES")
    }
}

fn format_firms_url(
    api_key: &str,
    source: &str,
    bbox_str: &str,
    start: NaiveDate,
    days: DayRange,
) -> String {
    format!(
        "{FIRMS_BASE_URL}/{api_key}/{source}/{bbox_str}/{}/{}",
        days.get(),
        start.format("%Y-%m-%d")
    )
}

fn parse_firms_response(
    status: StatusCode,
    body: &str,
    source: &str,
    safe_url: &str,
    api_key: &str,
) -> anyhow::Result<Vec<Hotspot>> {
    if !status.is_success() {
        let snippet = redact_key(&truncate_chars(body.trim(), MAX_ERROR_BODY_CHARS), api_key);
        anyhow::bail!("FIRMS request to {safe_url} failed with HTTP {status}: {snippet}");
    }

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut lines = trimmed.lines();
    let first_line = lines.next().unwrap_or("");
    if !first_line.contains("latitude") {
        let snippet = redact_key(&truncate_chars(trimmed, MAX_ERROR_BODY_CHARS), api_key);
        anyhow::bail!(
            "FIRMS returned non-CSV response (HTTP {status}) from {safe_url}: {snippet}"
        );
    }

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
}
