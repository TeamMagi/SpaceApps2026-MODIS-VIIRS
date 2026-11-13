use crate::models::{CalendarCell, HarmonizedHotspot};
use chrono::Datelike;
use std::collections::HashMap;

/// Build a burning activity calendar from harmonized hotspot data.
///
/// Groups hotspots by (year, month) and computes:
/// - count: number of detections
/// - mean_frp: average fire radiative power
/// - anomaly: deviation from multi-year monthly mean (if enough data)
pub fn build_calendar(hotspots: &[HarmonizedHotspot]) -> Vec<CalendarCell> {
    let mut buckets: HashMap<(i32, u32), Vec<f64>> = HashMap::new();

    for h in hotspots {
        let year = h.acquired.date_naive().year();
        let month = h.acquired.date_naive().month();
        buckets.entry((year, month)).or_default().push(h.frp_corrected);
    }

    let mut cells: Vec<CalendarCell> = buckets
        .iter()
        .map(|(&(year, month), frps)| {
            let count = frps.len() as u64;
            let mean_frp = frps.iter().sum::<f64>() / frps.len() as f64;
            CalendarCell {
                year,
                month,
                count,
                mean_frp,
                anomaly: None,
            }
        })
        .collect();

    compute_anomalies(&mut cells);
    cells.sort_by(|a, b| (a.year, a.month).cmp(&(b.year, b.month)));
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
        if let Some(&(mean, std)) = baselines.get(&cell.month) {
            if std > 0.0 {
                cell.anomaly = Some((cell.count as f64 - mean) / std);
            }
        }
    }
}
