use std::collections::BTreeSet;
use std::io::{BufReader, BufWriter, Write};
use std::ops::RangeInclusive;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use pyrosync_firms::archive::ARCHIVE_BASE;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tracing_subscriber::EnvFilter;
use zip::ZipArchive;

const DEFAULT_CACHE_DIR: &str = ".cache/firms-archive";
const DEFAULT_SET_FILE: &str = "data/archive-set.json";
const DEFAULT_CONCURRENCY: usize = 2;
const USER_AGENT: &str = "pyrosync/0.1 (+https://github.com/TeamMagi)";
const ESTIMATED_EXPANSION_RATIO: f64 = 4.5;

// Country archive zips are unauthenticated static files and spend zero FIRMS API
// transactions, so the seed binary intentionally never reads FIRMS_API_KEY.

#[derive(Debug, Deserialize)]
struct ArchiveSet {
    scope: String,
    sensors: Vec<SensorSpec>,
}

#[derive(Debug, Deserialize)]
struct SensorSpec {
    slug: String,
    from: i32,
    to: i32,
}

#[derive(Debug, PartialEq, Eq)]
struct CliOptions {
    force: bool,
    concurrency: usize,
    help: bool,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            force: false,
            concurrency: DEFAULT_CONCURRENCY,
            help: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeedTarget {
    slug: String,
    year: i32,
}

#[derive(Debug)]
enum SensorYearOutcome {
    Skipped,
    Downloaded {
        zip_bytes: u64,
        extracted_files: usize,
    },
    Failed {
        label: String,
        error: String,
    },
}

struct SensorArchiveMeta {
    years: RangeInclusive<i32>,
    avg_zip_bytes: u64,
}

fn sensor_archive_meta(slug: &str) -> Option<SensorArchiveMeta> {
    match slug {
        "modis" => Some(SensorArchiveMeta {
            years: 2000..=2025,
            avg_zip_bytes: 1_820_000_000 / 26,
        }),
        "viirs-snpp" => Some(SensorArchiveMeta {
            years: 2012..=2025,
            avg_zip_bytes: 4_660_000_000 / 14,
        }),
        // viirs-jpss1 is NOAA-20 (pre-launch name JPSS-1). viirs-jpss2 (NOAA-21)
        // has no all_countries zip archives on FIRMS for any year.
        "viirs-jpss1" => Some(SensorArchiveMeta {
            years: 2018..=2025,
            avg_zip_bytes: 2_600_000_000 / 8,
        }),
        _ => None,
    }
}

fn print_usage() {
    println!(
        "\
Usage: archive_seed [OPTIONS]

Download and extract FIRMS all_countries yearly zip archives without using an API key.

Options:
  --force              Re-download and extract sensor-years even if marked complete
  --concurrency <N>    Max concurrent zip downloads (default: {DEFAULT_CONCURRENCY})
  -h, --help           Print this help message

Environment:
  ARCHIVE_CACHE_DIR    Target cache directory (default: {DEFAULT_CACHE_DIR})
  ARCHIVE_SET_FILE     Canonical archive set JSON path (default: {DEFAULT_SET_FILE})"
    );
}

fn parse_args_from<I>(args: I) -> Result<CliOptions>
where
    I: IntoIterator<Item = String>,
{
    let mut opts = CliOptions::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                opts.help = true;
                return Ok(opts);
            }
            "--force" => {
                opts.force = true;
            }
            "--concurrency" => {
                let raw = iter
                    .next()
                    .context("missing value for --concurrency")?;
                let parsed: usize = raw
                    .parse()
                    .with_context(|| format!("invalid --concurrency value: {raw}"))?;
                if parsed == 0 {
                    bail!("--concurrency must be >= 1");
                }
                opts.concurrency = parsed;
            }
            other => bail!("unknown argument: {other} (try --help)"),
        }
    }

    Ok(opts)
}

fn env_path_or(name: &str, default: &str) -> PathBuf {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

fn load_archive_set(path: &Path) -> Result<ArchiveSet> {
    let raw = std::fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read archive set definition at '{}'",
            path.display()
        )
    })?;
    let set: ArchiveSet = serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse archive set JSON at '{}'",
            path.display()
        )
    })?;
    if set.scope != "all_countries" {
        bail!(
            "unsupported archive set scope '{}' in '{}' (expected 'all_countries')",
            set.scope,
            path.display()
        );
    }
    Ok(set)
}

