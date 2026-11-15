use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::SystemTime;

use axum::body::Bytes;
use serde::{Deserialize, Serialize};

use pyrosync_core::config::PipelineConfig;

pub const DEFAULT_DERIVED_CACHE_DIR: &str = ".cache/pyrosync-derived";
pub const DEFAULT_BUNDLED_DERIVED_DIR: &str = "data/derived";

/// Bump manually whenever the `/api/series` response schema or the harmonization/aggregation
/// logic that produces it changes, so previously persisted cache entries are invalidated.
// v3: MonthEntry adds mean_frp, mean_frp_lower, and mean_frp_upper.
pub const DERIVED_SCHEMA_VERSION: u32 = 3;

const FNV1A_64_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV1A_64_PRIME: u64 = 0x00000100000001b3;

static NEXT_TMP_ID: AtomicU64 = AtomicU64::new(0);

fn dir_from_env_or(var: &str, default: &str) -> PathBuf {
    std::env::var(var)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

/// Resolves the derived cache directory from `DERIVED_CACHE_DIR`, falling back to the default.
pub fn dir_from_env() -> PathBuf {
    dir_from_env_or("DERIVED_CACHE_DIR", DEFAULT_DERIVED_CACHE_DIR)
}

/// Resolves the read-only bundled derived directory from `BUNDLED_DERIVED_DIR`, falling back to the default.
pub fn bundled_dir_from_env() -> PathBuf {
    dir_from_env_or("BUNDLED_DERIVED_DIR", DEFAULT_BUNDLED_DERIVED_DIR)
}

/// Computes a deterministic 64-bit FNV-1a hash of `bytes`.
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV1A_64_OFFSET_BASIS;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(FNV1A_64_PRIME);
    }
    hash
}

/// Deterministic digest of the effective [`PipelineConfig`].
pub fn config_digest(config: &PipelineConfig) -> String {
    // PipelineConfig consists solely of structs with fixed field order, so JSON encoding is deterministic.
    let bytes = serde_json::to_vec(config).expect("PipelineConfig serialization cannot fail");
    format!("{:016x}", fnv1a_64(&bytes))
}

/// Metadata identifying one source archive CSV belonging to a region.
///
/// File mtime is omitted so stamps stay machine-independent and precomputed results can be
/// distributed through the repository; `make verify-data` (SHA-256 manifest) guarantees identical
/// file contents, while `bytes` catches missing, re-downloaded, or partial files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveFileStamp {
    pub path: String,
    pub bytes: u64,
}

/// Validation stamp that must match exactly for a cached `/api/series` file to be reused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeriesStamp {
    pub schema_version: u32,
    pub config_hash: String,
    pub files: Vec<ArchiveFileStamp>,
}

