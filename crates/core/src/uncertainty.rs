use std::f64::consts::FRAC_1_SQRT_2;

use serde::{Deserialize, Serialize};

use crate::models::{HotspotSource, Sensor};

/// Varsayım: `academic-papers/*_ANALYSIS.md` özetlerinde tek piksel MODIS radyometrik 1σ rastgele
/// gürültüsü doğrudan raporlanmadığından başlangıç varsayımı (1 km piksel ve yüksek tespit eşiği
/// nedeniyle VIIRS'ten yüksek; bkz. `02_ANALYSIS.md`, `06_07_08_COMBINED_ANALYSIS.md`).
pub const DEFAULT_MODIS_SENSOR_SIGMA: f64 = 0.12;

/// Varsayım: `academic-papers/*_ANALYSIS.md` özetlerinde tek piksel VIIRS radyometrik 1σ rastgele
/// gürültüsü doğrudan raporlanmadığından başlangıç varsayımı (375 m I-band çözünürlüğü ve
/// 0.4–3.7 MW düşük tespit tabanı nedeniyle MODIS'ten düşük; bkz. `02_ANALYSIS.md`, `06_07_08`).
pub const DEFAULT_VIIRS_SENSOR_SIGMA: f64 = 0.08;

/// Türetilmiş tahmin / varsayım: VZA düzeltmesi arttıkça (`factor - 1.0`) eklenen göreli 1σ hata
/// eğimi. Tarama kenarında (`factor ≈ 4.0`, MODIS piksel alanı ~9.66 km² ve ortalama FRP %300
/// artışı: `03_ANALYSIS.md`, `06_07_08_COMBINED_ANALYSIS.md`) σ_vza = %15 verir; `01_ANALYSIS.md`
/// gruplu polinomial fit RMSE ≤ %1.34 iken piksel ölçeği artık hatası daha büyüktür ve `vza.rs`
/// katsayıları henüz yer tutucudur.
pub const DEFAULT_VZA_ERROR_COEFF: f64 = 0.05;

/// Standart normal ±1σ kapsama olasılığı (~0.6827) için z çarpanı.
pub const DEFAULT_Z: f64 = 1.0;

/// Sensör ailesi; MODIS ve VIIRS birbirinden farklı radyometrik gürültü tabanına sahiptir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SensorFamily {
    Modis,
    Viirs,
}

impl From<Sensor> for SensorFamily {
    fn from(sensor: Sensor) -> Self {
        if sensor.is_modis() {
            Self::Modis
        } else {
            Self::Viirs
        }
    }
}

impl From<HotspotSource> for SensorFamily {
    fn from(source: HotspotSource) -> Self {
        match source {
            HotspotSource::ModisOnly => Self::Modis,
            // Kalite öncelikli birleştirme eşleşen kayıtlarda VIIRS FRP'yi seçer.
            HotspotSource::ViirsOnly | HotspotSource::Matched => Self::Viirs,
        }
    }
}

/// FRP belirsizlik yayılımı parametreleri (`PipelineConfig` ile uyumlu serde kuralları).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UncertaintyParams {
    pub modis_sensor_sigma: f64,
    pub viirs_sensor_sigma: f64,
    pub vza_error_coeff: f64,
    pub z: f64,
}

impl Default for UncertaintyParams {
    fn default() -> Self {
        Self {
            modis_sensor_sigma: DEFAULT_MODIS_SENSOR_SIGMA,
            viirs_sensor_sigma: DEFAULT_VIIRS_SENSOR_SIGMA,
            vza_error_coeff: DEFAULT_VZA_ERROR_COEFF,
            z: DEFAULT_Z,
        }
    }
}

impl UncertaintyParams {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("uncertainty.modis_sensor_sigma", self.modis_sensor_sigma),
            ("uncertainty.viirs_sensor_sigma", self.viirs_sensor_sigma),
            ("uncertainty.vza_error_coeff", self.vza_error_coeff),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(format!("{name} must be finite and >= 0, got {value}"));
            }
        }
        if !self.z.is_finite() || self.z <= 0.0 {
            return Err(format!("uncertainty.z must be finite and > 0, got {}", self.z));
        }
        Ok(())
    }
}

