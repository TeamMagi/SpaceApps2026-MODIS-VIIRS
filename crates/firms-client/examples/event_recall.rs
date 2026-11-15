//! Compare documented fire months with raw and harmonized monthly country series.
//!
//! Usage:
//!   ARCHIVE_CACHE_DIR=.cache/firms-archive cargo run --release -p pyrosync-firms \
//!     --example event_recall -- [--countries Turkey,Canada] [--from 2003] [--to 2025] [--out <csv>]

use anyhow::{Context, Result, bail};
use chrono::Datelike;
use pyrosync_core::calendar::build_calendar_with;
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::effort::FIRST_TERRA_AQUA_MONTH;
use pyrosync_core::harmonize::harmonize_with_config;
use pyrosync_core::models::Sensor;
use pyrosync_firms::archive::{ArchiveClient, ArchiveError};
use pyrosync_firms::parse::retain_wildfires;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing_subscriber::EnvFilter;

const ARCHIVE_FETCH_SENSORS: [Sensor; 3] =
    [Sensor::ModisTerra, Sensor::ViirsSNPP, Sensor::ViirsNOAA20];
const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const DEFAULT_FROM_YEAR: i32 = 2003;
const DEFAULT_TO_YEAR: i32 = 2025;

struct Event {
    country: &'static str,
    year: i32,
    month: u32,
    label: &'static str,
    reference: &'static str,
}

const EVENTS: [Event; 9] = [
    Event {
        country: "Australia",
        year: 2019,
        month: 12,
        label: "Black Summer",
        reference: "NASA Earth Observatory, Explosive Fire Activity in Australia (2020-01-10): NSW/Victoria, December-January firestorms",
    },
    Event {
        country: "Australia",
        year: 2020,
        month: 1,
        label: "Black Summer",
        reference: "NASA Earth Observatory, Explosive Fire Activity in Australia (2020-01-10): NSW/Victoria, December-January firestorms",
    },
    Event {
        country: "Brazil",
        year: 2019,
        month: 8,
        label: "Amazon fires",
        reference: "NASA Earth Observatory, A New Tool for Tracking Amazon Fires (2020-08-20): August 2019 increase in Amazon fire activity",
    },
    Event {
        country: "Canada",
        year: 2023,
        month: 6,
        label: "record wildfire season",
        reference: "NASA Earth Observatory, Fires Burn Across Quebec (2023-06-06); Tracking Canada's Extreme 2023 Fire Season (2023-10-25)",
    },
    Event {
        country: "Turkey",
        year: 2021,
        month: 8,
        label: "Manavgat / Mediterranean fires",
        reference: "Copernicus CAMS (2021-08-04): Mediterranean wildfires and record fire intensity in Turkey",
    },
    Event {
        country: "Indonesia",
        year: 2015,
        month: 9,
        label: "El Nino peat fires",
        reference: "NASA Earth Observatory, Seeing Through the Smoky Pall (2015-12-01): September-October El Nino drought and peat fires",
    },
    Event {
        country: "Indonesia",
        year: 2015,
        month: 10,
        label: "El Nino peat fires",
        reference: "NASA Earth Observatory, Seeing Through the Smoky Pall (2015-12-01): September-October El Nino drought and peat fires",
    },
    Event {
        country: "United_States",
        year: 2020,
        month: 9,
        label: "western US fire season (CA/OR)",
        reference: "NASA Earth Observatory, A Wall of Smoke on the U.S. West Coast (2020-09-10): Oregon and California fires",
    },
    Event {
        country: "Russian_Federation",
        year: 2021,
        month: 7,
        label: "Yakutia (Sakha) fires",
        reference: "NASA Earth Observatory, Fires Scorch the Sakha Republic (2021-07-07): fires in Yakutia",
    },
];

struct Args {
    countries: Vec<&'static str>,
    from: i32,
    to: i32,
    out: Option<PathBuf>,
}

