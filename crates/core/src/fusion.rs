use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::config::PipelineConfig;
use crate::harmonize::HarmonizeLinks;
use crate::models::{DayNight, HarmonizedHotspot, Hotspot, HotspotSource, Sensor};
use crate::uncertainty::{self, SensorFamily};
use crate::vza;

/// How a matched MODIS/VIIRS pair is turned into one harmonized FRP value. A view option applied
/// after harmonization, so it never changes the pipeline configuration or its digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FusionModel {
    /// VIIRS wins on matched pairs (the pipeline default).
    #[default]
    ViirsPreferred,
    /// Matched pairs average the MODIS and VIIRS group values.
    EqualWeight,
    /// MODIS is the reference sensor: matched pairs take the MODIS value and VIIRS-only
    /// detections MODIS could not have seen are dropped.
    ModisCalibrated,
}

impl FusionModel {
    pub const VALID: &'static str = "viirs_preferred, equal_weight, modis_calibrated";

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ViirsPreferred => "viirs_preferred",
            Self::EqualWeight => "equal_weight",
            Self::ModisCalibrated => "modis_calibrated",
        }
    }
}

impl fmt::Display for FusionModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FusionModel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "viirs_preferred" => Ok(Self::ViirsPreferred),
            "equal_weight" => Ok(Self::EqualWeight),
            "modis_calibrated" => Ok(Self::ModisCalibrated),
            other => Err(format!(
                "invalid 'fusion_model' value '{other}'; valid values: {}",
                Self::VALID
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FusionOutput {
    pub hotspots: Vec<HarmonizedHotspot>,
    /// Old harmonized index to new index; `None` when the record was dropped.
    pub index_map: Vec<Option<usize>>,
    pub dropped_below_modis_limit: usize,
}

struct GroupStats {
    frp_corrected: f64,
    frp_raw: f64,
    lat: f64,
    lon: f64,
    vza_factor: Option<f64>,
    n: u16,
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, n) = values.fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
    sum / n as f64
}

fn group_stats(rows: &[&Hotspot], config: &PipelineConfig) -> Option<GroupStats> {
    if rows.is_empty() {
        return None;
    }
    let factors: Vec<f64> = rows
        .iter()
        .filter_map(|h| vza::vza_factor_with(h, &config.vza))
        .collect();
    Some(GroupStats {
        frp_corrected: mean(rows.iter().map(|h| {
            vza::apply_vza_correction(h.frp, vza::vza_factor_with(h, &config.vza))
        })),
        frp_raw: mean(rows.iter().map(|h| h.frp)),
        lat: mean(rows.iter().map(|h| h.lat)),
        lon: mean(rows.iter().map(|h| h.lon)),
        vza_factor: (!factors.is_empty()).then(|| mean(factors.into_iter())),
        n: u16::try_from(rows.len()).unwrap_or(u16::MAX),
    })
}

fn set_interval(
    h: &mut HarmonizedHotspot,
    family: SensorFamily,
    n_obs: u16,
    config: &PipelineConfig,
) {
    let est = uncertainty::frp_uncertainty_for_obs(
        h.frp_corrected,
        h.vza_factor,
        family,
        None,
        n_obs,
        &config.uncertainty,
    );
    h.frp_lower = est.lower;
    h.frp_upper = est.upper;
}

/// `modis`/`viirs` must be the same rows, in the same order, that produced `links`.
pub fn apply_fusion_model(
    links: &HarmonizeLinks,
    modis: &[Hotspot],
    viirs: &[Hotspot],
    model: FusionModel,
    config: &PipelineConfig,
) -> FusionOutput {
    let n = links.hotspots.len();
    if model == FusionModel::ViirsPreferred {
        return FusionOutput {
            hotspots: links.hotspots.clone(),
            index_map: (0..n).map(Some).collect(),
            dropped_below_modis_limit: 0,
        };
    }

    let mut modis_members: Vec<Vec<&Hotspot>> = vec![Vec::new(); n];
    let mut viirs_members: Vec<Vec<&Hotspot>> = vec![Vec::new(); n];
    for (row, link) in modis.iter().zip(&links.modis_links) {
        if let Some(o) = *link {
            modis_members[o].push(row);
        }
    }
    for (row, link) in viirs.iter().zip(&links.viirs_links) {
        if let Some(o) = *link {
            viirs_members[o].push(row);
        }
    }

    let mut hotspots = Vec::with_capacity(n);
    let mut index_map = Vec::with_capacity(n);
    let mut dropped = 0usize;

    for (i, original) in links.hotspots.iter().enumerate() {
        let mut h = original.clone();
        match (model, original.source) {
            (FusionModel::EqualWeight, HotspotSource::Matched) => {
                if let (Some(m), Some(v)) = (
                    group_stats(&modis_members[i], config),
                    group_stats(&viirs_members[i], config),
                ) {
                    h.frp_corrected = (m.frp_corrected + v.frp_corrected) / 2.0;
                    h.lat = (m.lat + v.lat) / 2.0;
                    h.lon = (m.lon + v.lon) / 2.0;
                    h.vza_factor = None;
                    let n_obs = h.n_obs;
                    set_interval(&mut h, SensorFamily::Viirs, n_obs, config);
                }
            }
            (FusionModel::ModisCalibrated, HotspotSource::Matched) => {
                if let Some(m) = group_stats(&modis_members[i], config) {
                    h.frp = m.frp_raw;
                    h.frp_corrected = m.frp_corrected;
                    h.lat = m.lat;
                    h.lon = m.lon;
                    h.vza_factor = m.vza_factor;
                    set_interval(&mut h, SensorFamily::Modis, m.n, config);
                }
            }
            (FusionModel::ModisCalibrated, HotspotSource::ViirsOnly)
                if below_modis_limit(h.frp_corrected, h.daynight) =>
            {
                dropped += 1;
                index_map.push(None);
                continue;
            }
            _ => {}
        }
        index_map.push(Some(hotspots.len()));
        hotspots.push(h);
    }

    FusionOutput {
        hotspots,
        index_map,
        dropped_below_modis_limit: dropped,
    }
}

fn below_modis_limit(frp: f64, daynight: DayNight) -> bool {
    let coeffs = vza::detection_limit_coefficients(Sensor::ModisTerra, daynight);
    vza::detection_limit_mw(&coeffs, 1.0).is_some_and(|limit| frp < limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harmonize::harmonize_with_links_config;
    use crate::models::{Confidence, FireType};
    use chrono::{TimeZone, Utc};

    fn hotspot(
        sensor: Sensor,
        lat: f64,
        lon: f64,
        frp: f64,
        minutes: i64,
        vza: Option<f64>,
    ) -> Hotspot {
        let (scan, track, satellite) = if sensor.is_modis() {
            (1.0, 1.0, "Terra")
        } else {
            (0.4, 0.4, "N")
        };
        Hotspot {
            lat,
            lon,
            brightness: 330.0,
            frp,
            confidence: Confidence::Nominal,
            sensor,
            acquired: Utc.with_ymd_and_hms(2021, 8, 1, 10, 0, 0).unwrap()
                + chrono::Duration::minutes(minutes),
            scan,
            track,
            satellite: satellite.to_string(),
            daynight: DayNight::Day,
            vza,
            fire_type: FireType::VegetationFire,
        }
    }

    fn fixture() -> (Vec<Hotspot>, Vec<Hotspot>) {
        let modis = vec![hotspot(Sensor::ModisTerra, 38.000, 30.000, 40.0, 0, Some(0.0))];
        let viirs = vec![
            hotspot(Sensor::ViirsSNPP, 38.004, 30.004, 20.0, 60, None),
            hotspot(Sensor::ViirsSNPP, 10.0, 10.0, 1.0, 0, None),
            hotspot(Sensor::ViirsSNPP, -20.0, 20.0, 50.0, 0, None),
        ];
        (modis, viirs)
    }

    fn run(model: FusionModel) -> (HarmonizeLinks, FusionOutput) {
        let (modis, viirs) = fixture();
        let config = PipelineConfig::default();
        let links = harmonize_with_links_config(modis.clone(), viirs.clone(), &config);
        let out = apply_fusion_model(&links, &modis, &viirs, model, &config);
        (links, out)
    }

    fn matched(hs: &[HarmonizedHotspot]) -> &HarmonizedHotspot {
        hs.iter()
            .find(|h| h.source == HotspotSource::Matched)
            .expect("matched record")
    }

    #[test]
    fn default_is_identity() {
        let (links, out) = run(FusionModel::default());
        assert_eq!(FusionModel::default(), FusionModel::ViirsPreferred);
        assert_eq!(
            serde_json::to_string(&links.hotspots).unwrap(),
            serde_json::to_string(&out.hotspots).unwrap()
        );
        let identity: Vec<Option<usize>> = (0..links.hotspots.len()).map(Some).collect();
        assert_eq!(out.index_map, identity);
        assert_eq!(out.dropped_below_modis_limit, 0);
    }

    #[test]
    fn equal_weight_averages_and_moves_to_midpoint() {
        let (links, out) = run(FusionModel::EqualWeight);
        assert_eq!(out.hotspots.len(), links.hotspots.len());
        let m = matched(&out.hotspots);
        assert!((m.frp_corrected - 30.0).abs() < 1e-9, "{}", m.frp_corrected);
        assert!((m.lat - 38.002).abs() < 1e-9);
        assert!((m.lon - 30.002).abs() < 1e-9);
        assert!(m.frp_lower <= m.frp_corrected && m.frp_corrected <= m.frp_upper);
        assert_eq!(out.dropped_below_modis_limit, 0);
    }

    #[test]
    fn modis_calibrated_uses_modis_value_and_drops_weak_viirs_only() {
        let (links, out) = run(FusionModel::ModisCalibrated);
        let m = matched(&out.hotspots);
        assert!((m.frp_corrected - 40.0).abs() < 1e-9);
        assert!((m.lat - 38.0).abs() < 1e-9);
        assert_eq!(out.dropped_below_modis_limit, 1);
        assert_eq!(out.hotspots.len(), links.hotspots.len() - 1);
        assert!(out.hotspots.iter().all(|h| h.frp_corrected != 1.0));
        assert!(out.hotspots.iter().any(|h| h.frp_corrected == 50.0));

        let dropped: Vec<usize> = out
            .index_map
            .iter()
            .enumerate()
            .filter(|(_, m)| m.is_none())
            .map(|(i, _)| i)
            .collect();
        assert_eq!(dropped.len(), 1);
        assert_eq!(links.hotspots[dropped[0]].frp_corrected, 1.0);
        for (old, new) in out.index_map.iter().enumerate() {
            if let Some(new) = new {
                assert_eq!(links.hotspots[old].acquired, out.hotspots[*new].acquired);
                assert_eq!(links.hotspots[old].source, out.hotspots[*new].source);
            }
        }
    }

    #[test]
    fn parse_round_trips_and_rejects_unknown() {
        for m in [
            FusionModel::ViirsPreferred,
            FusionModel::EqualWeight,
            FusionModel::ModisCalibrated,
        ] {
            assert_eq!(m.as_str().parse::<FusionModel>().unwrap(), m);
            assert_eq!(
                serde_json::to_string(&m).unwrap(),
                format!("\"{}\"", m.as_str())
            );
        }
        let err = "bogus".parse::<FusionModel>().unwrap_err();
        assert!(err.contains("fusion_model") && err.contains("modis_calibrated"));
    }
}
