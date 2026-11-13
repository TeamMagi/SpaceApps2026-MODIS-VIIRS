use anyhow::{Context, Result};
use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use pyrosync_core::models::*;

pub fn parse_csv(csv_text: &str, source: &str) -> Result<Vec<Hotspot>> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(csv_text.as_bytes());

    let headers = reader
        .headers()
        .context("Missing CSV headers")?
        .clone();

    let col = |name: &str| -> Option<usize> {
        headers.iter().position(|h| h == name)
    };

    let lat_idx = col("latitude").context("Missing latitude column")?;
    let lon_idx = col("longitude").context("Missing longitude column")?;
    let bright_idx = col("brightness").or_else(|| col("bright_ti4"));
    let frp_idx = col("frp").context("Missing frp column")?;
    let conf_idx = col("confidence");
    let date_idx = col("acq_date").context("Missing acq_date column")?;
    let time_idx = col("acq_time").context("Missing acq_time column")?;
    let scan_idx = col("scan");
    let track_idx = col("track");
    let sat_idx = col("satellite");
    let dn_idx = col("daynight");

    let mut hotspots = Vec::new();

    for result in reader.records() {
        let record = result.context("Failed to parse CSV record")?;

        let lat: f64 = record[lat_idx].parse().context("Invalid latitude")?;
        let lon: f64 = record[lon_idx].parse().context("Invalid longitude")?;

        let brightness: f64 = bright_idx
            .and_then(|i| record.get(i)?.parse().ok())
            .unwrap_or(0.0);

        let frp: f64 = record[frp_idx].parse().unwrap_or(0.0);

        let confidence = conf_idx
            .and_then(|i| record.get(i))
            .map(parse_confidence)
            .unwrap_or(Confidence::Nominal);

        let date = NaiveDate::parse_from_str(&record[date_idx], "%Y-%m-%d")
            .context("Invalid acq_date")?;

        let time_str = &record[time_idx];
        let time = parse_hhmm(time_str).context("Invalid acq_time")?;

        let acquired = Utc
            .from_utc_datetime(&date.and_time(time));

        let scan: f64 = scan_idx
            .and_then(|i| record.get(i)?.parse().ok())
            .unwrap_or(1.0);

        let track: f64 = track_idx
            .and_then(|i| record.get(i)?.parse().ok())
            .unwrap_or(1.0);

        let satellite = sat_idx
            .and_then(|i| record.get(i).map(|s| s.to_string()))
            .unwrap_or_default();

        let daynight = dn_idx
            .and_then(|i| record.get(i))
            .map(|s| match s {
                "D" => DayNight::Day,
                _ => DayNight::Night,
            })
            .unwrap_or(DayNight::Day);

        let sensor = match source {
            s if s.contains("MODIS") => {
                if satellite.contains("Terra") || satellite == "T" {
                    Sensor::ModisTerra
                } else {
                    Sensor::ModisAqua
                }
            }
            s if s.contains("SNPP") => Sensor::ViirsSNPP,
            s if s.contains("NOAA20") => Sensor::ViirsNOAA20,
            s if s.contains("NOAA21") => Sensor::ViirsNOAA21,
            _ => Sensor::ViirsSNPP,
        };

        hotspots.push(Hotspot {
            lat,
            lon,
            brightness,
            frp,
            confidence,
            sensor,
            acquired,
            scan,
            track,
            satellite,
            daynight,
            vza: None,
        });
    }

    Ok(hotspots)
}

fn parse_confidence(s: &str) -> Confidence {
    match s.to_lowercase().as_str() {
        "h" | "high" => Confidence::High,
        "n" | "nominal" => Confidence::Nominal,
        "l" | "low" => Confidence::Low,
        _ => {
            if let Ok(n) = s.parse::<u32>() {
                if n >= 80 {
                    Confidence::High
                } else if n >= 30 {
                    Confidence::Nominal
                } else {
                    Confidence::Low
                }
            } else {
                Confidence::Nominal
            }
        }
    }
}

fn parse_hhmm(s: &str) -> Option<NaiveTime> {
    let s = s.trim();
    if s.len() == 4 {
        let h: u32 = s[..2].parse().ok()?;
        let m: u32 = s[2..].parse().ok()?;
        NaiveTime::from_hms_opt(h, m, 0)
    } else if s.contains(':') {
        NaiveTime::parse_from_str(s, "%H:%M").ok()
    } else {
        None
    }
}
