use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pyrosync_core::models::{BBox, Hotspot, Sensor};
use tokio::sync::Semaphore;

use crate::source::{FirmsSource, Processing};

pub const ARCHIVE_BASE: &str = "https://firms.modaps.eosdis.nasa.gov/data/country";

const USER_AGENT: &str = "pyrosync/0.1 (+https://github.com/TeamMagi)";

/// Inclusive year coverage of the pre-packaged country archive, per sensor.
pub fn archive_years(sensor: Sensor) -> Option<RangeInclusive<i32>> {
    match sensor {
        Sensor::ModisTerra | Sensor::ModisAqua => Some(2000..=2025),
        Sensor::ViirsSNPP => Some(2012..=2025),
        Sensor::ViirsNOAA20 | Sensor::ViirsNOAA21 => None,
    }
}

pub fn archive_slug(sensor: Sensor) -> Option<&'static str> {
    match sensor {
        Sensor::ModisTerra | Sensor::ModisAqua => Some("modis"),
        Sensor::ViirsSNPP => Some("viirs-snpp"),
        Sensor::ViirsNOAA20 | Sensor::ViirsNOAA21 => None,
    }
}

pub struct ArchiveClient {
    http: reqwest::Client,
    cache_dir: PathBuf,
}

impl ArchiveClient {
    pub fn new(cache_dir: PathBuf) -> anyhow::Result<Self> {
        // This constructor is synchronous, so the one-time directory creation cannot
        // await tokio::fs; every later cache read/write goes through tokio::fs.
        std::fs::create_dir_all(&cache_dir)?;

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .user_agent(USER_AGENT)
            .build()?;

        Ok(Self { http, cache_dir })
    }

    /// Returns true when (sensor, year) is served by the pre-packaged archive.
    pub fn covers(&self, sensor: Sensor, year: i32) -> bool {
        archive_years(sensor).is_some_and(|years| years.contains(&year))
            && archive_slug(sensor).is_some()
    }

    /// Download (or read from cache) one country-year CSV and return its raw text.
    pub async fn fetch_country_year_csv(
        &self,
        sensor: Sensor,
        country: &str,
        year: i32,
    ) -> Result<String, ArchiveError> {
        fetch_country_year_text(&self.http, &self.cache_dir, sensor, country, year).await
    }

    /// Same as [`Self::fetch_country_year_csv`], parsed into hotspots.
    pub async fn fetch_country_year(
        &self,
        sensor: Sensor,
        country: &str,
        year: i32,
    ) -> Result<Vec<Hotspot>, ArchiveError> {
        let csv = self.fetch_country_year_csv(sensor, country, year).await?;
        let source = FirmsSource::new(sensor, Processing::Sp);
        Ok(crate::parse::parse_csv(&csv, source.as_str())?)
    }

    /// Fetch many country-years concurrently, at most `concurrency` in flight.
    pub async fn fetch_many(
        &self,
        sensor: Sensor,
        countries: &[String],
        years: RangeInclusive<i32>,
        concurrency: usize,
    ) -> (Vec<Hotspot>, Vec<(String, i32, String)>) {
        let semaphore = Arc::new(Semaphore::new(concurrency.max(1)));
        let years: Vec<i32> = years.collect();
        let mut handles = Vec::new();

        for country in countries {
            for &year in &years {
                let permit = semaphore
                    .clone()
                    .acquire_owned()
                    .await
                    .expect("semaphore is never closed");

                let client = self.http.clone();
                let cache_dir = self.cache_dir.clone();
                let task_country = country.clone();

                let handle = tokio::spawn(async move {
                    let _permit = permit;
                    fetch_and_parse(&client, &cache_dir, sensor, &task_country, year).await
                });

                handles.push((country.clone(), year, handle));
            }
        }

        let mut hotspots = Vec::new();
        let mut failures = Vec::new();

        for (country, year, handle) in handles {
            match handle.await {
                Ok(Ok(mut found)) => hotspots.append(&mut found),
                Ok(Err(ArchiveError::NotFound { .. })) => {
                    tracing::debug!(country = %country, year, "archive file not found");
                }
                Ok(Err(err)) => {
                    tracing::warn!(country = %country, year, error = %err, "archive fetch failed");
                    failures.push((country, year, err.to_string()));
                }
                Err(err) => {
                    tracing::warn!(country = %country, year, error = %err, "archive fetch task failed");
                    failures.push((country, year, err.to_string()));
                }
            }
        }

        (hotspots, failures)
    }
}

async fn fetch_and_parse(
    http: &reqwest::Client,
    cache_dir: &Path,
    sensor: Sensor,
    country: &str,
    year: i32,
) -> Result<Vec<Hotspot>, ArchiveError> {
    let csv = fetch_country_year_text(http, cache_dir, sensor, country, year).await?;
    let source = FirmsSource::new(sensor, Processing::Sp);
    Ok(crate::parse::parse_csv(&csv, source.as_str())?)
}

