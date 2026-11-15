use std::fmt::Write;

use chrono::SecondsFormat;
use serde_json::{Value, json};

use crate::models::{HarmonizedHotspot, Hotspot};

pub const CSV_HEADER: &str = "lat,lon,frp_mw,frp_corrected_mw,vza_factor,n_obs,platforms,source,confidence,daynight,acquired_utc,frp_lower_mw,frp_upper_mw";

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
            ",{:?},{:?},{:?},{},{},{}",
            h.source,
            h.confidence,
            h.daynight,
            h.acquired.to_rfc3339_opts(SecondsFormat::Secs, true),
            h.frp_lower,
            h.frp_upper,
        );
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
                    "frp_lower_mw": h.frp_lower,
                    "frp_upper_mw": h.frp_upper,
                },
            })
        })
        .collect();

    json!({ "type": "FeatureCollection", "features": features }).to_string()
}

pub const RAW_CSV_HEADER: &str =
    "latitude,longitude,sensor,platform,frp,confidence,acquired,daynight";

fn sensor_family(h: &Hotspot) -> &'static str {
    if h.sensor.is_modis() { "MODIS" } else { "VIIRS" }
}

pub fn raw_to_csv(hotspots: &[Hotspot]) -> String {
    let mut out = String::with_capacity((hotspots.len() + 1) * CSV_ROW_CAPACITY);
    out.push_str(RAW_CSV_HEADER);
    out.push_str("\r\n");
    for h in hotspots {
        let _ = write!(
            out,
            "{},{},{},{:?},{},{:?},{},{:?}\r\n",
            h.lat,
            h.lon,
            sensor_family(h),
            h.sensor,
            h.frp,
            h.confidence,
            h.acquired.to_rfc3339_opts(SecondsFormat::Secs, true),
            h.daynight,
        );
    }
    out
}