/// Bir FRP değeri (veya hücre ortalaması) için güven aralığı tahmini.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UncertaintyEstimate {
    pub lower: f64,
    pub upper: f64,
    pub confidence: f64,
}

/// Tek bir gözlemin göreli 1σ belirsizliğinin bağımsız (rastgele) ve bölgesel korelasyonlu
/// (sistematik) bileşenlerine ayrıştırılmış hali.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RelativeUncertainty {
    pub random: f64,
    pub systematic: f64,
}

impl RelativeUncertainty {
    /// Bileşenler ortogonal (bağımsız hata kaynakları) olduğundan karesel toplam alınır.
    pub fn total(self) -> f64 {
        self.random.hypot(self.systematic)
    }
}

/// Normal dağılım z-skorundan iki kuyruklu güven düzeyini (`P(|Z| <= z) = erf(z / √2)`) hesaplar.
pub fn confidence_from_z(z: f64) -> f64 {
    if z.is_nan() || z <= 0.0 {
        return 0.0;
    }
    if z.is_infinite() {
        return 1.0;
    }
    erf_approx(z * FRAC_1_SQRT_2).clamp(0.0, 1.0)
}

/// VZA düzeltme faktörüne bağlı göreli 1σ belirsizlik: `k × max(factor - 1, 0)`.
///
/// Nadirde (`factor = 1.0`) veya düzeltme yokken (`None`) düzeltme uygulanmadığı için sıfırdır;
/// tarama kenarına doğru `factor > 1.0` büyüdükçe hem polinomial model dışpolasyonu hem de `scan`
/// üzerinden açı kestirimi hatası büyüdüğünden kesin monoton artar.
pub fn vza_relative_sigma(vza_factor: Option<f64>, params: &UncertaintyParams) -> f64 {
    let coeff = sanitized_non_neg(params.vza_error_coeff);
    match vza_factor {
        Some(factor) if factor.is_finite() && factor > 1.0 => coeff * (factor - 1.0),
        _ => 0.0,
    }
}

/// Tek kayıt için göreli 1σ belirsizliği rastgele (`sensor ⊕ vza`) ve sistematik
/// (`ratio_sigma / ratio`) bileşenlerine ayırır.
pub fn relative_uncertainty(
    vza_factor: Option<f64>,
    sensor_family: SensorFamily,
    calibration: Option<(f64, f64)>,
    params: &UncertaintyParams,
) -> RelativeUncertainty {
    let sensor_sigma = match sensor_family {
        SensorFamily::Modis => sanitized_non_neg(params.modis_sensor_sigma),
        SensorFamily::Viirs => sanitized_non_neg(params.viirs_sensor_sigma),
    };
    let vza_sigma = vza_relative_sigma(vza_factor, params);
    let systematic = match calibration {
        Some((ratio, ratio_sigma))
            if ratio.is_finite() && ratio > 0.0 && ratio_sigma.is_finite() && ratio_sigma > 0.0 =>
        {
            ratio_sigma / ratio
        }
        _ => 0.0,
    };

    RelativeUncertainty {
        random: sensor_sigma.hypot(vza_sigma),
        systematic,
    }
}

/// Tek bir harmonize FRP gözlemi için analitik güven aralığı üretir.
pub fn frp_uncertainty(
    frp: f64,
    vza_factor: Option<f64>,
    sensor_family: SensorFamily,
    calibration: Option<(f64, f64)>,
    params: &UncertaintyParams,
) -> UncertaintyEstimate {
    frp_uncertainty_for_obs(frp, vza_factor, sensor_family, calibration, 1, params)
}

/// Çoklu gözlem (`n_obs`) ortalamasından gelen kayıt için göreli belirsizliği ölçekler.
///
/// Bağımsız rastgele sensör ve VZA hataları ortalamada `1/√n_obs` oranında azalırken,
/// bölgesel MODIS→VIIRS kalibrasyon belirsizliği aynı yönde etki eden sistematik bir
/// bileşen olduğundan örnek sayısıyla küçülmez.
pub fn relative_uncertainty_for_obs(
    vza_factor: Option<f64>,
    sensor_family: SensorFamily,
    calibration: Option<(f64, f64)>,
    n_obs: u16,
    params: &UncertaintyParams,
) -> RelativeUncertainty {
    let mut rel = relative_uncertainty(vza_factor, sensor_family, calibration, params);
    let n = f64::from(n_obs.max(1));
    rel.random /= n.sqrt();
    rel
}

