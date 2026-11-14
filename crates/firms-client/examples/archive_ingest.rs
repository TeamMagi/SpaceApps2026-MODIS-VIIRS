use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use pyrosync_core::calendar::build_calendar;
use pyrosync_core::harmonize::harmonize;
use pyrosync_core::models::{BBox, CalendarCell, Hotspot, HotspotSource, Sensor};
use pyrosync_firms::archive::{ArchiveClient, archive_years, filter_bbox};
use pyrosync_firms::parse::retain_wildfires;
use tracing_subscriber::EnvFilter;

const COUNTRIES: &[&str] = &[
    "Turkey",
    "Greece",
    "Bulgaria",
    "Georgia",
    "Armenia",
    "Azerbaijan",
    "Iran",
    "Iraq",
    "Syria",
    "Cyprus",
    "Russian_Federation",
];

struct Config {
    from: i32,
    to: i32,
    cache_dir: PathBuf,
    concurrency: usize,
    bbox: BBox,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            from: 2020,
            to: 2023,
            cache_dir: PathBuf::from(".cache/firms-archive"),
            concurrency: 4,
            bbox: BBox {
                west: 25.0,
                south: 35.0,
                east: 45.0,
                north: 43.0,
            },
        }
    }
}

struct SensorIngest {
    raw_count: usize,
    hotspots: Vec<Hotspot>,
}

fn print_usage() {
    println!(
        "\
Usage: archive_ingest [OPTIONS]

Ingest pre-packaged FIRMS yearly country CSVs for Turkey and neighbours, then
run bbox filtering, wildfire filtering, harmonization, and calendar aggregation.

Options:
  --from <YEAR>        First year to ingest (default: 2020)
  --to <YEAR>          Last year to ingest (default: 2023)
  --cache <DIR>        Cache directory for downloaded CSVs (default: .cache/firms-archive)
  --concurrency <N>    Max concurrent downloads (default: 4)
  --bbox <W,S,E,N>     Bounding box filter (default: 25.0,35.0,45.0,43.0)
  --all-countries      Not implemented in this example; use the all_countries zip ingest
  -h, --help           Print this help message"
    );
}

fn parse_bbox(raw: &str) -> Result<BBox> {
    let parts: Vec<&str> = raw.split(',').collect();
    if parts.len() != 4 {
        bail!("--bbox expects 4 comma-separated numbers (W,S,E,N), got: {raw}");
    }
    let west: f64 = parts[0].trim().parse().context("invalid bbox west")?;
    let south: f64 = parts[1].trim().parse().context("invalid bbox south")?;
    let east: f64 = parts[2].trim().parse().context("invalid bbox east")?;
    let north: f64 = parts[3].trim().parse().context("invalid bbox north")?;
    if west >= east || south >= north {
        bail!("--bbox requires west < east and south < north, got: {raw}");
    }
    Ok(BBox {
        west,
        south,
        east,
        north,
    })
}

fn next_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .with_context(|| format!("missing value for {flag}"))
}

fn parse_args() -> Result<Option<Config>> {
    let mut cfg = Config::default();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return Ok(None);
            }
            "--from" => {
                let val = next_arg(&mut args, "--from")?;
                cfg.from = val.parse().with_context(|| format!("invalid --from year: {val}"))?;
            }
            "--to" => {
                let val = next_arg(&mut args, "--to")?;
                cfg.to = val.parse().with_context(|| format!("invalid --to year: {val}"))?;
            }
            "--cache" => {
                cfg.cache_dir = PathBuf::from(next_arg(&mut args, "--cache")?);
            }
            "--concurrency" => {
                let val = next_arg(&mut args, "--concurrency")?;
                cfg.concurrency = val
                    .parse()
                    .with_context(|| format!("invalid --concurrency: {val}"))?;
            }
            "--bbox" => {
                let val = next_arg(&mut args, "--bbox")?;
                cfg.bbox = parse_bbox(&val)?;
            }
            "--all-countries" => {
                bail!("not implemented in this example; use the all_countries zip ingest");
            }
            other => bail!("unknown argument: {other} (try --help)"),
        }
    }

    if cfg.from > cfg.to {
        bail!("--from ({}) must be <= --to ({})", cfg.from, cfg.to);
    }
    if cfg.concurrency == 0 {
        bail!("--concurrency must be >= 1");
    }

    Ok(Some(cfg))
}

fn clamp_years(sensor: Sensor, from: i32, to: i32) -> Option<RangeInclusive<i32>> {
    let Some(avail) = archive_years(sensor) else {
        eprintln!("warning: {sensor:?} has no pre-packaged archive coverage; skipping");
        return None;
    };
    let (avail_start, avail_end) = (*avail.start(), *avail.end());
    if from > avail_end || to < avail_start {
        eprintln!(
            "warning: requested years {from}..={to} do not overlap {sensor:?} archive coverage ({avail_start}..={avail_end}); skipping"
        );
        return None;
    }
    let clamped_from = from.max(avail_start);
    let clamped_to = to.min(avail_end);
    if clamped_from != from || clamped_to != to {
        eprintln!(
            "note: clamping {sensor:?} years from {from}..={to} to {clamped_from}..={clamped_to} (archive covers {avail_start}..={avail_end})"
        );
    }
    Some(clamped_from..=clamped_to)
}

