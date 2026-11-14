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
    let type_idx = col("type");

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

        let fire_type = type_idx
            .and_then(|i| record.get(i))
            .and_then(|s| s.trim().parse::<u8>().ok())
            .map(FireType::from_code)
            .unwrap_or(FireType::VegetationFire);

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
            fire_type,
        });
    }

    Ok(hotspots)
}

/// Drops non-vegetation detections so volcanoes, flares and offshore sources do not pollute
/// the burn calendar.
pub fn retain_wildfires(hotspots: &mut Vec<Hotspot>) {
    let before = hotspots.len();
    hotspots.retain(|h| h.fire_type.is_wildfire());
    tracing::debug!(
        dropped = before - hotspots.len(),
        "filtered non-wildfire FIRMS detections"
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    const MODIS_SP: &str = concat!(
        "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight,type\n",
        "-20.1,57.5,330.5,1.0,1.0,2024-01-15,1015,Terra,MODIS,75,6.1NRT,290.1,12.5,D,0\n",
        "-20.2,57.6,340.0,1.0,1.0,2024-01-15,1015,Terra,MODIS,80,6.1NRT,290.1,20.0,D,1\n",
        "-20.3,57.7,320.0,1.0,1.0,2024-01-15,1015,Terra,MODIS,60,6.1NRT,290.1,8.0,N,2\n",
    );

    #[test]
    fn modis_type_column_maps_to_fire_type() {
        let hotspots = parse_csv(MODIS_SP, "MODIS_NRT").unwrap();
        assert_eq!(hotspots.len(), 3);
        assert_eq!(hotspots[0].fire_type, FireType::VegetationFire);
        assert_eq!(hotspots[1].fire_type, FireType::ActiveVolcano);
        assert_eq!(hotspots[2].fire_type, FireType::OtherStaticLand);
        assert_eq!(hotspots[0].confidence, Confidence::Nominal);
    }

    #[test]
    fn retain_wildfires_drops_volcanoes_and_static_sources() {
        let mut hotspots = parse_csv(MODIS_SP, "MODIS_NRT").unwrap();
        retain_wildfires(&mut hotspots);
        assert_eq!(hotspots.len(), 1);
        assert_eq!(hotspots[0].lat, -20.1);
        assert_eq!(hotspots[0].fire_type, FireType::VegetationFire);
    }

    #[test]
    fn viirs_thermal_band_headers_are_read() {
        let csv = concat!(
            "latitude,longitude,bright_ti4,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_ti5,frp,daynight,type\n",
            "-5.0,105.0,320.5,0.5,0.5,2024-02-01,0130,N,VIIRS,h,2.0NRT,300.2,15.5,N,0\n",
        );
        let hotspots = parse_csv(csv, "VIIRS_SNPP_NRT").unwrap();
        assert_eq!(hotspots.len(), 1);
        assert_eq!(hotspots[0].brightness, 320.5);
        assert_eq!(hotspots[0].sensor, Sensor::ViirsSNPP);
        assert_eq!(hotspots[0].confidence, Confidence::High);
        assert_eq!(hotspots[0].fire_type, FireType::VegetationFire);
    }

    #[test]
    fn nrt_rows_without_type_default_to_vegetation_fire() {
        let csv = concat!(
            "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight\n",
            "-20.1,57.5,330.5,1.0,1.0,2024-01-15,1015,Terra,MODIS,75,6.1NRT,290.1,12.5,D\n",
            "-20.2,57.6,340.0,1.0,1.0,2024-01-15,1015,Terra,MODIS,80,6.1NRT,290.1,20.0,D\n",
        );
        let mut hotspots = parse_csv(csv, "MODIS_NRT").unwrap();
        assert!(hotspots.iter().all(|h| h.fire_type == FireType::VegetationFire));
        retain_wildfires(&mut hotspots);
        assert_eq!(hotspots.len(), 2);
    }

    #[test]
    fn empty_or_unparseable_type_defaults_to_vegetation_fire() {
        let csv = concat!(
            "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight,type\n",
            "-20.1,57.5,330.5,1.0,1.0,2024-01-15,1015,Terra,MODIS,75,6.1NRT,290.1,12.5,D,\n",
            "-20.2,57.6,340.0,1.0,1.0,2024-01-15,1015,Terra,MODIS,80,6.1NRT,290.1,20.0,D,x\n",
            "-20.3,57.7,320.0,1.0,1.0,2024-01-15,1015,Terra,MODIS,60,6.1NRT,290.1,8.0,N,3\n",
            "-20.4,57.8,320.0,1.0,1.0,2024-01-15,1015,Terra,MODIS,60,6.1NRT,290.1,8.0,N,9\n",
        );
        let mut hotspots = parse_csv(csv, "MODIS_NRT").unwrap();
        assert_eq!(hotspots[0].fire_type, FireType::VegetationFire);
        assert_eq!(hotspots[1].fire_type, FireType::VegetationFire);
        assert_eq!(hotspots[2].fire_type, FireType::Offshore);
        assert_eq!(hotspots[3].fire_type, FireType::Unknown);

        retain_wildfires(&mut hotspots);
        assert_eq!(hotspots.len(), 2);
    }
}