/// Çoklu gözlem (`n_obs`) ortalaması olan bir harmonize FRP kaydı için güven aralığı üretir.
pub fn frp_uncertainty_for_obs(
    frp: f64,
    vza_factor: Option<f64>,
    sensor_family: SensorFamily,
    calibration: Option<(f64, f64)>,
    n_obs: u16,
    params: &UncertaintyParams,
) -> UncertaintyEstimate {
    let confidence = confidence_from_z(params.z);
    if !frp.is_finite() || frp <= 0.0 {
        return UncertaintyEstimate {
            lower: 0.0,
            upper: 0.0,
            confidence,
        };
    }

    let rel = relative_uncertainty_for_obs(vza_factor, sensor_family, calibration, n_obs, params);
    let z = sanitized_non_neg(params.z);
    let half_rel = z * rel.total();

    UncertaintyEstimate {
        lower: (frp * (1.0 - half_rel)).max(0.0),
        upper: frp * (1.0 + half_rel),
        confidence,
    }
}

/// Hücre ortalaması (`mean_frp`) için birleşik belirsizlik tahmini.
///
/// Rastgele bileşenler gözlemler arasında bağımsız kabul edildiğinden ortalamada varyans `1/n²`
/// ölçeklenir (`σ_rand,mean = √(Σ (frp_i·σ_rand,i)²) / n`, özdeş kayıtlarda `1/√n` azalır).
/// Sistematik kalibrasyon belirsizliği aynı bölgede aynı yönde etki ettiğinden ortalamada
/// küçülmez ve FRP ağırlıklı ortalama olarak korunur (`σ_sys,mean = Σ (frp_i·σ_sys,i) / n`).
pub fn mean_frp_uncertainty(
    items: &[(f64, f64, f64)],
    params: &UncertaintyParams,
) -> Option<UncertaintyEstimate> {
    if items.is_empty() {
        return None;
    }

    let n = items.len() as f64;
    let mut sum_frp = 0.0;
    let mut sum_rand_var = 0.0;
    let mut sum_sys_abs = 0.0;

    for &(frp, rand_rel, sys_rel) in items {
        let x = sanitized_non_neg(frp);
        let r = sanitized_non_neg(rand_rel);
        let s = sanitized_non_neg(sys_rel);
        sum_frp += x;
        sum_rand_var += (x * r).powi(2);
        sum_sys_abs += x * s;
    }

    let mean_frp = sum_frp / n;
    let total_abs = (sum_rand_var.sqrt() / n).hypot(sum_sys_abs / n);
    let z = sanitized_non_neg(params.z);

    Some(UncertaintyEstimate {
        lower: (mean_frp - z * total_abs).max(0.0),
        upper: mean_frp + z * total_abs,
        confidence: confidence_from_z(params.z),
    })
}