impl SeriesStamp {
    /// Builds a stamp for a region given the active config and the region's archive files.
    pub fn for_region(config: &PipelineConfig, files: &[ArchiveFileStamp]) -> Self {
        let mut sorted = files.to_vec();
        sorted.sort_by(|a, b| a.path.cmp(&b.path));
        Self {
            schema_version: DERIVED_SCHEMA_VERSION,
            config_hash: config_digest(config),
            files: sorted,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SeriesMeta {
    schema_version: u32,
    config_hash: String,
    files: Vec<ArchiveFileStamp>,
    body_hash: String,
}

#[derive(Clone, PartialEq, Eq)]
struct FileFingerprint {
    len: u64,
    modified: SystemTime,
}

struct CachedSeriesValidity {
    meta: FileFingerprint,
    body: FileFingerprint,
    stamp: SeriesStamp,
    valid: bool,
}

fn file_fingerprint(path: &Path) -> io::Result<Option<FileFingerprint>> {
    let meta = fs::metadata(path)?;
    Ok(meta.modified().ok().map(|modified| FileFingerprint {
        len: meta.len(),
        modified,
    }))
}

/// Persistent on-disk cache for `/api/series` JSON responses.
pub struct DerivedCache {
    dir: PathBuf,
    read_only: bool,
    warned_unwritable: AtomicBool,
    warned_regions: Mutex<HashSet<String>>,
    validity_cache: Mutex<HashMap<String, CachedSeriesValidity>>,
}

impl DerivedCache {
    /// Creates a writable disk cache rooted at `dir`.
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            read_only: false,
            warned_unwritable: AtomicBool::new(false),
            warned_regions: Mutex::new(HashSet::new()),
            validity_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Creates a read-only disk cache rooted at `dir` (used for bundled repository outputs).
    pub fn read_only(dir: PathBuf) -> Self {
        Self {
            dir,
            read_only: true,
            warned_unwritable: AtomicBool::new(false),
            warned_regions: Mutex::new(HashSet::new()),
            validity_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Creates a writable disk cache using `DERIVED_CACHE_DIR` or its default path.
    pub fn from_env() -> Self {
        Self::new(dir_from_env())
    }

    /// Creates a read-only bundled cache using `BUNDLED_DERIVED_DIR` or its default path.
    pub fn bundled_from_env() -> Self {
        Self::read_only(bundled_dir_from_env())
    }

    fn should_warn(&self, region: &str) -> bool {
        if !self.read_only {
            return true;
        }
        self.warned_regions
            .lock()
            .expect("warned_regions lock")
            .insert(region.to_string())
    }

    fn series_paths(&self, region: &str) -> Option<(PathBuf, PathBuf, PathBuf)> {
        if region.is_empty()
            || region == "."
            || region == ".."
            || region.contains(['/', '\\'])
        {
            return None;
        }
        let series_dir = self.dir.join("series");
        let body_path = series_dir.join(format!("{region}.json"));
        let meta_path = series_dir.join(format!("{region}.meta.json"));
        Some((series_dir, body_path, meta_path))
    }

    fn warn_unwritable_once(&self, err: &io::Error) {
        if !self.warned_unwritable.swap(true, Ordering::Relaxed) {
            tracing::warn!(
                target: "pyrosync::derived_cache",
                dir = %self.dir.display(),
                error = %err,
                "derived cache directory is not writable; continuing without persisting derived cache"
            );
        }
    }

    fn read_matching_meta(
        &self,
        region: &str,
        expected: &SeriesStamp,
    ) -> Option<(PathBuf, SeriesMeta)> {
        let (_, body_path, meta_path) = self.series_paths(region)?;
        let meta_bytes = match fs::read(&meta_path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return None,
            Err(err) => {
                if self.should_warn(region) {
                    tracing::warn!(
                        target: "pyrosync::derived_cache",
                        region = %region,
                        path = %meta_path.display(),
                        error = %err,
                        "failed to read derived series metadata; ignoring cache"
                    );
                }
                return None;
            }
        };

        let meta: SeriesMeta = match serde_json::from_slice(&meta_bytes) {
            Ok(meta) => meta,
            Err(err) => {
                if self.should_warn(region) {
                    tracing::warn!(
                        target: "pyrosync::derived_cache",
                        region = %region,
                        path = %meta_path.display(),
                        error = %err,
                        "corrupt derived series metadata; ignoring cache"
                    );
                }
                return None;
            }
        };

        if meta.schema_version != expected.schema_version
            || meta.config_hash != expected.config_hash
            || meta.files != expected.files
        {
            if self.read_only && self.should_warn(region) {
                tracing::warn!(
                    target: "pyrosync::derived_cache",
                    region = %region,
                    path = %meta_path.display(),
                    found_schema = meta.schema_version,
                    expected_schema = expected.schema_version,
                    "bundled derived series stamp mismatch; ignoring cache"
                );
            }
            return None;
        }

        Some((body_path, meta))
    }

    /// Checks whether cached `/api/series` metadata and body exist, match `expected`, and pass
    /// the body checksum check, caching the result until either file's size or modified time changes.
    pub fn has_valid_series(&self, region: &str, expected: &SeriesStamp) -> bool {
        let Some((_, body_path, meta_path)) = self.series_paths(region) else {
            return false;
        };
        let (Ok(meta_fp), Ok(body_fp)) = (file_fingerprint(&meta_path), file_fingerprint(&body_path))
        else {
            self.validity_cache
                .lock()
                .expect("validity_cache lock")
                .remove(region);
            return false;
        };

        if let (Some(meta_fp), Some(body_fp)) = (meta_fp.as_ref(), body_fp.as_ref()) {
            let cache = self.validity_cache.lock().expect("validity_cache lock");
            if let Some(entry) = cache.get(region)
                && &entry.meta == meta_fp
                && &entry.body == body_fp
                && &entry.stamp == expected
            {
                return entry.valid;
            }
        }

        let valid = self.read_series(region, expected).is_some();

        if let (Some(meta_fp), Some(body_fp)) = (meta_fp, body_fp) {
            // Confirm files did not change concurrently during validation so we never cache a torn state.
            if file_fingerprint(&meta_path).ok().flatten().as_ref() == Some(&meta_fp)
                && file_fingerprint(&body_path).ok().flatten().as_ref() == Some(&body_fp)
            {
                self.validity_cache
                    .lock()
                    .expect("validity_cache lock")
                    .insert(
                        region.to_string(),
                        CachedSeriesValidity {
                            meta: meta_fp,
                            body: body_fp,
                            stamp: expected.clone(),
                            valid,
                        },
                    );
            }
        }

        valid
    }

    /// Reads cached `/api/series` bytes if the stamp and body checksum match `expected`.
    /// Read errors or corrupted files are logged, ignored, and treated as a cache miss.
    pub fn read_series(&self, region: &str, expected: &SeriesStamp) -> Option<Bytes> {
        let (body_path, meta) = self.read_matching_meta(region, expected)?;

        let body_bytes = match fs::read(&body_path) {
            Ok(bytes) => bytes,
            Err(err) => {
                if self.should_warn(region) {
                    tracing::warn!(
                        target: "pyrosync::derived_cache",
                        region = %region,
                        path = %body_path.display(),
                        error = %err,
                        "failed to read derived series body; ignoring cache"
                    );
                }
                return None;
            }
        };

        if format!("{:016x}", fnv1a_64(&body_bytes)) != meta.body_hash {
            if self.should_warn(region) {
                tracing::warn!(
                    target: "pyrosync::derived_cache",
                    region = %region,
                    path = %body_path.display(),
                    "corrupt derived series body (checksum mismatch); ignoring cache"
                );
            }
            return None;
        }

        Some(Bytes::from(body_bytes))
    }

    /// Atomically persists the `/api/series` JSON body and its stamp to disk.
    pub fn write_series(
        &self,
        region: &str,
        stamp: &SeriesStamp,
        body: &[u8],
    ) -> io::Result<()> {
        if self.read_only {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "bundled derived cache is read-only",
            ));
        }
        let Some((series_dir, body_path, meta_path)) = self.series_paths(region) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid region cache key '{region}'"),
            ));
        };

        if let Err(err) = fs::create_dir_all(&series_dir) {
            self.warn_unwritable_once(&err);
            return Err(err);
        }

        let meta = SeriesMeta {
            schema_version: stamp.schema_version,
            config_hash: stamp.config_hash.clone(),
            files: stamp.files.clone(),
            body_hash: format!("{:016x}", fnv1a_64(body)),
        };
        let meta_bytes = serde_json::to_vec_pretty(&meta)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

        let seq = NEXT_TMP_ID.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let tmp_body = series_dir.join(format!(".{region}.json.tmp.{pid}.{seq}"));
        let tmp_meta = series_dir.join(format!(".{region}.meta.json.tmp.{pid}.{seq}"));

        let write_result = (|| -> io::Result<()> {
            fs::write(&tmp_body, body)?;
            fs::write(&tmp_meta, &meta_bytes)?;
            // Persist the body before the stamp so readers never observe a new stamp with an old body.
            fs::rename(&tmp_body, &body_path)?;
            fs::rename(&tmp_meta, &meta_path)?;
            Ok(())
        })();

        if let Err(err) = write_result {
            let _ = fs::remove_file(&tmp_body);
            let _ = fs::remove_file(&tmp_meta);
            self.warn_unwritable_once(&err);
            return Err(err);
        }

        self.validity_cache
            .lock()
            .expect("validity_cache lock")
            .remove(region);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn has_valid_series_validates_body_and_caches_by_file_metadata() {
        let dir = std::env::temp_dir().join(format!(
            "pyrosync-derived-cache-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let cache = DerivedCache::new(dir.clone());
        let stamp = SeriesStamp::for_region(
            &PipelineConfig::default(),
            &[ArchiveFileStamp {
                path: "modis/2020/modis_2020_Test.csv".to_string(),
                bytes: 123,
            }],
        );
        let body = br#"{"region":"Test","years":[]}"#;
        cache.write_series("Test", &stamp, body).unwrap();

        assert!(cache.has_valid_series("Test", &stamp));

        let (_, body_path, _) = cache.series_paths("Test").unwrap();
        let orig_mtime = fs::metadata(&body_path).unwrap().modified().unwrap();

        // Corrupt 1 byte while keeping exact len + modified: memory cache must serve the cached result
        // without re-reading the body on every check.
        let mut corrupt = body.to_vec();
        corrupt[0] ^= 0x01;
        fs::write(&body_path, &corrupt).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&body_path)
            .unwrap()
            .set_modified(orig_mtime)
            .unwrap();
        assert!(cache.has_valid_series("Test", &stamp));

        // Bumping modified invalidates the memory cache, forcing body checksum verification.
        let next_mtime = orig_mtime + Duration::from_secs(5);
        fs::OpenOptions::new()
            .write(true)
            .open(&body_path)
            .unwrap()
            .set_modified(next_mtime)
            .unwrap();
        assert!(!cache.has_valid_series("Test", &stamp));

        // Restoring valid body via write_series invalidates the cached `false` entry.
        cache.write_series("Test", &stamp, body).unwrap();
        assert!(cache.has_valid_series("Test", &stamp));

        let _ = fs::remove_dir_all(&dir);
    }
}
