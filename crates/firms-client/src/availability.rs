use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use pyrosync_core::models::Sensor;
use tracing::debug;

use crate::source::{FirmsSource, Processing};

const AVAILABILITY_URL: &str = "https://firms.modaps.eosdis.nasa.gov/api/data_availability/csv";

#[derive(Debug, Clone)]
pub struct Availability {
    ranges: HashMap<String, (NaiveDate, NaiveDate)>,
}

impl Availability {
    pub async fn fetch(http: &reqwest::Client, api_key: &str) -> Result<Self> {
        let url = format!("{AVAILABILITY_URL}/{api_key}/ALL");
        let response = http
            .get(&url)
            .send()
            .await
            .context("FIRMS data_availability request failed")?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed to read FIRMS data_availability response")?;

        if !status.is_success() {
            bail!(
                "FIRMS data_availability HTTP {status}: {}",
                truncate_chars(body.trim(), 200)
            );
        }

        Self::from_csv(&body)
    }

    pub fn from_csv(csv_text: &str) -> Result<Self> {
        let trimmed = csv_text.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            bail!("empty FIRMS data_availability response");
        }

        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .trim(csv::Trim::All)
            .from_reader(trimmed.as_bytes());

        let headers = reader
            .headers()
            .context("failed to read FIRMS data_availability headers")?
            .clone();

        let find_col = |candidates: &[&str]| -> Option<usize> {
            headers.iter().position(|h| {
                let normalized = h.trim().to_ascii_lowercase();
                candidates.iter().any(|c| *c == normalized)
            })
        };

        let id_idx = find_col(&["data_id", "source", "id"]);
        let min_idx = find_col(&["min_date", "start_date", "begin_date"]);
        let max_idx = find_col(&["max_date", "end_date"]);

        // FIRMS often serves plain-text or HTML errors with HTTP 200; reject unexpected bodies here.
        let (Some(id_idx), Some(min_idx), Some(max_idx)) = (id_idx, min_idx, max_idx) else {
            bail!(
                "unexpected FIRMS data_availability response: {}",
                truncate_chars(trimmed, 200)
            );
        };

        let mut ranges: HashMap<String, (NaiveDate, NaiveDate)> = HashMap::new();

        for (idx, row) in reader.records().enumerate() {
            let record = match row {
                Ok(r) => r,
                Err(err) => {
                    debug!(row = idx + 1, error = %err, "ignoring unparseable availability row");
                    continue;
                }
            };

            let Some(source_id) = record.get(id_idx).map(str::trim).filter(|s| !s.is_empty())
            else {
                debug!(
                    row = idx + 1,
                    "ignoring availability row with missing source id"
                );
                continue;
            };

            let Some(min_raw) = record.get(min_idx).map(str::trim) else {
                debug!(
                    row = idx + 1,
                    source = source_id,
                    "ignoring availability row with missing start date"
                );
                continue;
            };
            let Some(max_raw) = record.get(max_idx).map(str::trim) else {
                debug!(
                    row = idx + 1,
                    source = source_id,
                    "ignoring availability row with missing end date"
                );
                continue;
            };

            let (Ok(min_date), Ok(max_date)) = (
                NaiveDate::parse_from_str(min_raw, "%Y-%m-%d"),
                NaiveDate::parse_from_str(max_raw, "%Y-%m-%d"),
            ) else {
                debug!(
                    row = idx + 1,
                    source = source_id,
                    min = min_raw,
                    max = max_raw,
                    "ignoring availability row with unparseable dates"
                );
                continue;
            };

            if min_date > max_date {
                debug!(
                    row = idx + 1,
                    source = source_id,
                    %min_date,
                    %max_date,
                    "ignoring availability row with inverted date range"
                );
                continue;
            }

            ranges
                .entry(source_id.to_string())
                .and_modify(|(lo, hi)| {
                    *lo = (*lo).min(min_date);
                    *hi = (*hi).max(max_date);
                })
                .or_insert((min_date, max_date));
        }

