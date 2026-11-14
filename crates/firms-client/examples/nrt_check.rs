use std::path::PathBuf;

use chrono::Days;
use pyrosync_firms::{DayRange, FirmsClient, FirmsSource, Processing};
use pyrosync_core::models::{BBox, Sensor};
use tracing_subscriber::EnvFilter;

const DEFAULT_BBOX: &str = "28,36,32,39";

struct CliArgs {
    bbox_raw: String,
    save_samples: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let api_key = match std::env::var("FIRMS_API_KEY") {
        Ok(val) if !val.trim().is_empty() => val.trim().to_string(),
        _ => {
            eprintln!(
                "FIRMS_API_KEY is unset or blank; this check requires FIRMS_API_KEY to query FIRMS."
            );
            std::process::exit(2);
        }
    };

    let args = parse_args()?;
    let bbox = parse_bbox(&args.bbox_raw)?;
    let client = FirmsClient::new(api_key.clone())?;

    let avail = match client.availability().await {
        Ok(a) => a,
        Err(err) => {
            eprintln!(
                "failed to fetch FIRMS availability: {}",
                redact_key(&format!("{err:#}"), &api_key)
            );
            std::process::exit(1);
        }
    };

    let sources = [
        FirmsSource::new(Sensor::ModisTerra, Processing::Nrt),
        FirmsSource::new(Sensor::ViirsSNPP, Processing::Nrt),
        FirmsSource::new(Sensor::ViirsNOAA20, Processing::Nrt),
    ];

    let mut failures = Vec::new();

    for source in sources {
        let source_id = source.as_str();
        let Some((min_date, max_date)) = avail.range_for(source) else {
            let msg = format!("{source_id}: not present in availability table");
            eprintln!("FAIL: {msg}");
            failures.push(msg);
            continue;
        };

        let (start, days) = if max_date > min_date {
            (
                max_date.pred_opt().unwrap_or(max_date),
                DayRange::clamped(2),
            )
        } else {
            (max_date, DayRange::clamped(1))
        };
        let end = start + Days::new(u64::from(days.get()) - 1);

        let raw_csv = match client.fetch_window_csv(source, &bbox, start, days).await {
            Ok(csv) => csv,
            Err(err) => {
                let msg = redact_key(
                    &format!("{source_id} ({start}..={end}): {err:#}"),
                    &api_key,
                );
                eprintln!("FAIL: {msg}");
                failures.push(msg);
                continue;
            }
        };

        if args.save_samples {
            let dir = PathBuf::from("data").join("sample");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join(format!("{}.csv", source_id.to_ascii_lowercase()));
            std::fs::write(&path, raw_csv.as_bytes())?;
            println!("saved {source_id} sample -> {}", path.display());
        }

        let hotspots = match client.fetch_window(source, &bbox, start, days).await {
            Ok(hs) => hs,
            Err(err) => {
                let msg = redact_key(
                    &format!("{source_id} ({start}..={end}) parse: {err:#}"),
                    &api_key,
                );
                eprintln!("FAIL: {msg}");
                failures.push(msg);
                continue;
            }
        };

        let data_rows = count_data_rows(&raw_csv);
        if hotspots.is_empty() {
            println!(
                "{source_id} | {start}..={end} | {} bytes | {data_rows} data rows | 0 hotspots | EMPTY",
                raw_csv.len()
            );
        } else {
            let first = &hotspots[0];
            println!(
                "{source_id} | {start}..={end} | {} bytes | {data_rows} data rows | {} hotspots | first: lat={}, lon={}, frp={:?}, fire_type={:?}",
                raw_csv.len(),
                hotspots.len(),
                first.lat,
                first.lon,
                first.frp,
                first.fire_type
            );
        }
    }

    if !failures.is_empty() {
        eprintln!("\nSummary of failures ({}/{}):", failures.len(), sources.len());
        for failure in &failures {
            eprintln!("  - {failure}");
        }
        std::process::exit(1);
    }

    Ok(())
}

fn parse_args() -> anyhow::Result<CliArgs> {
    let mut bbox_raw = DEFAULT_BBOX.to_string();
    let mut save_samples = false;
    let mut iter = std::env::args().skip(1);

    while let Some(arg) = iter.next() {
        if arg == "--save-samples" {
            save_samples = true;
        } else if arg == "--bbox" {
            bbox_raw = iter
                .next()
                .ok_or_else(|| anyhow::anyhow!("--bbox requires a W,S,E,N value"))?;
        } else if let Some(val) = arg.strip_prefix("--bbox=") {
            bbox_raw = val.to_string();
        } else {
            anyhow::bail!("unexpected argument: {arg}");
        }
    }

    Ok(CliArgs {
        bbox_raw,
        save_samples,
    })
}

fn parse_bbox(raw: &str) -> anyhow::Result<BBox> {
    let parts: Vec<&str> = raw.split(',').collect();
    if parts.len() != 4 {
        anyhow::bail!("expected --bbox W,S,E,N, got {raw:?}");
    }
    let west: f64 = parts[0].trim().parse()?;
    let south: f64 = parts[1].trim().parse()?;
    let east: f64 = parts[2].trim().parse()?;
    let north: f64 = parts[3].trim().parse()?;
    if !west.is_finite() || !south.is_finite() || !east.is_finite() || !north.is_finite() {
        anyhow::bail!("bbox coordinates must be finite numbers");
    }
    if west >= east || south >= north {
        anyhow::bail!("bbox must satisfy W < E and S < N");
    }
    Ok(BBox {
        west,
        south,
        east,
        north,
    })
}

fn count_data_rows(csv: &str) -> usize {
    let trimmed = csv.trim();
    if trimmed.is_empty() {
        return 0;
    }
    trimmed
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .count()
}

fn redact_key(text: &str, api_key: &str) -> String {
    if api_key.is_empty() {
        return text.to_string();
    }
    text.replace(api_key, "***")
}
