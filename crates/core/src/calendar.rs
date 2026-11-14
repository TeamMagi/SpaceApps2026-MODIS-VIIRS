use crate::models::{CalendarCell, HarmonizedHotspot};
use chrono::Datelike;
use std::collections::HashMap;

/// Build a burning activity calendar from harmonized hotspot data.
///
/// Groups hotspots by (year, month) and computes:
/// - count: number of detections
/// - mean_frp: average harmonized fire radiative power (`frp_corrected`)
/// - mean_frp_raw: average raw fire radiative power (`frp`)
/// - anomaly: deviation from multi-year monthly mean (if enough data)
pub fn build_calendar(hotspots: &[HarmonizedHotspot]) -> Vec<CalendarCell> {
    let mut buckets: HashMap<(i32, u32), (u64, f64, f64)> = HashMap::new();

    for h in hotspots {
        let date = h.acquired.date_naive();
        let entry = buckets
            .entry((date.year(), date.month()))
            .or_insert((0, 0.0, 0.0));
        entry.0 += 1;
        entry.1 += h.frp_corrected;
        entry.2 += h.frp;
    }

    let mut cells: Vec<CalendarCell> = buckets
        .into_iter()
        .map(|((year, month), (count, sum_corrected, sum_raw))| CalendarCell {
            year,
            month,
            count,
            mean_frp: sum_corrected / count as f64,
            mean_frp_raw: sum_raw / count as f64,
            anomaly: None,
        })
        .collect();

    compute_anomalies(&mut cells);
    cells.sort_by_key(|a| (a.year, a.month));
    cells
}

fn compute_anomalies(cells: &mut [CalendarCell]) {
    let mut monthly_means: HashMap<u32, Vec<f64>> = HashMap::new();

    for cell in cells.iter() {
        monthly_means
            .entry(cell.month)
            .or_default()
            .push(cell.count as f64);
    }

    let baselines: HashMap<u32, (f64, f64)> = monthly_means
        .into_iter()
        .filter(|(_, counts)| counts.len() >= 3)
        .map(|(month, counts)| {
            let n = counts.len() as f64;
            let mean = counts.iter().sum::<f64>() / n;
            let std = (counts.iter().map(|c| (c - mean).powi(2)).sum::<f64>() / n).sqrt();
            (month, (mean, std))
        })
        .collect();

    for cell in cells.iter_mut() {
        if let Some(&(mean, std)) = baselines.get(&cell.month) && std > 0.0 {
            cell.anomaly = Some((cell.count as f64 - mean) / std);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, DayNight, HotspotSource, Sensor};
    use chrono::{TimeZone, Utc};

    fn make_harmonized(
        year: i32,
        month: u32,
        day: u32,
        frp: f64,
        frp_corrected: f64,
    ) -> HarmonizedHotspot {
        HarmonizedHotspot {
            lat: 39.0,
            lon: 32.0,
            frp,
            frp_corrected,
            vza_factor: if (frp - frp_corrected).abs() > f64::EPSILON {
                Some(frp / frp_corrected)
            } else {
                None
            },
            n_obs: 1,
            platforms: vec![Sensor::ModisTerra],
            confidence: Confidence::Nominal,
            source: HotspotSource::ModisOnly,
            acquired: Utc.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap(),
            daynight: DayNight::Day,
        }
    }

    #[test]
    fn calendar_distinguishes_raw_and_corrected_mean_frp() {
        let hotspots = vec![
            make_harmonized(2025, 7, 1, 100.0, 50.0),
            make_harmonized(2025, 7, 2, 60.0, 30.0),
        ];

        let cells = build_calendar(&hotspots);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].year, 2025);
        assert_eq!(cells[0].month, 7);
        assert_eq!(cells[0].count, 2);
        assert!((cells[0].mean_frp_raw - 80.0).abs() < 1e-12);
        assert!((cells[0].mean_frp - 40.0).abs() < 1e-12);
        assert!(cells[0].mean_frp < cells[0].mean_frp_raw);
        assert_eq!(cells[0].anomaly, None);
    }

    #[test]
    fn calendar_computes_count_anomaly_when_three_or_more_years_exist() {
        let hotspots = vec![
            make_harmonized(2023, 8, 1, 40.0, 40.0),
            make_harmonized(2024, 8, 1, 40.0, 40.0),
            make_harmonized(2025, 8, 1, 40.0, 40.0),
            make_harmonized(2025, 8, 2, 60.0, 30.0),
            make_harmonized(2025, 8, 3, 60.0, 30.0),
            make_harmonized(2025, 8, 4, 60.0, 30.0),
        ];

        let cells = build_calendar(&hotspots);
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].year, 2023);
        assert_eq!(cells[1].year, 2024);
        assert_eq!(cells[2].year, 2025);
        assert!(cells.iter().all(|c| c.anomaly.is_some()));
        assert!(cells[2].anomaly.unwrap() > 0.0);
    }
}
