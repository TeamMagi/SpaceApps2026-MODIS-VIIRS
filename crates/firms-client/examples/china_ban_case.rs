//! C11 case study: the 2013-2018 Chinese straw-burning ban seen through raw FIRMS rows and the
//! PyroSync harmonised series (local FIRMS country archive, read-only).
//!
//! For China 2010-2022, nationally and inside a cropland belt (NE China + North China Plain),
//! it writes per month and per year the raw count / FRP of each sensor, the `/api/series`
//! harmonised count and the `/api/calendar` fields (`count_normalized`, `total_frp` with its
//! bounds), plus a size-class proxy and the seasonal shift, then draws two slide-ready SVGs.
//!
//! Usage: cargo run --release -p pyrosync-firms --example china_ban_case [out-dir]
//! (default out-dir `docs/vaka`; archive from ARCHIVE_CACHE_DIR or `.cache/firms-archive`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Datelike;
use pyrosync_core::calendar::{build_calendar_with, compute_anomalies};
use pyrosync_core::config::PipelineConfig;
use pyrosync_core::harmonize::harmonize_with_links_config;
use pyrosync_core::models::{CalendarCell, HarmonizedHotspot, Hotspot, HotspotSource, Sensor};
use pyrosync_firms::parse::parse_csv;
use pyrosync_firms::{FirmsSource, Processing};

const DEFAULT_ARCHIVE_CACHE_DIR: &str = ".cache/firms-archive";
const COUNTRY: &str = "China";
const FROM: i32 = 2010;
const TO: i32 = 2022;
const BAN: (i32, i32) = (2013, 2018);

/// Effort Ω is pooled per platform configuration over the whole requested series. The archive
/// changes configuration at S-NPP's first China row (2012-01-20) and NOAA-20's (2018-04-01), so
/// running the calendar once per era yields the same Ω as one 2010-2022 `/api/calendar` call
/// while holding only one era of harmonised rows in memory. `main` checks the assumption.
const ERAS: [((i32, u32), (i32, u32)); 3] = [((2010, 1), (2011, 12)), ((2012, 1), (2018, 3)), ((2018, 4), (2022, 12))];

/// [west, south, east, north]
type Bbox = [f64; 4];

struct Region {
    id: &'static str,
    label: &'static str,
    boxes: &'static [Bbox],
}

const REGIONS: [Region; 2] = [
    Region { id: "national", label: "China (national)", boxes: &[] },
    Region {
        id: "belt",
        label: "NE China + North China Plain",
        // NE: Heilongjiang, Jilin, Liaoning, eastern Inner Mongolia; NCP: Hebei, Beijing/Tianjin,
        // Shandong, Henan east of 113°E, northern Anhui/Jiangsu.
        boxes: &[[119.0, 40.0, 135.0, 54.0], [113.0, 32.0, 122.0, 40.0]],
    },
];

impl Region {
    fn contains(&self, h: &Hotspot) -> bool {
        self.boxes.is_empty()
            || self
                .boxes
                .iter()
                .any(|b| h.lon >= b[0] && h.lat >= b[1] && h.lon <= b[2] && h.lat <= b[3])
    }
}

/// Size-class proxy: biomass-equivalent of one detection, FRP x TAU_S x 0.368 kg/MJ
/// (Wooster et al. 2005 combustion factor), i.e. as if the pixel burned at the observed FRP for
/// one hour. The literature classes <2500 kg / >4000 kg map to these FRP limits.
const TAU_S: f64 = 3600.0;
const KG_PER_MJ: f64 = 0.368;
const SMALL_KG: f64 = 2500.0;
const LARGE_KG: f64 = 4000.0;

fn kg_to_mw(kg: f64) -> f64 {
    kg / KG_PER_MJ / TAU_S
}

const SENSOR_FILES: [(&str, Sensor); 3] = [
    ("modis", Sensor::ModisTerra),
    ("viirs-snpp", Sensor::ViirsSNPP),
    ("viirs-jpss1", Sensor::ViirsNOAA20),
];

fn archive_dir() -> PathBuf {
    match std::env::var_os("ARCHIVE_CACHE_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from(DEFAULT_ARCHIVE_CACHE_DIR),
    }
}

/// Type-0 rows of one sensor file, as `/api/series` and `/api/calendar` keep them.
fn load(dir: &Path, sensor_dir: &str, sensor: Sensor, year: i32) -> Result<Vec<Hotspot>> {
    let path = dir
        .join(sensor_dir)
        .join(year.to_string())
        .join(format!("{sensor_dir}_{year}_{COUNTRY}.csv"));
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
    let mut rows = parse_csv(&text, FirmsSource::new(sensor, Processing::Sp).as_str())
        .with_context(|| path.display().to_string())?;
    rows.retain(|h| h.fire_type.is_wildfire());
    Ok(rows)
}

/// Index 0 = MODIS (Terra+Aqua), 1 = S-NPP, 2 = NOAA-20.
#[derive(Default, Clone, Copy)]
struct RawMonth {
    count: [u64; 3],
    frp: [f64; 3],
}

#[derive(Default)]
struct YearPipe {
    harmonized: usize,
    matched: usize,
    modis_only: usize,
    viirs_only: usize,
    static_masked: usize,
    interscan_removed: usize,
    fleet_merged: usize,
}

/// Per-detection FRP of the sensor-consistent streams, for the size classes.
#[derive(Default)]
struct SizeYear {
    modis: Vec<f32>,
    snpp: Vec<f32>,
}

#[derive(Default)]
struct RegionResult {
    raw: BTreeMap<(i32, u32), RawMonth>,
    pipe: BTreeMap<i32, YearPipe>,
    size: BTreeMap<i32, SizeYear>,
    cells: Vec<CalendarCell>,
    omegas: Vec<(String, Option<f64>)>,
}