fn parse_args() -> Result<Option<Args>> {
    let mut args = Args {
        countries: Vec::new(),
        from: DEFAULT_FROM_YEAR,
        to: DEFAULT_TO_YEAR,
        out: None,
    };
    let mut countries = None;
    let mut input = std::env::args().skip(1);
    while let Some(arg) = input.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!(
                    "Usage: event_recall [--countries a,b] [--from {DEFAULT_FROM_YEAR}] [--to {DEFAULT_TO_YEAR}] [--out <csv>]\n\
                     Countries: Australia,Brazil,Canada,Turkey,Indonesia,United_States,Russian_Federation (default: all).\n\
                     Archive: ARCHIVE_CACHE_DIR or {DEFAULT_ARCHIVE_CACHE_DIR}; offline, read-only.\n\
                     Recall per metric (count and FRP): harmonized rank <= 3 or leave-one-out z >= 2; missed events do not fail the run."
                );
                return Ok(None);
            }
            "--countries" => {
                countries = Some(input.next().context("missing value for --countries")?)
            }
            "--from" => {
                args.from = input
                    .next()
                    .context("missing value for --from")?
                    .parse()
                    .context("invalid --from year")?
            }
            "--to" => {
                args.to = input
                    .next()
                    .context("missing value for --to")?
                    .parse()
                    .context("invalid --to year")?
            }
            "--out" => args.out = Some(input.next().context("missing value for --out")?.into()),
            _ => bail!("unknown argument: {arg} (try --help)"),
        }
    }
    if args.from > args.to || args.from < 2000 || args.to > 2025 {
        bail!("expected 2000 <= --from <= --to <= 2025 (country archive coverage)");
    }
    if let Some(countries) = countries {
        for name in countries.split(',').map(str::trim) {
            let country = EVENTS
                .iter()
                .find(|e| e.country.eq_ignore_ascii_case(name))
                .with_context(|| format!("unknown event country: '{name}' (try --help)"))?
                .country;
            if !args.countries.contains(&country) {
                args.countries.push(country);
            }
        }
    } else {
        for event in &EVENTS {
            if !args.countries.contains(&event.country) {
                args.countries.push(event.country);
            }
        }
    }
    Ok(Some(args))
}

#[derive(Clone, Copy, Debug, Default)]
struct Score {
    z: Option<f64>,
    rank: Option<usize>,
}

fn score(year: i32, values: &[(i32, f64)]) -> Score {
    let Some(&(_, value)) = values.iter().find(|&&(y, _)| y == year) else {
        return Score::default();
    };
    let reference: Vec<f64> = values
        .iter()
        .filter(|&&(y, _)| y != year)
        .map(|&(_, x)| x)
        .collect();
    let z = if reference.len() >= 2 {
        let mean = reference.iter().sum::<f64>() / reference.len() as f64;
        let sd = (reference.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
            / reference.len() as f64)
            .sqrt();
        (sd > 0.0)
            .then(|| (value - mean) / sd)
            .filter(|z| z.is_finite())
    } else {
        None
    };
    Score {
        z,
        rank: Some(1 + values.iter().filter(|&&(_, x)| x > value).count()),
    }
}

#[derive(Default)]
struct Month {
    year: i32,
    month: u32,
    raw_count: u64,
    count_normalized: Option<f64>,
    raw_frp: f64,
    total_frp: Option<f64>,
    raw: Score,
    harmonized: Score,
    raw_frp_score: Score,
    harmonized_frp_score: Score,
}

fn recalled(s: Score) -> Option<bool> {
    s.rank
        .map(|rank| rank <= 3 || s.z.is_some_and(|z| z >= 2.0))
}

#[derive(Clone, Copy, Default)]
struct Alarms {
    flagged: usize,
    evaluated: usize,
}

impl Alarms {
    fn share(self) -> Option<f64> {
        (self.evaluated > 0).then(|| self.flagged as f64 / self.evaluated as f64)
    }

    fn include(&mut self, score: Score) {
        if let Some(z) = score.z {
            self.evaluated += 1;
            self.flagged += usize::from(z >= 2.0);
        }
    }
}

struct CountryResult {
    country: &'static str,
    months: Vec<Month>,
    raw_alarms: Alarms,
    harmonized_alarms: Alarms,
    raw_frp_alarms: Alarms,
    harmonized_frp_alarms: Alarms,
}

