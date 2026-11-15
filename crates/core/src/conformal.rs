//! Mondrian split conformal intervals for monthly country `total_frp`.
//!
//! Pure statistics: the calibration samples come from a held-out-sensor comparison produced
//! offline, this module only fits the quantile table and applies it. The result is a
//! sensor-reproducibility band, not a ground-truth accuracy interval.

use serde::{Deserialize, Serialize};

use crate::config::PipelineConfig;

pub const CONFORMAL_SCHEMA_VERSION: u32 = 1;
pub const MAX_BINS: usize = 6;
pub const TARGET_BIN_SIZE: usize = 100;
pub const MIN_BIN_SIZE: usize = 40;

/// Miscoverage levels in percent: 32 for the 68% band, 10 for the 90% band.
const ALPHA_PCT_68: u32 = 32;
const ALPHA_PCT_90: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformalEra {
    PreViirs,
    Viirs,
}

/// One calibration pair: `mu` is the prediction, `y` the held-out value, both in MW.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConformalSample {
    pub era: ConformalEra,
    pub mu: f64,
    pub y: f64,
}

/// Signed relative-residual quantiles `e = (y - mu) / mu` of one (era, mu-bin) cell.
/// `mu_lo` is inclusive and `mu_hi` exclusive; `None` marks the open outer edge.
/// A quantile is `None` when the bin is too small for that level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformalEntry {
    pub era: ConformalEra,
    pub bin: usize,
    pub mu_lo: Option<f64>,
    pub mu_hi: Option<f64>,
    pub cal_mu_min: f64,
    pub cal_mu_max: f64,
    pub n_cal: usize,
    pub q68_lower: Option<f64>,
    pub q68_upper: Option<f64>,
    pub q90_lower: Option<f64>,
    pub q90_upper: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformalTable {
    pub schema_version: u32,
    pub calibration_fingerprint: String,
    pub entries: Vec<ConformalEntry>,
    /// Measured out-of-sample coverage report written by the calibration tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConformalBand {
    pub p68_lower: f64,
    pub p68_upper: f64,
    pub p90_lower: f64,
    pub p90_upper: f64,
    pub era: ConformalEra,
    pub bin: usize,
    pub n_cal: usize,
    /// `mu` lies outside the calibration range and was assigned to the nearest bin.
    pub extrapolated: bool,
}

/// 1-based order-statistic indices `(l, u)` of the equal-tailed interval for `n` residuals:
/// `l = floor((n+1) a/2)`, `u = ceil((n+1)(1 - a/2))` with `a = alpha_pct / 100`.
/// Integer arithmetic avoids float rounding at exact boundaries. `None` when the bin is too
/// small (`l < 1` or `u > n`).
pub fn conformal_indices(n: usize, alpha_pct: u32) -> Option<(usize, usize)> {
    let a = alpha_pct as usize;
    if a == 0 || a >= 100 {
        return None;
    }
    let l = (n + 1) * a / 200;
    let u = ((n + 1) * (200 - a)).div_ceil(200);
    (l >= 1 && u <= n).then_some((l, u))
}

fn level_quantiles(sorted_e: &[f64], alpha_pct: u32) -> (Option<f64>, Option<f64>) {
    match conformal_indices(sorted_e.len(), alpha_pct) {
        Some((l, u)) => (Some(sorted_e[l - 1]), Some(sorted_e[u - 1])),
        None => (None, None),
    }
}

/// Fit one table from calibration samples, separately per era (eras are never pooled).
/// Samples with non-finite values, `mu <= 0` or `y < 0` are ignored. An era with fewer
/// than [`MIN_BIN_SIZE`] usable samples gets no entries. The fingerprint is left empty for
/// the caller to fill.
pub fn fit_mondrian(samples: &[ConformalSample]) -> ConformalTable {
    let mut entries = Vec::new();
    for era in [ConformalEra::PreViirs, ConformalEra::Viirs] {
        let mut pairs: Vec<(f64, f64)> = samples
            .iter()
            .filter(|s| {
                s.era == era && s.mu.is_finite() && s.y.is_finite() && s.mu > 0.0 && s.y >= 0.0
            })
            .map(|s| (s.mu, s.y))
            .collect();
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        fit_era(era, &pairs, &mut entries);
    }
    ConformalTable {
        schema_version: CONFORMAL_SCHEMA_VERSION,
        calibration_fingerprint: String::new(),
        entries,
        coverage: None,
        notes: None,
    }
}