fn in_era(h: &HarmonizedHotspot, era: ((i32, u32), (i32, u32))) -> bool {
    let ym = (h.acquired.year(), h.acquired.month());
    ym >= era.0 && ym <= era.1
}

fn platform_key(h: &HarmonizedHotspot) -> (bool, BTreeSet<Sensor>) {
    (
        h.platforms.iter().any(|s| s.is_modis()),
        h.platforms.iter().copied().filter(|s| s.is_viirs()).collect(),
    )
}

fn run_region(dir: &Path, region: &Region, config: &PipelineConfig) -> Result<RegionResult> {
    let mut out = RegionResult::default();
    for era in ERAS {
        let mut harmonized: Vec<HarmonizedHotspot> = Vec::new();
        for year in era.0.0..=era.1.0 {
            let [modis, snpp, noaa20] = SENSOR_FILES.map(|(d, s)| {
                load(dir, d, s, year).map(|rows| rows.into_iter().filter(|h| region.contains(h)).collect::<Vec<_>>())
            });
            let (modis, snpp, noaa20) = (modis?, snpp?, noaa20?);
            let first_visit = !out.pipe.contains_key(&year);
            if first_visit {
                for (i, rows) in [&modis, &snpp, &noaa20].into_iter().enumerate() {
                    for h in rows {
                        let m = out.raw.entry((year, h.acquired.month())).or_default();
                        m.count[i] += 1;
                        m.frp[i] += h.frp;
                    }
                }
                let size = out.size.entry(year).or_default();
                size.modis = modis.iter().map(|h| h.frp as f32).collect();
                size.snpp = snpp.iter().map(|h| h.frp as f32).collect();
            }
            let mut viirs = snpp;
            viirs.extend(noaa20);
            // Whole calendar year per call, so the static mask sees the same cell-years as the app.
            let links = harmonize_with_links_config(modis, viirs, config);
            if first_visit {
                let by = |s: HotspotSource| links.hotspots.iter().filter(|h| h.source == s).count();
                out.pipe.insert(
                    year,
                    YearPipe {
                        harmonized: links.hotspots.len(),
                        matched: by(HotspotSource::Matched),
                        modis_only: by(HotspotSource::ModisOnly),
                        viirs_only: by(HotspotSource::ViirsOnly),
                        static_masked: links.static_masked,
                        interscan_removed: links.interscan_removed,
                        fleet_merged: links.fleet_merged,
                    },
                );
            }
            harmonized.extend(links.hotspots.into_iter().filter(|h| in_era(h, era)));
        }
        let mut month_keys: BTreeMap<(i32, u32), (bool, BTreeSet<Sensor>)> = BTreeMap::new();
        for h in &harmonized {
            let (m, v) = platform_key(h);
            let k = month_keys.entry((h.acquired.year(), h.acquired.month())).or_default();
            k.0 |= m;
            k.1.extend(v);
        }
        let keys: BTreeSet<_> = month_keys.values().cloned().collect();
        if keys.len() != 1 {
            eprintln!(
                "warning: {} era {:?}-{:?} mixes {} platform configurations; Ω differs from a single /api/calendar call",
                region.id,
                era.0,
                era.1,
                keys.len()
            );
        }
        let cells = build_calendar_with(&harmonized, &config.uncertainty, &config.calibration);
        let omega = cells.iter().find_map(|c| c.effort_omega);
        out.omegas.push((format!("{}-{:02}..{}-{:02}", era.0.0, era.0.1, era.1.0, era.1.1), omega));
        out.cells.extend(cells);
    }
    compute_anomalies(&mut out.cells);
    Ok(out)
}

// ---------- derived metrics ----------

#[derive(Default, Clone, Copy)]
struct Annual {
    year: i32,
    raw_count: [u64; 3],
    raw_frp: [f64; 3],
    harmonized: usize,
    count_normalized: f64,
    total_frp: f64,
    total_frp_lower: f64,
    total_frp_upper: f64,
    /// Share of the year's harmonised `total_frp` in Jan-May + Dec.
    winter_spring_share: f64,
    /// Same share on raw MODIS FRP (sensor-consistent reference).
    winter_spring_share_modis: f64,
    pcd: f64,
    pcp_month: f64,
    pcd_modis: f64,
    pcp_month_modis: f64,
    /// MODIS and S-NPP detections per kg-equivalent class: [small, mid, large].
    kg_class_modis: [u64; 3],
    kg_class_snpp: [u64; 3],
    /// Same, classes from the pre-ban baseline terciles of each sensor.
    tercile_class_modis: [u64; 3],
    tercile_class_snpp: [u64; 3],
}

fn winter_spring(month: u32) -> bool {
    month <= 5 || month == 12
}

/// Precipitation-concentration-style circular statistics on 12 monthly totals ([30]):
/// PCD = |Σ r·e^{iθ}| / Σ r in [0, 1]; PCP returned as the fractional month (1.0 = mid-January).
fn pcd_pcp(monthly: &[f64; 12]) -> (f64, f64) {
    let total: f64 = monthly.iter().sum();
    if total <= 0.0 {
        return (f64::NAN, f64::NAN);
    }
    let (mut x, mut y) = (0.0, 0.0);
    for (i, r) in monthly.iter().enumerate() {
        let theta = 2.0 * std::f64::consts::PI * (i as f64 + 0.5) / 12.0;
        x += r * theta.cos();
        y += r * theta.sin();
    }
    let angle = y.atan2(x).rem_euclid(2.0 * std::f64::consts::PI);
    ((x * x + y * y).sqrt() / total, angle * 12.0 / (2.0 * std::f64::consts::PI) + 0.5)
}