async fn analyze_country(
    archive: &ArchiveClient,
    config: &PipelineConfig,
    country: &'static str,
    args: &Args,
) -> Result<CountryResult> {
    let started = Instant::now();
    let mut modis = Vec::new();
    let mut viirs = Vec::new();
    let mut raw_totals: BTreeMap<(i32, u32), (u64, f64)> = BTreeMap::new();
    let mut found_any_file = false;
    for year in args.from..=args.to {
        for sensor in ARCHIVE_FETCH_SENSORS {
            if !archive.covers(sensor, year) {
                continue;
            }
            match archive.fetch_country_year(sensor, country, year).await {
                Ok(mut hotspots) => {
                    found_any_file = true;
                    retain_wildfires(&mut hotspots);
                    for h in &hotspots {
                        let totals = raw_totals
                            .entry((h.acquired.year(), h.acquired.month()))
                            .or_default();
                        totals.0 += 1;
                        totals.1 += h.frp;
                    }
                    if sensor.is_modis() {
                        modis.append(&mut hotspots);
                    } else {
                        viirs.append(&mut hotspots);
                    }
                }
                Err(ArchiveError::NotCached { .. })
                    if archive.sensor_year_complete(sensor, year) => {}
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!("failed to load {sensor:?} {country} {year} from archive")
                    });
                }
            }
        }
    }
    if !found_any_file {
        bail!(
            "no archive CSV files found for '{country}' ({}..={}) in {}",
            args.from,
            args.to,
            archive.cache_dir().display()
        );
    }
    println!(
        "\nCOUNTRY: {country} ({}..={}) | raw_modis={} raw_viirs={}",
        args.from,
        args.to,
        modis.len(),
        viirs.len()
    );
    let harmonized = harmonize_with_config(modis, viirs, config);
    let cells = build_calendar_with(&harmonized, &config.uncertainty, &config.calibration);
    drop(harmonized);
    let cells: BTreeMap<_, _> = cells.into_iter().map(|c| ((c.year, c.month), c)).collect();
    let mut months = Vec::new();
    for year in args.from..=args.to {
        for month in 1..=12 {
            let (raw_count, raw_frp) = raw_totals.get(&(year, month)).copied().unwrap_or_default();
            let (count_normalized, total_frp) = match cells.get(&(year, month)) {
                Some(cell) => (
                    cell.count_normalized.filter(|x| x.is_finite()),
                    cell.total_frp.is_finite().then_some(cell.total_frp),
                ),
                // Complete archive coverage and no raw detections establish a true zero.
                None if raw_count == 0 && (year, month) >= FIRST_TERRA_AQUA_MONTH => {
                    (Some(0.0), Some(0.0))
                }
                None => (None, None),
            };
            months.push(Month {
                year,
                month,
                raw_count,
                count_normalized,
                raw_frp,
                total_frp,
                ..Month::default()
            });
        }
    }
    for calendar_month in 1..=12 {
        let raw: Vec<_> = months
            .iter()
            .filter(|m| m.month == calendar_month)
            .map(|m| (m.year, m.raw_count as f64))
            .collect();
        let normalized: Vec<_> = months
            .iter()
            .filter(|m| m.month == calendar_month)
            .filter_map(|m| m.count_normalized.map(|x| (m.year, x)))
            .collect();
        let raw_frp: Vec<_> = months
            .iter()
            .filter(|m| m.month == calendar_month)
            .map(|m| (m.year, m.raw_frp))
            .collect();
        let total_frp: Vec<_> = months
            .iter()
            .filter(|m| m.month == calendar_month)
            .filter_map(|m| m.total_frp.map(|x| (m.year, x)))
            .collect();
        for month in months.iter_mut().filter(|m| m.month == calendar_month) {
            month.raw = score(month.year, &raw);
            month.harmonized = score(month.year, &normalized);
            month.raw_frp_score = score(month.year, &raw_frp);
            month.harmonized_frp_score = score(month.year, &total_frp);
        }
    }
    let mut raw_alarms = Alarms::default();
    let mut harmonized_alarms = Alarms::default();
    let mut raw_frp_alarms = Alarms::default();
    let mut harmonized_frp_alarms = Alarms::default();
    for month in months.iter().filter(|m| m.year >= 2012) {
        raw_alarms.include(month.raw);
        harmonized_alarms.include(month.harmonized);
        raw_frp_alarms.include(month.raw_frp_score);
        harmonized_frp_alarms.include(month.harmonized_frp_score);
    }
    println!("Completed in {:.2}s", started.elapsed().as_secs_f64());
    Ok(CountryResult {
        country,
        months,
        raw_alarms,
        harmonized_alarms,
        raw_frp_alarms,
        harmonized_frp_alarms,
    })
}

