use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use chrono::NaiveDate;

use crate::availability::Availability;
use crate::day_range::DayRange;
use crate::source::{FirmsSource, Processing};

const AVAILABILITY_FILE: &str = "availability.csv";

static PART_COUNTER: AtomicU64 = AtomicU64::new(0);

/// On-disk cache of validated Area API window CSVs. Every real request costs FIRMS
/// transactions, so a window that was already bought is never bought twice.
#[derive(Debug, Clone)]
pub struct AreaCache {
    dir: PathBuf,
    nrt_ttl: Duration,
}

impl AreaCache {
    pub fn new(dir: PathBuf, nrt_ttl: Duration) -> Self {
        Self { dir, nrt_ttl }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// An empty string is a valid hit: a window in which FIRMS reported no fires.
    pub async fn load(
        &self,
        source: FirmsSource,
        bbox_str: &str,
        start: NaiveDate,
        days: DayRange,
    ) -> Option<String> {
        // SP data is frozen; NRT keeps being revised, so it only lives for the TTL.
        if source.processing == Processing::Nrt && self.nrt_ttl.is_zero() {
            return None;
        }
        let path = self.window_path(source, bbox_str, start, days);

        if source.processing == Processing::Nrt {
            let modified = tokio::fs::metadata(&path).await.ok()?.modified().ok()?;
            let age = SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::ZERO);
            if age >= self.nrt_ttl {
                return None;
            }
        }

        tokio::fs::read_to_string(&path).await.ok()
    }

    pub async fn store(
        &self,
        source: FirmsSource,
        bbox_str: &str,
        start: NaiveDate,
        days: DayRange,
        csv: &str,
    ) {
        let path = self.window_path(source, bbox_str, start, days);
        write_atomic(&path, csv).await;
    }

    pub async fn store_availability(&self, csv: &str) {
        write_atomic(&self.dir.join(AVAILABILITY_FILE), csv).await;
    }

    /// `None` when the file is missing or no longer parses.
    pub async fn load_availability(&self) -> Option<Availability> {
        let text = tokio::fs::read_to_string(self.dir.join(AVAILABILITY_FILE))
            .await
            .ok()?;
        Availability::from_csv(&text).ok()
    }

    fn window_path(
        &self,
        source: FirmsSource,
        bbox_str: &str,
        start: NaiveDate,
        days: DayRange,
    ) -> PathBuf {
        self.dir.join(source.as_str()).join(format!(
            "{}_{}d_{}.csv",
            start.format("%Y-%m-%d"),
            days.get(),
            sanitize(bbox_str)
        ))
    }
}

fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Temp file + rename so a reader never sees a partial file. Failures are only logged:
/// the cache is an optimisation and must never fail a fetch that already succeeded.
async fn write_atomic(path: &Path, content: &str) {
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(err) = tokio::fs::create_dir_all(parent).await {
        tracing::warn!(path = %path.display(), error = %err, "area cache: cannot create directory");
        return;
    }

    let mut part_name = path.file_name().unwrap_or_default().to_os_string();
    part_name.push(format!(
        ".{}.{}.part",
        std::process::id(),
        PART_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let part = path.with_file_name(part_name);

    if let Err(err) = tokio::fs::write(&part, content.as_bytes()).await {
        tracing::warn!(path = %part.display(), error = %err, "area cache: write failed");
        let _ = tokio::fs::remove_file(&part).await;
        return;
    }

    if let Err(err) = tokio::fs::rename(&part, path).await {
        let _ = tokio::fs::remove_file(&part).await;
        // Another task may have won the race; its complete entry is just as good.
        if tokio::fs::metadata(path).await.is_err() {
            tracing::warn!(path = %path.display(), error = %err, "area cache: rename failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyrosync_core::models::Sensor;

    const BBOX: &str = "-10,10,10,20";
    const CSV: &str = "latitude,longitude\n1.0,2.0\n";

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn sp() -> FirmsSource {
        FirmsSource::new(Sensor::ModisTerra, Processing::Sp)
    }

    fn nrt() -> FirmsSource {
        FirmsSource::new(Sensor::ModisTerra, Processing::Nrt)
    }

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "pyrosync-area-test-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn cache(dir: &Path, ttl_secs: u64) -> AreaCache {
        AreaCache::new(dir.to_path_buf(), Duration::from_secs(ttl_secs))
    }

    #[tokio::test]
    async fn store_then_load_round_trips() {
        let dir = temp_dir("rt");
        let c = cache(&dir, 600);
        assert_eq!(c.load(sp(), BBOX, d(1), DayRange::clamped(5)).await, None);
        c.store(sp(), BBOX, d(1), DayRange::clamped(5), CSV).await;
        assert_eq!(
            c.load(sp(), BBOX, d(1), DayRange::clamped(5)).await.as_deref(),
            Some(CSV)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn empty_csv_round_trips_as_hit() {
        let dir = temp_dir("empty");
        let c = cache(&dir, 600);
        c.store(sp(), BBOX, d(1), DayRange::clamped(1), "").await;
        assert_eq!(
            c.load(sp(), BBOX, d(1), DayRange::clamped(1)).await,
            Some(String::new())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn different_keys_do_not_collide() {
        let dir = temp_dir("keys");
        let c = cache(&dir, 600);
        c.store(sp(), BBOX, d(1), DayRange::clamped(5), CSV).await;
        assert!(c.load(sp(), "-10,10,10,21", d(1), DayRange::clamped(5)).await.is_none());
        assert!(c.load(sp(), BBOX, d(2), DayRange::clamped(5)).await.is_none());
        assert!(c.load(sp(), BBOX, d(1), DayRange::clamped(4)).await.is_none());
        let other = FirmsSource::new(Sensor::ViirsSNPP, Processing::Sp);
        assert!(c.load(other, BBOX, d(1), DayRange::clamped(5)).await.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn sp_is_a_hit_with_zero_ttl_but_nrt_is_not() {
        let dir = temp_dir("ttl0");
        let c = cache(&dir, 0);
        let days = DayRange::clamped(2);
        c.store(sp(), BBOX, d(1), days, CSV).await;
        c.store(nrt(), BBOX, d(1), days, CSV).await;
        assert!(c.load(sp(), BBOX, d(1), days).await.is_some());
        assert!(c.load(nrt(), BBOX, d(1), days).await.is_none());

        let long = cache(&dir, 3600);
        assert!(long.load(nrt(), BBOX, d(1), days).await.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn store_into_impossible_path_does_not_panic() {
        let dir = temp_dir("bad");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("plain-file");
        std::fs::write(&file, "x").unwrap();
        let c = AreaCache::new(file.join("sub"), Duration::from_secs(600));
        c.store(sp(), BBOX, d(1), DayRange::clamped(1), CSV).await;
        c.store_availability("x").await;
        assert!(c.load(sp(), BBOX, d(1), DayRange::clamped(1)).await.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_name_is_free_of_commas_and_separators() {
        let c = cache(Path::new("cache"), 1);
        let path = c.window_path(sp(), "-10.5,10,10/\\:20", d(1), DayRange::clamped(3));
        let name = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "2024-01-01_3d_-10.5_10_10___20.csv");
        assert!(!name.contains([',', '/', '\\', ':']));
    }

    #[tokio::test]
    async fn availability_persists_and_reloads() {
        let dir = temp_dir("avail");
        let c = cache(&dir, 600);
        assert!(c.load_availability().await.is_none());
        let csv = "data_id,min_date,max_date\nMODIS_SP,2000-11-01,2026-03-31\n";
        c.store_availability(csv).await;
        let avail = c.load_availability().await.unwrap();
        assert!(avail.range_for(sp()).is_some());

        c.store_availability("Invalid MAP_KEY").await;
        assert!(c.load_availability().await.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn set_age(path: &Path, age: Duration) {
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
    }

    fn part_files(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
                    .count()
            })
            .unwrap_or(0)
    }

    #[tokio::test]
    async fn nrt_entry_walks_miss_fresh_expired_refreshed() {
        // State transition 0-switch: Absent -store-> Fresh -time-> Expired -store-> Fresh.
        let dir = temp_dir("nrt-lifecycle");
        let c = cache(&dir, 3600);
        let days = DayRange::clamped(2);
        let path = c.window_path(nrt(), BBOX, d(1), days);

        assert_eq!(c.load(nrt(), BBOX, d(1), days).await, None);
        c.store(nrt(), BBOX, d(1), days, "old\n").await;
        assert_eq!(c.load(nrt(), BBOX, d(1), days).await.as_deref(), Some("old\n"));

        set_age(&path, Duration::from_secs(7200));
        assert_eq!(c.load(nrt(), BBOX, d(1), days).await, None);

        c.store(nrt(), BBOX, d(1), days, "new\n").await;
        assert_eq!(c.load(nrt(), BBOX, d(1), days).await.as_deref(), Some("new\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn nrt_ttl_boundary_just_inside_and_just_outside() {
        let dir = temp_dir("nrt-ttl-bva");
        let c = cache(&dir, 100);
        let days = DayRange::clamped(1);
        let path = c.window_path(nrt(), BBOX, d(1), days);
        c.store(nrt(), BBOX, d(1), days, CSV).await;

        set_age(&path, Duration::from_secs(60));
        assert!(c.load(nrt(), BBOX, d(1), days).await.is_some());
        set_age(&path, Duration::from_secs(101));
        assert!(c.load(nrt(), BBOX, d(1), days).await.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn sp_entry_never_expires_however_old_the_file_is() {
        let dir = temp_dir("sp-forever");
        let c = cache(&dir, 1);
        let days = DayRange::clamped(5);
        c.store(sp(), BBOX, d(1), days, CSV).await;
        set_age(&c.window_path(sp(), BBOX, d(1), days), Duration::from_secs(10 * 365 * 86_400));
        assert_eq!(c.load(sp(), BBOX, d(1), days).await.as_deref(), Some(CSV));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn storing_again_overwrites_the_previous_entry() {
        let dir = temp_dir("overwrite");
        let c = cache(&dir, 600);
        let days = DayRange::clamped(1);
        c.store(sp(), BBOX, d(1), days, "first").await;
        c.store(sp(), BBOX, d(1), days, "second").await;
        assert_eq!(c.load(sp(), BBOX, d(1), days).await.as_deref(), Some("second"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn unreadable_entry_is_a_miss_not_a_panic() {
        // Invalid state: a directory occupies the entry path.
        let dir = temp_dir("dir-as-entry");
        let c = cache(&dir, 600);
        let days = DayRange::clamped(1);
        std::fs::create_dir_all(c.window_path(sp(), BBOX, d(1), days)).unwrap();
        assert_eq!(c.load(sp(), BBOX, d(1), days).await, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn stores_leave_no_part_files_even_when_racing() {
        let dir = temp_dir("atomic");
        let c = std::sync::Arc::new(cache(&dir, 600));
        let days = DayRange::clamped(1);
        let mut tasks = Vec::new();
        for i in 0..8 {
            let c = std::sync::Arc::clone(&c);
            tasks.push(tokio::spawn(async move {
                c.store(sp(), BBOX, d(1), days, &format!("writer-{i}\n")).await;
            }));
        }
        for t in tasks {
            t.await.unwrap();
        }
        let hit = c.load(sp(), BBOX, d(1), days).await.unwrap();
        assert!(hit.starts_with("writer-") && hit.ends_with('\n'));
        assert_eq!(part_files(&dir.join(sp().as_str())), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn availability_walks_absent_valid_corrupt_recovered() {
        let dir = temp_dir("avail-lifecycle");
        let c = cache(&dir, 600);
        let good = "data_id,min_date,max_date\nMODIS_SP,2000-11-01,2026-03-31\n";

        assert!(c.load_availability().await.is_none());
        c.store_availability(good).await;
        assert!(c.load_availability().await.is_some());
        c.store_availability("<html>oops</html>").await;
        assert!(c.load_availability().await.is_none());
        c.store_availability(good).await;
        assert!(c.load_availability().await.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dir_accessor_and_window_path_layout() {
        let c = cache(Path::new("cache-root"), 1);
        assert_eq!(c.dir(), Path::new("cache-root"));
        let path = c.window_path(nrt(), BBOX, d(9), DayRange::clamped(1));
        assert!(path.starts_with(Path::new("cache-root").join("MODIS_NRT")));
    }
}