fn sanitized_non_neg(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

/// Abramowitz & Stegun (7.1.26) rasyonel yaklaşımı: ek bağımlılık eklemeden ≤ 1.5e-7 doğruluk.
fn erf_approx(x: f64) -> f64 {
    const P: f64 = 0.3275911;
    const A1: f64 = 0.254829592;
    const A2: f64 = -0.284496736;
    const A3: f64 = 1.421413741;
    const A4: f64 = -1.453152027;
    const A5: f64 = 1.061405429;

    let t = 1.0 / (1.0 + P * x);
    let poly = ((((A5 * t + A4) * t + A3) * t + A2) * t + A1) * t;
    1.0 - poly * (-x * x).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_widens_strictly_with_vza_factor() {
        let params = UncertaintyParams::default();
        let factors = [1.0, 1.5, 2.5, 4.0];
        let widths: Vec<f64> = factors
            .iter()
            .map(|&f| {
                let est = frp_uncertainty(100.0, Some(f), SensorFamily::Modis, None, &params);
                est.upper - est.lower
            })
            .collect();

        for pair in widths.windows(2) {
            assert!(
                pair[1] > pair[0],
                "expected strictly increasing width, got {widths:?}"
            );
        }
    }

    #[test]
    fn nadir_without_calibration_equals_sensor_noise() {
        let params = UncertaintyParams::default();
        for (family, expected_sigma) in [
            (SensorFamily::Modis, params.modis_sensor_sigma),
            (SensorFamily::Viirs, params.viirs_sensor_sigma),
        ] {
            let rel = relative_uncertainty(Some(1.0), family, None, &params);
            assert!((rel.random - expected_sigma).abs() < 1e-12);
            assert_eq!(rel.systematic, 0.0);
            assert!((rel.total() - expected_sigma).abs() < 1e-12);

            let est = frp_uncertainty(100.0, Some(1.0), family, None, &params);
            assert!((est.lower - 100.0 * (1.0 - expected_sigma)).abs() < 1e-12);
            assert!((est.upper - 100.0 * (1.0 + expected_sigma)).abs() < 1e-12);
        }
    }

    #[test]
    fn calibration_sigma_widens_interval() {
        let params = UncertaintyParams::default();
        let without_cal = frp_uncertainty(100.0, Some(1.0), SensorFamily::Modis, None, &params);
        let mid_lat_cal = frp_uncertainty(
            100.0,
            Some(1.0),
            SensorFamily::Modis,
            Some((1.0, 0.03)),
            &params,
        );
        let high_lat_cal = frp_uncertainty(
            100.0,
            Some(1.0),
            SensorFamily::Modis,
            Some((1.0, 0.18)),
            &params,
        );

        let w0 = without_cal.upper - without_cal.lower;
        let w1 = mid_lat_cal.upper - mid_lat_cal.lower;
        let w2 = high_lat_cal.upper - high_lat_cal.lower;
        assert!(w1 > w0 && w2 > w1, "expected {w0} < {w1} < {w2}");
    }

    #[test]
    fn lower_is_clamped_and_non_positive_or_nan_frp_is_safe() {
        let params = UncertaintyParams {
            z: 10.0,
            ..UncertaintyParams::default()
        };
        let clamped = frp_uncertainty(50.0, Some(4.0), SensorFamily::Modis, Some((1.0, 0.18)), &params);
        assert_eq!(clamped.lower, 0.0);
        assert!(clamped.upper > 50.0);

        for bad_frp in [0.0, -10.0, f64::NAN] {
            let est = frp_uncertainty(bad_frp, Some(2.0), SensorFamily::Viirs, None, &params);
            assert_eq!(est.lower, 0.0);
            assert_eq!(est.upper, 0.0);
            assert!((est.confidence - clamped.confidence).abs() < 1e-12);
        }
    }

    #[test]
    fn aggregation_shrinks_random_by_sqrt_n_and_preserves_systematic() {
        let params = UncertaintyParams::default();
        assert_eq!(mean_frp_uncertainty(&[], &params), None);

        let rand_only_1 = mean_frp_uncertainty(&[(100.0, 0.12, 0.0)], &params).unwrap();
        let rand_only_4 = mean_frp_uncertainty(&[(100.0, 0.12, 0.0); 4], &params).unwrap();
        let rand_only_16 = mean_frp_uncertainty(&[(100.0, 0.12, 0.0); 16], &params).unwrap();

        let half_1 = rand_only_1.upper - 100.0;
        let half_4 = rand_only_4.upper - 100.0;
        let half_16 = rand_only_16.upper - 100.0;
        assert!((half_4 - half_1 / 2.0).abs() < 1e-12);
        assert!((half_16 - half_1 / 4.0).abs() < 1e-12);

        let sys_only_1 = mean_frp_uncertainty(&[(100.0, 0.0, 0.15)], &params).unwrap();
        let sys_only_16 = mean_frp_uncertainty(&[(100.0, 0.0, 0.15); 16], &params).unwrap();
        assert!((sys_only_1.lower - sys_only_16.lower).abs() < 1e-12);
        assert!((sys_only_1.upper - sys_only_16.upper).abs() < 1e-12);
        assert!((sys_only_16.upper - 115.0).abs() < 1e-12);
        assert!((sys_only_16.lower - 85.0).abs() < 1e-12);
    }

    #[test]
    fn z_two_doubles_unclamped_width_and_matches_normal_confidence() {
        let p1 = UncertaintyParams::default();
        let p2 = UncertaintyParams {
            z: 2.0,
            ..UncertaintyParams::default()
        };

        let e1 = frp_uncertainty(100.0, Some(1.5), SensorFamily::Modis, Some((1.0, 0.03)), &p1);
        let e2 = frp_uncertainty(100.0, Some(1.5), SensorFamily::Modis, Some((1.0, 0.03)), &p2);

        assert!(e1.lower > 0.0 && e2.lower > 0.0);
        let w1 = e1.upper - e1.lower;
        let w2 = e2.upper - e2.lower;
        assert!((w2 - 2.0 * w1).abs() < 1e-12);
        assert!((e1.confidence - 0.6827).abs() < 1e-4);
        assert!((e2.confidence - 0.9545).abs() < 1e-4);
    }

    #[test]
    fn params_serde_and_validation_match_pipeline_config_conventions() {
        let empty: UncertaintyParams = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, UncertaintyParams::default());
        assert_eq!(empty.validate(), Ok(()));

        let partial: UncertaintyParams = serde_json::from_str(r#"{"z":2.0}"#).unwrap();
        assert_eq!(partial.z, 2.0);
        assert_eq!(partial.modis_sensor_sigma, DEFAULT_MODIS_SENSOR_SIGMA);

        assert!(serde_json::from_str::<UncertaintyParams>(r#"{"bogus":1}"#).is_err());

        let bad_z = UncertaintyParams {
            z: 0.0,
            ..UncertaintyParams::default()
        };
        assert!(bad_z.validate().is_err());
        let bad_vza = UncertaintyParams {
            vza_error_coeff: -0.1,
            ..UncertaintyParams::default()
        };
        assert!(bad_vza.validate().is_err());
    }

    #[test]
    fn sensor_family_conversions_respect_quality_priority() {
        assert_eq!(SensorFamily::from(Sensor::ModisTerra), SensorFamily::Modis);
        assert_eq!(SensorFamily::from(Sensor::ModisAqua), SensorFamily::Modis);
        assert_eq!(SensorFamily::from(Sensor::ViirsSNPP), SensorFamily::Viirs);
        assert_eq!(SensorFamily::from(Sensor::ViirsNOAA20), SensorFamily::Viirs);
        assert_eq!(SensorFamily::from(Sensor::ViirsNOAA21), SensorFamily::Viirs);

        assert_eq!(SensorFamily::from(HotspotSource::ModisOnly), SensorFamily::Modis);
        assert_eq!(SensorFamily::from(HotspotSource::ViirsOnly), SensorFamily::Viirs);
        assert_eq!(SensorFamily::from(HotspotSource::Matched), SensorFamily::Viirs);
    }

    #[test]
    fn obs_scaling_shrinks_random_by_sqrt_n_obs_and_keeps_systematic() {
        let params = UncertaintyParams::default();
        let cal = Some((1.0, 0.15));
        let rel1 = relative_uncertainty_for_obs(Some(2.0), SensorFamily::Modis, cal, 1, &params);
        let rel4 = relative_uncertainty_for_obs(Some(2.0), SensorFamily::Modis, cal, 4, &params);

        assert!((rel4.random - rel1.random / 2.0).abs() < 1e-12);
        assert!((rel4.systematic - rel1.systematic).abs() < 1e-12);

        let e1 = frp_uncertainty_for_obs(100.0, Some(2.0), SensorFamily::Modis, cal, 1, &params);
        let e4 = frp_uncertainty_for_obs(100.0, Some(2.0), SensorFamily::Modis, cal, 4, &params);
        assert!((e4.upper - e4.lower) < (e1.upper - e1.lower));
    }
}