fn fmt_number(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".into(), |x| format!("{x:.3}"))
}

fn fmt_rank(value: Option<usize>) -> String {
    value.map_or_else(|| "NA".into(), |x| x.to_string())
}

fn print_events(result: &CountryResult) {
    println!(
        "{:<32} {:<7} {:>11} {:>14} {:>12} {:>15} {:>9} {:>12} {:>10} {:>13} {:>14} {:>12}",
        "Event",
        "Month",
        "Count raw z",
        "Count raw rank",
        "Count harm z",
        "Count harm rank",
        "FRP raw z",
        "FRP raw rank",
        "FRP harm z",
        "FRP harm rank",
        "recalled_count",
        "recalled_frp"
    );
    for event in EVENTS.iter().filter(|e| e.country == result.country) {
        let month = result
            .months
            .iter()
            .find(|m| (m.year, m.month) == (event.year, event.month));
        let raw = month.map_or(Score::default(), |m| m.raw);
        let harm = month.map_or(Score::default(), |m| m.harmonized);
        let raw_frp = month.map_or(Score::default(), |m| m.raw_frp_score);
        let harm_frp = month.map_or(Score::default(), |m| m.harmonized_frp_score);
        println!(
            "{:<32} {:04}-{:02} {:>11} {:>14} {:>12} {:>15} {:>9} {:>12} {:>10} {:>13} {:>14} {:>12}",
            event.label,
            event.year,
            event.month,
            fmt_number(raw.z),
            fmt_rank(raw.rank),
            fmt_number(harm.z),
            fmt_rank(harm.rank),
            fmt_number(raw_frp.z),
            fmt_rank(raw_frp.rank),
            fmt_number(harm_frp.z),
            fmt_rank(harm_frp.rank),
            recalled(harm).map_or("NA", |yes| if yes { "yes" } else { "no" }),
            recalled(harm_frp).map_or("NA", |yes| if yes { "yes" } else { "no" })
        );
        println!(
            "  Reference: {}{}",
            event.reference,
            if month.is_none() {
                " [outside requested range]"
            } else {
                ""
            }
        );
    }
}

fn print_alarms(country: &str, raw: Alarms, harm: Alarms, raw_frp: Alarms, harm_frp: Alarms) {
    println!(
        "{country:<20} {:>7}/{:<7} {:>11} {:>7}/{:<7} {:>12} {:>7}/{:<7} {:>9} {:>7}/{:<7} {:>10}",
        raw.flagged,
        raw.evaluated,
        fmt_number(raw.share().map(|x| x * 100.0)),
        harm.flagged,
        harm.evaluated,
        fmt_number(harm.share().map(|x| x * 100.0)),
        raw_frp.flagged,
        raw_frp.evaluated,
        fmt_number(raw_frp.share().map(|x| x * 100.0)),
        harm_frp.flagged,
        harm_frp.evaluated,
        fmt_number(harm_frp.share().map(|x| x * 100.0))
    );
}