fn quantile(sorted: &[f32], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize] as f64
}

fn classify(values: &[f32], lo: f64, hi: f64) -> [u64; 3] {
    let mut c = [0u64; 3];
    for &v in values {
        let v = v as f64;
        c[if v < lo { 0 } else if v > hi { 2 } else { 1 }] += 1;
    }
    c
}

struct Baselines {
    modis: (f64, f64),
    snpp: (f64, f64),
}

/// Pre-ban terciles: MODIS 2010-2012, S-NPP 2012 (its only pre-ban year).
fn baselines(r: &RegionResult) -> Baselines {
    let terciles = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        (quantile(&v, 1.0 / 3.0), quantile(&v, 2.0 / 3.0))
    };
    let modis: Vec<f32> = (FROM..BAN.0)
        .filter_map(|y| r.size.get(&y))
        .flat_map(|s| s.modis.iter().copied())
        .collect();
    let snpp: Vec<f32> = r.size.get(&2012).map(|s| s.snpp.clone()).unwrap_or_default();
    Baselines { modis: terciles(modis), snpp: terciles(snpp) }
}

fn annual(r: &RegionResult, base: &Baselines) -> Vec<Annual> {
    let (small_mw, large_mw) = (kg_to_mw(SMALL_KG), kg_to_mw(LARGE_KG));
    (FROM..=TO)
        .map(|year| {
            let mut a = Annual { year, ..Default::default() };
            let mut harm_month = [0.0; 12];
            let mut modis_month = [0.0; 12];
            for ((y, m), raw) in r.raw.range((year, 1)..=(year, 12)) {
                debug_assert_eq!(*y, year);
                for i in 0..3 {
                    a.raw_count[i] += raw.count[i];
                    a.raw_frp[i] += raw.frp[i];
                }
                modis_month[*m as usize - 1] = raw.frp[0];
            }
            for c in r.cells.iter().filter(|c| c.year == year) {
                a.count_normalized += c.count_normalized.unwrap_or(0.0);
                a.total_frp += c.total_frp;
                a.total_frp_lower += c.total_frp_lower.unwrap_or(c.total_frp);
                a.total_frp_upper += c.total_frp_upper.unwrap_or(c.total_frp);
                harm_month[c.month as usize - 1] = c.total_frp;
            }
            a.harmonized = r.pipe.get(&year).map_or(0, |p| p.harmonized);
            let share = |v: &[f64; 12]| {
                let t: f64 = v.iter().sum();
                let ws: f64 = (1..=12u32).filter(|&m| winter_spring(m)).map(|m| v[m as usize - 1]).sum();
                if t > 0.0 { ws / t } else { f64::NAN }
            };
            a.winter_spring_share = share(&harm_month);
            a.winter_spring_share_modis = share(&modis_month);
            (a.pcd, a.pcp_month) = pcd_pcp(&harm_month);
            (a.pcd_modis, a.pcp_month_modis) = pcd_pcp(&modis_month);
            if let Some(s) = r.size.get(&year) {
                a.kg_class_modis = classify(&s.modis, small_mw, large_mw);
                a.kg_class_snpp = classify(&s.snpp, small_mw, large_mw);
                a.tercile_class_modis = classify(&s.modis, base.modis.0, base.modis.1);
                a.tercile_class_snpp = classify(&s.snpp, base.snpp.0, base.snpp.1);
            }
            a
        })
        .collect()
}

/// Picks one [small, mid, large] class count of a year.
type ClassOf = fn(&Annual) -> [u64; 3];

fn change(from: f64, to: f64) -> f64 {
    if from > 0.0 { (to - from) / from * 100.0 } else { f64::NAN }
}

fn share(c: [u64; 3], i: usize) -> f64 {
    let t: u64 = c.iter().sum();
    if t == 0 { f64::NAN } else { 100.0 * c[i] as f64 / t as f64 }
}

fn mean_of(a: &[Annual], years: std::ops::RangeInclusive<i32>, f: impl Fn(&Annual) -> f64) -> f64 {
    let v: Vec<f64> = a.iter().filter(|x| years.contains(&x.year)).map(f).filter(|x| x.is_finite()).collect();
    v.iter().sum::<f64>() / v.len() as f64
}

// ---------- output ----------

fn write_monthly(path: &Path, results: &[(&Region, RegionResult)]) -> Result<()> {
    let mut s = String::from(
        "region,year,month,raw_modis_count,raw_modis_frp_mw,raw_snpp_count,raw_snpp_frp_mw,raw_noaa20_count,raw_noaa20_frp_mw,raw_total_count,raw_total_frp_mw,harmonized_count,effort_omega,count_normalized,total_frp_mw,total_frp_lower_mw,total_frp_upper_mw,frp_per_detection_eq_mw,anomaly\n",
    );
    let opt = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.4}"));
    for (region, r) in results {
        for c in &r.cells {
            let raw = r.raw.get(&(c.year, c.month)).copied().unwrap_or_default();
            writeln!(
                s,
                "{},{},{},{},{:.2},{},{:.2},{},{:.2},{},{:.2},{},{},{},{:.2},{},{},{},{}",
                region.id,
                c.year,
                c.month,
                raw.count[0],
                raw.frp[0],
                raw.count[1],
                raw.frp[1],
                raw.count[2],
                raw.frp[2],
                raw.count.iter().sum::<u64>(),
                raw.frp.iter().sum::<f64>(),
                c.count,
                opt(c.effort_omega),
                opt(c.count_normalized),
                c.total_frp,
                opt(c.total_frp_lower),
                opt(c.total_frp_upper),
                opt(c.frp_per_detection_eq),
                opt(c.anomaly),
            )?;
        }
    }
    std::fs::write(path, s).with_context(|| path.display().to_string())
}

