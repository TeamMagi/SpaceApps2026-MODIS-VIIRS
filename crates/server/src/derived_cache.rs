use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use axum::body::Bytes;
use serde::{Deserialize, Serialize};

use pyrosync_core::config::PipelineConfig;

pub const DEFAULT_DERIVED_CACHE_DIR: &str = ".cache/pyrosync-derived";

/// Bump manually whenever the `/api/series` response schema or the harmonization/aggregation
/// logic that produces it changes, so previously persisted cache entries are invalidated.
pub const DERIVED_SCHEMA_VERSION: u32 = 1;

const FNV1A_64_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV1A_64_PRIME: u64 = 0x00000100000001b3;

static NEXT_TMP_ID: AtomicU64 = AtomicU64::new(0);

/// Resolves the derived cache directory from `DERIVED_CACHE_DIR`, falling back to the default.
pub fn dir_from_env() -> PathBuf {
    std::env::var("DERIVED_CACHE_DIR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DERIVED_CACHE_DIR))
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveFileStamp {
    pub path: String,
    pub bytes: u64,
    pub mtime_secs: u64,
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

/// Persistent on-disk cache for `/api/series` JSON responses.
pub struct DerivedCache {
    dir: PathBuf,
    warned_unwritable: AtomicBool,
}

impl DerivedCache {
    /// Creates a disk cache rooted at `dir`.
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            warned_unwritable: AtomicBool::new(false),
        }
    }

    /// Creates a disk cache using `DERIVED_CACHE_DIR` or its default path.
    pub fn from_env() -> Self {
        Self::new(dir_from_env())
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

    /// Reads cached `/api/series` bytes if the stamp and body checksum match `expected`.
    /// Read errors or corrupted files are logged, ignored, and treated as a cache miss.
    pub fn read_series(&self, region: &str, expected: &SeriesStamp) -> Option<Bytes> {
        let (_, body_path, meta_path) = self.series_paths(region)?;
        let meta_bytes = match fs::read(&meta_path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return None,
            Err(err) => {
                tracing::warn!(
                    target: "pyrosync::derived_cache",
                    region = %region,
                    path = %meta_path.display(),
                    error = %err,
                    "failed to read derived series metadata; ignoring cache"
                );
                return None;
            }
        };

        let meta: SeriesMeta = match serde_json::from_slice(&meta_bytes) {
            Ok(meta) => meta,
            Err(err) => {
                tracing::warn!(
                    target: "pyrosync::derived_cache",
                    region = %region,
                    path = %meta_path.display(),
                    error = %err,
                    "corrupt derived series metadata; ignoring cache"
                );
                return None;
            }
        };

        if meta.schema_version != expected.schema_version
            || meta.config_hash != expected.config_hash
            || meta.files != expected.files
        {
            return None;
        }

        let body_bytes = match fs::read(&body_path) {
            Ok(bytes) => bytes,
            Err(err) => {
                tracing::warn!(
                    target: "pyrosync::derived_cache",
                    region = %region,
                    path = %body_path.display(),
                    error = %err,
                    "failed to read derived series body; ignoring cache"
                );
                return None;
            }
        };

        if format!("{:016x}", fnv1a_64(&body_bytes)) != meta.body_hash {
            tracing::warn!(
                target: "pyrosync::derived_cache",
                region = %region,
                path = %body_path.display(),
                "corrupt derived series body (checksum mismatch); ignoring cache"
            );
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

        Ok(())
    }
}
