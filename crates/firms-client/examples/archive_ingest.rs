use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use pyrosync_core::calendar::build_calendar;
use pyrosync_core::effort::estimate_effort;
use pyrosync_core::harmonize::harmonize;
use pyrosync_core::models::{
    BBox, CalendarCell, HarmonizedHotspot, Hotspot, HotspotSource, Sensor,
};
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
    countries: Vec<String>,
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
            countries: COUNTRIES.iter().map(|s| (*s).to_string()).collect(),
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
  --countries <LIST>   Comma-separated countries (default: all 11; valid: Turkey, Greece, Bulgaria, Georgia, Armenia, Azerbaijan, Iran, Iraq, Syria, Cyprus, Russian_Federation)
  --all-countries      Not implemented in this example; use the all_countries zip ingest
  -h, --help           Print this help message"
    );
}

fn parse_countries(raw: &str) -> Result<Vec<String>> {
    let mut countries = Vec::new();
    for item in raw.split(',') {
        let name = item.trim();
        if !COUNTRIES.contains(&name) {
            bail!(
                "unknown country {name:?}; valid countries: {}",
                COUNTRIES.join(", ")
            );
        }
        if !countries.iter().any(|existing| existing == name) {
            countries.push(name.to_string());
        }
    }
    if countries.is_empty() {
        bail!(
            "--countries requires at least one country; valid countries: {}",
            COUNTRIES.join(", ")
        );
    }
    Ok(countries)
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
            "--countries" => {
                let val = next_arg(&mut args, "--countries")?;
                cfg.countries = parse_countries(&val)?;
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

struct AnnualRollup {
    year: i32,
    raw_total: u64,
    norm_total: f64,
    norm_months: usize,
    total_months: usize,
    omega_labels: Vec<String>,
}

fn rollup_calendar_by_year(calendar: &[CalendarCell]) -> Vec<AnnualRollup> {
    let mut by_year: BTreeMap<i32, Vec<&CalendarCell>> = BTreeMap::new();
    for cell in calendar {
        by_year.entry(cell.year).or_default().push(cell);
    }

    by_year
        .into_iter()
        .map(|(year, mut cells)| {
            cells.sort_by_key(|c| c.month);
            let mut raw_total = 0u64;
            let mut norm_total = 0.0f64;
            let mut norm_months = 0usize;
            let mut omega_labels = Vec::new();

            for cell in &cells {
                raw_total = raw_total.saturating_add(cell.count);
                if let Some(norm) = cell.count_normalized
                    && norm.is_finite()
                {
                    norm_total += norm;
                    norm_months += 1;
                }
                let omega_label = match cell.effort_omega {
                    Some(w) if w.is_finite() => format!("{w:.3}"),
                    _ => "-".to_string(),
                };
                if !omega_labels.contains(&omega_label) {
                    omega_labels.push(omega_label);
                }
            }

            AnnualRollup {
                year,
                raw_total,
                norm_total,
                norm_months,
                total_months: cells.len(),
                omega_labels,
            }
        })
        .collect()
}

fn print_effort_factors(hotspots: &[HarmonizedHotspot]) {
    println!("\nPlatform konfigurasyonlari (effort):");
    let factors = estimate_effort(hotspots);
    if factors.is_empty() {
        println!("  (veri yok)");
        return;
    }

    println!(
        "{:<42} | {:>9} | {:>10} | {:>13} | omega",
        "platformlar", "ay_sayisi", "toplam", "modis_gorunur"
    );
    for f in &factors {
        let platforms = if f.platforms.is_empty() {
            "-".to_string()
        } else {
            f.platforms
                .iter()
                .map(|s| format!("{s:?}"))
                .collect::<Vec<_>>()
                .join("+")
        };
        let omega_str = match f.omega {
            Some(w) => format!("{w:.3}"),
            None => {
                let reason = match (f.months < 3, f.modis_observable == 0) {
                    (true, true) => "az ay, MODIS yok",
                    (true, false) => "az ay",
                    (false, true) => "MODIS yok",
                    (false, false) => "gecersiz oran",
                };
                format!("- ({reason})")
            }
        };
        println!(
            "{:<42} | {:>9} | {:>10} | {:>13} | {}",
            platforms, f.months, f.total, f.modis_observable, omega_str
        );
    }
}

fn print_annual_summary(calendar: &[CalendarCell]) {
    println!("\nYillik ozet:");
    let rollups = rollup_calendar_by_year(calendar);
    if rollups.is_empty() {
        println!("  (takvim bos)");
        return;
    }

    println!(
        "{:<6} | {:>12} | {:>20} | {:<18} | {:>13}",
        "YIL", "ham_toplam", "normalize_toplam", "omega", "normalize/ham"
    );
    let mut skipped_any = false;
    for r in &rollups {
        let skipped = r.total_months.saturating_sub(r.norm_months);
        if skipped > 0 {
            skipped_any = true;
        }
        let norm_str = if r.norm_months == 0 {
            "- (None atlandi)".to_string()
        } else if skipped > 0 {
            format!("{:.1} ({}/{} ay)", r.norm_total, r.norm_months, r.total_months)
        } else {
            format!("{:.1}", r.norm_total)
        };
        let omega_str = if r.omega_labels.is_empty() {
            "-".to_string()
        } else {
            r.omega_labels.join(", ")
        };
        let ratio_str = if r.raw_total > 0 && r.norm_months > 0 {
            format!("{:.3}", r.norm_total / r.raw_total as f64)
        } else {
            "-".to_string()
        };
        println!(
            "{:<6} | {:>12} | {:>20} | {:<18} | {:>13}",
            r.year, r.raw_total, norm_str, omega_str, ratio_str
        );
    }
    if skipped_any {
        println!("Not: count_normalized degeri None olan aylar normalize_toplam hesabinda atlandi.");
    }
}

struct TransitionYear {
    year: i32,
    raw_total: u64,
    norm_total: f64,
    norm_months: usize,
    distinct_omegas: Vec<f64>,
}

struct OmegaTransition {
    year: i32,
    before_end: usize,
    after_start: usize,
}

fn collect_transition_years(calendar: &[CalendarCell]) -> (Vec<(i32, usize)>, Vec<TransitionYear>) {
    let mut by_year: BTreeMap<i32, Vec<&CalendarCell>> = BTreeMap::new();
    for cell in calendar {
        by_year.entry(cell.year).or_default().push(cell);
    }

    let mut partial_years = Vec::new();
    let mut full_years = Vec::new();

    for (year, mut cells) in by_year {
        cells.sort_by_key(|c| c.month);
        let valid_months: BTreeSet<u32> = cells
            .iter()
            .map(|c| c.month)
            .filter(|m| (1..=12).contains(m))
            .collect();

        // Eksik aylara sahip yillar (ornegin MODIS'in basladigi 2000 yili yalnizca Kasim-Aralik)
        // yillik toplamlari yapay dusurup sahte sicrama urettigi icin tam yil disindakiler cikarilir.
        if valid_months.len() < 12 {
            partial_years.push((year, valid_months.len()));
            continue;
        }

        let mut raw_total = 0u64;
        let mut norm_total = 0.0f64;
        let mut norm_months = 0usize;
        let mut distinct_omegas: Vec<f64> = Vec::new();

        for cell in cells {
            raw_total = raw_total.saturating_add(cell.count);
            if let Some(norm) = cell.count_normalized
                && norm.is_finite()
            {
                norm_total += norm;
                norm_months += 1;
            }
            if let Some(omega) = cell.effort_omega
                && omega.is_finite()
                && !distinct_omegas
                    .iter()
                    .any(|&existing| (existing - omega).abs() <= 1e-9)
            {
                distinct_omegas.push(omega);
            }
        }

        full_years.push(TransitionYear {
            year,
            raw_total,
            norm_total,
            norm_months,
            distinct_omegas,
        });
    }

    (partial_years, full_years)
}

fn detect_omega_transitions(full_years: &[TransitionYear]) -> Vec<OmegaTransition> {
    let mut transitions = Vec::new();
    let mut prev_omega: Option<f64> = None;

    for (idx, yr) in full_years.iter().enumerate() {
        match yr.distinct_omegas.as_slice() {
            [] => {}
            [omega] => {
                if let Some(prev) = prev_omega
                    && (omega - prev).abs() > 1e-9
                {
                    transitions.push(OmegaTransition {
                        year: yr.year,
                        before_end: idx,
                        after_start: idx,
                    });
                }
                prev_omega = Some(*omega);
            }
            [.., last] => {
                // Yil ici gecislerde o yil eski ve yeni konfigurasyonu karistirdigi icin
                // oncesi/sonrasi pencerelerinin disinda tutulur.
                transitions.push(OmegaTransition {
                    year: yr.year,
                    before_end: idx,
                    after_start: idx + 1,
                });
                prev_omega = Some(*last);
            }
        }
    }

    transitions
}

fn average_ranks(values: &[f64]) -> Vec<f64> {
    let n = values.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| values[a].total_cmp(&values[b]));

    let mut ranks = vec![0.0f64; n];
    let mut start = 0usize;
    while start < n {
        let mut end = start + 1;
        while end < n && values[order[end]] == values[order[start]] {
            end += 1;
        }
        let avg_rank = (start + 1 + end) as f64 / 2.0;
        for &idx in &order[start..end] {
            ranks[idx] = avg_rank;
        }
        start = end;
    }
    ranks
}

fn spearman_rho(xs: &[f64], ys: &[f64]) -> Option<f64> {
    if xs.len() != ys.len() || xs.len() < 3 {
        return None;
    }
    if xs.iter().any(|v| !v.is_finite()) || ys.iter().any(|v| !v.is_finite()) {
        return None;
    }

    let rx = average_ranks(xs);
    let ry = average_ranks(ys);
    let n = rx.len() as f64;
    let mean_x = rx.iter().sum::<f64>() / n;
    let mean_y = ry.iter().sum::<f64>() / n;

    let mut num = 0.0f64;
    let mut den_x = 0.0f64;
    let mut den_y = 0.0f64;
    for (x, y) in rx.iter().zip(ry.iter()) {
        let dx = x - mean_x;
        let dy = y - mean_y;
        num += dx * dy;
        den_x += dx * dx;
        den_y += dy * dy;
    }

    let denom = (den_x * den_y).sqrt();
    if denom <= 0.0 || !denom.is_finite() {
        return None;
    }

    Some((num / denom).clamp(-1.0, 1.0))
}

fn format_shift(
    label: &str,
    before: f64,
    after: f64,
    shift: Option<f64>,
) -> String {
    match shift {
        Some(ratio) => format!("{label} once={before:.0} sonra={after:.0} kayma= x{ratio:.2}"),
        None => format!("{label} once={before:.0} sonra={after:.0} kayma= hesaplanamadi (sifir bolen)"),
    }
}

fn print_transition_check(calendar: &[CalendarCell]) {
    println!("\nSicrama kontrolu:");
    if calendar.is_empty() {
        println!("  (takvim bos)");
        return;
    }

    let (partial_years, full_years) = collect_transition_years(calendar);
    if !partial_years.is_empty() {
        let listed: Vec<String> = partial_years
            .iter()
            .map(|(y, months)| format!("{y} ({months} ay)"))
            .collect();
        println!(
            "  Kismi yillar analiz disi birakildi (12 aydan az veri, seviye karsilastirmasini bozar): {}",
            listed.join(", ")
        );
    }

    if full_years.is_empty() {
        println!("  (analiz icin tam yil yok; tum yillar 12 aydan az veri iceriyor)");
        return;
    }

    let transitions = detect_omega_transitions(&full_years);
    if transitions.is_empty() {
        println!("  veride uydu konfigurasyonu degisimi yok");
        return;
    }

    // Tek yillik sicramalar gercek yangin yili dalgalanmasini yansittigi icin gecis noktalarindaki
    // cok yilli seviye kaymasi ve seri genelindeki monoton trend olculur.
    for t in &transitions {
        let before = &full_years[t.before_end.saturating_sub(5)..t.before_end];
        let after_end = (t.after_start + 5).min(full_years.len());
        let after = full_years.get(t.after_start..after_end).unwrap_or(&[]);

        if before.len() < 2 || after.len() < 2 {
            println!(
                "  Gecis {}: atlandi (iki yanda en az 2 tam yil gerekli; once={}, sonra={})",
                t.year,
                before.len(),
                after.len()
            );
            continue;
        }

        let raw_before =
            before.iter().map(|r| r.raw_total as f64).sum::<f64>() / before.len() as f64;
        let raw_after =
            after.iter().map(|r| r.raw_total as f64).sum::<f64>() / after.len() as f64;
        let raw_shift = (raw_before > 0.0 && raw_before.is_finite() && raw_after.is_finite())
            .then(|| raw_after / raw_before);

        let norm_before_vals: Vec<f64> = before
            .iter()
            .filter(|r| r.norm_months > 0)
            .map(|r| r.norm_total)
            .collect();
        let norm_after_vals: Vec<f64> = after
            .iter()
            .filter(|r| r.norm_months > 0)
            .map(|r| r.norm_total)
            .collect();

        let (norm_part, norm_shift) = if norm_before_vals.is_empty() || norm_after_vals.is_empty() {
            (
                "normalize hesaplanamadi (bir yanda normalize veri yok)".to_string(),
                None,
            )
        } else {
            let norm_before =
                norm_before_vals.iter().sum::<f64>() / norm_before_vals.len() as f64;
            let norm_after =
                norm_after_vals.iter().sum::<f64>() / norm_after_vals.len() as f64;
            let shift = (norm_before > 0.0 && norm_before.is_finite() && norm_after.is_finite())
                .then(|| norm_after / norm_before);
            (
                format_shift("normalize", norm_before, norm_after, shift),
                shift,
            )
        };

        let raw_part = format_shift("ham", raw_before, raw_after, raw_shift);
        println!("  Gecis {}: {} | {}", t.year, raw_part, norm_part);

        match (raw_shift, norm_shift) {
            (Some(rs), Some(ns)) if rs > 0.0 && ns > 0.0 && rs.is_finite() && ns.is_finite() => {
                let raw_log = rs.ln().abs();
                let norm_log = ns.ln().abs();
                let threshold = raw_log * 0.5;
                if norm_log < threshold {
                    println!(
                        "    Sonuc: Gecis basariyla duzeltildi (|log(norm)|={norm_log:.3} < |log(ham)|/2={threshold:.3})."
                    );
                } else {
                    println!(
                        "    Uyari: Gecis kaymasi yeterince duzeltilemedi (|log(norm)|={norm_log:.3} >= |log(ham)|/2={threshold:.3})."
                    );
                }
            }
            _ => {
                println!(
                    "    Uyari: Gecis degerlendirmesi yapilamadi (gecerli pozitif kayma orani yok)."
                );
            }
        }
    }

    if full_years.len() < 3 {
        println!(
            "  Monoton artan trend (Spearman rho): hesaplanamadi (en az 3 tam yil gerekli, mevcut={}).",
            full_years.len()
        );
        return;
    }

    let first_year = full_years[0].year;
    let last_year = full_years[full_years.len() - 1].year;
    let years_all: Vec<f64> = full_years.iter().map(|r| r.year as f64).collect();
    let raw_vals: Vec<f64> = full_years.iter().map(|r| r.raw_total as f64).collect();
    let raw_rho = spearman_rho(&years_all, &raw_vals);

    let norm_pairs: Vec<(f64, f64)> = full_years
        .iter()
        .filter(|r| r.norm_months > 0)
        .map(|r| (r.year as f64, r.norm_total))
        .collect();
    let norm_rho = if norm_pairs.len() >= 3 {
        let ny: Vec<f64> = norm_pairs.iter().map(|&(y, _)| y).collect();
        let nv: Vec<f64> = norm_pairs.iter().map(|&(_, v)| v).collect();
        spearman_rho(&ny, &nv)
    } else {
        None
    };

    let raw_str = match raw_rho {
        Some(r) => format!("{r:+.3}"),
        None => "hesaplanamadi".to_string(),
    };
    let norm_str = match norm_rho {
        Some(r) => format!("{r:+.3}"),
        None => "hesaplanamadi".to_string(),
    };
    println!(
        "  Monoton artan trend (Spearman rho, {first_year}-{last_year}): ham={raw_str}  normalize={norm_str}"
    );

    match (raw_rho, norm_rho) {
        (Some(rr), Some(nr)) => {
            let threshold = rr.abs() * 0.5;
            if nr.abs() < threshold {
                println!(
                    "    Sonuc: Yapay trend kalkti (|normalize rho|={:.3} < |ham rho|/2={threshold:.3}).",
                    nr.abs()
                );
            } else {
                println!(
                    "    Uyari: Normalize serideki trend yeterince azalmadi (|normalize rho|={:.3} >= |ham rho|/2={threshold:.3}).",
                    nr.abs()
                );
            }
        }
        _ => {
            println!("    Uyari: Trend karsilastirmasi yapilamadi (yeterli farkli/normalize veri yok).");
        }
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

    let started = Instant::now();

    let client = ArchiveClient::new(cfg.cache_dir.clone())?;
    let modis = ingest_sensor(&client, Sensor::ModisTerra, &cfg.countries, &cfg).await;
    let viirs = ingest_sensor(&client, Sensor::ViirsSNPP, &cfg.countries, &cfg).await;

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
    print_effort_factors(&harmonized);
    print_annual_summary(&calendar);
    print_transition_check(&calendar);

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