fn write_annual(path: &Path, rows: &[(&Region, Vec<Annual>, &RegionResult)]) -> Result<()> {
    let mut s = String::from(
        "region,year,raw_modis_count,raw_snpp_count,raw_noaa20_count,raw_total_count,raw_modis_frp_mw,raw_snpp_frp_mw,raw_noaa20_frp_mw,raw_total_frp_mw,harmonized_count,matched,modis_only,viirs_only,static_masked,interscan_removed,fleet_merged,count_normalized,total_frp_mw,total_frp_lower_mw,total_frp_upper_mw,winter_spring_frp_share,winter_spring_frp_share_raw_modis,pcd,pcp_month,pcd_raw_modis,pcp_month_raw_modis,snpp_kg_small,snpp_kg_mid,snpp_kg_large,modis_kg_small,modis_kg_mid,modis_kg_large,snpp_tercile_small,snpp_tercile_mid,snpp_tercile_large,modis_tercile_small,modis_tercile_mid,modis_tercile_large\n",
    );
    for (region, annual, r) in rows {
        for a in annual {
            let p = r.pipe.get(&a.year);
            let pv = |f: fn(&YearPipe) -> usize| p.map_or(0, f);
            let c = |x: [u64; 3]| format!("{},{},{}", x[0], x[1], x[2]);
            writeln!(
                s,
                "{},{},{},{},{},{},{:.2},{:.2},{:.2},{:.2},{},{},{},{},{},{},{},{:.2},{:.2},{:.2},{:.2},{:.4},{:.4},{:.4},{:.3},{:.4},{:.3},{},{},{},{}",
                region.id,
                a.year,
                a.raw_count[0],
                a.raw_count[1],
                a.raw_count[2],
                a.raw_count.iter().sum::<u64>(),
                a.raw_frp[0],
                a.raw_frp[1],
                a.raw_frp[2],
                a.raw_frp.iter().sum::<f64>(),
                a.harmonized,
                pv(|p| p.matched),
                pv(|p| p.modis_only),
                pv(|p| p.viirs_only),
                pv(|p| p.static_masked),
                pv(|p| p.interscan_removed),
                pv(|p| p.fleet_merged),
                a.count_normalized,
                a.total_frp,
                a.total_frp_lower,
                a.total_frp_upper,
                a.winter_spring_share,
                a.winter_spring_share_modis,
                a.pcd,
                a.pcp_month,
                a.pcd_modis,
                a.pcp_month_modis,
                c(a.kg_class_snpp),
                c(a.kg_class_modis),
                c(a.tercile_class_snpp),
                c(a.tercile_class_modis),
            )?;
        }
    }
    std::fs::write(path, s).with_context(|| path.display().to_string())
}

fn json_num(v: f64) -> String {
    if v.is_finite() { format!("{v:.2}") } else { "null".into() }
}

struct Summary {
    json: String,
    text: String,
}