#[derive(Default, Serialize)]
struct CsvRow {
    section: &'static str,
    country: &'static str,
    from_year: i32,
    to_year: i32,
    year: Option<i32>,
    month: Option<u32>,
    event: Option<&'static str>,
    reference: Option<&'static str>,
    raw_count: Option<u64>,
    count_normalized: Option<f64>,
    raw_frp: Option<f64>,
    total_frp: Option<f64>,
    raw_z: Option<f64>,
    raw_rank: Option<usize>,
    harmonized_z: Option<f64>,
    harmonized_rank: Option<usize>,
    raw_frp_z: Option<f64>,
    raw_frp_rank: Option<usize>,
    harmonized_frp_z: Option<f64>,
    harmonized_frp_rank: Option<usize>,
    recalled_count: Option<bool>,
    recalled_frp: Option<bool>,
    raw_alarm_months: Option<usize>,
    raw_evaluated_months: Option<usize>,
    raw_alarm_share: Option<f64>,
    harmonized_alarm_months: Option<usize>,
    harmonized_evaluated_months: Option<usize>,
    harmonized_alarm_share: Option<f64>,
    raw_frp_alarm_months: Option<usize>,
    raw_frp_evaluated_months: Option<usize>,
    raw_frp_alarm_share: Option<f64>,
    harmonized_frp_alarm_months: Option<usize>,
    harmonized_frp_evaluated_months: Option<usize>,
    harmonized_frp_alarm_share: Option<f64>,
}

fn alarm_csv(
    country: &'static str,
    raw: Alarms,
    harm: Alarms,
    raw_frp: Alarms,
    harm_frp: Alarms,
    args: &Args,
) -> CsvRow {
    CsvRow {
        section: "false_alarms",
        country,
        from_year: args.from.max(2012),
        to_year: args.to,
        raw_alarm_months: Some(raw.flagged),
        raw_evaluated_months: Some(raw.evaluated),
        raw_alarm_share: raw.share(),
        harmonized_alarm_months: Some(harm.flagged),
        harmonized_evaluated_months: Some(harm.evaluated),
        harmonized_alarm_share: harm.share(),
        raw_frp_alarm_months: Some(raw_frp.flagged),
        raw_frp_evaluated_months: Some(raw_frp.evaluated),
        raw_frp_alarm_share: raw_frp.share(),
        harmonized_frp_alarm_months: Some(harm_frp.flagged),
        harmonized_frp_evaluated_months: Some(harm_frp.evaluated),
        harmonized_frp_alarm_share: harm_frp.share(),
        ..CsvRow::default()
    }
}

