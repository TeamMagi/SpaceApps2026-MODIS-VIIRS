use anyhow::{anyhow, bail, Context, Result};
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

    for (index, result) in reader.records().enumerate() {
        let row = index + 1;
        let record = result.with_context(|| format!("failed to parse CSV data row {row}"))?;

        let lat = parse_number(field(&record, lat_idx, row, "latitude")?, "latitude", row)?;
        let lon = parse_number(
            field(&record, lon_idx, row, "longitude")?,
            "longitude",
            row,
        )?;

        let brightness: f64 = bright_idx
            .and_then(|i| record.get(i)?.parse().ok())
            .unwrap_or(0.0);

        let frp = parse_number(field(&record, frp_idx, row, "frp")?, "frp", row)?;

        let confidence = conf_idx
            .and_then(|i| record.get(i))
            .map(parse_confidence)
            .unwrap_or(Confidence::Nominal);

        let date_str = field(&record, date_idx, row, "acq_date")?;
        let date = NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d")
            .with_context(|| format!("CSV data row {row}: invalid `acq_date` value {date_str:?}"))?;

        let time_str = field(&record, time_idx, row, "acq_time")?;
        let time = parse_hhmm(time_str).with_context(|| {
            format!("CSV data row {row}: invalid `acq_time` value {time_str:?}")
        })?;

        let acquired = Utc
            .from_utc_datetime(&date.and_time(time));

        let scan: f64 = scan_idx
            .and_then(|i| record.get(i)?.parse().ok())
            .unwrap_or(1.0);

        let track: f64 = track_idx
            .and_then(|i| record.get(i)?.parse().ok())
            .unwrap_or(1.0);

        let satellite = match sat_idx {
            Some(i) => field(&record, i, row, "satellite")?,
            None => "",
        }
        .trim()
        .to_string();

        let daynight = match dn_idx {
            Some(i) => match field(&record, i, row, "daynight")?.trim() {
                "D" => DayNight::Day,
                "N" => DayNight::Night,
                other => {
                    bail!("CSV data row {row}: invalid `daynight` value {other:?} (expected D or N)")
                }
            },
            // Some synthetic NRT inputs omit the column entirely; there is no value to validate.
            None => DayNight::Day,
        };

        let sensor = match source {
            s if s.contains("MODIS") => {
                match satellite.to_ascii_lowercase().as_str() {
                    "t" | "terra" => Sensor::ModisTerra,
                    "a" | "aqua" => Sensor::ModisAqua,
                    // Older feeds omit the column, and historically that meant Aqua.
                    _ if sat_idx.is_none() => Sensor::ModisAqua,
                    _ => bail!(
                        "CSV data row {row}: invalid `satellite` value {satellite:?} for MODIS"
                    ),
                }
            }
            s if s.contains("SNPP") => Sensor::ViirsSNPP,
            s if s.contains("NOAA20") => Sensor::ViirsNOAA20,
            s if s.contains("NOAA21") => Sensor::ViirsNOAA21,
            _ => Sensor::ViirsSNPP,
        };

        let fire_type = match type_idx {
            Some(i) => {
                let code = field(&record, i, row, "type")?.trim();
                if code.is_empty() {
                    // Some SP exports leave `type` blank; keeping the historical default stops
                    // those detections from being dropped later as non-wildfire.
                    FireType::VegetationFire
                } else {
                    FireType::from_code(code.parse::<u8>().with_context(|| {
                        format!("CSV data row {row}: invalid `type` value {code:?}")
                    })?)
                }
            }
            None => FireType::VegetationFire,
        };

        // FIRMS omits viewing zenith angle; derive it from MODIS along-scan pixel expansion,
        // leaving VIIRS as None because VIIRS uses different swath geometry and needs no correction.
        let vza = if sensor.is_modis() {
            pyrosync_core::vza::vza_from_scan(scan)
        } else {
            None
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
            vza,
            fire_type,
        });
    }

    Ok(hotspots)
}

