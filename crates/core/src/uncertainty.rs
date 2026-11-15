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
/// eğimi. Tarama kenarında (`factor ≈ 1.32`, Zheng et al. 2026 normalize eğrisi; `01_ANALYSIS.md`
/// gruplu polinomial fit RMSE ≤ %1.34) piksel ölçeği artık hatası daha büyüktür.
pub const DEFAULT_VZA_ERROR_COEFF: f64 = 0.05;

/// Varsayım: Zheng R(θ) eğrisinin ölçek/şekil belirsizliği ve Terra'ya geometrik vekil
/// aktarımı; yayımlanmış değer değil.
pub const DEFAULT_MODIS_SCALE_SIGMA: f64 = 0.10;

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
    pub modis_scale_sigma: f64,
    pub z: f64,
    /// Optional tropical MODIS swath-gap compensation (default off). When on, VIIRS-less
    /// `total_frp` from detections within 30°S-30°N is scaled by
    /// [`crate::effort::swath_gap_compensation`] and its spread joins the systematic term.
    /// Lives here rather than in `PipelineConfig` so it reuses the `[uncertainty]` TOML table;
    /// skipped when false so the config digest (and every precomputed series stamp) is unchanged.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub tropical_swath_gap_compensation: bool,
}

impl Default for UncertaintyParams {
    fn default() -> Self {
        Self {
            modis_sensor_sigma: DEFAULT_MODIS_SENSOR_SIGMA,
            viirs_sensor_sigma: DEFAULT_VIIRS_SENSOR_SIGMA,
            vza_error_coeff: DEFAULT_VZA_ERROR_COEFF,
            modis_scale_sigma: DEFAULT_MODIS_SCALE_SIGMA,
            z: DEFAULT_Z,
            tropical_swath_gap_compensation: false,
        }
    }
}

impl UncertaintyParams {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("uncertainty.modis_sensor_sigma", self.modis_sensor_sigma),
            ("uncertainty.viirs_sensor_sigma", self.viirs_sensor_sigma),
            ("uncertainty.vza_error_coeff", self.vza_error_coeff),
            ("uncertainty.modis_scale_sigma", self.modis_scale_sigma),
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
/// (`modis_scale_sigma ⊕ ratio_sigma / ratio`) bileşenlerine ayırır.
///
/// `ModisOnly` (`SensorFamily::Modis`) kayıtlarında `modis_scale_sigma` Zheng R(θ) ölçek/şekil
/// ve Terra vekil aktarımı belirsizliği olarak sistematik bileşene girer; `Matched` ve `ViirsOnly`
/// (`SensorFamily::Viirs`) kayıtlarında sistematik bileşen 0 kalır.
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
    let scale_sigma = match sensor_family {
        SensorFamily::Modis => sanitized_non_neg(params.modis_scale_sigma),
        SensorFamily::Viirs => 0.0,
    };
    let cal_sigma = match calibration {
        Some((ratio, ratio_sigma))
            if ratio.is_finite() && ratio > 0.0 && ratio_sigma.is_finite() && ratio_sigma > 0.0 =>
        {
            ratio_sigma / ratio
        }
        _ => 0.0,
    };

    RelativeUncertainty {
        random: sensor_sigma.hypot(vza_sigma),
        systematic: scale_sigma.hypot(cal_sigma),
    }
}