fn build_targets(set: &ArchiveSet) -> Vec<SeedTarget> {
    let mut targets = Vec::new();
    let mut seen_targets = BTreeSet::new();

    for spec in &set.sensors {
        let Some(meta) = sensor_archive_meta(&spec.slug) else {
            tracing::warn!(
                target: "pyrosync::archive_seed",
                slug = %spec.slug,
                "unsupported archive sensor slug in set definition; skipping"
            );
            continue;
        };

        if spec.from > spec.to {
            tracing::warn!(
                target: "pyrosync::archive_seed",
                slug = %spec.slug,
                from = spec.from,
                to = spec.to,
                "invalid sensor year range (from > to); skipping"
            );
            continue;
        }

        for year in spec.from..=spec.to {
            if !meta.years.contains(&year) {
                tracing::warn!(
                    target: "pyrosync::archive_seed",
                    slug = %spec.slug,
                    year,
                    covered_from = *meta.years.start(),
                    covered_to = *meta.years.end(),
                    "year outside archive coverage; skipping"
                );
                continue;
            }

            if seen_targets.insert((spec.slug.clone(), year)) {
                targets.push(SeedTarget {
                    slug: spec.slug.clone(),
                    year,
                });
            }
        }
    }

    targets
}

fn complete_marker_path(cache_dir: &Path, slug: &str, year: i32) -> PathBuf {
    cache_dir.join(slug).join(year.to_string()).join(".complete")
}

fn temp_zip_path(cache_dir: &Path, slug: &str, year: i32) -> PathBuf {
    cache_dir
        .join(".download")
        .join(format!("{slug}_{year}_all_countries.zip.part"))
}

fn zip_url(slug: &str, year: i32) -> String {
    format!("{ARCHIVE_BASE}/zips/{slug}_{year}_all_countries.zip")
}

fn target_label(slug: &str, year: i32) -> String {
    format!("{slug}/{year}")
}

fn needs_download(cache_dir: &Path, target: &SeedTarget, force: bool) -> Result<bool> {
    if force {
        return Ok(true);
    }
    let marker = complete_marker_path(cache_dir, &target.slug, target.year);
    match std::fs::metadata(&marker) {
        Ok(meta) => Ok(!meta.is_file()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(err) => Err(err).with_context(|| {
            format!("failed to inspect completion marker '{}'", marker.display())
        }),
    }
}

fn estimate_download_bytes(cache_dir: &Path, targets: &[SeedTarget], force: bool) -> u64 {
    targets
        .iter()
        .filter(|target| needs_download(cache_dir, target, force).unwrap_or(true))
        .filter_map(|target| sensor_archive_meta(&target.slug).map(|m| m.avg_zip_bytes))
        .sum()
}

/// Validate a zip entry name against zip-slip traversal and enforce that every
/// entry stays strictly under `{slug}/{year}/` within the cache directory.
fn validate_zip_entry_path(raw_name: &str, slug: &str, year: i32) -> Result<PathBuf> {
    if raw_name.is_empty() {
        bail!("zip entry has an empty path");
    }
    if raw_name.contains('\0') || raw_name.contains('\\') {
        bail!("unsafe character in zip entry path '{raw_name}'");
    }
    if raw_name.starts_with('/') {
        bail!("absolute zip entry path rejected: '{raw_name}'");
    }

    let expected_prefix = format!("{slug}/{year}/");
    if !raw_name.starts_with(&expected_prefix) {
        bail!("zip entry '{raw_name}' is outside expected prefix '{expected_prefix}'");
    }

    let trimmed = raw_name.strip_suffix('/').unwrap_or(raw_name);
    let mut rel = PathBuf::new();
    for seg in trimmed.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." || seg.contains(':') {
            bail!("unsafe path component in zip entry '{raw_name}'");
        }
        rel.push(seg);
    }

    if rel
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("non-normal path component in zip entry '{raw_name}'");
    }

    Ok(rel)
}

fn atomic_rename(part_path: &Path, dest_path: &Path) -> Result<()> {
    // Windows refuses to rename over an existing file, so remove the target first.
    if dest_path.try_exists()? {
        std::fs::remove_file(dest_path)?;
    }
    std::fs::rename(part_path, dest_path)?;
    Ok(())
}