fn field<'a>(
    record: &'a csv::StringRecord,
    idx: usize,
    row: usize,
    name: &str,
) -> Result<&'a str> {
    record
        .get(idx)
        .ok_or_else(|| anyhow!("CSV data row {row}: missing `{name}` field"))
}

fn parse_number(value: &str, name: &str, row: usize) -> Result<f64> {
    let parsed: f64 = value
        .trim()
        .parse()
        .with_context(|| format!("CSV data row {row}: invalid `{name}` value {value:?}"))?;
    if !parsed.is_finite() {
        bail!("CSV data row {row}: non-finite `{name}` value {value:?}");
    }
    Ok(parsed)
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
    let s = s.trim();
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

    if s.contains(':') {
        return NaiveTime::parse_from_str(s, "%H:%M").ok();
    }

    if s.is_empty() || s.len() > 4 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    // MODIS_NRT strips leading zeros, so "614" is 06:14 rather than a malformed HHMM.
    let digits: u32 = s.parse().ok()?;
    NaiveTime::from_hms_opt(digits / 100, digits % 100, 0)
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
    fn blank_type_defaults_to_vegetation_fire_and_numeric_codes_map() {
        let csv = concat!(
            "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight,type\n",
            "-20.1,57.5,330.5,1.0,1.0,2024-01-15,1015,Terra,MODIS,75,6.1NRT,290.1,12.5,D,\n",
            "-20.2,57.6,340.0,1.0,1.0,2024-01-15,1015,Terra,MODIS,80,6.1NRT,290.1,20.0,D,0\n",
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

    #[test]
    fn parse_hhmm_reads_four_digit_value() {
        assert_eq!(
            parse_hhmm("1507"),
            Some(NaiveTime::from_hms_opt(15, 7, 0).unwrap())
        );
    }

    #[test]
    fn parse_hhmm_pads_three_digit_value_with_leading_zero() {
        assert_eq!(
            parse_hhmm("614"),
            Some(NaiveTime::from_hms_opt(6, 14, 0).unwrap())
        );
        assert_eq!(
            parse_hhmm("149"),
            Some(NaiveTime::from_hms_opt(1, 49, 0).unwrap())
        );
        assert_eq!(
            parse_hhmm("102"),
            Some(NaiveTime::from_hms_opt(1, 2, 0).unwrap())
        );
    }

    #[test]
    fn parse_hhmm_pads_short_values_with_leading_zeroes() {
        assert_eq!(
            parse_hhmm("45"),
            Some(NaiveTime::from_hms_opt(0, 45, 0).unwrap())
        );
        assert_eq!(
            parse_hhmm("7"),
            Some(NaiveTime::from_hms_opt(0, 7, 0).unwrap())
        );
        assert_eq!(
            parse_hhmm("0"),
            Some(NaiveTime::from_hms_opt(0, 0, 0).unwrap())
        );
    }

    #[test]
    fn parse_hhmm_accepts_colon_separated_value() {
        assert_eq!(
            parse_hhmm("15:07"),
            Some(NaiveTime::from_hms_opt(15, 7, 0).unwrap())
        );
    }

    #[test]
    fn parse_hhmm_rejects_malformed_values() {
        for input in ["", "abcd", "12345", "2561", "1999", "12:3x", "1 5", "é"] {
            assert!(
                parse_hhmm(input).is_none(),
                "input {input:?} should be rejected"
            );
        }
    }

    #[test]
    fn modis_nrt_three_digit_acq_time_parses_to_utc_timestamp() {
        let csv = concat!(
            "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight,type\n",
            "-20.1,57.5,330.5,1.0,1.0,2024-01-15,614,Terra,MODIS,75,6.1NRT,290.1,12.5,D,0\n",
        );
        let hotspots = parse_csv(csv, "MODIS_NRT").unwrap();
        assert_eq!(hotspots.len(), 1);
        assert_eq!(
            hotspots[0].acquired,
            Utc.with_ymd_and_hms(2024, 1, 15, 6, 14, 0).unwrap()
        );
    }

    #[test]
    fn short_data_row_reports_missing_frp_instead_of_panicking() {
        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time\n",
            "39.0,32.0\n",
        );
        let err = parse_csv(csv, "MODIS_NRT").unwrap_err().to_string();
        assert!(err.contains("row 1"), "unexpected error: {err}");
        assert!(err.contains("frp"), "unexpected error: {err}");
    }

    #[test]
    fn non_numeric_latitude_reports_row_and_field() {
        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time\n",
            "39.0,32.0,10,2024-01-15,1015\n",
            "not-a-number,32.0,10,2024-01-15,1015\n",
        );
        let err = parse_csv(csv, "MODIS_NRT").unwrap_err().to_string();
        assert!(err.contains("row 2"), "unexpected error: {err}");
        assert!(err.contains("latitude"), "unexpected error: {err}");
    }

    #[test]
    fn non_finite_lat_lon_frp_are_rejected() {
        for (field, row) in [
            ("latitude", "-20.1,57.5,12.5,2024-01-15,1015"),
            ("longitude", "-20.1,57.5,12.5,2024-01-15,1015"),
            ("frp", "-20.1,57.5,12.5,2024-01-15,1015"),
        ] {
            let row = match field {
                "latitude" => row.replacen("-20.1", "NaN", 1),
                "longitude" => row.replacen("57.5", "inf", 1),
                _ => row.replacen("12.5", "-inf", 1),
            };
            let csv = format!(
                "latitude,longitude,frp,acq_date,acq_time\n{row}\n"
            );
            let err = parse_csv(&csv, "MODIS_NRT").unwrap_err().to_string();
            assert!(err.contains("row 1"), "{field}: unexpected error: {err}");
            assert!(err.contains(field), "{field}: unexpected error: {err}");
        }
    }

    #[test]
    fn invalid_acq_date_reports_row_and_field() {
        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time\n",
            "-20.1,57.5,12.5,2024-01-15,1015\n",
            "-20.2,57.6,12.5,2024-13-40,1015\n",
        );
        let err = parse_csv(csv, "MODIS_NRT").unwrap_err().to_string();
        assert!(err.contains("row 2"), "unexpected error: {err}");
        assert!(err.contains("acq_date"), "unexpected error: {err}");
    }

    #[test]
    fn invalid_acq_time_reports_row_and_field() {
        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time\n",
            "-20.1,57.5,12.5,2024-01-15,1015\n",
            "-20.2,57.6,12.5,2024-01-15,2561\n",
        );
        let err = parse_csv(csv, "MODIS_NRT").unwrap_err().to_string();
        assert!(err.contains("row 2"), "unexpected error: {err}");
        assert!(err.contains("acq_time"), "unexpected error: {err}");
    }

    #[test]
    fn confidence_is_trimmed_without_changing_thresholds() {
        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time,confidence,daynight\n",
            "-20.1,57.5,12.5,2024-01-15,1015, 85 ,N\n",
            "-20.1,57.5,12.5,2024-01-15,1015, 30 ,N\n",
            "-20.1,57.5,12.5,2024-01-15,1015, 29 ,N\n",
            "-20.1,57.5,12.5,2024-01-15,1015,79,N\n",
            "-20.1,57.5,12.5,2024-01-15,1015,80,N\n",
            "-20.1,57.5,12.5,2024-01-15,1015, H ,N\n",
            "-20.1,57.5,12.5,2024-01-15,1015, l ,N\n",
        );
        let hotspots = parse_csv(csv, "MODIS_NRT").unwrap();
        let got: Vec<Confidence> = hotspots.iter().map(|h| h.confidence).collect();
        assert_eq!(
            got,
            vec![
                Confidence::High,
                Confidence::Nominal,
                Confidence::Low,
                Confidence::Nominal,
                Confidence::High,
                Confidence::High,
                Confidence::Low,
            ]
        );
    }

    #[test]
    fn daynight_is_trimmed_and_validated() {
        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time,daynight\n",
            "-20.1,57.5,12.5,2024-01-15,1015, D \n",
            "-20.2,57.6,12.5,2024-01-15,1015,N\n",
            "-20.3,57.7,12.5,2024-01-15,1015,X\n",
        );
        let err = parse_csv(csv, "MODIS_NRT").unwrap_err().to_string();
        assert!(err.contains("row 3"), "unexpected error: {err}");
        assert!(err.contains("daynight"), "unexpected error: {err}");

        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time,daynight\n",
            "-20.1,57.5,12.5,2024-01-15,1015, D \n",
            "-20.2,57.6,12.5,2024-01-15,1015,N\n",
        );
        let hotspots = parse_csv(csv, "MODIS_NRT").unwrap();
        assert_eq!(hotspots[0].daynight, DayNight::Day);
        assert_eq!(hotspots[1].daynight, DayNight::Night);
    }

    #[test]
    fn daynight_column_absent_defaults_to_day() {
        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time\n",
            "-20.1,57.5,12.5,2024-01-15,1015\n",
        );
        let hotspots = parse_csv(csv, "MODIS_NRT").unwrap();
        assert_eq!(hotspots[0].daynight, DayNight::Day);
    }

    #[test]
    fn non_numeric_type_is_rejected() {
        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time,type\n",
            "-20.1,57.5,12.5,2024-01-15,1015,x\n",
        );
        let err = parse_csv(csv, "MODIS_SP").unwrap_err().to_string();
        assert!(err.contains("row 1"), "unexpected error: {err}");
        assert!(err.contains("type"), "unexpected error: {err}");
    }

    #[test]
    fn blank_type_defaults_to_vegetation_fire() {
        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time,type\n",
            "-20.1,57.5,12.5,2024-01-15,1015,\n",
        );
        let hotspots = parse_csv(csv, "MODIS_SP").unwrap();
        assert_eq!(hotspots[0].fire_type, FireType::VegetationFire);
    }

    #[test]
    fn modis_satellite_variants_map_to_terra_and_aqua() {
        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time,satellite\n",
            "-20.1,57.5,12.5,2024-01-15,1015,T\n",
            "-20.2,57.6,12.5,2024-01-15,1015, Terra \n",
            "-20.3,57.7,12.5,2024-01-15,1015,a\n",
            "-20.4,57.8,12.5,2024-01-15,1015,AQUA\n",
        );
        let hotspots = parse_csv(csv, "MODIS_SP").unwrap();
        let got: Vec<Sensor> = hotspots.iter().map(|h| h.sensor).collect();
        assert_eq!(
            got,
            vec![
                Sensor::ModisTerra,
                Sensor::ModisTerra,
                Sensor::ModisAqua,
                Sensor::ModisAqua,
            ]
        );
    }

    #[test]
    fn invalid_modis_satellite_is_rejected() {
        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time,satellite\n",
            "-20.1,57.5,12.5,2024-01-15,1015,N20\n",
        );
        let err = parse_csv(csv, "MODIS_SP").unwrap_err().to_string();
        assert!(err.contains("row 1"), "unexpected error: {err}");
        assert!(err.contains("satellite"), "unexpected error: {err}");
    }

    #[test]
    fn missing_modis_satellite_column_still_defaults_to_aqua() {
        let csv = concat!(
            "latitude,longitude,frp,acq_date,acq_time\n",
            "-20.1,57.5,12.5,2024-01-15,1015\n",
        );
        let hotspots = parse_csv(csv, "MODIS_SP").unwrap();
        assert_eq!(hotspots[0].sensor, Sensor::ModisAqua);
    }

    #[test]
    fn modis_nrt_derives_vza_from_scan() {
        let csv = concat!(
            "latitude,longitude,brightness,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_t31,frp,daynight\n",
            "-12.07836,26.41462,307.1,2.21,1.44,2026-11-13,614,Terra,MODIS,43,6.1NRT,291.89,12.34,D\n",
        );
        let hotspots = parse_csv(csv, "MODIS_NRT").unwrap();
        assert_eq!(hotspots.len(), 1);
        let vza = hotspots[0].vza.expect("expected derived VZA for MODIS row");
        assert!(vza > 0.0 && vza < 70.0, "unexpected derived VZA: {vza}");
    }
}