        Ok(Self { ranges })
    }

    pub fn range_for(&self, source: FirmsSource) -> Option<(NaiveDate, NaiveDate)> {
        self.ranges.get(source.as_str()).copied()
    }

    pub fn covers(&self, source: FirmsSource, date: NaiveDate) -> bool {
        self.range_for(source)
            .is_some_and(|(min, max)| date >= min && date <= max)
    }

    /// Choose the right processing level for a date: prefer SP when it covers the date
    /// (science quality, and the only thing that reaches back years), else NRT.
    /// Returns None when neither covers the date.
    pub fn pick(&self, sensor: Sensor, date: NaiveDate) -> Option<FirmsSource> {
        let sp = FirmsSource::new(sensor, Processing::Sp);
        if self.covers(sp, date) {
            return Some(sp);
        }

        let nrt = FirmsSource::new(sensor, Processing::Nrt);
        if self.covers(nrt, date) {
            return Some(nrt);
        }

        None
    }

    /// Split [start, end] into contiguous spans that each use a single source, so a query
    /// crossing the SP/NRT boundary is served correctly instead of silently losing data.
    pub fn plan(
        &self,
        sensor: Sensor,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Vec<(FirmsSource, NaiveDate, NaiveDate)> {
        if start > end {
            return Vec::new();
        }

        let sp = FirmsSource::new(sensor, Processing::Sp);
        let nrt = FirmsSource::new(sensor, Processing::Nrt);
        let sp_range = self.range_for(sp);
        let nrt_range = self.range_for(nrt);

        let intersect = |a_start: NaiveDate,
                         a_end: NaiveDate,
                         range: (NaiveDate, NaiveDate)|
         -> Option<(NaiveDate, NaiveDate)> {
            let s = a_start.max(range.0);
            let e = a_end.min(range.1);
            (s <= e).then_some((s, e))
        };

        let push_nrt =
            |spans: &mut Vec<(FirmsSource, NaiveDate, NaiveDate)>, s: NaiveDate, e: NaiveDate| {
                if s <= e
                    && let Some(range) = nrt_range
                    && let Some((ns, ne)) = intersect(s, e, range)
                {
                    spans.push((nrt, ns, ne));
                }
            };

        let mut spans = Vec::new();
        match sp_range.and_then(|r| intersect(start, end, r)) {
            Some((sp_start, sp_end)) => {
                if start < sp_start && let Some(left_end) = sp_start.pred_opt() {
                    push_nrt(&mut spans, start, left_end);
                }
                spans.push((sp, sp_start, sp_end));
                if sp_end < end && let Some(right_start) = sp_end.succ_opt() {
                    push_nrt(&mut spans, right_start, end);
                }
            }
            None => push_nrt(&mut spans, start, end),
        }

        spans
    }
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn fixture_csv() -> &'static str {
        "\
data_id,min_date,max_date
MODIS_SP,2000-11-01,2026-03-31
MODIS_NRT,2026-01-01,2026-06-14
VIIRS_SNPP_SP,2012-01-20,2026-02-28
VIIRS_SNPP_NRT,2026-01-01,2026-06-14
"
    }

    #[test]
    fn pick_prefers_sp_for_old_and_nrt_for_recent_and_none_outside() {
        let avail = Availability::from_csv(fixture_csv()).unwrap();

        assert_eq!(
            avail.pick(Sensor::ModisTerra, d(2015, 7, 1)),
            Some(FirmsSource::new(Sensor::ModisTerra, Processing::Sp))
        );
        assert_eq!(
            avail.pick(Sensor::ModisTerra, d(2026, 2, 15)),
            Some(FirmsSource::new(Sensor::ModisTerra, Processing::Sp))
        );
        assert_eq!(
            avail.pick(Sensor::ModisTerra, d(2026, 5, 1)),
            Some(FirmsSource::new(Sensor::ModisTerra, Processing::Nrt))
        );
        assert_eq!(avail.pick(Sensor::ModisTerra, d(1999, 12, 31)), None);
        assert_eq!(avail.pick(Sensor::ModisTerra, d(2026, 6, 15)), None);

        assert_eq!(
            avail.pick(Sensor::ViirsSNPP, d(2020, 1, 1)),
            Some(FirmsSource::new(Sensor::ViirsSNPP, Processing::Sp))
        );
        assert_eq!(
            avail.pick(Sensor::ViirsSNPP, d(2026, 4, 10)),
            Some(FirmsSource::new(Sensor::ViirsSNPP, Processing::Nrt))
        );
        assert_eq!(avail.pick(Sensor::ViirsNOAA20, d(2026, 4, 10)), None);
    }

    #[test]
    fn plan_splits_across_sp_nrt_boundary_without_gap_or_overlap() {
        let avail = Availability::from_csv(fixture_csv()).unwrap();

        let spans = avail.plan(Sensor::ModisTerra, d(2026, 3, 25), d(2026, 4, 5));
        assert_eq!(
            spans,
            vec![
                (
                    FirmsSource::new(Sensor::ModisTerra, Processing::Sp),
                    d(2026, 3, 25),
                    d(2026, 3, 31),
                ),
                (
                    FirmsSource::new(Sensor::ModisTerra, Processing::Nrt),
                    d(2026, 4, 1),
                    d(2026, 4, 5),
                ),
            ]
        );
    }

    #[test]
    fn plan_returns_single_span_when_one_source_covers_range() {
        let avail = Availability::from_csv(fixture_csv()).unwrap();

        assert_eq!(
            avail.plan(Sensor::ViirsSNPP, d(2026, 1, 10), d(2026, 2, 10)),
            vec![(
                FirmsSource::new(Sensor::ViirsSNPP, Processing::Sp),
                d(2026, 1, 10),
                d(2026, 2, 10),
            )]
        );
        assert_eq!(
            avail.plan(Sensor::ViirsSNPP, d(2026, 4, 1), d(2026, 4, 15)),
            vec![(
                FirmsSource::new(Sensor::ViirsSNPP, Processing::Nrt),
                d(2026, 4, 1),
                d(2026, 4, 15),
            )]
        );
    }

    #[test]
    fn parses_alternate_column_names() {
        let csv = "\
source,start_date,end_date
MODIS_SP,2000-11-01,2026-03-31
VIIRS_SNPP_NRT,2026-01-01,2026-06-14
";
        let avail = Availability::from_csv(csv).unwrap();

        assert_eq!(
            avail.range_for(FirmsSource::new(Sensor::ModisTerra, Processing::Sp)),
            Some((d(2000, 11, 1), d(2026, 3, 31)))
        );
        assert_eq!(
            avail.range_for(FirmsSource::new(Sensor::ViirsSNPP, Processing::Nrt)),
            Some((d(2026, 1, 1), d(2026, 6, 14)))
        );
    }

    #[test]
    fn skips_malformed_rows_without_failing() {
        let csv = "\
data_id,min_date,max_date
MODIS_SP,2000-11-01,2026-03-31
BAD_DATE,not-a-date,2026-06-14
INVERTED,2026-06-14,2026-01-01
MISSING_MAX,2026-01-01
,2026-01-01,2026-06-14
VIIRS_SNPP_NRT,2026-01-01,2026-06-14
";
        let avail = Availability::from_csv(csv).unwrap();

        assert_eq!(
            avail.range_for(FirmsSource::new(Sensor::ModisTerra, Processing::Sp)),
            Some((d(2000, 11, 1), d(2026, 3, 31)))
        );
        assert_eq!(
            avail.range_for(FirmsSource::new(Sensor::ViirsSNPP, Processing::Nrt)),
            Some((d(2026, 1, 1), d(2026, 6, 14)))
        );
        assert_eq!(avail.ranges.len(), 2);
    }

    #[test]
    fn rejects_non_csv_error_body_and_truncates_message() {
        let err = Availability::from_csv("Invalid MAP_KEY").unwrap_err();
        assert!(err.to_string().contains("Invalid MAP_KEY"));

        let long_html = format!("<html>{}</html>", "x".repeat(300));
        let err = Availability::from_csv(&long_html).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("<html>"));
        assert!(!msg.contains(&"x".repeat(250)));
    }
}
