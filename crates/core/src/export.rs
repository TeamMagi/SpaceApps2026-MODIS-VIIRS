use std::fmt::Write;

use chrono::SecondsFormat;
use serde_json::{Value, json};

use crate::models::HarmonizedHotspot;

pub const CSV_HEADER: &str = "lat,lon,frp_mw,frp_corrected_mw,vza_factor,n_obs,platforms,source,confidence,daynight,acquired_utc,calibration_ratio,frp_lower_mw,frp_upper_mw";

const CSV_ROW_CAPACITY: usize = 128;

pub fn to_csv(hotspots: &[HarmonizedHotspot]) -> String {
    let mut out = String::with_capacity((hotspots.len() + 1) * CSV_ROW_CAPACITY);
    out.push_str(CSV_HEADER);
    out.push_str("\r\n");

    for h in hotspots {
        let _ = write!(out, "{},{},{},{},", h.lat, h.lon, h.frp, h.frp_corrected);
        if let Some(vza) = h.vza_factor {
            let _ = write!(out, "{vza}");
        }
        let _ = write!(out, ",{},", h.n_obs);
        // '|' keeps the platforms cell free of commas, so it never needs CSV quoting.
        for (i, platform) in h.platforms.iter().enumerate() {
            if i > 0 {
                out.push('|');
            }
            let _ = write!(out, "{platform:?}");
        }
        let _ = write!(
            out,
            ",{:?},{:?},{:?},{},",
            h.source,
            h.confidence,
            h.daynight,
            h.acquired.to_rfc3339_opts(SecondsFormat::Secs, true),
        );
        if let Some(ratio) = h.calibration_ratio {
            let _ = write!(out, "{ratio}");
        }
        let _ = write!(out, ",{},{}", h.frp_lower, h.frp_upper);
        out.push_str("\r\n");
    }

    out
}

