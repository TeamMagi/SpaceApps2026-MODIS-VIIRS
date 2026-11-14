# pyrosync-core — Harmonizasyon Motoru

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- Bu crate saf hesaplama kütüphanesidir: I/O, network veya dosya işlemi yapılmaz.
- Tüm public fonksiyonlar unit test ile kapsanmalıdır.

## Modül Sorumlulukları

| Modül | Görev |
|-------|-------|
| `config.rs` | `PipelineConfig`: dedup, filo, eşleştirme ve VZA eşikleri; varsayılanlar önceki sabit değerlerle birebir aynı, `validate()` ile doğrulanır |
| `models.rs` | Veri modelleri: Hotspot, HarmonizedHotspot, Sensor, Confidence, BBox, CalendarCell |
| `dedup.rs` | İnterscan duplikat temizleme (varsayılan Δt<8s, overlap>30%), haversine mesafe hesabı |
| `fleet.rs` | Aynı sensör ailesinin (MODIS Terra/Aqua, VIIRS SNPP/NOAA-20/21) eşzamanlı tespitlerini tek gözleme birleştirme |
| `h3index.rs` | H3 (çözünürlük 8) uzamsal indeks; `k_ring_for_radius` arama halkasını eşleştirme yarıçapına göre ölçekler |
| `vza.rs` | MODIS FRP görüş açısı düzeltmesi (kuadratik polinom, katsayılar `VzaConfig`'ten) |
| `calibration.rs` | Enlem bandına dayalı MODIS→VIIRS FRP kalibrasyonu (Li et al. 2018 oranları, geçiş boşluklarında doğrusal interpolasyon) |
| `harmonize.rs` | 7 aşamalı pipeline: dedup → VZA → cross-sensor matching → fusion |
| `effort.rs` | Gözlem eforu (EffortFactor) tahmini |
| `calendar.rs` | Aylık yanma takvimi oluşturma, anomali hesabı |
| `export.rs` | Harmonize hotspot'ları CSV ve GeoJSON metnine dönüştürme |

## Teknik Kısıtlar

- Terra-only dönem (2002-07 öncesi, Aqua yok): seri 2002-07 veya sonrasını da içeriyorsa bu aylar karşılaştırılamaz sayılır (`count_normalized`, `effort_omega`, `anomaly` = `None`; Ω havuzlarına ve anomali tabanına girmez, `count`/FRP ham kalır). Seri tamamen 2002-07 öncesindeyse değişiklik yok. Gerekçe: Ω, MODIS referans kanalının kendi içindeki filo değişimini ölçemez (`effort::FIRST_TERRA_AQUA_MONTH`).
- `Confidence` enum'u `PartialOrd, Ord` derive eder: `Low < Nominal < High`.
- `calendar.rs` içinde `use chrono::Datelike;` import'u gereklidir (`.year()` ve `.month()` için).
- Cross-sensor matching: varsayılan 2 km mesafe, 180 dakika zaman penceresi (aynı yangının MODIS/VIIRS geçişleri FIRMS verisinde en az 34 dk arayla gelir). Tüm eşikler `PipelineConfig`'ten gelir; eski `harmonize`/`harmonize_with_links` fonksiyonları varsayılan konfigürasyonla çalışır, `*_config` varyantları konfigürasyon alır.
- Fusion: VIIRS tercih edilir (yüksek çözünürlük), eşleşmeyenler her iki sensörden alınır.
- Enlem bandı FRP kalibrasyonu (Li et al. 2018): harmonize ölçek VIIRS olduğu için kalibrasyon (`frp_corrected = vza_mean * ratio`) yalnız `ModisOnly` kayıtlara uygulanır (`Matched` ve `ViirsOnly` değişmez); bantlar `|lat|` simetrisiyle uygulanır (30°: 1.16, 30–50°: 0.97, 55–70°: 0.82; 50–55° doğrusal interpolasyon, >70° sabitlenir).
- Büyük boreal küme sınırlaması: Li et al. (2018) >1000 MW boreal kümelerde yoğun duman etkisiyle MODIS FRP'nin VIIRS'tan %47'ye kadar büyük olduğunu bildirir; tespit başına küme FRP'si bilinmediğinden bu ek düzeltme uygulanmaz.
- FRP birimi: MW (megawatt). Negatif FRP olamaz.

## Bağımlılıklar

Sadece workspace dependency'leri kullanılır (`{ workspace = true }`). Yeni crate eklemeden önce workspace Cargo.toml'a eklenmeli.