fn cleanup_partial_extraction(cache_dir: &Path, slug: &str, year: i32, extracted: &[PathBuf]) {
    for path in extracted {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::remove_file(complete_marker_path(cache_dir, slug, year));
    // Remove the year directory only if it is now empty.
    let _ = std::fs::remove_dir(cache_dir.join(slug).join(year.to_string()));
}

fn extract_zip_archive(
    zip_path: &Path,
    cache_dir: &Path,
    slug: &str,
    year: i32,
) -> Result<usize> {
    let file = std::fs::File::open(zip_path)
        .with_context(|| format!("failed to open zip file '{}'", zip_path.display()))?;
    let mut archive = ZipArchive::new(BufReader::new(file))
        .with_context(|| format!("failed to read zip archive '{}'", zip_path.display()))?;

    let mut extracted_files: Vec<PathBuf> = Vec::new();
    let result = (|| -> Result<usize> {
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .with_context(|| format!("failed to read zip entry #{index}"))?;
            let raw_name = entry
                .name()
                .with_context(|| format!("invalid UTF-8 name in zip entry #{index}"))?
                .into_owned();
            let rel_path = validate_zip_entry_path(&raw_name, slug, year)?;
            let dest_path = cache_dir.join(&rel_path);

            if entry.is_dir() || raw_name.ends_with('/') {
                std::fs::create_dir_all(&dest_path)?;
                continue;
            }

            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let file_name = dest_path
                .file_name()
                .context("zip file entry missing filename")?;
            let mut part_name = file_name.to_os_string();
            part_name.push(".part");
            let part_path = dest_path.with_file_name(part_name);

            let write_res = (|| -> Result<()> {
                let out = std::fs::File::create(&part_path)?;
                let mut writer = BufWriter::new(out);
                std::io::copy(&mut entry, &mut writer)?;
                writer.flush()?;
                drop(writer);
                atomic_rename(&part_path, &dest_path)?;
                Ok(())
            })();

            if let Err(err) = write_res {
                let _ = std::fs::remove_file(&part_path);
                return Err(err);
            }

            extracted_files.push(dest_path);
        }

        if extracted_files.is_empty() {
            bail!("zip archive for {slug}/{year} contained no files");
        }

        Ok(extracted_files.len())
    })();

    if result.is_err() {
        cleanup_partial_extraction(cache_dir, slug, year, &extracted_files);
    }

    result
}

fn write_complete_marker(cache_dir: &Path, slug: &str, year: i32) -> Result<()> {
    let marker = complete_marker_path(cache_dir, slug, year);
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let part_marker = marker.with_file_name(".complete.part");
    if let Err(err) = std::fs::write(&part_marker, b"") {
        let _ = std::fs::remove_file(&part_marker);
        return Err(err.into());
    }
    if let Err(err) = atomic_rename(&part_marker, &marker) {
        let _ = std::fs::remove_file(&part_marker);
        return Err(err);
    }
    Ok(())
}

fn extract_and_mark_complete(
    zip_path: &Path,
    cache_dir: &Path,
    slug: &str,
    year: i32,
) -> Result<usize> {
    let extracted_count = extract_zip_archive(zip_path, cache_dir, slug, year)?;
    // Write .complete only after all ~220 entries succeed so interrupted runs
    // never mistake a half-extracted directory for a complete cache entry.
    write_complete_marker(cache_dir, slug, year)?;
    // Delete the temporary zip after extraction to avoid keeping ~9 GB of duplicate archives on disk.
    let _ = std::fs::remove_file(zip_path);
    Ok(extracted_count)
}

async fn download_zip_to_disk(
    http: &reqwest::Client,
    url: &str,
    part_zip_path: &Path,
) -> Result<u64> {
    if let Some(parent) = part_zip_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut resp = http
        .get(url)
        .send()
        .await
        .with_context(|| format!("request failed for {url}"))?;

    let status = resp.status();
    if status != reqwest::StatusCode::OK {
        bail!("HTTP {status} fetching {url}");
    }

    let expected_len = resp.content_length();
    let file = tokio::fs::File::create(part_zip_path)
        .await
        .with_context(|| format!("failed to create temp zip '{}'", part_zip_path.display()))?;
    let mut writer = tokio::io::BufWriter::new(file);
    let mut downloaded_bytes: u64 = 0;

    // Stream chunks straight to disk instead of buffering up to 370 MB per archive in RAM.
    while let Some(chunk) = resp.chunk().await? {
        writer.write_all(&chunk).await?;
        downloaded_bytes += chunk.len() as u64;
    }
    writer.flush().await?;
    writer.shutdown().await?;

    if downloaded_bytes == 0 {
        bail!("downloaded empty zip (0 bytes) from {url}");
    }
    if let Some(expected) = expected_len
        && downloaded_bytes != expected
    {
        bail!(
            "truncated download from {url}: got {downloaded_bytes} bytes, expected {expected}"
        );
    }

    Ok(downloaded_bytes)
}