pub fn to_geojson(hotspots: &[HarmonizedHotspot]) -> String {
    let features: Vec<Value> = hotspots
        .iter()
        .map(|h| {
            json!({
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [h.lon, h.lat] },
                "properties": {
                    "frp_mw": h.frp,
                    "frp_corrected_mw": h.frp_corrected,
                    "vza_factor": h.vza_factor,
                    "n_obs": h.n_obs,
                    "platforms": h.platforms.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>(),
                    "source": format!("{:?}", h.source),
                    "confidence": format!("{:?}", h.confidence),
                    "daynight": format!("{:?}", h.daynight),
                    "acquired_utc": h.acquired.to_rfc3339_opts(SecondsFormat::Secs, true),
                    "calibration_ratio": h.calibration_ratio,
                    "frp_lower_mw": h.frp_lower,
                    "frp_upper_mw": h.frp_upper,
                },
            })
        })
        .collect();

    json!({ "type": "FeatureCollection", "features": features }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Confidence, DayNight, HotspotSource, Sensor};
    use chrono::{TimeZone, Utc};

    fn hotspot(vza_factor: Option<f64>, platforms: Vec<Sensor>) -> HarmonizedHotspot {
        HarmonizedHotspot {
            lat: 38.5,
            lon: 27.25,
            frp: 12.5,
            frp_corrected: 10.0,
            frp_lower: 9.2,
            frp_upper: 10.8,
            vza_factor,
            calibration_ratio: None,
            n_obs: platforms.len() as u16,
            platforms,
            confidence: Confidence::High,
            source: HotspotSource::Matched,
            acquired: Utc.with_ymd_and_hms(2026, 7, 1, 10, 30, 0).unwrap(),
            daynight: DayNight::Day,
        }
    }

    #[test]
    fn csv_rows_match_header_column_count() {
        let hotspots = vec![
            hotspot(Some(0.8), vec![Sensor::ModisTerra, Sensor::ViirsSNPP]),
            hotspot(None, vec![Sensor::ViirsNOAA20]),
        ];
        let csv = to_csv(&hotspots);
        assert!(csv.ends_with("\r\n"));
        let lines: Vec<&str> = csv.trim_end_matches("\r\n").split("\r\n").collect();
        assert_eq!(lines[0], CSV_HEADER);
        assert_eq!(lines.len(), 3);
        let columns = CSV_HEADER.split(',').count();
        for line in &lines {
            assert_eq!(line.split(',').count(), columns);
        }
    }

    #[test]
    fn empty_input_gives_header_only_and_empty_features() {
        assert_eq!(to_csv(&[]), format!("{CSV_HEADER}\r\n"));
        assert_eq!(
            to_geojson(&[]),
            r#"{"features":[],"type":"FeatureCollection"}"#
        );
    }

    #[test]
    fn missing_vza_and_calibration_are_empty_cell_and_json_null() {
        let hotspots = vec![hotspot(None, vec![Sensor::ViirsSNPP])];
        let csv = to_csv(&hotspots);
        let row = csv.lines().nth(1).unwrap();
        assert_eq!(row.split(',').nth(4), Some(""));
        assert_eq!(row.split(',').nth(11), Some(""));

        let parsed: Value = serde_json::from_str(&to_geojson(&hotspots)).unwrap();
        assert!(parsed["features"][0]["properties"]["vza_factor"].is_null());
        assert!(parsed["features"][0]["properties"]["calibration_ratio"].is_null());
    }

    #[test]
    fn present_calibration_ratio_is_exported_in_csv_and_geojson() {
        let mut h = hotspot(Some(1.2), vec![Sensor::ModisTerra]);
        h.source = HotspotSource::ModisOnly;
        h.calibration_ratio = Some(1.16);
        let hotspots = vec![h];

        let csv = to_csv(&hotspots);
        let row = csv.lines().nth(1).unwrap();
        assert_eq!(row.split(',').nth(11), Some("1.16"));

        let parsed: Value = serde_json::from_str(&to_geojson(&hotspots)).unwrap();
        assert_eq!(parsed["features"][0]["properties"]["calibration_ratio"], json!(1.16));
    }

    #[test]
    fn platforms_are_pipe_joined_in_csv() {
        let hotspots = vec![hotspot(Some(1.0), vec![Sensor::ModisTerra, Sensor::ViirsSNPP])];
        let csv = to_csv(&hotspots);
        let row = csv.lines().nth(1).unwrap();
        assert_eq!(row.split(',').nth(6), Some("ModisTerra|ViirsSNPP"));
        assert_eq!(row.split(',').nth(10), Some("2026-07-01T10:30:00Z"));
    }

    #[test]
    fn geojson_round_trips_with_lon_first() {
        let hotspots = vec![hotspot(Some(0.5), vec![Sensor::ModisTerra, Sensor::ViirsSNPP])];
        let parsed: Value = serde_json::from_str(&to_geojson(&hotspots)).unwrap();
        assert_eq!(parsed["type"], "FeatureCollection");
        let feature = &parsed["features"][0];
        assert_eq!(feature["geometry"]["type"], "Point");
        assert_eq!(feature["geometry"]["coordinates"], json!([27.25, 38.5]));
        assert_eq!(
            feature["properties"]["platforms"],
            json!(["ModisTerra", "ViirsSNPP"])
        );
        assert_eq!(feature["properties"]["vza_factor"], json!(0.5));
        assert_eq!(feature["properties"]["source"], "Matched");
        assert_eq!(feature["properties"]["frp_lower_mw"], json!(9.2));
        assert_eq!(feature["properties"]["frp_upper_mw"], json!(10.8));

        let csv = to_csv(&hotspots);
        let row = csv.lines().nth(1).unwrap();
        assert_eq!(row.split(',').nth(12), Some("9.2"));
        assert_eq!(row.split(',').nth(13), Some("10.8"));
    }

    #[test]
    fn large_input_yields_one_line_per_hotspot() {
        let hotspots: Vec<_> = (0..10_000)
            .map(|_| hotspot(Some(1.0), vec![Sensor::ViirsSNPP]))
            .collect();
        assert_eq!(to_csv(&hotspots).lines().count(), 10_001);
    }
}