fn write_csv(
    path: &Path,
    results: &[CountryResult],
    raw: Alarms,
    harm: Alarms,
    raw_frp: Alarms,
    harm_frp: Alarms,
    args: &Args,
) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = csv::Writer::from_path(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    for result in results {
        for month in &result.months {
            let event = EVENTS.iter().find(|e| {
                e.country == result.country && (e.year, e.month) == (month.year, month.month)
            });
            writer.serialize(CsvRow {
                section: if event.is_some() { "event" } else { "month" },
                country: result.country,
                from_year: args.from,
                to_year: args.to,
                year: Some(month.year),
                month: Some(month.month),
                event: event.map(|e| e.label),
                reference: event.map(|e| e.reference),
                raw_count: Some(month.raw_count),
                count_normalized: month.count_normalized,
                raw_frp: Some(month.raw_frp),
                total_frp: month.total_frp,
                raw_z: month.raw.z,
                raw_rank: month.raw.rank,
                harmonized_z: month.harmonized.z,
                harmonized_rank: month.harmonized.rank,
                raw_frp_z: month.raw_frp_score.z,
                raw_frp_rank: month.raw_frp_score.rank,
                harmonized_frp_z: month.harmonized_frp_score.z,
                harmonized_frp_rank: month.harmonized_frp_score.rank,
                recalled_count: event.and_then(|_| recalled(month.harmonized)),
                recalled_frp: event.and_then(|_| recalled(month.harmonized_frp_score)),
                ..CsvRow::default()
            })?;
        }
        for event in EVENTS
            .iter()
            .filter(|e| e.country == result.country && !(args.from..=args.to).contains(&e.year))
        {
            writer.serialize(CsvRow {
                section: "event_outside_range",
                country: result.country,
                from_year: args.from,
                to_year: args.to,
                year: Some(event.year),
                month: Some(event.month),
                event: Some(event.label),
                reference: Some(event.reference),
                ..CsvRow::default()
            })?;
        }
        writer.serialize(alarm_csv(
            result.country,
            result.raw_alarms,
            result.harmonized_alarms,
            result.raw_frp_alarms,
            result.harmonized_frp_alarms,
            args,
        ))?;
    }
    writer.serialize(alarm_csv("ALL_POOLED", raw, harm, raw_frp, harm_frp, args))?;
    writer.flush()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    let cache_dir = std::env::var_os("ARCHIVE_CACHE_DIR")
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR), PathBuf::from);
    let archive = ArchiveClient::offline(cache_dir);
    let config = PipelineConfig::default();
    println!("Archive cache: {}", archive.cache_dir().display());
    println!(
        "Countries: {} | {}..={}",
        args.countries.join(", "),
        args.from,
        args.to
    );
    println!(
        "Same-calendar-month leave-one-out population z; rank 1 = highest (ties share rank).\nRecall per metric (count and FRP): harmonized rank <= 3 OR z >= 2. Undefined z/rank = NA."
    );
    let mut results = Vec::new();
    let mut raw = Alarms::default();
    let mut harm = Alarms::default();
    let mut raw_frp = Alarms::default();
    let mut harm_frp = Alarms::default();
    for &country in &args.countries {
        let result = analyze_country(&archive, &config, country, &args).await?;
        print_events(&result);
        raw.flagged += result.raw_alarms.flagged;
        raw.evaluated += result.raw_alarms.evaluated;
        harm.flagged += result.harmonized_alarms.flagged;
        harm.evaluated += result.harmonized_alarms.evaluated;
        raw_frp.flagged += result.raw_frp_alarms.flagged;
        raw_frp.evaluated += result.raw_frp_alarms.evaluated;
        harm_frp.flagged += result.harmonized_frp_alarms.flagged;
        harm_frp.evaluated += result.harmonized_frp_alarms.evaluated;
        results.push(result);
    }
    println!(
        "\n[False-alarm proxy: z >= 2 over {}..={}; includes real fire months]",
        args.from.max(2012),
        args.to
    );
    println!(
        "{:<20} {:>15} {:>11} {:>15} {:>12} {:>15} {:>9} {:>15} {:>10}",
        "Country",
        "Count raw n/N",
        "Count raw %",
        "Count harm n/N",
        "Count harm %",
        "FRP raw n/N",
        "FRP raw %",
        "FRP harm n/N",
        "FRP harm %"
    );
    for result in &results {
        print_alarms(
            result.country,
            result.raw_alarms,
            result.harmonized_alarms,
            result.raw_frp_alarms,
            result.harmonized_frp_alarms,
        );
    }
    print_alarms("ALL_POOLED", raw, harm, raw_frp, harm_frp);
    println!(
        "Denominators exclude undefined z scores; pooled shares use summed counts, not country averages."
    );
    if let Some(path) = &args.out {
        write_csv(path, &results, raw, harm, raw_frp, harm_frp, &args)?;
        println!("\nCSV written: {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_out_extreme_does_not_inflate_its_reference_variance() {
        let values = [(2003, 1.0), (2004, 2.0), (2005, 3.0), (2006, 20.0)];
        let extreme = score(2006, &values);
        assert!((extreme.z.unwrap() - 18.0 / (2.0_f64 / 3.0).sqrt()).abs() < 1e-12);
        assert_eq!(extreme.rank, Some(1));
        let tied = score(2005, &[(2003, 2.0), (2004, 3.0), (2005, 3.0), (2006, 9.0)]);
        assert_eq!(tied.rank, Some(2));
    }

    #[test]
    fn undefined_scores_are_excluded_from_alarm_denominators() {
        let mut alarms = Alarms::default();
        alarms.include(score(2005, &[(2003, 0.0), (2004, 0.0), (2005, 9.0)]));
        alarms.include(score(2005, &[(2003, 1.0), (2004, 2.0), (2005, 9.0)]));
        alarms.include(score(2005, &[(2003, 1.0), (2004, 2.0)]));
        assert_eq!(
            (alarms.flagged, alarms.evaluated, alarms.share()),
            (1, 1, Some(1.0))
        );
        assert!(score(2003, &[(2003, 1.0)]).z.is_none());
    }
}
