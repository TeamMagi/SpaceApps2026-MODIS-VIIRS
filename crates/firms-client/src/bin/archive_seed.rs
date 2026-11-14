use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use pyrosync_core::models::Sensor;
use pyrosync_firms::archive::{ArchiveClient, archive_slug, archive_years};
use serde::Deserialize;
use tokio::sync::Semaphore;
use tracing_subscriber::EnvFilter;

const DEFAULT_CACHE_DIR: &str = ".cache/firms-archive";
const DEFAULT_SET_FILE: &str = "data/archive-set.json";
const DEFAULT_CONCURRENCY: usize = 4;

// Country archive CSVs are unauthenticated static files and spend zero FIRMS API
// transactions, so the seed binary intentionally never reads FIRMS_API_KEY.

#[derive(Debug, Deserialize)]
struct ArchiveSet {
    countries: Vec<String>,
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

#[derive(Debug, Clone)]
struct SeedTarget {
    sensor: Sensor,
    slug: String,
    country: String,
    year: i32,
}

#[derive(Debug)]
enum FileOutcome {
    Skipped,
    Downloaded,
    Failed { rel_path: String, error: String },
}

fn print_usage() {
    println!(
        "\
Usage: archive_seed [OPTIONS]

Download the canonical FIRMS country archive CSV set without using an API key.

Options:
  --force              Re-download files even if already present and non-empty
  --concurrency <N>    Max concurrent downloads (default: {DEFAULT_CONCURRENCY})
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

fn sensor_for_slug(slug: &str) -> Option<Sensor> {
    // MODIS Terra and Aqua share the single `modis` archive directory on FIRMS.
    match slug {
        "modis" => Some(Sensor::ModisTerra),
        "viirs-snpp" => Some(Sensor::ViirsSNPP),
        _ => None,
    }
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
    Ok(set)
}

fn build_targets(set: &ArchiveSet) -> Vec<SeedTarget> {
    let mut countries = Vec::new();
    let mut seen_countries = BTreeSet::new();
    for country in &set.countries {
        let trimmed = country.trim();
        if trimmed.is_empty() {
            tracing::warn!(target: "pyrosync::archive_seed", "empty country entry in set definition; skipping");
            continue;
        }
        if seen_countries.insert(trimmed.to_string()) {
            countries.push(trimmed.to_string());
        }
    }

    let mut targets = Vec::new();
    let mut seen_targets = BTreeSet::new();

    for spec in &set.sensors {
        let Some(sensor) = sensor_for_slug(&spec.slug) else {
            tracing::warn!(
                target: "pyrosync::archive_seed",
                slug = %spec.slug,
                "unsupported archive sensor slug in set definition; skipping"
            );
            continue;
        };

        let Some(expected_slug) = archive_slug(sensor) else {
            tracing::warn!(
                target: "pyrosync::archive_seed",
                slug = %spec.slug,
                "sensor has no archive slug; skipping"
            );
            continue;
        };

        if expected_slug != spec.slug {
            tracing::warn!(
                target: "pyrosync::archive_seed",
                slug = %spec.slug,
                expected = expected_slug,
                "sensor slug mismatch with archive module; skipping"
            );
            continue;
        }

        let Some(avail_years) = archive_years(sensor) else {
            tracing::warn!(
                target: "pyrosync::archive_seed",
                slug = %spec.slug,
                "sensor has no archive year range; skipping"
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
            if !avail_years.contains(&year) {
                tracing::warn!(
                    target: "pyrosync::archive_seed",
                    slug = %spec.slug,
                    year,
                    covered_from = *avail_years.start(),
                    covered_to = *avail_years.end(),
                    "year outside archive coverage; skipping"
                );
                continue;
            }

            for country in &countries {
                if seen_targets.insert((spec.slug.clone(), country.clone(), year)) {
                    targets.push(SeedTarget {
                        sensor,
                        slug: spec.slug.clone(),
                        country: country.clone(),
                        year,
                    });
                }
            }
        }
    }

    targets
}

fn cache_entry_paths(cache_dir: &Path, slug: &str, country: &str, year: i32) -> (String, PathBuf) {
    let filename = format!("{slug}_{year}_{country}.csv");
    let rel_path = format!("{slug}/{year}/{filename}");
    let full_path = cache_dir.join(slug).join(year.to_string()).join(filename);
    (rel_path, full_path)
}

async fn process_target(
    client: Arc<ArchiveClient>,
    cache_dir: PathBuf,
    semaphore: Arc<Semaphore>,
    target: SeedTarget,
    force: bool,
) -> FileOutcome {
    let (rel_path, full_path) =
        cache_entry_paths(&cache_dir, &target.slug, &target.country, target.year);

    // Idempotency: skip existing non-empty files so interrupted 2.4 GB runs resume
    // cleanly and teammates can re-run seed without re-downloading cached CSVs.
    if !force {
        match tokio::fs::metadata(&full_path).await {
            Ok(meta) if meta.is_file() && meta.len() > 0 => {
                let bytes = meta.len();
                tracing::info!(
                    target: "pyrosync::archive_seed",
                    file = %rel_path,
                    bytes,
                    "skipped (already cached)"
                );
                return FileOutcome::Skipped;
            }
            Ok(_) => {
                // Remove zero-byte leftovers so ArchiveClient fetches fresh data instead
                // of treating an empty file on disk as a cache hit.
                let _ = tokio::fs::remove_file(&full_path).await;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                tracing::warn!(
                    target: "pyrosync::archive_seed",
                    file = %rel_path,
                    error = %err,
                    "failed to inspect cached file"
                );
                return FileOutcome::Failed {
                    rel_path,
                    error: err.to_string(),
                };
            }
        }
    } else if let Err(err) = tokio::fs::remove_file(&full_path).await
        && err.kind() != std::io::ErrorKind::NotFound
    {
        // ArchiveClient serves directly from disk when present, so --force removes the
        // existing entry before delegating the download to ArchiveClient.
        tracing::warn!(
            target: "pyrosync::archive_seed",
            file = %rel_path,
            error = %err,
            "failed to remove existing file for --force"
        );
        return FileOutcome::Failed {
            rel_path,
            error: err.to_string(),
        };
    }

    let Ok(_permit) = semaphore.acquire().await else {
        return FileOutcome::Failed {
            rel_path,
            error: "download semaphore closed unexpectedly".to_string(),
        };
    };

    match client
        .fetch_country_year_csv(target.sensor, &target.country, target.year)
        .await
    {
        Ok(csv) => {
            let bytes = csv.len();
            if bytes == 0 {
                let _ = tokio::fs::remove_file(&full_path).await;
                tracing::warn!(
                    target: "pyrosync::archive_seed",
                    file = %rel_path,
                    "downloaded empty CSV (0 bytes)"
                );
                return FileOutcome::Failed {
                    rel_path,
                    error: "downloaded empty CSV (0 bytes)".to_string(),
                };
            }
            tracing::info!(
                target: "pyrosync::archive_seed",
                file = %rel_path,
                bytes,
                "downloaded"
            );
            FileOutcome::Downloaded
        }
        Err(err) => {
            tracing::warn!(
                target: "pyrosync::archive_seed",
                file = %rel_path,
                error = %err,
                "download failed"
            );
            FileOutcome::Failed {
                rel_path,
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
    let client = Arc::new(
        ArchiveClient::new(cache_dir.clone()).with_context(|| {
            format!(
                "failed to initialize archive cache directory '{}'",
                cache_dir.display()
            )
        })?,
    );

    let targets = build_targets(&set);
    if targets.is_empty() {
        bail!(
            "no valid archive targets found in '{}'",
            set_path.display()
        );
    }

    let expected = targets.len();
    tracing::info!(
        target: "pyrosync::archive_seed",
        set_file = %set_path.display(),
        cache_dir = %cache_dir.display(),
        expected,
        concurrency = opts.concurrency,
        force = opts.force,
        "starting archive seed"
    );

    let semaphore = Arc::new(Semaphore::new(opts.concurrency));
    let mut handles = Vec::with_capacity(expected);

    for target in targets {
        let rel_fallback = cache_entry_paths(
            &cache_dir,
            &target.slug,
            &target.country,
            target.year,
        )
        .0;
        let client = Arc::clone(&client);
        let cache_dir = cache_dir.clone();
        let semaphore = Arc::clone(&semaphore);
        let force = opts.force;

        let handle = tokio::spawn(async move {
            process_target(client, cache_dir, semaphore, target, force).await
        });
        handles.push((rel_fallback, handle));
    }

    let mut skipped = 0usize;
    let mut downloaded = 0usize;
    let mut failures: Vec<(String, String)> = Vec::new();

    // Collect every outcome rather than aborting on the first failure: when 439 of 440
    // files succeed, keeping the completed files lets the next run fetch only what failed.
    for (rel_fallback, handle) in handles {
        match handle.await {
            Ok(FileOutcome::Skipped) => skipped += 1,
            Ok(FileOutcome::Downloaded) => downloaded += 1,
            Ok(FileOutcome::Failed { rel_path, error }) => failures.push((rel_path, error)),
            Err(err) => failures.push((rel_fallback, format!("task join error: {err}"))),
        }
    }

    let present = skipped + downloaded;
    tracing::info!(
        target: "pyrosync::archive_seed",
        expected,
        skipped,
        downloaded,
        failed = failures.len(),
        "archive seed summary"
    );

    if failures.is_empty() {
        tracing::info!(
            target: "pyrosync::archive_seed",
            "set complete: {present}/{expected} files"
        );
        Ok(ExitCode::SUCCESS)
    } else {
        tracing::error!(
            target: "pyrosync::archive_seed",
            failed = failures.len(),
            "archive seed finished with failures; re-run to retry missing files"
        );
        for (file, err) in &failures {
            tracing::error!(
                target: "pyrosync::archive_seed",
                file = %file,
                error = %err,
                "failed archive file"
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

    #[test]
    fn parse_args_accepts_defaults_and_flags() {
        let defaults = parse_args_from(Vec::<String>::new()).expect("defaults should parse");
        assert_eq!(defaults, CliOptions::default());

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
    fn build_targets_produces_440_files_for_canonical_set() {
        let canonical = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .join("data/archive-set.json");
        let set = load_archive_set(&canonical).expect("canonical archive-set.json should load");
        let targets = build_targets(&set);
        assert_eq!(set.countries.len(), 11);
        assert_eq!(targets.len(), 440);
    }

    #[test]
    fn build_targets_skips_unsupported_years_and_slugs() {
        let set = ArchiveSet {
            countries: vec!["Turkey".to_string(), "Turkey".to_string()],
            sensors: vec![
                SensorSpec {
                    slug: "modis".to_string(),
                    from: 1999,
                    to: 2001,
                },
                SensorSpec {
                    slug: "viirs-noaa20".to_string(),
                    from: 2020,
                    to: 2021,
                },
            ],
        };
        let targets = build_targets(&set);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].year, 2000);
        assert_eq!(targets[1].year, 2001);
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

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn process_target_skips_existing_non_empty_file() {
        let dir = unique_temp_dir("idempotent_skip");
        let client = Arc::new(ArchiveClient::new(dir.clone()).expect("client"));
        let (_, path) = cache_entry_paths(&dir, "modis", "Turkey", 2020);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, "latitude,longitude\n39.0,32.0\n").expect("write csv");

        let target = SeedTarget {
            sensor: Sensor::ModisTerra,
            slug: "modis".to_string(),
            country: "Turkey".to_string(),
            year: 2020,
        };
        let sem = Arc::new(Semaphore::new(2));
        let outcome = process_target(client, dir.clone(), sem, target, false).await;
        assert!(matches!(outcome, FileOutcome::Skipped));
        let content = std::fs::read_to_string(&path).expect("cached file should remain intact");
        assert_eq!(content, "latitude,longitude\n39.0,32.0\n");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
