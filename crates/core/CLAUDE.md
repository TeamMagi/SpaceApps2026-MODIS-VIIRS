# pyrosync-core — Harmonizasyon Motoru

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- Bu crate saf hesaplama kütüphanesidir: I/O, network veya dosya işlemi yapılmaz.
- Tüm public fonksiyonlar unit test ile kapsanmalıdır.

## Modül Sorumlulukları

| Modül | Görev |
|-------|-------|
| `config.rs` | `PipelineConfig`: dedup, filo, eşleştirme, VZA, kalibrasyon ve belirsizlik eşikleri; `validate()` ile doğrulanır |
| `models.rs` | Veri modelleri: Hotspot, HarmonizedHotspot, Sensor, Confidence, BBox, CalendarCell |
| `dedup.rs` | İnterscan duplikat temizleme (varsayılan Δt<8s, overlap>30%), haversine mesafe hesabı |
| `fleet.rs` | Aynı sensör ailesinin (MODIS Terra/Aqua, VIIRS SNPP/NOAA-20/21) eşzamanlı tespitlerini tek gözleme birleştirme |
| `h3index.rs` | H3 (çözünürlük 8) uzamsal indeks; `k_ring_for_radius` arama halkasını eşleştirme yarıçapına göre ölçekler |
| `vza.rs` | MODIS FRP görüş açısı düzeltmesi (`R_rel(θ)` kuadratik polinom × ham FRP, `VzaConfig`'ten), piksel alanı (`A_p`) ve analitik tespit limiti (`D_p`, `r_p`) |
| `calibration.rs` | VIIRS öncesi (2012 öncesi) enlem bandı tamlık katsayısı `k_c` (gözlenen geçiş oranı + interpolasyon) |
| `uncertainty.rs` | Analitik FRP güven aralığı (`frp_lower`/`frp_upper`, `mean_frp_*` ve `total_frp_*`) yayılımı |
| `conformal.rs` | Mondrian split conformal aralıkları (saf istatistik): `fit_mondrian`, `conformal_interval`, `calibration_fingerprint`; `uncertainty.rs` üzerinden de dışa açılır. Sensör-dışarıda tekrarlanabilirlik bandıdır, gerçek doğruluk değil |
| `harmonize.rs` | 7 aşamalı pipeline: dedup → VZA → cross-sensor matching → fusion |
| `effort.rs` | Gözlem eforu (EffortFactor) tahmini, ≥ 40 MW şiddet rejimli Ω (`omega_high_intensity`) ve ayrı raporlanan S-NPP/Aqua gündüz oranı |
| `calendar.rs` | Aylık yanma takvimi oluşturma, anomali hesabı |
| `export.rs` | Harmonize hotspot'ları CSV ve GeoJSON metnine dönüştürme |
| `static_mask.rs` | Kalıcı sıcak hücre (gaz meşalesi/endüstri) maskesi: H3 r7 hücre-yılı, yıl içinde ≥ `min_active_months` (varsayılan 10) farklı ayda tespit |
| `sama.rs` | Güney Atlantik Manyetik Anomalisi kutusu (110°B–11°D, 55°G–7°K) içindeki gece VIIRS tespiti testi |
| `events.rs` | Yangın olayı kümeleme (ST-DBSCAN + ardışık gün kuralı) ve `no_detection_days` (bilinen Terra kesintileri dahil) |
| `quality.rs` | Tarihsel sensör anomalisi ve kesinti dönemleri için ay bazında `QualityFlag` etiketleri ve uyarı açıklamaları |

## Teknik Kısıtlar

- Terra-only dönem (2002-07 öncesi, Aqua yok): seri 2002-07 veya sonrasını da içeriyorsa bu aylar karşılaştırılamaz sayılır (`count_normalized`, `effort_omega`, `anomaly` = `None`; Ω havuzlarına ve anomali tabanına girmez, `count`/FRP ham kalır). Seri tamamen 2002-07 öncesindeyse değişiklik yok. Gerekçe: Ω, MODIS referans kanalının kendi içindeki filo değişimini ölçemez (`effort::FIRST_TERRA_AQUA_MONTH`).
- Şiddet rejimli efor (`effort.rs`): `frp >= HIGH_INTENSITY_FRP_MW` (`40.0` MW, MODIS tarama kenarı tespit limiti) kayıtları için ayrı `omega_high_intensity` hesaplanır (küçük yangın uçurumundan etkilenmez; en az 3 ay ve `modis_observable_high_intensity >= 30` gerekir). Gündüz yüksek şiddetli S-NPP / Aqua oranı (`snpp_aqua_high_intensity_ratio`) diurnal/swath farkı tanısı olarak ayrı raporlanır ve sayımlara uygulanmaz (`CalendarCell.count_normalized` tüm tespit Ω'sını kullanmaya devam eder).
- `Confidence` enum'u `PartialOrd, Ord` derive eder: `Low < Nominal < High`.
- `calendar.rs` içinde `use chrono::Datelike;` import'u gereklidir (`.year()` ve `.month()` için).
- Cross-sensor matching: varsayılan 2 km mesafe, 180 dakika zaman penceresi (G2 ölçümündeki 4 canlı NRT bölgesinde aynı konumdaki MODIS/VIIRS çiftleri en az 34 dk, medyan ~158 dk aralıklıdır; Terra ~10:30 geçişi VIIRS ile çakışmaz, S-NPP / NOAA-20 ise ~50 dk aralıklıdır). Eşleştirme 1 MODIS = N VIIRS kuralıyla çalışır: her VIIRS filo gözlemi, `matching.radius_km` ve `matching.window_secs` içindeki en yakın MODIS gözleminin olur; sahip olunan en yüksek skorlu VIIRS gözlemi bir geçişi başlatır ve başlangıç gözleminden `VIIRS_PASS_WINDOW_SECS` (600 sn) içindeki ikincil pikseller yalnızca MODIS merkezinden `scan`/`track` yarı köşegeniyle belirlenen ayak izi içindeyse (en az 0.75 km, en fazla `matching.radius_km`; geçerli `scan`/`track` yoksa eşleştirme yarıçapı kullanılır) aynı `Matched` kayıtta toplanır (FRP ve `n_obs` toplanır). Geçiş dışında kalan sahip olunan pikseller `ViirsOnly` kalır. `HarmonizeLinks.simultaneous_matched`, MODIS-VIIRS |Δt| ≤ `matching.simultaneous_window_secs` (varsayılan 1200 sn) olan Matched kayıtların indeksleridir ve yalnızca radyometrik kestirimde kullanılır. 2000–2025 FIRMS ülke arşivinde bol bulunan yakın eşzamanlı Aqua / S-NPP çiftleri (her ikisinin ekvator geçişi ~13:30; |Δt| ≤ 20 dk) bu alt kümeyi oluşturur; bütünlük çarpanı `k_c` ayrı olarak aynı türden Aqua/S-NPP 1° hücre-gün çiftlerinden ölçülmüştür. VZA eğrisi (Zheng et al. 2026) ve geçici VIIRS σ = 0.08 yeniden uydurulmaz; arşiv çiftleriyle tanısal kontrol kodda yapılmamıştır. Sayım özdeşliği: `harmonized = raw - sama_removed - static_masked - interscan_removed - fleet_merged - matched_pairs - viirs_aggregated` (`viirs_aggregated`: bir Matched kayda ilk gözlemin ötesinde eklenen VIIRS gözlemleri). Tüm eşikler `PipelineConfig`'ten gelir; eski `harmonize`/`harmonize_with_links` fonksiyonları varsayılan konfigürasyonla çalışır, `*_config` varyantları konfigürasyon alır.
- Fusion: VIIRS tercih edilir (yüksek çözünürlük; Matched kayıtta toplanan VIIRS FRP'si), eşleşmeyenler her iki sensörden alınır.
- VZA düzeltmesi ve ayak izi ölçeklemesi (`vza.rs`): Zheng et al. (2026) Şekil 4 nadirde 1'e normalize edilen oran (`R_rel(θ) = 0.2017θ² + 0.0168θ + 1.0`, θ radyan) MODIS ham FRP'si ile çarpılır (`raw_frp * ratio`). `ModisOnly` tespit başına enlem kalibrasyonu uygulanmaz.
- VIIRS öncesi tamlık katsayısı `k_c` (`calibration.rs` + `calendar.rs`): `0–30°: 1.11 (σ 0.08)`, `30–50°: 1.18 (σ 0.15)`, `55–70°: 1.28 (σ 0.11)` (50–55° lineer geçiş, >70° sabit) yalnız VIIRS tespiti içermeyen aylık `CalendarCell.total_frp` toplamına uygulanır; `frp_per_detection_eq = total_frp / count_normalized`. `mean_frp` ham piksel ortalamasıdır ve sensör geçişinde süreksizdir.
- FRP belirsizlik modeli (`uncertainty.rs`): `frp_lower`/`frp_upper`, `mean_frp_*` ve `total_frp_*` sınırları rastgele hataları karesel (`1/√n` ölçeklenen sensör + VZA), sistematik bileşeni ise doğrusal (`modis_scale_sigma = 0.10` ve `total_frp`'de `k_c` uygulanınca `ratio_sigma/ratio`) birleştirir; `modis_sensor_sigma = 0.266` yayımlanmış değerdir (Freeborn et al. 2014a alt-piksel yerleşim belirsizliği, `[12, Bölüm 5.1]`); `viirs_sensor_sigma = 0.08` geçici varsayımdır (literatürde yok; üst sınır `[05, Bölüm 3.4]` MIR ±%30, arşiv çift ölçümü `tools/viirs_sigma.py`); VZA hata katsayısı ve `modis_scale_sigma` model varsayımlarıdır. Bir geçişteki VIIRS piksellerini toplayan `Matched` kayıtları üye belirsizliklerini `1/√n_obs` yerine karesel olarak birleştirir (etkin örnek sayısı `n_eff = (Σ f)² / Σ f²`'ye eşdeğerdir).
- Ön filtre (aşama 0, `harmonize_with_links_masked`): önce SAMA (varsayılan kapalı), sonra statik maske (varsayılan açık); elenen satırlar `HarmonizeLinks.sama_removed` / `static_masked` ile sayılır ve link'leri `None` olur. Maske verilmezse partiden kurulur; yıl yıl karar verildiği için tam yıllardan oluşan her bölümleme aynı sonucu verir, NRT pencereleri (< 10 ay) maskelenmez.
- FRP birimi: MW (megawatt). Negatif FRP olamaz.

## Bağımlılıklar

Sadece workspace dependency'leri kullanılır (`{ workspace = true }`). Yeni crate eklemeden önce workspace Cargo.toml'a eklenmeli.