fn summarize(region: &Region, a: &[Annual], r: &RegionResult, base: &Baselines) -> Summary {
    let y = |year: i32| a.iter().find(|x| x.year == year).copied().unwrap_or_default();
    let raw_c = |x: &Annual| x.raw_count.iter().sum::<u64>() as f64;
    let raw_f = |x: &Annual| x.raw_frp.iter().sum::<f64>();
    let pairs: [(&str, i32, i32); 5] = [
        ("snpp_entry_2011_2012", 2011, 2012),
        ("noaa20_entry_2017_2018", 2017, 2018),
        ("ban_2012_2018", 2012, 2018),
        ("peak_2014_2018", 2014, 2018),
        ("pre_post_2011_2022", 2011, 2022),
    ];
    let mut json = format!("{{\"region\":\"{}\",\"label\":\"{}\",\"changes_pct\":{{", region.id, region.label);
    let mut text = format!("\n== {} ==\n  change %             raw_count raw_frp raw_modis_count | harm_count count_norm total_frp | modis_frp snpp_cnt snpp_frp\n", region.label);
    for (i, (key, from, to)) in pairs.iter().enumerate() {
        let (f, t) = (y(*from), y(*to));
        let v = [
            change(raw_c(&f), raw_c(&t)),
            change(raw_f(&f), raw_f(&t)),
            change(f.raw_count[0] as f64, t.raw_count[0] as f64),
            change(f.harmonized as f64, t.harmonized as f64),
            change(f.count_normalized, t.count_normalized),
            change(f.total_frp, t.total_frp),
            change(f.raw_frp[0], t.raw_frp[0]),
            change(f.raw_count[1] as f64, t.raw_count[1] as f64),
            change(f.raw_frp[1], t.raw_frp[1]),
        ];
        if i > 0 {
            json.push(',');
        }
        write!(
            json,
            "\"{key}\":{{\"raw_count\":{},\"raw_frp\":{},\"raw_modis_count\":{},\"harmonized_count\":{},\"count_normalized\":{},\"total_frp\":{},\"raw_modis_frp\":{},\"raw_snpp_count\":{},\"raw_snpp_frp\":{}}}",
            json_num(v[0]), json_num(v[1]), json_num(v[2]), json_num(v[3]), json_num(v[4]), json_num(v[5]),
            json_num(v[6]), json_num(v[7]), json_num(v[8])
        )
        .ok();
        writeln!(
            text,
            "  {key:<22} {:>8.1} {:>7.1} {:>15.1} | {:>10.1} {:>10.1} {:>9.1} | {:>9.1} {:>8.1} {:>8.1}",
            v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8]
        )
        .ok();
    }
    json.push_str("},\"omega_by_era\":{");
    for (i, (era, omega)) in r.omegas.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        write!(json, "\"{era}\":{}", omega.map_or("null".into(), json_num)).ok();
        writeln!(text, "  Ω {era}: {}", omega.map_or("n/a".into(), |o| format!("{o:.3}"))).ok();
    }
    let ws = |years: std::ops::RangeInclusive<i32>| {
        (
            100.0 * mean_of(a, years.clone(), |x| x.winter_spring_share),
            100.0 * mean_of(a, years.clone(), |x| x.winter_spring_share_modis),
            mean_of(a, years.clone(), |x| x.pcd),
            mean_of(a, years, |x| x.pcd_modis),
        )
    };
    let periods = [("2010_2012", FROM..=2012), ("2013_2018", BAN.0..=BAN.1), ("2019_2022", 2019..=TO)];
    json.push_str("},\"season_mean\":{");
    for (i, (k, years)) in periods.iter().enumerate() {
        let (h, m, pcd, pcdm) = ws(years.clone());
        if i > 0 {
            json.push(',');
        }
        write!(
            json,
            "\"{k}\":{{\"winter_spring_frp_share_pct\":{},\"winter_spring_frp_share_raw_modis_pct\":{},\"pcd\":{},\"pcd_raw_modis\":{}}}",
            json_num(h), json_num(m), json_num(pcd), json_num(pcdm)
        )
        .ok();
        writeln!(
            text,
            "  season {k}: Dec-May FRP share harmonised {h:.1}% (raw MODIS {m:.1}%), PCD {pcd:.3} (raw MODIS {pcdm:.3})"
        )
        .ok();
    }
    let size = |years: std::ops::RangeInclusive<i32>, f: ClassOf| {
        let mut c = [0u64; 3];
        for x in a.iter().filter(|x| years.contains(&x.year)) {
            for (acc, v) in c.iter_mut().zip(f(x)) {
                *acc += v;
            }
        }
        c
    };
    json.push_str("},\"size_classes\":{");
    write!(
        json,
        "\"kg_proxy\":{{\"tau_s\":{TAU_S},\"kg_per_mj\":{KG_PER_MJ},\"small_below_mw\":{},\"large_above_mw\":{}}},\"tercile_mw\":{{\"modis\":[{},{}],\"snpp\":[{},{}]}}",
        json_num(kg_to_mw(SMALL_KG)),
        json_num(kg_to_mw(LARGE_KG)),
        json_num(base.modis.0),
        json_num(base.modis.1),
        json_num(base.snpp.0),
        json_num(base.snpp.1)
    )
    .ok();
    writeln!(
        text,
        "  size: kg proxy small < {:.2} MW, large > {:.2} MW; terciles MODIS {:.1}/{:.1} MW, S-NPP {:.2}/{:.2} MW",
        kg_to_mw(SMALL_KG),
        kg_to_mw(LARGE_KG),
        base.modis.0,
        base.modis.1,
        base.snpp.0,
        base.snpp.1
    )
    .ok();
    let classes: [(&str, ClassOf); 4] = [
        ("snpp_kg", |x| x.kg_class_snpp),
        ("modis_kg", |x| x.kg_class_modis),
        ("snpp_tercile", |x| x.tercile_class_snpp),
        ("modis_tercile", |x| x.tercile_class_modis),
    ];
    for (name, f) in classes {
        for (k, years) in [("2012", 2012..=2012), ("2013_2018", BAN.0..=BAN.1), ("2019_2022", 2019..=TO)] {
            let c = size(years.clone(), f);
            let n = years.clone().count() as f64;
            write!(
                json,
                ",\"{name}_{k}\":{{\"small_share_pct\":{},\"large_share_pct\":{},\"small_per_year\":{},\"large_per_year\":{}}}",
                json_num(share(c, 0)),
                json_num(share(c, 2)),
                json_num(c[0] as f64 / n),
                json_num(c[2] as f64 / n)
            )
            .ok();
            writeln!(
                text,
                "  {name:<14} {k:<9}: small {:>5.1}% ({:>8.0}/yr)  large {:>5.1}% ({:>8.0}/yr)",
                share(c, 0),
                c[0] as f64 / n,
                share(c, 2),
                c[2] as f64 / n
            )
            .ok();
        }
    }
    json.push_str("}}");
    Summary { json, text }
}

// ---------- SVG ----------

const BG: &str = "#0d0f13";
const TEXT: &str = "#f1f3f7";
const MUTED: &str = "#949cab";
const GRID: &str = "#222731";
const BAN_FILL: &str = "#c9b98f";
const RAW_MODIS: &str = "#d4d9e2";
const RAW_VIIRS: &str = "#8d96a6";
const HARMONISED: &str = "#3cc8f0";
const SIZE_SMALL: &str = "#f6e27a";
const SIZE_MID: &str = "#f37819";
const SIZE_LARGE: &str = "#bc3754";

#[derive(Clone, Copy)]
enum Marker {
    Circle,
    Square,
    Diamond,
    Triangle,
}

struct Line<'a> {
    color: &'a str,
    dash: Option<&'a str>,
    width: f64,
    marker: Marker,
    points: Vec<(f64, f64)>,
    band: Option<Vec<(f64, f64, f64)>>,
}

struct Panel<'a> {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    title: &'a str,
    log: bool,
    y_min: f64,
    y_max: f64,
    ticks: &'a [f64],
    tick_suffix: &'a str,
    x_from: i32,
    lines: Vec<Line<'a>>,
}

fn svg_open(w: f64, h: f64, title: &str, desc: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" font-family=\"Inter, 'Segoe UI', Helvetica, Arial, sans-serif\" role=\"img\" aria-labelledby=\"t d\">\n<title id=\"t\">{title}</title>\n<desc id=\"d\">{desc}</desc>\n<rect width=\"{w}\" height=\"{h}\" fill=\"{BG}\"/>\n"
    )
}