/// İki bağımsız çarpımsal katsayıyı (`ratio`, `ratio_sigma`) tek katsayıya birleştirir: oranlar
/// çarpılır, göreli 1σ'lar karesel toplanır (`modis_scale_sigma ⊕ cal_sigma` ile aynı kural).
///
/// Takvimde `k_c` ile tropikal swath-gap telafisini birleştirmek için kullanılır; böylece
/// `relative_uncertainty` sistematik bileşeni her iki yayılımı da tek seferde taşır.
pub fn combine_factors(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let rel = |(ratio, sigma): (f64, f64)| {
        if ratio.is_finite() && ratio > 0.0 {
            sanitized_non_neg(sigma) / ratio
        } else {
            0.0
        }
    };
    let ratio = a.0 * b.0;
    (ratio, ratio * rel(a).hypot(rel(b)))
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

fn aggregate_uncertainty(
    items: &[(f64, f64, f64)],
    divisor: f64,
    params: &UncertaintyParams,
) -> Option<UncertaintyEstimate> {
    if items.is_empty() {
        return None;
    }

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

    let center = sum_frp / divisor;
    let total_abs = (sum_rand_var.sqrt() / divisor).hypot(sum_sys_abs / divisor);
    let z = sanitized_non_neg(params.z);

    Some(UncertaintyEstimate {
        lower: (center - z * total_abs).max(0.0),
        upper: center + z * total_abs,
        confidence: confidence_from_z(params.z),
    })
}

/// Hücre ortalaması (`mean_frp`) için birleşik belirsizlik tahmini.
///
/// Rastgele bileşenler gözlemler arasında bağımsız kabul edildiğinden ortalamada varyans `1/n²`
/// ölçeklenir (`σ_rand,mean = √(Σ (frp_i·σ_rand,i)²) / n`, özdeş kayıtlarda `1/√n` azalır).
/// Sistematik belirsizlik aynı bölgede aynı yönde etki ettiğinden ortalamada küçülmez ve FRP
/// ağırlıklı ortalama olarak korunur (`σ_sys,mean = Σ (frp_i·σ_sys,i) / n`).
pub fn mean_frp_uncertainty(
    items: &[(f64, f64, f64)],
    params: &UncertaintyParams,
) -> Option<UncertaintyEstimate> {
    aggregate_uncertainty(items, items.len() as f64, params)
}

/// Hücre toplamı (`total_frp`) için birleşik belirsizlik tahmini.
///
/// Rastgele bileşen karesel toplamla (`√(Σ (frp_i·σ_rand,i)²)`), sistematik bileşen (MODIS
/// ölçek/vekil belirsizliği ve VIIRS öncesi tamlık katsayısı `k_c` belirsizliği) ise doğrusal
/// toplamla (`Σ (frp_i·σ_sys,i)`) yayılır.
pub fn total_frp_uncertainty(
    items: &[(f64, f64, f64)],
    params: &UncertaintyParams,
) -> Option<UncertaintyEstimate> {
    aggregate_uncertainty(items, 1.0, params)
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
    fn nadir_without_calibration_uses_sensor_noise_and_modis_scale_sigma() {
        let params = UncertaintyParams::default();

        let modis_rel = relative_uncertainty(Some(1.0), SensorFamily::Modis, None, &params);
        assert!((modis_rel.random - params.modis_sensor_sigma).abs() < 1e-12);
        assert!((modis_rel.systematic - params.modis_scale_sigma).abs() < 1e-12);
        let expected_modis_total = params.modis_sensor_sigma.hypot(params.modis_scale_sigma);
        assert!((modis_rel.total() - expected_modis_total).abs() < 1e-12);

        let modis_est = frp_uncertainty(100.0, Some(1.0), SensorFamily::Modis, None, &params);
        assert!((modis_est.lower - 100.0 * (1.0 - expected_modis_total)).abs() < 1e-12);
        assert!((modis_est.upper - 100.0 * (1.0 + expected_modis_total)).abs() < 1e-12);

        let viirs_rel = relative_uncertainty(Some(1.0), SensorFamily::Viirs, None, &params);
        assert!((viirs_rel.random - params.viirs_sensor_sigma).abs() < 1e-12);
        assert_eq!(viirs_rel.systematic, 0.0);
        assert!((viirs_rel.total() - params.viirs_sensor_sigma).abs() < 1e-12);

        let viirs_est = frp_uncertainty(100.0, Some(1.0), SensorFamily::Viirs, None, &params);
        assert!((viirs_est.lower - 100.0 * (1.0 - params.viirs_sensor_sigma)).abs() < 1e-12);
        assert!((viirs_est.upper - 100.0 * (1.0 + params.viirs_sensor_sigma)).abs() < 1e-12);
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

        assert_eq!(total_frp_uncertainty(&[], &params), None);
        let tot_rand_4 = total_frp_uncertainty(&[(100.0, 0.12, 0.0); 4], &params).unwrap();
        assert!((tot_rand_4.upper - (400.0 + 24.0)).abs() < 1e-12);
        assert!((tot_rand_4.lower - (400.0 - 24.0)).abs() < 1e-12);

        let tot_sys_4 = total_frp_uncertainty(&[(100.0, 0.0, 0.10); 4], &params).unwrap();
        assert!((tot_sys_4.upper - (400.0 + 40.0)).abs() < 1e-12);
        assert!((tot_sys_4.lower - (400.0 - 40.0)).abs() < 1e-12);
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
        let bad_scale = UncertaintyParams {
            modis_scale_sigma: -0.05,
            ..UncertaintyParams::default()
        };
        assert!(bad_scale.validate().is_err());
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

    const FACTORS: [Option<f64>; 6] = [None, Some(1.0), Some(1.1), Some(1.32), Some(2.0), Some(4.0)];
    const FAMILIES: [SensorFamily; 2] = [SensorFamily::Modis, SensorFamily::Viirs];

    fn width(e: UncertaintyEstimate) -> f64 {
        e.upper - e.lower
    }

    #[test]
    fn interval_brackets_frp_strictly_over_input_grid() {
        let params = UncertaintyParams::default();
        let cals = [None, Some((1.11, 0.08)), Some((1.28, 0.11))];
        for frp in [0.001, 0.4, 3.7, 100.0, 5000.0] {
            for family in FAMILIES {
                for factor in FACTORS {
                    for cal in cals {
                        for n in [0, 1, 2, 4, 16, u16::MAX] {
                            let e = frp_uncertainty_for_obs(frp, factor, family, cal, n, &params);
                            assert!(
                                0.0 < e.lower && e.lower < frp && frp < e.upper,
                                "{e:?} frp={frp} {family:?} {factor:?} {cal:?} n={n}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn scan_edge_interval_is_wider_than_nadir_for_both_families_and_all_n() {
        let params = UncertaintyParams::default();
        for family in FAMILIES {
            for n in [1, 3, 10] {
                let nadir = frp_uncertainty_for_obs(50.0, Some(1.0), family, None, n, &params);
                let edge = frp_uncertainty_for_obs(50.0, Some(1.32), family, None, n, &params);
                assert!(width(edge) > width(nadir), "{family:?} n={n}");
            }
        }
    }

    #[test]
    fn width_is_non_decreasing_in_vza_factor_and_nadir_equals_missing_factor() {
        let params = UncertaintyParams::default();
        for family in FAMILIES {
            let mut prev = 0.0;
            for factor in FACTORS {
                let w = width(frp_uncertainty(80.0, factor, family, None, &params));
                assert!(w >= prev, "{family:?} {factor:?}");
                prev = w;
            }
            let none = frp_uncertainty(80.0, None, family, None, &params);
            let nadir = frp_uncertainty(80.0, Some(1.0), family, None, &params);
            assert_eq!(none, nadir);
        }
    }

    #[test]
    fn vza_sigma_boundaries_are_zero_up_to_one_then_linear() {
        // 3-value BVA around factor = 1.0, plus non-finite factors.
        let p = UncertaintyParams::default();
        assert_eq!(vza_relative_sigma(Some(0.999), &p), 0.0);
        assert_eq!(vza_relative_sigma(Some(1.0), &p), 0.0);
        let just_above = vza_relative_sigma(Some(1.001), &p);
        assert!(just_above > 0.0 && just_above < 1e-4);
        assert!((vza_relative_sigma(Some(1.32), &p) - p.vza_error_coeff * 0.32).abs() < 1e-12);
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(vza_relative_sigma(Some(bad), &p), 0.0, "{bad}");
        }
    }

    #[test]
    fn zero_n_obs_behaves_like_single_observation() {
        let params = UncertaintyParams::default();
        let zero = frp_uncertainty_for_obs(10.0, Some(2.0), SensorFamily::Viirs, None, 0, &params);
        let one = frp_uncertainty_for_obs(10.0, Some(2.0), SensorFamily::Viirs, None, 1, &params);
        assert_eq!(zero, one);
    }

    #[test]
    fn invalid_calibration_is_ignored_like_missing_calibration() {
        let p = UncertaintyParams::default();
        let baseline = relative_uncertainty(Some(1.5), SensorFamily::Modis, None, &p);
        for bad in [(0.0, 0.1), (-1.0, 0.1), (f64::NAN, 0.1), (1.0, f64::NAN), (1.0, 0.0), (1.0, -0.1)] {
            let rel = relative_uncertainty(Some(1.5), SensorFamily::Modis, Some(bad), &p);
            assert_eq!(rel, baseline, "{bad:?}");
        }
    }

    #[test]
    fn modis_interval_is_wider_than_viirs_at_same_geometry() {
        let p = UncertaintyParams::default();
        let m = frp_uncertainty(20.0, Some(1.2), SensorFamily::Modis, None, &p);
        let v = frp_uncertainty(20.0, Some(1.2), SensorFamily::Viirs, None, &p);
        assert!(width(m) > width(v));
    }

    #[test]
    fn combine_factors_multiplies_ratios_and_adds_relative_sigmas_in_quadrature() {
        let (ratio, sigma) = combine_factors((1.11, 0.08), (1.13, 0.0113));
        assert!((ratio - 1.11 * 1.13).abs() < 1e-12);
        let rel = (0.08_f64 / 1.11).hypot(0.0113 / 1.13);
        assert!((sigma / ratio - rel).abs() < 1e-12);

        // Identity on either side leaves the other factor untouched.
        assert_eq!(combine_factors((1.0, 0.0), (1.2, 0.06)), (1.2, 1.2 * (0.06 / 1.2)));
        let (r, s) = combine_factors((1.11, 0.08), (1.0, 0.0));
        assert!((r - 1.11).abs() < 1e-12 && (s - 0.08).abs() < 1e-12);

        // The combined systematic term is wider than k_c alone, so the band widens.
        let p = UncertaintyParams::default();
        let kc_only = relative_uncertainty(None, SensorFamily::Modis, Some((1.11, 0.08)), &p);
        let both = relative_uncertainty(
            None,
            SensorFamily::Modis,
            Some(combine_factors((1.11, 0.08), (1.13, 0.0113))),
            &p,
        );
        assert!(both.systematic > kc_only.systematic);
        assert_eq!(both.random, kc_only.random);
    }

    #[test]
    fn swath_gap_compensation_flag_defaults_off_and_is_omitted_from_serialization() {
        let default = UncertaintyParams::default();
        assert!(!default.tropical_swath_gap_compensation);
        let json = serde_json::to_string(&default).unwrap();
        assert!(!json.contains("tropical_swath_gap_compensation"), "{json}");

        let on: UncertaintyParams =
            serde_json::from_str(r#"{"tropical_swath_gap_compensation":true}"#).unwrap();
        assert!(on.tropical_swath_gap_compensation);
        assert_eq!(on.validate(), Ok(()));
        assert!(serde_json::to_string(&on).unwrap().contains("\"tropical_swath_gap_compensation\":true"));
    }

    #[test]
    fn total_is_quadrature_of_random_and_systematic() {
        let rel = RelativeUncertainty { random: 0.3, systematic: 0.4 };
        assert!((rel.total() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn confidence_from_z_covers_degenerate_and_known_values() {
        assert_eq!(confidence_from_z(f64::NAN), 0.0);
        assert_eq!(confidence_from_z(-1.0), 0.0);
        assert_eq!(confidence_from_z(0.0), 0.0);
        assert_eq!(confidence_from_z(f64::INFINITY), 1.0);
        assert!((confidence_from_z(3.0) - 0.9973).abs() < 1e-4);
        let (a, b, c) = (confidence_from_z(0.5), confidence_from_z(1.0), confidence_from_z(2.0));
        assert!(0.0 < a && a < b && b < c && c < 1.0);
    }

    #[test]
    fn validate_boundaries_zero_is_allowed_and_negative_or_non_finite_rejected() {
        type Setter = fn(&mut UncertaintyParams, f64);
        let fields: [(&str, Setter); 4] = [
            ("modis_sensor_sigma", |p, v| p.modis_sensor_sigma = v),
            ("viirs_sensor_sigma", |p, v| p.viirs_sensor_sigma = v),
            ("vza_error_coeff", |p, v| p.vza_error_coeff = v),
            ("modis_scale_sigma", |p, v| p.modis_scale_sigma = v),
        ];
        for (name, set) in fields {
            let with = |v: f64| {
                let mut p = UncertaintyParams::default();
                set(&mut p, v);
                p.validate()
            };
            assert_eq!(with(0.0), Ok(()), "{name}");
            assert_eq!(with(f64::EPSILON), Ok(()), "{name}");
            for bad in [-f64::EPSILON, f64::NAN, f64::INFINITY] {
                assert!(with(bad).unwrap_err().contains(name), "{name} {bad}");
            }
        }
        let z = |v: f64| UncertaintyParams { z: v, ..UncertaintyParams::default() }.validate();
        assert_eq!(z(f64::MIN_POSITIVE), Ok(()));
        assert!(z(0.0).is_err() && z(f64::NAN).is_err() && z(f64::INFINITY).is_err());
    }

    #[test]
    fn aggregates_bracket_center_and_ignore_invalid_items() {
        let p = UncertaintyParams::default();
        let items = [(10.0, 0.1, 0.05), (30.0, 0.2, 0.0)];
        let mean = mean_frp_uncertainty(&items, &p).unwrap();
        let total = total_frp_uncertainty(&items, &p).unwrap();
        assert!(mean.lower < 20.0 && 20.0 < mean.upper);
        assert!(total.lower < 40.0 && 40.0 < total.upper);

        let junk = [(f64::NAN, 0.1, 0.1), (-5.0, 0.1, 0.1)];
        let e = total_frp_uncertainty(&junk, &p).unwrap();
        assert_eq!((e.lower, e.upper), (0.0, 0.0));
    }
}