fn fit_era(era: ConformalEra, pairs: &[(f64, f64)], out: &mut Vec<ConformalEntry>) {
    let n = pairs.len();
    if n < MIN_BIN_SIZE {
        return;
    }
    let k = MAX_BINS.min(n / TARGET_BIN_SIZE).max(1);

    // Equal-count cut points; a cut inside a run of equal mu moves past the run so that a
    // mu value never belongs to two bins.
    let mut cuts: Vec<usize> = Vec::new();
    for i in 1..k {
        let mut idx = i * n / k;
        while idx < n && pairs[idx].0 == pairs[idx - 1].0 {
            idx += 1;
        }
        if idx < n && cuts.last().is_none_or(|&last| idx > last) {
            cuts.push(idx);
        }
    }
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for &c in &cuts {
        ranges.push((start, c));
        start = c;
    }
    ranges.push((start, n));

    // Merge the smallest undersized bin into its smaller neighbour until none is left.
    while ranges.len() > 1 {
        let Some((i, _)) = ranges
            .iter()
            .enumerate()
            .map(|(i, r)| (i, r.1 - r.0))
            .filter(|&(_, size)| size < MIN_BIN_SIZE)
            .min_by_key(|&(_, size)| size)
        else {
            break;
        };
        let left = i.checked_sub(1).map(|j| ranges[j].1 - ranges[j].0);
        let right = ranges.get(i + 1).map(|r| r.1 - r.0);
        let into_left = match (left, right) {
            (Some(l), Some(r)) => l <= r,
            (Some(_), None) => true,
            _ => false,
        };
        if into_left {
            ranges[i - 1].1 = ranges[i].1;
        } else {
            ranges[i + 1].0 = ranges[i].0;
        }
        ranges.remove(i);
    }

    let last = ranges.len() - 1;
    for (bin, &(s, e)) in ranges.iter().enumerate() {
        let slice = &pairs[s..e];
        let mut residuals: Vec<f64> = slice.iter().map(|&(mu, y)| (y - mu) / mu).collect();
        residuals.sort_by(|a, b| a.total_cmp(b));
        let (q68_lower, q68_upper) = level_quantiles(&residuals, ALPHA_PCT_68);
        let (q90_lower, q90_upper) = level_quantiles(&residuals, ALPHA_PCT_90);
        out.push(ConformalEntry {
            era,
            bin,
            mu_lo: (bin > 0).then(|| pairs[s].0),
            mu_hi: (bin < last).then(|| pairs[e].0),
            cal_mu_min: slice[0].0,
            cal_mu_max: slice[slice.len() - 1].0,
            n_cal: slice.len(),
            q68_lower,
            q68_upper,
            q90_lower,
            q90_upper,
        });
    }
}

/// Interval for a predicted monthly total `mu` (MW) in `era`. `None` when `mu` is not a
/// positive finite number, the table has no usable bin for the era, or the bin lacks a
/// quantile. Lower bounds are clamped at 0.
pub fn conformal_interval(
    table: &ConformalTable,
    era: ConformalEra,
    mu: f64,
) -> Option<ConformalBand> {
    if !mu.is_finite() || mu <= 0.0 {
        return None;
    }
    let bins: Vec<&ConformalEntry> = table.entries.iter().filter(|e| e.era == era).collect();
    let first = bins.first()?;
    let last = bins.last()?;
    let entry = bins
        .iter()
        .find(|e| e.mu_hi.is_none_or(|hi| mu < hi))
        .unwrap_or(last);
    let extrapolated = mu < first.cal_mu_min || mu > last.cal_mu_max;
    let bound = |q: Option<f64>| q.map(|q| (mu * (1.0 + q)).max(0.0));
    Some(ConformalBand {
        p68_lower: bound(entry.q68_lower)?,
        p68_upper: bound(entry.q68_upper)?,
        p90_lower: bound(entry.q90_lower)?,
        p90_upper: bound(entry.q90_upper)?,
        era,
        bin: entry.bin,
        n_cal: entry.n_cal,
        extrapolated,
    })
}