async fn ingest_sensor(
    client: &ArchiveClient,
    sensor: Sensor,
    countries: &[String],
    cfg: &Config,
) -> SensorIngest {
    let Some(years) = clamp_years(sensor, cfg.from, cfg.to) else {
        println!("[{sensor:?}] raw=0 failures=0 (outside archive coverage)");
        return SensorIngest {
            raw_count: 0,
            hotspots: Vec::new(),
        };
    };

    let (hotspots, failures) = client
        .fetch_many(sensor, countries, years, cfg.concurrency)
        .await;
    let raw_count = hotspots.len();

    println!(
        "[{sensor:?}] raw={raw_count} failures={}",
        failures.len()
    );
    for (country, year, err) in failures.iter().take(3) {
        println!("  failure: {country}/{year}: {err}");
    }

    let mut filtered = filter_bbox(hotspots, &cfg.bbox);
    let after_bbox = filtered.len();
    let dropped_bbox = raw_count - after_bbox;

    retain_wildfires(&mut filtered);
    let kept = filtered.len();
    let dropped_non_wildfire = after_bbox - kept;

    println!(
        "[{sensor:?}] bbox_dropped={dropped_bbox} non_wildfire_dropped={dropped_non_wildfire} kept={kept}"
    );

    SensorIngest {
        raw_count,
        hotspots: filtered,
    }
}

fn print_calendar_table(calendar: &[CalendarCell]) {
    let mut grid: BTreeMap<i32, [u64; 12]> = BTreeMap::new();
    for cell in calendar {
        if (1..=12).contains(&cell.month) {
            grid.entry(cell.year).or_insert([0; 12])[(cell.month - 1) as usize] = cell.count;
        }
    }

    println!("\nCalendar (count by year x month):");
    println!(
        "{:<6} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7}",
        "Year", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"
    );
    for (year, counts) in &grid {
        print!("{year:<6}");
        for count in counts {
            print!(" {count:>7}");
        }
        println!();
    }
}

fn print_top_months(calendar: &[CalendarCell]) {
    let mut ranked: Vec<&CalendarCell> = calendar.iter().collect();
    ranked.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.year.cmp(&b.year))
            .then_with(|| a.month.cmp(&b.month))
    });

    println!("\nTop 5 months by count:");
    for cell in ranked.into_iter().take(5) {
        let anomaly = match cell.anomaly {
            Some(v) => format!("{v:+.2}"),
            None => "n/a".to_string(),
        };
        println!(
            "  {:04}-{:02}  count={:<8} mean_frp={:.2} MW  anomaly={}",
            cell.year, cell.month, cell.count, cell.mean_frp, anomaly
        );
    }
}

fn dir_size_bytes(root: &Path) -> std::io::Result<u64> {
    if !root.exists() {
        return Ok(0);
    }
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let Some(cfg) = parse_args()? else {
        return Ok(());
    };

    let countries: Vec<String> = COUNTRIES.iter().map(|s| (*s).to_string()).collect();
    let started = Instant::now();

    let client = ArchiveClient::new(cfg.cache_dir.clone())?;
    let modis = ingest_sensor(&client, Sensor::ModisTerra, &countries, &cfg).await;
    let viirs = ingest_sensor(&client, Sensor::ViirsSNPP, &countries, &cfg).await;

    // Both sensors returning nothing points to a broken archive URL/parser or an empty range.
    if modis.raw_count == 0 && viirs.raw_count == 0 {
        bail!(
            "both ModisTerra and ViirsSNPP yielded zero raw hotspots; archive path may be broken or outside coverage"
        );
    }

    let harmonized = harmonize(modis.hotspots, viirs.hotspots);
    let (mut modis_only, mut viirs_only, mut matched) = (0usize, 0usize, 0usize);
    for h in &harmonized {
        match h.source {
            HotspotSource::ModisOnly => modis_only += 1,
            HotspotSource::ViirsOnly => viirs_only += 1,
            HotspotSource::Matched => matched += 1,
        }
    }
    println!(
        "\nHarmonized total={} (ModisOnly={}, ViirsOnly={}, Matched={})",
        harmonized.len(),
        modis_only,
        viirs_only,
        matched
    );

    let calendar = build_calendar(&harmonized);
    print_calendar_table(&calendar);
    print_top_months(&calendar);

    let elapsed = started.elapsed();
    let cache_bytes = dir_size_bytes(&cfg.cache_dir)
        .with_context(|| format!("failed to measure cache dir {}", cfg.cache_dir.display()))?;
    let cache_mb = cache_bytes as f64 / (1024.0 * 1024.0);
    println!(
        "\nCompleted in {:.2}s; cache size on disk: {:.1} MB ({})",
        elapsed.as_secs_f64(),
        cache_mb,
        cfg.cache_dir.display()
    );

    Ok(())
}