pub fn raw_to_geojson(hotspots: &[Hotspot]) -> String {
    let features: Vec<Value> = hotspots
        .iter()
        .map(|h| {
            json!({
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [h.lon, h.lat] },
                "properties": {
                    "sensor": sensor_family(h),
                    "platform": format!("{:?}", h.sensor),
                    "frp": h.frp,
                    "confidence": format!("{:?}", h.confidence),
                    "acquired": h.acquired.to_rfc3339_opts(SecondsFormat::Secs, true),
                    "daynight": format!("{:?}", h.daynight),
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

    fn raw_hotspot(sensor: Sensor) -> Hotspot {
        Hotspot {
            lat: 38.5,
            lon: 27.25,
            brightness: 330.0,
            frp: 7.5,
            confidence: Confidence::Nominal,
            sensor,
            acquired: chrono::Utc.with_ymd_and_hms(2026, 7, 1, 10, 30, 0).unwrap(),
            scan: 1.0,
            track: 1.0,
            satellite: "N".to_string(),
            daynight: DayNight::Night,
            vza: None,
            fire_type: crate::models::FireType::VegetationFire,
        }
    }

    #[test]
    fn raw_export_writes_family_platform_and_lon_first_geometry() {
        let hs = [raw_hotspot(Sensor::ModisAqua), raw_hotspot(Sensor::ViirsSNPP)];
        let csv = raw_to_csv(&hs);
        let lines: Vec<&str> = csv.trim_end_matches("\r\n").split("\r\n").collect();
        assert_eq!(lines[0], RAW_CSV_HEADER);
        assert_eq!(lines[1], "38.5,27.25,MODIS,ModisAqua,7.5,Nominal,2026-07-01T10:30:00Z,Night");
        assert_eq!(lines[2].split(',').nth(2), Some("VIIRS"));

        let parsed: Value = serde_json::from_str(&raw_to_geojson(&hs)).unwrap();
        assert_eq!(parsed["features"].as_array().map(Vec::len), Some(2));
        assert_eq!(parsed["features"][0]["geometry"]["coordinates"], json!([27.25, 38.5]));
        assert_eq!(parsed["features"][1]["properties"]["platform"], "ViirsSNPP");
        assert_eq!(raw_to_csv(&[]), format!("{RAW_CSV_HEADER}\r\n"));
    }

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
            hotspot(Some(1.2), vec![Sensor::ModisTerra, Sensor::ViirsSNPP]),
            hotspot(None, vec![Sensor::ViirsNOAA20]),
        ];
        let csv = to_csv(&hotspots);
        assert!(csv.ends_with("\r\n"));
        assert!(!CSV_HEADER.contains("calibration_ratio"));
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
    fn missing_vza_is_empty_cell_and_json_null() {
        let hotspots = vec![hotspot(None, vec![Sensor::ViirsSNPP])];
        let csv = to_csv(&hotspots);
        let row = csv.lines().nth(1).unwrap();
        assert_eq!(row.split(',').nth(4), Some(""));

        let parsed: Value = serde_json::from_str(&to_geojson(&hotspots)).unwrap();
        assert!(parsed["features"][0]["properties"]["vza_factor"].is_null());
        assert!(parsed["features"][0]["properties"].get("calibration_ratio").is_none());
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
        let hotspots = vec![hotspot(Some(1.25), vec![Sensor::ModisTerra, Sensor::ViirsSNPP])];
        let parsed: Value = serde_json::from_str(&to_geojson(&hotspots)).unwrap();
        assert_eq!(parsed["type"], "FeatureCollection");
        let feature = &parsed["features"][0];
        assert_eq!(feature["geometry"]["type"], "Point");
        assert_eq!(feature["geometry"]["coordinates"], json!([27.25, 38.5]));
        assert_eq!(
            feature["properties"]["platforms"],
            json!(["ModisTerra", "ViirsSNPP"])
        );
        assert_eq!(feature["properties"]["vza_factor"], json!(1.25));
        assert_eq!(feature["properties"]["source"], "Matched");
        assert_eq!(feature["properties"]["frp_lower_mw"], json!(9.2));
        assert_eq!(feature["properties"]["frp_upper_mw"], json!(10.8));
        assert!(feature["properties"].get("calibration_ratio").is_none());

        let csv = to_csv(&hotspots);
        let row = csv.lines().nth(1).unwrap();
        assert_eq!(row.split(',').nth(11), Some("9.2"));
        assert_eq!(row.split(',').nth(12), Some("10.8"));
    }

    #[test]
    fn large_input_yields_one_line_per_hotspot() {
        let hotspots: Vec<_> = (0..10_000)
            .map(|_| hotspot(Some(1.0), vec![Sensor::ViirsSNPP]))
            .collect();
        assert_eq!(to_csv(&hotspots).lines().count(), 10_001);
    }

    const ALL_SENSORS: [Sensor; 5] = [
        Sensor::ModisTerra,
        Sensor::ModisAqua,
        Sensor::ViirsSNPP,
        Sensor::ViirsNOAA20,
        Sensor::ViirsNOAA21,
    ];

    fn csv_rows(csv: &str) -> Vec<Vec<&str>> {
        csv.trim_end_matches("\r\n")
            .split("\r\n")
            .skip(1)
            .map(|l| l.split(',').collect())
            .collect()
    }

    fn col(name: &str) -> usize {
        CSV_HEADER.split(',').position(|c| c == name).unwrap_or_else(|| panic!("no column {name}"))
    }

    #[test]
    fn header_has_thirteen_unique_columns_and_geojson_properties_mirror_them() {
        let columns: Vec<&str> = CSV_HEADER.split(',').collect();
        assert_eq!(columns.len(), 13);
        let unique: std::collections::BTreeSet<_> = columns.iter().collect();
        assert_eq!(unique.len(), columns.len());

        let parsed: Value =
            serde_json::from_str(&to_geojson(&[hotspot(Some(1.0), vec![Sensor::ViirsSNPP])])).unwrap();
        let mut actual: Vec<&str> = parsed["features"][0]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        actual.sort_unstable();
        let mut expected: Vec<&str> = columns[2..].to_vec();
        expected.sort_unstable();
        assert_eq!(actual, expected);
    }

    #[test]
    fn empty_input_is_valid_empty_output_not_an_error() {
        let csv = to_csv(&[]);
        assert_eq!(csv.matches("\r\n").count(), 1);
        let parsed: Value = serde_json::from_str(&to_geojson(&[])).unwrap();
        assert_eq!(parsed["type"], "FeatureCollection");
        assert_eq!(parsed["features"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn every_single_sensor_exports_one_platform_without_separator() {
        for sensor in ALL_SENSORS {
            let hs = [hotspot(Some(1.0), vec![sensor])];
            let csv = to_csv(&hs);
            let rows = csv_rows(&csv);
            assert_eq!(rows[0].len(), 13);
            assert_eq!(rows[0][col("platforms")], format!("{sensor:?}"));
            assert_eq!(rows[0][col("n_obs")], "1");
            let parsed: Value = serde_json::from_str(&to_geojson(&hs)).unwrap();
            assert_eq!(
                parsed["features"][0]["properties"]["platforms"],
                json!([format!("{sensor:?}")])
            );
        }
    }

    #[test]
    fn all_low_confidence_input_is_exported_unfiltered() {
        let hs: Vec<_> = (0..4)
            .map(|_| {
                let mut h = hotspot(Some(1.1), vec![Sensor::ModisAqua]);
                h.confidence = Confidence::Low;
                h
            })
            .collect();
        let csv = to_csv(&hs);
        let rows = csv_rows(&csv);
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|r| r[col("confidence")] == "Low"));
        let parsed: Value = serde_json::from_str(&to_geojson(&hs)).unwrap();
        let features = parsed["features"].as_array().unwrap();
        assert_eq!(features.len(), 4);
        assert!(features.iter().all(|f| f["properties"]["confidence"] == "Low"));
    }

    #[test]
    fn every_enum_combination_keeps_column_count_and_has_no_quoting_characters() {
        let mut hs = Vec::new();
        for confidence in [Confidence::Low, Confidence::Nominal, Confidence::High] {
            for source in [HotspotSource::ModisOnly, HotspotSource::ViirsOnly, HotspotSource::Matched] {
                for daynight in [DayNight::Day, DayNight::Night] {
                    let mut h = hotspot(None, ALL_SENSORS.to_vec());
                    h.confidence = confidence;
                    h.source = source;
                    h.daynight = daynight;
                    hs.push(h);
                }
            }
        }
        let csv = to_csv(&hs);
        assert!(!csv.contains('"'));
        let rows = csv_rows(&csv);
        assert_eq!(rows.len(), 18);
        assert!(rows.iter().all(|r| r.len() == 13));
        assert!(!csv.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn extreme_coordinates_stay_in_range_and_lon_comes_first() {
        let corners = [(90.0, 180.0), (-90.0, -180.0), (0.0, 0.0), (-33.5, 151.25)];
        let hs: Vec<_> = corners
            .iter()
            .map(|&(lat, lon)| {
                let mut h = hotspot(Some(1.0), vec![Sensor::ViirsNOAA21]);
                h.lat = lat;
                h.lon = lon;
                h
            })
            .collect();
        let parsed: Value = serde_json::from_str(&to_geojson(&hs)).unwrap();
        let csv = to_csv(&hs);
        let rows = csv_rows(&csv);
        for (i, &(lat, lon)) in corners.iter().enumerate() {
            let geom = &parsed["features"][i]["geometry"];
            assert_eq!(geom["type"], "Point");
            let c = geom["coordinates"].as_array().unwrap();
            assert_eq!(c.len(), 2);
            let (glon, glat) = (c[0].as_f64().unwrap(), c[1].as_f64().unwrap());
            assert_eq!((glon, glat), (lon, lat));
            assert!((-180.0..=180.0).contains(&glon) && (-90.0..=90.0).contains(&glat));
            assert_eq!(rows[i][col("lat")].parse::<f64>().unwrap(), lat);
            assert_eq!(rows[i][col("lon")].parse::<f64>().unwrap(), lon);
        }
    }

    #[test]
    fn row_order_and_values_are_preserved_between_csv_and_geojson() {
        let hs: Vec<_> = [3.0, 1.0, 2.0]
            .iter()
            .map(|&frp| {
                let mut h = hotspot(Some(1.0), vec![Sensor::ViirsSNPP]);
                h.frp = frp;
                h
            })
            .collect();
        let csv = to_csv(&hs);
        let csv_frp: Vec<f64> = csv_rows(&csv).iter().map(|r| r[col("frp_mw")].parse().unwrap()).collect();
        let parsed: Value = serde_json::from_str(&to_geojson(&hs)).unwrap();
        let geo_frp: Vec<f64> = parsed["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["properties"]["frp_mw"].as_f64().unwrap())
            .collect();
        assert_eq!(csv_frp, vec![3.0, 1.0, 2.0]);
        assert_eq!(geo_frp, csv_frp);
    }
}