fn marker(s: &mut String, m: Marker, x: f64, y: f64, color: &str) {
    match m {
        Marker::Circle => write!(s, "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"4.5\" fill=\"{BG}\" stroke=\"{color}\" stroke-width=\"2\"/>"),
        Marker::Square => write!(s, "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"8\" height=\"8\" fill=\"{color}\"/>", x - 4.0, y - 4.0),
        Marker::Diamond => write!(
            s,
            "<path d=\"M{x:.1} {:.1}L{:.1} {y:.1}L{x:.1} {:.1}L{:.1} {y:.1}Z\" fill=\"{color}\"/>",
            y - 6.0,
            x + 6.0,
            y + 6.0,
            x - 6.0
        ),
        Marker::Triangle => write!(
            s,
            "<path d=\"M{x:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}Z\" fill=\"{color}\"/>",
            y - 6.0,
            x + 5.5,
            y + 4.5,
            x - 5.5,
            y + 4.5
        ),
    }
    .ok();
}

fn draw_panel(s: &mut String, p: &Panel) {
    let ymap = |v: f64| {
        let t = if p.log {
            (v.ln() - p.y_min.ln()) / (p.y_max.ln() - p.y_min.ln())
        } else {
            (v - p.y_min) / (p.y_max - p.y_min)
        };
        p.y + p.h - t.clamp(0.0, 1.0) * p.h
    };
    let years = (TO - p.x_from + 1) as f64;
    let xmap = |year: f64| p.x + (year - p.x_from as f64 + 0.5) / years * p.w;
    writeln!(s, "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{TEXT}\" font-size=\"20\" font-weight=\"600\">{}</text>", p.x, p.y - 16.0, p.title).ok();
    // Ban period shading spans whole years 2013-2018.
    let (bx0, bx1) = (xmap(BAN.0 as f64 - 0.5), xmap(BAN.1 as f64 + 0.5));
    write!(
        s,
        "<rect x=\"{bx0:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{BAN_FILL}\" fill-opacity=\"0.10\"/>\n<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{BAN_FILL}\" font-size=\"15\" text-anchor=\"middle\">straw-burning ban 2013–2018</text>\n",
        p.y,
        bx1 - bx0,
        p.h,
        (bx0 + bx1) / 2.0,
        p.y + 18.0
    )
    .ok();
    for &t in p.ticks {
        let ty = ymap(t);
        writeln!(
            s,
            "<line x1=\"{:.1}\" x2=\"{:.1}\" y1=\"{ty:.1}\" y2=\"{ty:.1}\" stroke=\"{GRID}\"/><text x=\"{:.1}\" y=\"{:.1}\" fill=\"{MUTED}\" font-size=\"14\" text-anchor=\"end\">{}{}</text>",
            p.x,
            p.x + p.w,
            p.x - 8.0,
            ty + 5.0,
            t,
            p.tick_suffix
        )
        .ok();
    }
    for year in p.x_from..=TO {
        if (year - p.x_from) % 2 == 0 || year == TO {
            writeln!(
                s,
                "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{MUTED}\" font-size=\"14\" text-anchor=\"middle\">{year}</text>",
                xmap(year as f64),
                p.y + p.h + 22.0
            )
            .ok();
        }
    }
    // Sensor entries: S-NPP from 2012-01-20, NOAA-20 from 2018-04-01 (fractional-year position).
    for (frac, label) in [(2012.05, "S-NPP enters"), (2018.25, "NOAA-20 enters")] {
        if frac < p.x_from as f64 {
            continue;
        }
        let mx = xmap(frac - 0.5);
        writeln!(
            s,
            "<line x1=\"{mx:.1}\" x2=\"{mx:.1}\" y1=\"{:.1}\" y2=\"{:.1}\" stroke=\"{HARMONISED}\" stroke-width=\"1.5\" stroke-dasharray=\"2 4\"/><text x=\"{:.1}\" y=\"{:.1}\" fill=\"{HARMONISED}\" font-size=\"14\">{label}</text>",
            p.y + 26.0,
            p.y + p.h,
            mx + 5.0,
            p.y + p.h - 10.0
        )
        .ok();
    }
    writeln!(s, "<line x1=\"{:.1}\" x2=\"{:.1}\" y1=\"{:.1}\" y2=\"{:.1}\" stroke=\"{MUTED}\"/>", p.x, p.x + p.w, p.y + p.h, p.y + p.h).ok();
    for line in &p.lines {
        if let Some(band) = &line.band {
            let mut d = String::new();
            for (i, (x, lo, _)) in band.iter().enumerate() {
                write!(d, "{}{:.1} {:.1}", if i == 0 { "M" } else { "L" }, xmap(*x), ymap(*lo)).ok();
            }
            for (x, _, hi) in band.iter().rev() {
                write!(d, "L{:.1} {:.1}", xmap(*x), ymap(*hi)).ok();
            }
            writeln!(s, "<path d=\"{d}Z\" fill=\"{}\" fill-opacity=\"0.22\"/>", line.color).ok();
        }
        let pts: Vec<String> = line
            .points
            .iter()
            .filter(|(_, v)| v.is_finite() && (!p.log || *v > 0.0))
            .map(|(x, v)| format!("{:.1},{:.1}", xmap(*x), ymap(*v)))
            .collect();
        writeln!(
            s,
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\"{}/>",
            pts.join(" "),
            line.color,
            line.width,
            line.dash.map_or(String::new(), |d| format!(" stroke-dasharray=\"{d}\""))
        )
        .ok();
        for (x, v) in &line.points {
            if v.is_finite() && (!p.log || *v > 0.0) {
                marker(s, line.marker, xmap(*x), ymap(*v), line.color);
            }
        }
        s.push('\n');
    }
}