/// Digest of the config sections that move the `total_frp` centre. The `uncertainty` sigmas
/// are excluded because they change the analytic bounds only; the swath-gap flag is the one
/// `uncertainty` field that shifts the centre. FNV-1a over canonical JSON, so it is stable
/// across runs and platforms.
pub fn calibration_fingerprint(config: &PipelineConfig) -> String {
    fn val<T: Serialize>(v: &T) -> serde_json::Value {
        serde_json::to_value(v).unwrap_or(serde_json::Value::Null)
    }
    let canonical = serde_json::json!({
        "dedup": val(&config.dedup),
        "fleet": val(&config.fleet),
        "matching": val(&config.matching),
        "vza": val(&config.vza),
        "calibration": val(&config.calibration),
        "static_mask": val(&config.static_mask),
        "sama": val(&config.sama),
        "tropical_swath_gap_compensation": config.uncertainty.tropical_swath_gap_compensation,
    });
    let text = serde_json::to_string(&canonical).unwrap_or_default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rng(u64);

    impl Rng {
        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        fn uniform(&mut self) -> f64 {
            ((self.next_u64() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
        }

        fn normal(&mut self) -> f64 {
            let (u1, u2) = (self.uniform(), self.uniform());
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
        }
    }

    fn sigma_rel(mu: f64) -> f64 {
        0.3 * mu.powf(-0.3)
    }

    fn draw_mu(rng: &mut Rng) -> f64 {
        (rng.uniform() * 1000f64.ln()).exp()
    }

    fn sample(era: ConformalEra, mu: f64, e: f64) -> ConformalSample {
        ConformalSample {
            era,
            mu,
            y: (mu * (1.0 + e)).max(0.0),
        }
    }

    fn exchangeable(rng: &mut Rng, n: usize) -> Vec<ConformalSample> {
        (0..n)
            .map(|_| {
                let mu = draw_mu(rng);
                sample(ConformalEra::Viirs, mu, sigma_rel(mu) * rng.normal())
            })
            .collect()
    }

    fn covered(band: &ConformalBand, y: f64) -> (bool, bool) {
        (
            y >= band.p68_lower && y <= band.p68_upper,
            y >= band.p90_lower && y <= band.p90_upper,
        )
    }

    #[test]
    fn index_rule_table() {
        let cases = [
            (19, 10, Some((1, 19))),
            (20, 10, Some((1, 20))),
            (39, 10, Some((2, 38))),
            (40, 10, Some((2, 39))),
            (99, 10, Some((5, 95))),
            (100, 10, Some((5, 96))),
            (18, 10, None),
            (19, 32, Some((3, 17))),
            (99, 32, Some((16, 84))),
            (100, 32, Some((16, 85))),
            (3, 32, None),
        ];
        for (n, alpha, expected) in cases {
            assert_eq!(conformal_indices(n, alpha), expected, "n={n} alpha={alpha}");
        }
        assert_eq!(conformal_indices(100, 0), None);
        assert_eq!(conformal_indices(100, 100), None);
    }

    #[test]
    fn coverage_on_exchangeable_data() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let table = fit_mondrian(&exchangeable(&mut rng, 6000));
        assert_eq!(table.entries.len(), MAX_BINS);
        assert!(table.entries.iter().all(|e| e.n_cal >= TARGET_BIN_SIZE));

        let test = exchangeable(&mut rng, 12_000);
        let mut per_bin = vec![(0usize, 0usize, 0usize); MAX_BINS];
        let (mut c68, mut c90) = (0usize, 0usize);
        for s in &test {
            let band = conformal_interval(&table, s.era, s.mu).expect("band");
            let (a, b) = covered(&band, s.y);
            c68 += a as usize;
            c90 += b as usize;
            let cell = &mut per_bin[band.bin];
            cell.0 += 1;
            cell.1 += a as usize;
            cell.2 += b as usize;
        }
        let n = test.len() as f64;
        let (p68, p90) = (c68 as f64 / n, c90 as f64 / n);
        assert!((p68 - 0.68).abs() <= 0.03, "pooled 68% coverage {p68}");
        assert!((p90 - 0.90).abs() <= 0.03, "pooled 90% coverage {p90}");
        for (bin, (total, a, b)) in per_bin.into_iter().enumerate() {
            assert!(total > 0);
            let (p68, p90) = (a as f64 / total as f64, b as f64 / total as f64);
            assert!(p68 >= 0.68 - 0.05, "bin {bin} 68% coverage {p68}");
            assert!(p90 >= 0.90 - 0.05, "bin {bin} 90% coverage {p90}");
        }
    }

    #[test]
    fn year_shocks_break_the_analytic_band_but_not_leave_one_year_out_conformal() {
        let mut rng = Rng(0xD1B5_4A32_D192_ED03);
        let years = 20;
        let per_year = 600;
        let shocks: Vec<f64> = (0..years).map(|_| 0.10 * rng.normal()).collect();
        let mut data: Vec<(usize, ConformalSample, f64)> = Vec::new();
        for (year, shock) in shocks.iter().enumerate() {
            for _ in 0..per_year {
                let mu = draw_mu(&mut rng);
                let e = shock + sigma_rel(mu) * rng.normal();
                let n_det = 50.0 + 5000.0 * rng.uniform();
                let analytic_half = 0.12 / n_det.sqrt();
                data.push((year, sample(ConformalEra::PreViirs, mu, e), analytic_half));
            }
        }

        let analytic_hits = data
            .iter()
            .filter(|(_, s, half)| s.y >= s.mu * (1.0 - half) && s.y <= s.mu * (1.0 + half))
            .count();
        let analytic = analytic_hits as f64 / data.len() as f64;
        assert!(analytic < 0.60, "analytic coverage {analytic}");

        let (mut hits68, mut hits90, mut held) = (0usize, 0usize, 0usize);
        for year in 0..years {
            let train: Vec<ConformalSample> = data
                .iter()
                .filter(|(y, _, _)| *y != year)
                .map(|(_, s, _)| *s)
                .collect();
            let table = fit_mondrian(&train);
            for (_, s, _) in data.iter().filter(|(y, _, _)| *y == year) {
                let band = conformal_interval(&table, s.era, s.mu).expect("band");
                let (a, b) = covered(&band, s.y);
                held += 1;
                hits68 += a as usize;
                hits90 += b as usize;
            }
        }
        let (p68, p90) = (hits68 as f64 / held as f64, hits90 as f64 / held as f64);
        assert!(p68 >= 0.64, "leave-one-year-out 68% coverage {p68}");
        assert!(p90 >= 0.84, "leave-one-year-out 90% coverage {p90}");
    }

    #[test]
    fn small_bins_are_merged_and_small_eras_dropped() {
        let mut samples = Vec::new();
        for i in 0..90 {
            samples.push(sample(ConformalEra::Viirs, 1.0 + i as f64 * 0.01, 0.1));
        }
        for _ in 0..100 {
            samples.push(sample(ConformalEra::Viirs, 10.0, -0.1));
        }
        for i in 0..110 {
            samples.push(sample(ConformalEra::Viirs, 20.0 + i as f64, 0.2));
        }
        let table = fit_mondrian(&samples);
        let viirs: Vec<_> = table
            .entries
            .iter()
            .filter(|e| e.era == ConformalEra::Viirs)
            .collect();
        assert_eq!(viirs.len(), 2, "the 10-sample middle bin must merge");
        assert_eq!(viirs.iter().map(|e| e.n_cal).sum::<usize>(), 300);
        assert!(viirs.iter().all(|e| e.n_cal >= MIN_BIN_SIZE));
        assert_eq!(viirs[0].mu_lo, None);
        assert_eq!(viirs[1].mu_hi, None);
        assert_eq!(viirs[0].mu_hi, viirs[1].mu_lo);

        let few: Vec<_> = (0..30)
            .map(|i| sample(ConformalEra::PreViirs, 1.0 + i as f64, 0.1))
            .collect();
        let table = fit_mondrian(&few);
        assert!(table.entries.is_empty());
        assert!(conformal_interval(&table, ConformalEra::PreViirs, 5.0).is_none());
    }

    #[test]
    fn invalid_samples_are_ignored_and_eras_are_not_pooled() {
        let mut samples: Vec<_> = (0..50)
            .map(|i| sample(ConformalEra::Viirs, 1.0 + i as f64, 0.1))
            .collect();
        samples.push(ConformalSample { era: ConformalEra::Viirs, mu: 0.0, y: 1.0 });
        samples.push(ConformalSample { era: ConformalEra::Viirs, mu: f64::NAN, y: 1.0 });
        samples.push(ConformalSample { era: ConformalEra::Viirs, mu: 5.0, y: -1.0 });
        let table = fit_mondrian(&samples);
        assert_eq!(table.entries.len(), 1);
        assert_eq!(table.entries[0].n_cal, 50);
        assert!(conformal_interval(&table, ConformalEra::PreViirs, 5.0).is_none());
    }

    #[test]
    fn interval_edges_extrapolation_and_clamping() {
        let mut rng = Rng(42);
        let table = fit_mondrian(&exchangeable(&mut rng, 1000));
        let (min, max) = (
            table.entries.first().unwrap().cal_mu_min,
            table.entries.last().unwrap().cal_mu_max,
        );
        let era = ConformalEra::Viirs;
        assert!(conformal_interval(&table, era, 0.0).is_none());
        assert!(conformal_interval(&table, era, -3.0).is_none());
        assert!(conformal_interval(&table, era, f64::NAN).is_none());

        let inside = conformal_interval(&table, era, (min * max).sqrt()).unwrap();
        assert!(!inside.extrapolated);
        let low = conformal_interval(&table, era, min / 10.0).unwrap();
        assert!(low.extrapolated);
        assert_eq!(low.bin, 0);
        let high = conformal_interval(&table, era, max * 10.0).unwrap();
        assert!(high.extrapolated);
        assert_eq!(high.bin, table.entries.len() - 1);
        assert!(inside.p90_lower <= inside.p68_lower && inside.p68_upper <= inside.p90_upper);

        let mut hand = ConformalTable {
            entries: vec![ConformalEntry {
                era,
                bin: 0,
                mu_lo: None,
                mu_hi: None,
                cal_mu_min: 1.0,
                cal_mu_max: 10.0,
                n_cal: 60,
                q68_lower: Some(-1.5),
                q68_upper: Some(0.5),
                q90_lower: Some(-2.0),
                q90_upper: None,
            }],
            ..table.clone()
        };
        assert!(conformal_interval(&hand, era, 5.0).is_none(), "missing quantile");
        hand.entries[0].q90_upper = Some(1.0);
        let band = conformal_interval(&hand, era, 5.0).unwrap();
        assert_eq!(band.p68_lower, 0.0);
        assert_eq!(band.p90_lower, 0.0);
        assert!((band.p68_upper - 7.5).abs() < 1e-12);
    }

    #[test]
    fn table_json_round_trips_with_open_edges() {
        let mut rng = Rng(7);
        let mut table = fit_mondrian(&exchangeable(&mut rng, 500));
        table.calibration_fingerprint = "x".into();
        let json = serde_json::to_string(&table).unwrap();
        assert!(json.contains("\"era\":\"viirs\""));
        assert!(json.contains("\"mu_lo\":null"));
        let back: ConformalTable = serde_json::from_str(&json).unwrap();
        // serde_json parses floats without exact round-trip, so compare within 1e-12.
        assert_eq!(back.entries.len(), table.entries.len());
        assert_eq!(back.calibration_fingerprint, table.calibration_fingerprint);
        let close = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (Some(a), Some(b)) => (a - b).abs() <= 1e-12 * a.abs().max(1.0),
            (None, None) => true,
            _ => false,
        };
        for (a, b) in back.entries.iter().zip(&table.entries) {
            assert_eq!((a.era, a.bin, a.n_cal), (b.era, b.bin, b.n_cal));
            assert!(close(a.mu_lo, b.mu_lo) && close(a.mu_hi, b.mu_hi));
            assert!(close(a.q68_lower, b.q68_lower) && close(a.q68_upper, b.q68_upper));
            assert!(close(a.q90_lower, b.q90_lower) && close(a.q90_upper, b.q90_upper));
        }
    }

    #[test]
    fn fingerprint_tracks_centre_config_but_not_sigmas() {
        let base = PipelineConfig::default();
        assert_eq!(calibration_fingerprint(&base), calibration_fingerprint(&base.clone()));

        let mut sigma = base.clone();
        sigma.uncertainty.modis_sensor_sigma += 0.05;
        sigma.uncertainty.viirs_sensor_sigma += 0.05;
        assert_eq!(calibration_fingerprint(&sigma), calibration_fingerprint(&base));

        let mut radius = base.clone();
        radius.matching.radius_km += 0.5;
        assert_ne!(calibration_fingerprint(&radius), calibration_fingerprint(&base));

        let mut gap = base.clone();
        gap.uncertainty.tropical_swath_gap_compensation = true;
        assert_ne!(calibration_fingerprint(&gap), calibration_fingerprint(&base));

        let mut cal = base.clone();
        cal.calibration.enabled = !cal.calibration.enabled;
        assert_ne!(calibration_fingerprint(&cal), calibration_fingerprint(&base));
    }
}