async fn download_and_extract(
    http: &reqwest::Client,
    cache_dir: &Path,
    target: &SeedTarget,
    force: bool,
) -> Result<(u64, usize)> {
    let url = zip_url(&target.slug, target.year);
    let part_zip = temp_zip_path(cache_dir, &target.slug, target.year);

    let zip_bytes = match download_zip_to_disk(http, &url, &part_zip).await {
        Ok(bytes) => bytes,
        Err(err) => {
            let _ = tokio::fs::remove_file(&part_zip).await;
            return Err(err);
        }
    };

    if force {
        let marker = complete_marker_path(cache_dir, &target.slug, target.year);
        if let Err(err) = tokio::fs::remove_file(&marker).await
            && err.kind() != std::io::ErrorKind::NotFound
        {
            let _ = tokio::fs::remove_file(&part_zip).await;
            return Err(err).with_context(|| {
                format!("failed to remove completion marker '{}'", marker.display())
            });
        }
    }

    let zip_path_owned = part_zip.clone();
    let cache_dir_owned = cache_dir.to_path_buf();
    let slug_owned = target.slug.clone();
    let year = target.year;

    // Zip decompression is CPU-bound and synchronous; offload so the tokio runtime stays responsive.
    let extract_res = tokio::task::spawn_blocking(move || {
        extract_and_mark_complete(&zip_path_owned, &cache_dir_owned, &slug_owned, year)
    })
    .await
    .map_err(|err| anyhow::anyhow!("extraction task join error: {err}"))?;

    match extract_res {
        Ok(extracted_files) => Ok((zip_bytes, extracted_files)),
        Err(err) => {
            let _ = tokio::fs::remove_file(&part_zip).await;
            Err(err)
        }
    }
}

async fn process_target(
    http: Arc<reqwest::Client>,
    cache_dir: PathBuf,
    semaphore: Arc<Semaphore>,
    target: SeedTarget,
    force: bool,
) -> SensorYearOutcome {
    let label = target_label(&target.slug, target.year);

    match needs_download(&cache_dir, &target, force) {
        Ok(false) => {
            tracing::info!(
                target: "pyrosync::archive_seed",
                sensor_year = %label,
                "skipped (already complete)"
            );
            return SensorYearOutcome::Skipped;
        }
        Ok(true) => {}
        Err(err) => {
            tracing::warn!(
                target: "pyrosync::archive_seed",
                sensor_year = %label,
                error = %err,
                "failed to inspect completion marker"
            );
            return SensorYearOutcome::Failed {
                label,
                error: err.to_string(),
            };
        }
    }

    let Ok(_permit) = semaphore.acquire().await else {
        return SensorYearOutcome::Failed {
            label,
            error: "download semaphore closed unexpectedly".to_string(),
        };
    };

    match download_and_extract(&http, &cache_dir, &target, force).await {
        Ok((zip_bytes, extracted_files)) => {
            tracing::info!(
                target: "pyrosync::archive_seed",
                sensor_year = %label,
                zip_bytes,
                extracted_files,
                "downloaded and extracted"
            );
            SensorYearOutcome::Downloaded {
                zip_bytes,
                extracted_files,
            }
        }
        Err(err) => {
            tracing::warn!(
                target: "pyrosync::archive_seed",
                sensor_year = %label,
                error = %err,
                "sensor-year failed"
            );
            SensorYearOutcome::Failed {
                label,
                error: err.to_string(),
            }
        }
    }
}