async fn fetch_country_year_text(
    http: &reqwest::Client,
    cache_dir: &Path,
    sensor: Sensor,
    country: &str,
    year: i32,
) -> Result<String, ArchiveError> {
    let Some(slug) = archive_slug(sensor) else {
        return Err(ArchiveError::UnsupportedYear { sensor, year });
    };
    let Some(years) = archive_years(sensor) else {
        return Err(ArchiveError::UnsupportedYear { sensor, year });
    };
    if !years.contains(&year) {
        return Err(ArchiveError::UnsupportedYear { sensor, year });
    }

    let path = cache_dir
        .join(slug)
        .join(year.to_string())
        .join(format!("{slug}_{year}_{country}.csv"));

    match tokio::fs::read_to_string(&path).await {
        Ok(csv) => return Ok(csv),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }

    let url = format!("{ARCHIVE_BASE}/{slug}/{year}/{slug}_{year}_{country}.csv");
    let response = http.get(&url).send().await?;
    let status = response.status();

    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(ArchiveError::NotFound {
            country: country.to_string(),
            year,
        });
    }
    if !status.is_success() {
        return Err(ArchiveError::Http {
            status: status.as_u16(),
            url,
        });
    }

    let body = response.text().await?;

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut part_name = path.file_name().unwrap_or_default().to_os_string();
    part_name.push(".part");
    let part = path.with_file_name(part_name);

    if let Err(err) = tokio::fs::write(&part, body.as_bytes()).await {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(err.into());
    }

    if let Err(err) = tokio::fs::rename(&part, &path).await {
        let _ = tokio::fs::remove_file(&part).await;
        // Windows refuses to rename over an existing file. If another task won
        // the race, its complete cache entry is just as good as ours.
        if tokio::fs::metadata(&path).await.is_err() {
            return Err(err.into());
        }
    }

    Ok(body)
}

/// Keep only points inside the inclusive bounding box. Archive files are whole-country,
/// so this trims them down to the caller's region of interest.
pub fn filter_bbox(hotspots: Vec<Hotspot>, bbox: &BBox) -> Vec<Hotspot> {
    hotspots
        .into_iter()
        .filter(|hotspot| {
            hotspot.lon >= bbox.west
                && hotspot.lon <= bbox.east
                && hotspot.lat >= bbox.south
                && hotspot.lat <= bbox.north
        })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("archive has no data for {sensor:?} {year}")]
    UnsupportedYear { sensor: Sensor, year: i32 },
    #[error("archive has no file for {country} {year}")]
    NotFound { country: String, year: i32 },
    #[error("http {status} fetching {url}")]
    Http { status: u16, url: String },
    #[error(transparent)]
    Transport(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Parse(#[from] anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyrosync_core::models::{Confidence, DayNight, FireType};

    fn test_client() -> ArchiveClient {
        ArchiveClient {
            http: reqwest::Client::new(),
            cache_dir: PathBuf::new(),
        }
    }

    fn hotspot(lat: f64, lon: f64) -> Hotspot {
        Hotspot {
            lat,
            lon,
            brightness: 300.0,
            frp: 1.0,
            confidence: Confidence::Nominal,
            sensor: Sensor::ModisTerra,
            acquired: chrono::Utc::now(),
            scan: 1.0,
            track: 1.0,
            satellite: "Terra".to_string(),
            daynight: DayNight::Day,
            vza: None,
            fire_type: FireType::VegetationFire,
        }
    }

    #[test]
    fn archive_years_match_packaged_sensors() {
        assert_eq!(archive_years(Sensor::ModisTerra), Some(2000..=2025));
        assert_eq!(archive_years(Sensor::ModisAqua), Some(2000..=2025));
        assert_eq!(archive_years(Sensor::ViirsSNPP), Some(2012..=2025));
        assert_eq!(archive_years(Sensor::ViirsNOAA20), None);
        assert_eq!(archive_years(Sensor::ViirsNOAA21), None);
    }

    #[test]
    fn archive_slugs_match_packaged_directories() {
        assert_eq!(archive_slug(Sensor::ModisTerra), Some("modis"));
        assert_eq!(archive_slug(Sensor::ModisAqua), Some("modis"));
        assert_eq!(archive_slug(Sensor::ViirsSNPP), Some("viirs-snpp"));
        assert_eq!(archive_slug(Sensor::ViirsNOAA20), None);
        assert_eq!(archive_slug(Sensor::ViirsNOAA21), None);
    }

    #[test]
    fn covers_enforces_modis_year_boundaries() {
        let client = test_client();

        assert!(!client.covers(Sensor::ModisTerra, 1999));
        assert!(client.covers(Sensor::ModisTerra, 2000));
        assert!(client.covers(Sensor::ModisAqua, 2025));
        assert!(!client.covers(Sensor::ModisAqua, 2026));
    }

    #[test]
    fn covers_enforces_snpp_year_boundaries() {
        let client = test_client();

        assert!(!client.covers(Sensor::ViirsSNPP, 2011));
        assert!(client.covers(Sensor::ViirsSNPP, 2012));
        assert!(client.covers(Sensor::ViirsSNPP, 2025));
        assert!(!client.covers(Sensor::ViirsSNPP, 2026));
    }

    #[test]
    fn covers_rejects_sensors_without_archive_files() {
        let client = test_client();

        assert!(!client.covers(Sensor::ViirsNOAA20, 2020));
        assert!(!client.covers(Sensor::ViirsNOAA21, 2023));
    }

    #[test]
    fn filter_bbox_keeps_boundary_points_and_drops_outside() {
        let bbox = BBox {
            west: -10.0,
            south: -5.0,
            east: 10.0,
            north: 5.0,
        };

        let points = vec![
            hotspot(0.0, -10.0),
            hotspot(0.0, 10.0),
            hotspot(-5.0, 0.0),
            hotspot(5.0, 0.0),
            hotspot(1.0, 1.0),
            hotspot(0.0, -10.0001),
            hotspot(0.0, 10.0001),
            hotspot(-5.0001, 0.0),
            hotspot(5.0001, 0.0),
        ];

        let kept = filter_bbox(points, &bbox);

        assert_eq!(kept.len(), 5);
        assert!(kept.iter().all(|hotspot| {
            hotspot.lon >= bbox.west
                && hotspot.lon <= bbox.east
                && hotspot.lat >= bbox.south
                && hotspot.lat <= bbox.north
        }));
    }
}