fn legend(s: &mut String, x: f64, y: f64, items: &[(&str, &str, Option<&str>, f64, Marker)]) {
    let mut cx = x;
    for (label, color, dash, width, m) in items {
        write!(
            s,
            "<line x1=\"{cx:.1}\" x2=\"{:.1}\" y1=\"{y:.1}\" y2=\"{y:.1}\" stroke=\"{color}\" stroke-width=\"{width}\"{}/>",
            cx + 34.0,
            dash.map_or(String::new(), |d| format!(" stroke-dasharray=\"{d}\""))
        )
        .ok();
        marker(s, *m, cx + 17.0, y, color);
        let text = label.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        writeln!(s, "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{TEXT}\" font-size=\"16\">{text}</text>", cx + 42.0, y + 5.5).ok();
        cx += 50.0 + 8.6 * label.chars().count() as f64;
    }
}

fn series_chart(a: &[Annual]) -> String {
    let base = a.iter().find(|x| x.year == 2011).copied().unwrap_or_default();
    let idx = |v: f64, b: f64| if b > 0.0 { 100.0 * v / b } else { f64::NAN };
    let raw_c = |x: &Annual| x.raw_count.iter().sum::<u64>() as f64;
    let raw_f = |x: &Annual| x.raw_frp.iter().sum::<f64>();
    let pts = |f: &dyn Fn(&Annual) -> f64| a.iter().map(|x| (x.year as f64, f(x))).collect::<Vec<_>>();
    let ticks: &[f64] = &[25.0, 50.0, 100.0, 200.0, 400.0, 800.0];
    let panel = |x: f64, title, count: bool| Panel {
        x,
        y: 178.0,
        w: 500.0,
        h: 352.0,
        title,
        log: true,
        y_min: 22.0,
        y_max: 900.0,
        ticks,
        tick_suffix: "",
        x_from: FROM,
        lines: vec![
            Line {
                color: RAW_VIIRS,
                dash: None,
                width: 2.5,
                marker: Marker::Square,
                points: if count { pts(&|x| idx(raw_c(x), raw_c(&base))) } else { pts(&|x| idx(raw_f(x), raw_f(&base))) },
                band: None,
            },
            Line {
                color: RAW_MODIS,
                dash: Some("3 4"),
                width: 2.0,
                marker: Marker::Circle,
                points: if count {
                    pts(&|x| idx(x.raw_count[0] as f64, base.raw_count[0] as f64))
                } else {
                    pts(&|x| idx(x.raw_frp[0], base.raw_frp[0]))
                },
                band: None,
            },
            Line {
                color: HARMONISED,
                dash: None,
                width: 3.5,
                marker: Marker::Diamond,
                points: if count {
                    pts(&|x| idx(x.count_normalized, base.count_normalized))
                } else {
                    pts(&|x| idx(x.total_frp, base.total_frp))
                },
                band: (!count).then(|| {
                    a.iter()
                        .map(|x| (x.year as f64, idx(x.total_frp_lower, base.total_frp), idx(x.total_frp_upper, base.total_frp)))
                        .collect()
                }),
            },
        ],
    };
    let mut s = svg_open(
        1200.0,
        640.0,
        "China 2010–2022: raw FIRMS vs PyroSync harmonised",
        "Annual detections and total FRP for China, indexed to 2011 on a log axis. Raw counts and raw FRP jump when S-NPP (2012) and NOAA-20 (2018) enter; the harmonised series stays in MODIS-equivalent units across both entries.",
    );
    write!(s, "<text x=\"70\" y=\"48\" fill=\"{TEXT}\" font-size=\"26\" font-weight=\"700\">China 2010–2022: raw FIRMS vs PyroSync harmonised</text>\n<text x=\"70\" y=\"78\" fill=\"{MUTED}\" font-size=\"16\">Annual totals, index 2011 = 100, log scale · type-0 rows · static-source mask on · harmonised = count_normalized (MODIS-eq.) and total_frp ± band</text>\n").ok();
    legend(
        &mut s,
        70.0,
        108.0,
        &[
            ("raw, all sensors", RAW_VIIRS, None, 2.5, Marker::Square),
            ("raw, MODIS only", RAW_MODIS, Some("3 4"), 2.0, Marker::Circle),
            ("harmonised", HARMONISED, None, 3.5, Marker::Diamond),
        ],
    );
    draw_panel(&mut s, &panel(90.0, "Detections", true));
    draw_panel(&mut s, &panel(670.0, "Total FRP", false));
    write!(s, "<text x=\"70\" y=\"620\" fill=\"{MUTED}\" font-size=\"13\">Source: NASA FIRMS SP country archive (MODIS C6.1, VIIRS 375 m S-NPP/NOAA-20), PyroSync default pipeline. docs/vaka/china_ban_annual.csv</text>\n</svg>\n").ok();
    s
}

