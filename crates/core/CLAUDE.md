# pyrosync-core — Harmonizasyon Motoru

## Kesin Kurallar
- Commit mesajlarına `Co-Authored-By` veya AI imzası eklenmez.
- Bu crate saf hesaplama kütüphanesidir: I/O, network veya dosya işlemi yapılmaz.
- Tüm public fonksiyonlar unit test ile kapsanmalıdır.

## Modül Sorumlulukları

| Modül | Görev |
|-------|-------|
| `models.rs` | Veri modelleri: Hotspot, HarmonizedHotspot, Sensor, Confidence, BBox, CalendarCell |
| `dedup.rs` | İnterscan duplikat temizleme (Δt<8s, overlap>30%), haversine mesafe hesabı |
| `vza.rs` | MODIS FRP görüş açısı düzeltmesi (kuadratik polinom) |
| `harmonize.rs` | 7 aşamalı pipeline: dedup → VZA → cross-sensor matching → fusion |
| `calendar.rs` | Aylık yanma takvimi oluşturma, anomali hesabı |

## Teknik Kısıtlar

- `Confidence` enum'u `PartialOrd, Ord` derive eder: `Low < Nominal < High`.
- `calendar.rs` içinde `use chrono::Datelike;` import'u gereklidir (`.year()` ve `.month()` için).
- Cross-sensor matching: 2 km mesafe, 5 dakika zaman penceresi.
- Fusion: VIIRS tercih edilir (yüksek çözünürlük), eşleşmeyenler her iki sensörden alınır.
- FRP birimi: MW (megawatt). Negatif FRP olamaz.

## Bağımlılıklar

Sadece workspace dependency'leri kullanılır (`{ workspace = true }`). Yeni crate eklemeden önce workspace Cargo.toml'a eklenmeli.
