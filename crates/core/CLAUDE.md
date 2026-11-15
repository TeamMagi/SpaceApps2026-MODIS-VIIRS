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
| `vza.rs` | MODIS FRP görüş açısı düzeltmesi (`R_rel(θ)` kuadratik polinom × ham FRP, `VzaConfig`'ten) |
| `calibration.rs` | VIIRS öncesi (2012 öncesi) enlem bandı tamlık katsayısı `k_c` (gözlenen geçiş oranı + interpolasyon) |
| `uncertainty.rs` | Analitik FRP güven aralığı (`frp_lower`/`frp_upper`, `mean_frp_*` ve `total_frp_*`) yayılımı |
| `harmonize.rs` | 7 aşamalı pipeline: dedup → VZA → cross-sensor matching → fusion |
| `effort.rs` | Gözlem eforu (EffortFactor) tahmini |
| `calendar.rs` | Aylık yanma takvimi oluşturma, anomali hesabı |
| `export.rs` | Harmonize hotspot'ları CSV ve GeoJSON metnine dönüştürme |
| `static_mask.rs` | Kalıcı sıcak hücre (gaz meşalesi/endüstri) maskesi: H3 r7 hücre-yılı, yıl içinde ≥ `min_active_months` (varsayılan 10) farklı ayda tespit |
| `sama.rs` | Güney Atlantik Manyetik Anomalisi kutusu (110°B–11°D, 55°G–7°K) içindeki gece VIIRS tespiti testi |

## Teknik Kısıtlar

- Terra-only dönem (2002-07 öncesi, Aqua yok): seri 2002-07 veya sonrasını da içeriyorsa bu aylar karşılaştırılamaz sayılır (`count_normalized`, `effort_omega`, `anomaly` = `None`; Ω havuzlarına ve anomali tabanına girmez, `count`/FRP ham kalır). Seri tamamen 2002-07 öncesindeyse değişiklik yok. Gerekçe: Ω, MODIS referans kanalının kendi içindeki filo değişimini ölçemez (`effort::FIRST_TERRA_AQUA_MONTH`).
- `Confidence` enum'u `PartialOrd, Ord` derive eder: `Low < Nominal < High`.
- `calendar.rs` içinde `use chrono::Datelike;` import'u gereklidir (`.year()` ve `.month()` için).
- Cross-sensor matching: varsayılan 2 km mesafe, 180 dakika zaman penceresi (aynı yangının MODIS/VIIRS geçişleri FIRMS verisinde en az 34 dk arayla gelir). Tüm eşikler `PipelineConfig`'ten gelir; eski `harmonize`/`harmonize_with_links` fonksiyonları varsayılan konfigürasyonla çalışır, `*_config` varyantları konfigürasyon alır.
- Fusion: VIIRS tercih edilir (yüksek çözünürlük), eşleşmeyenler her iki sensörden alınır.
- VZA düzeltmesi ve ayak izi ölçeklemesi (`vza.rs`): Zheng et al. (2026) Şekil 4 nadirde 1'e normalize edilen oran (`R_rel(θ) = 0.2017θ² + 0.0168θ + 1.0`, θ radyan) MODIS ham FRP'si ile çarpılır (`raw_frp * ratio`). `ModisOnly` tespit başına enlem kalibrasyonu uygulanmaz.
- VIIRS öncesi tamlık katsayısı `k_c` (`calibration.rs` + `calendar.rs`): `0–30°: 1.11 (σ 0.08)`, `30–50°: 1.18 (σ 0.15)`, `55–70°: 1.28 (σ 0.11)` (50–55° lineer geçiş, >70° sabit) yalnız VIIRS tespiti içermeyen aylık `CalendarCell.total_frp` toplamına uygulanır; `frp_per_detection_eq = total_frp / count_normalized`. `mean_frp` ham piksel ortalamasıdır ve sensör geçişinde süreksizdir.
- FRP belirsizlik modeli (`uncertainty.rs`): `frp_lower`/`frp_upper`, `mean_frp_*` ve `total_frp_*` sınırları rastgele hataları karesel (`1/√n` ölçeklenen sensör + VZA), sistematik bileşeni ise doğrusal (`modis_scale_sigma = 0.10` ve `total_frp`'de `k_c` uygulanınca `ratio_sigma/ratio`) birleştirir; varsayılan sigma ve VZA hata katsayıları model varsayımlarıdır.
- Ön filtre (aşama 0, `harmonize_with_links_masked`): önce SAMA (varsayılan kapalı), sonra statik maske (varsayılan açık); elenen satırlar `HarmonizeLinks.sama_removed` / `static_masked` ile sayılır ve link'leri `None` olur. Maske verilmezse partiden kurulur; yıl yıl karar verildiği için tam yıllardan oluşan her bölümleme aynı sonucu verir, NRT pencereleri (< 10 ay) maskelenmez.
- FRP birimi: MW (megawatt). Negatif FRP olamaz.

## Bağımlılıklar

Sadece workspace dependency'leri kullanılır (`{ workspace = true }`). Yeni crate eklemeden önce workspace Cargo.toml'a eklenmeli.