async fn run(opts: CliOptions) -> Result<ExitCode> {
    if opts.help {
        print_usage();
        return Ok(ExitCode::SUCCESS);
    }

    let set_path = env_path_or("ARCHIVE_SET_FILE", DEFAULT_SET_FILE);
    let cache_dir = env_path_or("ARCHIVE_CACHE_DIR", DEFAULT_CACHE_DIR);

    let set = load_archive_set(&set_path)?;
    std::fs::create_dir_all(&cache_dir).with_context(|| {
        format!(
            "failed to initialize archive cache directory '{}'",
            cache_dir.display()
        )
    })?;

    let http = Arc::new(
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(120))
            .user_agent(USER_AGENT)
            .build()
            .context("failed to build HTTP client")?,
    );

    let targets = build_targets(&set);
    if targets.is_empty() {
        bail!(
            "no valid archive targets found in '{}'",
            set_path.display()
        );
    }

    let expected = targets.len();
    let est_zip_bytes = estimate_download_bytes(&cache_dir, &targets, opts.force);
    let est_zip_gb = est_zip_bytes as f64 / 1_000_000_000.0;
    let est_unpacked_gb = est_zip_gb * ESTIMATED_EXPANSION_RATIO;

    tracing::info!(
        target: "pyrosync::archive_seed",
        set_file = %set_path.display(),
        cache_dir = %cache_dir.display(),
        expected,
        concurrency = opts.concurrency,
        force = opts.force,
        "starting archive seed"
    );

    if est_zip_bytes > 0 {
        tracing::warn!(
            target: "pyrosync::archive_seed",
            estimated_zip_gb = format_args!("{est_zip_gb:.2}"),
            estimated_unpacked_gb = format_args!("{est_unpacked_gb:.1}"),
            "estimated disk space required for pending sensor-years: ~{est_unpacked_gb:.1} GB (~{est_zip_gb:.2} GB zip downloads at ~4.5x expansion)"
        );
    }

    let semaphore = Arc::new(Semaphore::new(opts.concurrency));
    let mut handles = Vec::with_capacity(expected);

    for target in targets {
        let fallback_label = target_label(&target.slug, target.year);
        let http = Arc::clone(&http);
        let cache_dir = cache_dir.clone();
        let semaphore = Arc::clone(&semaphore);
        let force = opts.force;

        let handle = tokio::spawn(async move {
            process_target(http, cache_dir, semaphore, target, force).await
        });
        handles.push((fallback_label, handle));
    }

    let mut skipped = 0usize;
    let mut downloaded = 0usize;
    let mut total_extracted_files = 0usize;
    let mut total_zip_bytes = 0u64;
    let mut failures: Vec<(String, String)> = Vec::new();

    // Collect every outcome rather than aborting on the first failure so completed
    // sensor-years stay cached and the next run retries only what failed.
    for (fallback_label, handle) in handles {
        match handle.await {
            Ok(SensorYearOutcome::Skipped) => skipped += 1,
            Ok(SensorYearOutcome::Downloaded {
                zip_bytes,
                extracted_files,
            }) => {
                downloaded += 1;
                total_zip_bytes += zip_bytes;
                total_extracted_files += extracted_files;
            }
            Ok(SensorYearOutcome::Failed { label, error }) => failures.push((label, error)),
            Err(err) => failures.push((fallback_label, format!("task join error: {err}"))),
        }
    }

    let present = skipped + downloaded;
    tracing::info!(
        target: "pyrosync::archive_seed",
        expected,
        skipped,
        downloaded,
        failed = failures.len(),
        extracted_files = total_extracted_files,
        downloaded_zip_bytes = total_zip_bytes,
        "archive seed summary"
    );

    if failures.is_empty() {
        tracing::info!(
            target: "pyrosync::archive_seed",
            "set complete: {present}/{expected} sensor-years"
        );
        Ok(ExitCode::SUCCESS)
    } else {
        tracing::error!(
            target: "pyrosync::archive_seed",
            failed = failures.len(),
            "archive seed finished with failures; re-run to retry missing sensor-years"
        );
        for (sensor_year, err) in &failures {
            tracing::error!(
                target: "pyrosync::archive_seed",
                sensor_year = %sensor_year,
                error = %err,
                "failed sensor-year"
            );
        }
        Ok(ExitCode::FAILURE)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let opts = match parse_args_from(std::env::args().skip(1)) {
        Ok(opts) => opts,
        Err(err) => {
            tracing::error!(target: "pyrosync::archive_seed", error = %err, "invalid arguments");
            return ExitCode::FAILURE;
        }
    };

    match run(opts).await {
        Ok(code) => code,
        Err(err) => {
            tracing::error!(target: "pyrosync::archive_seed", error = %err, "archive seed error");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "pyrosync_archive_seed_{label}_{}_{}",
            std::process::id(),
            nanos
        ))
    }

    fn build_test_zip_bytes(entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buf);
            let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, data) in entries {
                match data {
                    Some(bytes) => {
                        writer.start_file(*name, options).expect("start zip file");
                        writer.write_all(bytes).expect("write zip file bytes");
                    }
                    None => {
                        writer.add_directory(*name, options).expect("add zip dir");
                    }
                }
            }
            writer.finish().expect("finish zip");
        }
        buf.into_inner()
    }

    #[test]
    fn parse_args_accepts_defaults_and_flags() {
        let defaults = parse_args_from(Vec::<String>::new()).expect("defaults should parse");
        assert_eq!(defaults, CliOptions::default());
        assert_eq!(defaults.concurrency, 2);

        let custom = parse_args_from(vec![
            "--force".to_string(),
            "--concurrency".to_string(),
            "8".to_string(),
        ])
        .expect("custom flags should parse");
        assert!(custom.force);
        assert_eq!(custom.concurrency, 8);
        assert!(!custom.help);
    }

    #[test]
    fn parse_args_rejects_zero_concurrency_and_unknown_flags() {
        assert!(parse_args_from(vec!["--concurrency".into(), "0".into()]).is_err());
        assert!(parse_args_from(vec!["--concurrency".into()]).is_err());
        assert!(parse_args_from(vec!["--unknown".into()]).is_err());
    }

    #[test]
    fn build_targets_produces_48_sensor_years_for_canonical_set() {
        let canonical = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .join("data/archive-set.json");
        let set = load_archive_set(&canonical).expect("canonical archive-set.json should load");
        assert_eq!(set.scope, "all_countries");
        assert_eq!(set.sensors.len(), 3);

        let targets = build_targets(&set);
        assert_eq!(targets.len(), 48);
        assert_eq!(
            targets.iter().filter(|t| t.slug == "modis").count(),
            26
        );
        assert_eq!(
            targets.iter().filter(|t| t.slug == "viirs-snpp").count(),
            14
        );
        assert_eq!(
            targets.iter().filter(|t| t.slug == "viirs-jpss1").count(),
            8
        );
    }

    #[test]
    fn build_targets_skips_unsupported_years_and_slugs() {
        let set = ArchiveSet {
            scope: "all_countries".to_string(),
            sensors: vec![
                SensorSpec {
                    slug: "modis".to_string(),
                    from: 1999,
                    to: 2001,
                },
                SensorSpec {
                    slug: "modis".to_string(),
                    from: 2000,
                    to: 2001,
                },
                SensorSpec {
                    slug: "viirs-jpss2".to_string(),
                    from: 2020,
                    to: 2025,
                },
                SensorSpec {
                    slug: "viirs-snpp".to_string(),
                    from: 2022,
                    to: 2020,
                },
            ],
        };
        let targets = build_targets(&set);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0], SeedTarget { slug: "modis".into(), year: 2000 });
        assert_eq!(targets[1], SeedTarget { slug: "modis".into(), year: 2001 });
    }

    #[test]
    fn load_archive_set_rejects_missing_and_corrupt_json() {
        let dir = unique_temp_dir("invalid_json");
        std::fs::create_dir_all(&dir).expect("temp dir");

        let missing = dir.join("missing.json");
        assert!(load_archive_set(&missing).is_err());

        let corrupt = dir.join("corrupt.json");
        std::fs::write(&corrupt, "{ not valid json").expect("write corrupt");
        assert!(load_archive_set(&corrupt).is_err());

        let wrong_scope = dir.join("wrong_scope.json");
        std::fs::write(&wrong_scope, r#"{"scope":"custom","sensors":[]}"#).expect("write scope");
        assert!(load_archive_set(&wrong_scope).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn complete_marker_skips_target_unless_force_is_set() {
        let dir = unique_temp_dir("idempotent_skip");
        let done_target = SeedTarget {
            slug: "modis".to_string(),
            year: 2020,
        };
        let pending_target = SeedTarget {
            slug: "modis".to_string(),
            year: 2021,
        };

        write_complete_marker(&dir, "modis", 2020).expect("write marker");

        assert!(
            !needs_download(&dir, &done_target, false).expect("check done"),
            "completed sensor-year should be skipped when --force is false"
        );
        assert!(
            needs_download(&dir, &pending_target, false).expect("check pending"),
            "sensor-year without .complete should be targeted"
        );
        assert!(
            needs_download(&dir, &done_target, true).expect("check force"),
            "--force should re-target sensor-years with .complete"
        );

        let http = Arc::new(reqwest::Client::new());
        let sem = Arc::new(Semaphore::new(2));
        let outcome = process_target(http, dir.clone(), sem, done_target, false).await;
        assert!(matches!(outcome, SensorYearOutcome::Skipped));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_zip_entry_path_blocks_zip_slip_and_wrong_prefix() {
        assert!(validate_zip_entry_path("../evil.csv", "modis", 2023).is_err());
        assert!(validate_zip_entry_path("/etc/passwd", "modis", 2023).is_err());
        assert!(validate_zip_entry_path("modis/2023/../../x.csv", "modis", 2023).is_err());
        assert!(
            validate_zip_entry_path("viirs-snpp/2023/viirs-snpp_2023_Turkey.csv", "modis", 2023)
                .is_err()
        );
        assert!(
            validate_zip_entry_path("modis/2022/modis_2022_Turkey.csv", "modis", 2023).is_err()
        );
        assert!(validate_zip_entry_path(r"modis\2023\x.csv", "modis", 2023).is_err());

        let dir_entry =
            validate_zip_entry_path("modis/2023/", "modis", 2023).expect("valid dir entry");
        assert_eq!(dir_entry, PathBuf::from("modis/2023"));

        let csv_entry = validate_zip_entry_path("modis/2023/modis_2023_Turkey.csv", "modis", 2023)
            .expect("valid csv entry");
        assert_eq!(csv_entry, PathBuf::from("modis/2023/modis_2023_Turkey.csv"));
    }

    #[test]
    fn extract_and_mark_complete_writes_relative_files_and_marker() {
        let dir = unique_temp_dir("extract_zip");
        std::fs::create_dir_all(&dir).expect("temp dir");

        let zip_bytes = build_test_zip_bytes(&[
            ("modis/2023/", None),
            (
                "modis/2023/modis_2023_Turkey.csv",
                Some(b"latitude,longitude\n39.0,32.0\n"),
            ),
            (
                "modis/2023/modis_2023_Greece.csv",
                Some(b"latitude,longitude\n38.0,23.7\n"),
            ),
        ]);

        let zip_path = temp_zip_path(&dir, "modis", 2023);
        std::fs::create_dir_all(zip_path.parent().expect("download dir")).expect("mkdir download");
        std::fs::write(&zip_path, &zip_bytes).expect("write test zip");

        let count = extract_and_mark_complete(&zip_path, &dir, "modis", 2023)
            .expect("extract should succeed");
        assert_eq!(count, 2);

        let turkey_csv = std::fs::read_to_string(
            dir.join("modis").join("2023").join("modis_2023_Turkey.csv"),
        )
        .expect("read Turkey CSV");
        assert_eq!(turkey_csv, "latitude,longitude\n39.0,32.0\n");

        let greece_csv = std::fs::read_to_string(
            dir.join("modis").join("2023").join("modis_2023_Greece.csv"),
        )
        .expect("read Greece CSV");
        assert_eq!(greece_csv, "latitude,longitude\n38.0,23.7\n");

        assert!(complete_marker_path(&dir, "modis", 2023).is_file());
        assert!(!zip_path.exists(), "temp zip should be removed after extraction");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_zip_archive_cleans_up_on_zip_slip_failure() {
        let dir = unique_temp_dir("extract_zip_slip");
        std::fs::create_dir_all(&dir).expect("temp dir");

        let zip_bytes = build_test_zip_bytes(&[
            (
                "modis/2023/modis_2023_Turkey.csv",
                Some(b"latitude,longitude\n39.0,32.0\n"),
            ),
            ("modis/2023/../../evil.csv", Some(b"bad")),
        ]);

        let zip_path = temp_zip_path(&dir, "modis", 2023);
        std::fs::create_dir_all(zip_path.parent().expect("download dir")).expect("mkdir download");
        std::fs::write(&zip_path, &zip_bytes).expect("write test zip");

        let err = extract_zip_archive(&zip_path, &dir, "modis", 2023);
        assert!(err.is_err());
        assert!(!dir.join("evil.csv").exists());
        assert!(
            !dir.join("modis")
                .join("2023")
                .join("modis_2023_Turkey.csv")
                .exists(),
            "partially extracted files should be cleaned up on failure"
        );
        assert!(!complete_marker_path(&dir, "modis", 2023).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