fn shift_chart(national: &[Annual], belt: &[Annual]) -> String {
    let mut s = svg_open(
        1200.0,
        640.0,
        "China 2010–2022: size and season shift",
        "Left: share of S-NPP detections in kg-equivalent size classes per year (proxy from FRP). Right: share of annual FRP falling in December–May, harmonised and raw MODIS, national and cropland belt.",
    );
    write!(s, "<text x=\"70\" y=\"48\" fill=\"{TEXT}\" font-size=\"26\" font-weight=\"700\">Size and season shift (proxy, our archive)</text>\n<text x=\"70\" y=\"78\" fill=\"{MUTED}\" font-size=\"16\">Left: S-NPP detections per size class, kg-eq. = FRP × 1 h × 0.368 kg/MJ (proxy) · Right: share of the year's FRP in Dec–May</text>\n").ok();
    let snpp_years: Vec<&Annual> = national.iter().filter(|x| x.year >= 2012).collect();
    let cls = |i: usize| snpp_years.iter().map(|x| (x.year as f64, share(x.kg_class_snpp, i))).collect::<Vec<_>>();
    let small_label = format!("small < 2500 kg-eq (< {:.1} MW)", kg_to_mw(SMALL_KG));
    let large_label = format!("large > 4000 kg-eq (> {:.1} MW)", kg_to_mw(LARGE_KG));
    let left = Panel {
        x: 90.0,
        y: 150.0,
        w: 500.0,
        h: 380.0,
        title: "S-NPP detections by size class (%, national)",
        log: false,
        y_min: 0.0,
        y_max: 70.0,
        ticks: &[0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0],
        tick_suffix: "%",
        x_from: FROM,
        lines: vec![
            Line { color: SIZE_SMALL, dash: None, width: 3.0, marker: Marker::Circle, points: cls(0), band: None },
            Line { color: SIZE_MID, dash: Some("6 4"), width: 2.0, marker: Marker::Square, points: cls(1), band: None },
            Line { color: SIZE_LARGE, dash: None, width: 3.0, marker: Marker::Triangle, points: cls(2), band: None },
        ],
    };
    let ws = |a: &[Annual], modis: bool| {
        a.iter()
            .map(|x| (x.year as f64, 100.0 * if modis { x.winter_spring_share_modis } else { x.winter_spring_share }))
            .collect::<Vec<_>>()
    };
    let right = Panel {
        x: 670.0,
        y: 150.0,
        w: 500.0,
        h: 380.0,
        title: "FRP share in Dec–May (%)",
        log: false,
        y_min: 0.0,
        y_max: 100.0,
        ticks: &[0.0, 20.0, 40.0, 60.0, 80.0, 100.0],
        tick_suffix: "%",
        x_from: FROM,
        lines: vec![
            Line { color: RAW_MODIS, dash: Some("3 4"), width: 2.0, marker: Marker::Circle, points: ws(national, true), band: None },
            Line { color: HARMONISED, dash: Some("8 5"), width: 2.5, marker: Marker::Square, points: ws(belt, false), band: None },
            Line { color: HARMONISED, dash: None, width: 3.5, marker: Marker::Diamond, points: ws(national, false), band: None },
        ],
    };
    draw_panel(&mut s, &left);
    draw_panel(&mut s, &right);
    let legend_left: [(&str, &str, Option<&str>, f64, Marker); 3] = [
        (&small_label, SIZE_SMALL, None, 3.0, Marker::Circle),
        ("mid", SIZE_MID, Some("6 4"), 2.0, Marker::Square),
        (&large_label, SIZE_LARGE, None, 3.0, Marker::Triangle),
    ];
    legend(&mut s, 90.0, 570.0, &legend_left[..1]);
    legend(&mut s, 400.0, 570.0, &legend_left[1..2]);
    legend(&mut s, 90.0, 596.0, &legend_left[2..]);
    legend(&mut s, 670.0, 570.0, &[("harmonised, national", HARMONISED, None, 3.5, Marker::Diamond)]);
    legend(&mut s, 940.0, 570.0, &[("raw MODIS, national", RAW_MODIS, Some("3 4"), 2.0, Marker::Circle)]);
    legend(&mut s, 670.0, 596.0, &[("harmonised, NE China + North China Plain", HARMONISED, Some("8 5"), 2.5, Marker::Square)]);
    s.push_str("</svg>\n");
    s
}

fn main() -> Result<()> {
    let started = Instant::now();
    let out_dir = std::env::args().nth(1).map_or_else(|| PathBuf::from("docs/vaka"), PathBuf::from);
    std::fs::create_dir_all(&out_dir)?;
    let dir = archive_dir();
    let config = PipelineConfig::default();
    let mut results = Vec::new();
    for region in &REGIONS {
        let t = Instant::now();
        let r = run_region(&dir, region, &config)?;
        eprintln!("{}: {} calendar cells in {:.1}s", region.id, r.cells.len(), t.elapsed().as_secs_f64());
        results.push((region, r));
    }
    write_monthly(&out_dir.join("china_ban_monthly.csv"), &results)?;
    let derived: Vec<(&Region, Vec<Annual>, &RegionResult, Baselines)> = results
        .iter()
        .map(|(region, r)| {
            let base = baselines(r);
            (*region, annual(r, &base), r, base)
        })
        .collect();
    write_annual(
        &out_dir.join("china_ban_annual.csv"),
        &derived.iter().map(|(g, a, r, _)| (*g, a.clone(), *r)).collect::<Vec<_>>(),
    )?;
    let mut json = format!(
        "{{\"country\":\"{COUNTRY}\",\"years\":[{FROM},{TO}],\"ban\":[{},{}],\"config\":\"PipelineConfig::default() (static mask on, SAMA off)\",\"regions\":[",
        BAN.0, BAN.1
    );
    for (i, (region, a, r, base)) in derived.iter().enumerate() {
        let s = summarize(region, a, r, base);
        print!("{}", s.text);
        if i > 0 {
            json.push(',');
        }
        json.push_str(&s.json);
    }
    json.push_str("]}\n");
    std::fs::write(out_dir.join("china_ban_summary.json"), json)?;
    std::fs::write(out_dir.join("china_ban_series.svg"), series_chart(&derived[0].1))?;
    std::fs::write(out_dir.join("china_ban_shift.svg"), shift_chart(&derived[0].1, &derived[1].1))?;
    eprintln!("done in {:.1}s, outputs in {}", started.elapsed().as_secs_f64(), out_dir.display());
    Ok(())
}
