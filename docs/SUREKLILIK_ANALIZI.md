# Süreklilik Analizi — Sensör Dikişlerinde Ham ve Harmonize Metrikler (G11 Referans Ölçümü)

> [!IMPORTANT]
> **Bu bir REFERANS (baseline) ölçümdür:** `R10` (eşleşmiş kayıtlarda `1 MODIS = N VIIRS` ayak izi toplamı) ve `K11` (şiddet rejimli Ω) henüz uygulanmamıştır. İkisi tamamlandığında aynı komut ve ülke kümesiyle yeniden çalıştırılıp bu referans ölçümle karşılaştırılacaktır.

## 1. Amaç (G11)

MODIS ve VIIRS arşivleri 2000–2025 aralığında üç büyük sensör filosu geçişi içerir:
1. **Aqua (2002-07):** Yalnız Terra'dan Terra + Aqua çift-MODIS filosuna geçiş (`2001 → 2003`).
2. **SNPP (2012-01-20):** MODIS (1 km) yanına ilk 375 m VIIRS sensörünün eklenmesi (`2011 → 2012` ve `2011 → 2013`).
3. **NOAA-20 (2018-04):** Tek VIIRS (SNPP) filosundan çift VIIRS (SNPP + NOAA-20) filosuna geçiş (`2017 → 2019`).

Bu çalışma (`G11`), `crates/core` üzerindeki mevcut harmonizasyon, `Ω` efor havuzlama ve `build_calendar_with` takvim üreticisinin bu üç dikiş noktasındaki süreklilik performansını ham serilere kıyasla nicel olarak ölçer ve `R10` + `K11` öncesi başlangıç çizgisini kaydeder.

---

## 2. Yöntem

### 2.1 Veri ve Hesaplama Yolu
- **Kaynak:** Salt okunur yerel FIRMS ülke arşivi (`ARCHIVE_CACHE_DIR=C:/Users/user/Desktop/SpaceApps2026-MODIS-VIIRS/.cache/firms-archive`), `2000..=2025`.
- **Sensörler:** `ModisTerra` (FIRMS `modis` CSV'si içinde Terra + Aqua kayıtları), `ViirsSNPP` (`viirs-snpp`), `ViirsNOAA20` (`viirs-jpss1`). Her CSV okunduktan hemen sonra `retain_wildfires` ile yalnızca bitki örtüsü yangınları (`VegetationFire`) tutulmuştur.
- **Pipeline:** Sunucudaki `/api/calendar` yoluyla birebir aynı olacak şekilde, her ülke için tüm 2000–2025 yılları **tek seferde** `harmonize_with_config(modis, viirs, &PipelineConfig::default())` ile harmonize edilmiş ve `build_calendar_with(&harmonized, &config.uncertainty, &config.calibration)` ile aylık takvim hücreleri (`CalendarCell`) üretilmiştir. Böylece `Ω` havuzlaması tüm 26 yıllık seri üzerinden (`effort.rs`) yapılmıştır.
- **Ülke seçimi (10 biyom-çeşitli ülke, `data/archive-set.json` adlarıyla):**
  `Brazil` (tropikal/savana), `Angola` (Afrika savana/miombo), `Australia` (okaliptüs/kurak savana), `Indonesia` (tropikal turba), `India` (tarımsal anız + muson ormanı), `Turkey` (Akdeniz), `United_States` (karma ılıman/boreal/çalı), `Canada` (boreal), `Russian_Federation` (boreal/step) ve `Kazakhstan` (ılıman step).
  - Tüm 10 hedef ülke tek seferlik `harmonize_with_config` çağrısı altında belleğe sığmış ve tamamlanmıştır; hiçbir ülke küçültülmemiş veya değiştirilmemiştir (`Angola` 27.34 dk ile 20 dk sınırını aşsa da tek seferde başarıyla tamamlandığı için tutulmuştur).

### 2.2 Yıllık Metrik Tanımları
Her yıl `y` için `CalendarCell` hücrelerinden ve ham girdiden 6 yıllık metrik hesaplanır:
- `raw`: `retain_wildfires` sonrası o yıla ait ham FIRMS kayıt sayısı (`modis + viirs`).
- `count`: Yıllık harmonize kayıt toplamı (`Σ cell.count`).
- `count_normalized`: Yıllık `Ω`-normalize edilmiş MODIS-eşdeğeri kayıt toplamı (`Σ cell.count_normalized`). `effort.rs` içindeki `FIRST_TERRA_AQUA_MONTH = (2002, 7)` kuralı gereği 2000, 2001 ve 2002'nin ilk 6 ayında `effort_omega = None` olduğundan 2000 ve 2001 yılları `NA`dır.
- `total_frp`: Yıllık düzeltilmiş toplam FRP (`Σ cell.total_frp`, MW; VIIRS içermeyen aylarda enlem bandı tamlık çarpanı `k_c(lat)` içerir: `1.11`, `1.18`, `1.28`).
- `F_yıl`: Yıllık ağırlıklı birim tespit başına MODIS-eşdeğeri FRP:
  $$F_{\text{yıl}} = \frac{\sum \text{total\_frp}}{\sum \text{count\_normalized}}$$
  (`2000–2001` yıllarında `count_normalized = NA` olduğu için `NA`).
- `mean_frp_raw`: Yıllık kayıt-ağırlıklı ham ortalama FRP (`Σ (cell.mean_frp_raw · cell.count) / Σ cell.count`, MW).

### 2.3 Dikiş Adımları, Gürültü Referansı ve Skor
- **Dikiş adımı (log oranı):** $\Delta_{\log} = \ln(\text{after} / \text{before})$.
  - `aqua_2002`: `2001 → 2003` (`Δ = 2` yıl)
  - `snpp_2012_1y`: `2011 → 2012` (`Δ = 1` yıl)
  - `snpp_2012_2y`: `2011 → 2013` (`Δ = 2` yıl)
  - `noaa20_2018`: `2017 → 2019` (`Δ = 2` yıl)
- **Gürültü referansı:** Aynı ülkede `[y₁, y₂]` kapalı aralığı hiçbir dikiş yılını (`2002, 2012, 2018`) içermeyen tüm `Δ = 1` (`n = 19`; `count_normalized` ve `F_yıl` için `n = 18`) ve `Δ = 2` (`n = 15`) yıl çiftlerinin `|ln(v(y₂)/v(y₁))|` değerleri hesaplanır. Doğrusal interpolasyonlu (R-7) medyan (`m`) ve %90 yüzdeliği (`p90`) çıkarılır.
- **Z-benzeri skor (`z_like`):**
  $$z_{\text{benzeri}} = \frac{|\Delta_{\log}| - m}{p_{90} - m}$$
  Yorumlama: `z ≤ 0` doğal medyan dalgalanmanın altında; `0 < z ≤ 1` medyan ile `%90` yüzdeliği arasında; `z > 1` (`>P90`) doğal dikişsiz dalgalanmanın %90 sınırını aşar. Ayrıca CSV çıktısında ampirik yüzdelik sırası (`pct_rank`) da kaydedilir.
- **Hacim katmanı (`fire-harmony` eşiği):** Yıllık harmonize `count < 100` olan yıllar `"gürültü"` (`<10` ve `<100`) olarak etiketlenir; uç yıllarından biri bu katmanda kalan dikiş adımları `NOISE(<100)` bayrağı alır.

### 2.4 Yeniden Çalıştırma Komutu
Araç: [seam_continuity.rs](file:///C:/Users/user/Desktop/SpaceApps2026-MODIS-VIIRS/crates/firms-client/examples/seam_continuity.rs)

PowerShell (Windows'ta MinGW `C:\MinGW\bin\dlltool.exe` `os error 623` hatasını önlemek için WinLibs `mingw64\bin` PATH başına eklenir):
```powershell
$env:PATH = "$HOME\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.MSVCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin;$env:PATH"
$env:ARCHIVE_CACHE_DIR = "C:/Users/user/Desktop/SpaceApps2026-MODIS-VIIRS/.cache/firms-archive"
cargo run --release -p pyrosync-firms --example seam_continuity -- `
  Brazil Angola Australia Indonesia India Turkey United_States Canada Russian_Federation Kazakhstan `
  --from 2000 --to 2025 --out seam_continuity_results.csv
```

---

## 3. Çalışma Süreleri, Hacim Katmanı ve `Ω` Katsayıları

Tüm 10 ülkede dikiş uç yılları (`2001, 2003, 2011, 2012, 2013, 2017, 2019`) `>= 1000/yıl` yüksek hacim katmanındadır; hiçbir satırda `NOISE(<100)` bayrağı tetiklenmemiştir (`0/240`).

| Ülke | Ham MODIS | Ham VIIRS | Harmonize | Süre (sn / dk) | `Ω` (MODIS) | `Ω` (+SNPP) | `Ω` (+SNPP+NOAA20) |
|---|---:|---:|---:|---:|---:|---:|---:|
| `Brazil` | 9,857,726 | 29,680,591 | 26,830,437 | 923.85s (15.40 dk) | 1.0000 | 4.6284 | 6.3014 |
| `Angola` | 7,781,547 | 28,252,045 | 23,138,932 | 1640.19s (27.34 dk) | 1.0000 | 4.3490 | 6.1834 |
| `Australia` | 6,411,108 | 26,211,270 | 20,109,227 | 919.66s (15.33 dk) | 1.0000 | 5.1666 | 6.9677 |
| `Indonesia` | 1,443,591 | 4,554,085 | 4,565,342 | 197.99s (3.30 dk) | 1.0000 | 5.8904 | 9.1196 |
| `India` | 1,774,219 | 11,342,404 | 9,975,833 | 532.41s (8.87 dk) | 1.0000 | 6.9470 | 11.0491 |
| `Turkey` | 244,093 | 672,337 | 677,467 | 21.01s (0.35 dk) | 1.0000 | 3.9208 | 6.0356 |
| `United_States` | 2,913,228 | 9,455,808 | 7,736,551 | 292.91s (4.88 dk) | 1.0000 | 3.9795 | 5.8986 |
| `Canada` | 1,679,473 | 10,723,536 | 6,989,214 | 244.82s (4.08 dk) | 1.0000 | 6.2209 | 7.6728 |
| `Russian_Federation` | 7,384,347 | 23,315,041 | 18,595,474 | 774.53s (12.91 dk) | 1.0000 | 5.1023 | 6.5077 |
| `Kazakhstan` | 1,143,474 | 2,127,642 | 2,035,091 | 69.45s (1.16 dk) | 1.0000 | 4.2060 | 5.5869 |

*Not:* `Kazakhstan` serisinde yalnızca 1 kış ayında MODIS gözlemi sıfır olduğu için (`ViirsSNPP+ViirsNOAA20`, 52 kayıt) o ay için `Ω = NA` oluşmuştur.

---

## 4. Sonuçlar

### 4.1 (a) Ülke × Dikiş × Metrik Log-Adım Tabloları (`raw` vs Harmonize Yan Yana)

Hücre biçimi: `ln(after/before)` `(z_benzeri)`; sonundaki `*` işareti `|log_step| > p90` (doğal dalgalanmanın %90 sınırını aşan adım) anlamındadır.

#### Dikiş 1: `aqua_2002` (`2001 → 2003`, `Δ = 2` yıl)

> `effort.rs` 2002-07 öncesi ayları `Ω` referansından dışladığı için 2001 yılında `count_normalized` ve `F_yıl` hesaplanmaz (`NA`).

| Ülke | `raw` | `count` | `count_normalized` | `total_frp` | `F_yıl` | `mean_frp_raw` |
|---|---:|---:|---:|---:|---:|---:|
| `Brazil` | +1.3049 (+2.49)* | +1.2387 (+2.42)* | NA | +1.6681 (+3.52)* | NA | +0.4494 (+7.11)* |
| `Angola` | +1.7037 (+42.14)* | +1.6643 (+22.37)* | NA | +1.6873 (+20.42)* | NA | +0.0377 (+0.68) |
| `Australia` | -0.1046 (-0.62) | -0.1333 (-0.53) | NA | -0.1938 (-0.67) | NA | -0.0553 (-0.72) |
| `Indonesia` | +1.4834 (+0.65) | +1.4014 (+0.66) | NA | +1.6826 (+0.89) | NA | +0.2856 (+2.80)* |
| `India` | +1.0942 (+7.64)* | +1.0660 (+9.06)* | NA | +1.4717 (+7.53)* | NA | +0.4139 (+5.88)* |
| `Turkey` | +0.4001 (+0.07) | +0.3750 (+0.05) | NA | +0.3657 (-0.08) | NA | +0.0097 (-0.27) |
| `United_States` | +0.9524 (+3.03)* | +0.8565 (+3.01)* | NA | +1.1238 (+2.33)* | NA | +0.2860 (+0.80) |
| `Canada` | +1.9085 (+1.54)* | +1.6886 (+1.29)* | NA | +2.1395 (+1.72)* | NA | +0.4778 (+2.78)* |
| `Russian_Federation` | +1.2466 (+1.89)* | +1.0310 (+1.64)* | NA | +1.0810 (+1.37)* | NA | +0.1021 (-0.32) |
| `Kazakhstan` | +1.1328 (+1.63)* | +0.9597 (+1.35)* | NA | +0.9069 (+0.90) | NA | -0.0145 (-0.58) |

#### Dikiş 2a: `snpp_2012_1y` (`2011 → 2012`, `Δ = 1` yıl)

| Ülke | `raw` | `count` | `count_normalized` | `total_frp` | `F_yıl` | `mean_frp_raw` |
|---|---:|---:|---:|---:|---:|---:|
| `Brazil` | +2.0085 (+3.40)* | +1.9072 (+3.42)* | +0.3750 (+0.13) | +0.6650 (+0.58) | +0.2900 (+2.43)* | -1.0786 (+8.05)* |
| `Angola` | +1.4663 (+13.07)* | +1.3551 (+13.95)* | -0.1149 (+0.52) | +0.0610 (-0.15) | +0.1759 (+7.82)* | -1.1161 (+40.76)* |
| `Australia` | +1.7869 (+2.68)* | +1.7012 (+2.79)* | +0.0590 (-0.92) | +0.0792 (-0.91) | +0.0202 (-0.77) | -1.4591 (+10.06)* |
| `Indonesia` | +1.8149 (+1.09)* | +1.7663 (+1.12)* | -0.0070 (-1.08) | +0.6084 (-0.22) | +0.6154 (+7.59)* | -0.9983 (+9.62)* |
| `India` | +2.1938 (+8.12)* | +2.0916 (+7.77)* | +0.1533 (+0.17) | +0.7299 (+3.40)* | +0.5766 (+6.48)* | -1.1807 (+10.77)* |
| `Turkey` | +0.5500 (+0.33) | +0.4796 (+0.22) | -0.8867 (+1.57)* | -0.7786 (+0.62) | +0.1081 (+0.05) | -1.0410 (+3.75)* |
| `United_States` | +1.5608 (+5.95)* | +1.4493 (+6.89)* | +0.0682 (-0.95) | +0.1351 (-0.83) | +0.0670 (-0.44) | -1.1014 (+4.48)* |
| `Canada` | +1.7204 (+0.68) | +1.7699 (+0.72) | -0.0580 (-0.40) | -0.2423 (-0.36) | -0.1843 (+0.46) | -1.7386 (+11.76)* |
| `Russian_Federation` | +2.1521 (+2.56)* | +2.1883 (+3.17)* | +0.5586 (+0.48) | +0.5663 (+0.25) | +0.0077 (-0.69) | -1.3646 (+8.94)* |
| `Kazakhstan` | +2.2502 (+3.15)* | +2.1900 (+2.97)* | +0.7535 (+0.73) | +0.6898 (+0.30) | -0.0636 (-0.18) | -1.2839 (+7.90)* |

#### Dikiş 2b: `snpp_2012_2y` (`2011 → 2013`, `Δ = 2` yıl)

| Ülke | `raw` | `count` | `count_normalized` | `total_frp` | `F_yıl` | `mean_frp_raw` |
|---|---:|---:|---:|---:|---:|---:|
| `Brazil` | +1.3779 (+2.64)* | +1.3038 (+2.57)* | -0.2284 (+0.23) | +0.0482 (-0.25) | +0.2766 (+3.94)* | -1.0949 (+19.10)* |
| `Angola` | +1.5461 (+38.01)* | +1.4323 (+19.14)* | -0.0376 (-0.29) | +0.1361 (+0.67) | +0.1737 (+6.72)* | -1.1182 (+32.98)* |
| `Australia` | +0.9449 (+2.06)* | +0.8763 (+2.08)* | -0.7660 (+1.70)* | -0.9194 (+1.45)* | -0.1535 (+0.17) | -1.6266 (+13.28)* |
| `Indonesia` | +1.5881 (+0.76) | +1.5291 (+0.81) | -0.2442 (-0.68) | +0.4194 (-0.59) | +0.6635 (+8.50)* | -0.9497 (+12.13)* |
| `India` | +1.9850 (+14.99)* | +1.8944 (+17.27)* | -0.0439 (-1.07) | +0.5520 (+2.35)* | +0.5959 (+9.41)* | -1.1601 (+18.74)* |
| `Turkey` | +1.0277 (+1.47)* | +0.9494 (+1.38)* | -0.4169 (+0.15) | -0.2831 (-0.31) | +0.1338 (+0.13) | -1.0141 (+2.91)* |
| `United_States` | +1.2265 (+4.14)* | +1.1196 (+4.20)* | -0.2616 (+0.33) | -0.2729 (-0.08) | -0.0113 (-0.48) | -1.1783 (+5.36)* |
| `Canada` | +2.3207 (+1.98)* | +2.3530 (+2.00)* | +0.5251 (+0.05) | +0.5469 (+0.00) | +0.0218 (-0.44) | -1.5244 (+10.36)* |
| `Russian_Federation` | +1.3934 (+2.17)* | +1.4283 (+2.43)* | -0.2014 (-0.02) | -0.1808 (-0.29) | +0.0205 (-1.48) | -1.3500 (+14.99)* |
| `Kazakhstan` | +1.0373 (+1.39)* | +1.0197 (+1.51)* | -0.4169 (-0.09) | -0.4608 (+0.07) | -0.0440 (-0.14) | -1.2625 (+9.76)* |

#### Dikiş 3: `noaa20_2018` (`2017 → 2019`, `Δ = 2` yıl)

| Ülke | `raw` | `count` | `count_normalized` | `total_frp` | `F_yıl` | `mean_frp_raw` |
|---|---:|---:|---:|---:|---:|---:|
| `Brazil` | +0.5238 (+0.82) | +0.2922 (+0.36) | -0.0163 (-0.24) | +0.1627 (+0.02) | +0.1791 (+2.15)* | -0.1066 (+0.74) |
| `Angola` | +0.6629 (+14.89)* | +0.4045 (+4.82)* | +0.0526 (-0.09) | +0.2704 (+2.38)* | +0.2178 (+8.75)* | -0.0867 (+2.15)* |
| `Australia` | +0.8218 (+1.66)* | +0.4779 (+0.68) | +0.1788 (-0.37) | +0.3007 (-0.36) | +0.1219 (-0.14) | -0.1445 (+0.07) |
| `Indonesia` | +2.2327 (+1.44)* | +1.9667 (+1.31)* | +1.5296 (+0.81) | +1.9285 (+1.18)* | +0.3989 (+4.60)* | -0.0190 (-0.94) |
| `India` | +0.4824 (+2.60)* | +0.3138 (+1.61)* | -0.1503 (-0.01) | +0.2743 (+0.78) | +0.4245 (+6.36)* | -0.0184 (-0.94) |
| `Turkey` | +0.4567 (+0.19) | +0.3111 (-0.09) | -0.1202 (-0.53) | -0.0258 (-1.01) | +0.0944 (-0.01) | -0.3126 (+0.69) |
| `United_States` | +0.2562 (+0.20) | +0.0047 (-0.83) | -0.3889 (+0.90) | -0.3072 (+0.02) | +0.0817 (-0.15) | -0.2982 (+0.86) |
| `Canada` | -0.2964 (-0.19) | -0.6599 (+0.19) | -0.8696 (+0.42) | -0.6899 (+0.16) | +0.1798 (+0.55) | -0.0324 (-0.44) |
| `Russian_Federation` | +1.1281 (+1.67)* | +0.7651 (+1.11)* | +0.5218 (+0.62) | +0.6628 (+0.60) | +0.1410 (+0.11) | -0.0936 (-0.43) |
| `Kazakhstan` | +0.2029 (-0.70) | -0.1108 (-0.90) | -0.3947 (-0.15) | -0.2937 (-0.24) | +0.1010 (+0.27) | -0.1313 (+0.38) |

---

### 4.2 (b) Dikiş Başına Ülkeler Üzerinden Özet (N = 10 Ülke)

| Dikiş | Metrik | Medyan `|log_step|` | IQR `[Q25, Q75]` | Medyan işaretli `log_step` | Medyan `z_benzeri` | `>P90` aşan ülke |
|---|---|---:|---|---:|---:|---:|
| `aqua_2002` (2001→2003) | `raw` | **1.1897** | `[0.9878, 1.4388]` | +1.1897 | +1.76 | 7/10 |
| `aqua_2002` (2001→2003) | `count` | **1.0485** | `[0.8823, 1.3607]` | +1.0485 | +1.49 | 7/10 |
| `aqua_2002` (2001→2003) | `count_normalized` | **NA** | — | — | — | 0/0 |
| `aqua_2002` (2001→2003) | `total_frp` | **1.2978** | `[0.9504, 1.6790]` | +1.2978 | +1.54 | 6/10 |
| `aqua_2002` (2001→2003) | `F_yıl` | **NA** | — | — | — | 0/0 |
| `aqua_2002` (2001→2003) | `mean_frp_raw` | **0.1938** | `[0.0421, 0.3819]` | +0.1938 | +0.74 | 4/10 |
| `snpp_2012_1y` (2011→2012) | `raw` | **1.8009** | `[1.6007, 2.1162]` | +1.8009 | +2.91 | 8/10 |
| `snpp_2012_1y` (2011→2012) | `count` | **1.7681** | `[1.5123, 2.0455]` | +1.7681 | +3.07 | 8/10 |
| `snpp_2012_1y` (2011→2012) | `count_normalized` | **0.1341** | `[0.0613, 0.5127]` | +0.0636 | +0.15 | 1/10 |
| `snpp_2012_1y` (2011→2012) | `total_frp` | **0.5873** | `[0.1619, 0.6836]` | +0.3507 | +0.05 | 1/10 |
| `snpp_2012_1y` (2011→2012) | `F_yıl` | **0.1420** | `[0.0645, 0.2636]` | +0.0875 | +0.25 | 4/10 |
| `snpp_2012_1y` (2011→2012) | `mean_frp_raw` | **1.1484** | `[1.0843, 1.3444]` | -1.1484 | +9.28 | **10/10** |
| `snpp_2012_2y` (2011→2013) | `raw` | **1.3856** | `[1.0846, 1.5776]` | +1.3856 | +2.11 | 9/10 |
| `snpp_2012_2y` (2011→2013) | `count` | **1.3661** | `[1.0446, 1.5049]` | +1.3661 | +2.26 | 9/10 |
| `snpp_2012_2y` (2011→2013) | `count_normalized` | **0.2529** | `[0.2081, 0.4169]` | -0.2363 | +0.01 | 1/10 |
| `snpp_2012_2y` (2011→2013) | `total_frp` | **0.3512** | `[0.2038, 0.5254]` | -0.0663 | -0.04 | 2/10 |
| `snpp_2012_2y` (2011→2013) | `F_yıl` | **0.1437** | `[0.0274, 0.2509]` | +0.0778 | +0.15 | 4/10 |
| `snpp_2012_2y` (2011→2013) | `mean_frp_raw` | **1.1692** | `[1.1007, 1.3281]` | -1.1692 | +12.70 | **10/10** |
| `noaa20_2018` (2017→2019) | `raw` | **0.5031** | `[0.3365, 0.7821]` | +0.5031 | +1.13 | 5/10 |
| `noaa20_2018` (2017→2019) | `count` | **0.3591** | `[0.2970, 0.6144]` | +0.3124 | +0.52 | 4/10 |
| `noaa20_2018` (2017→2019) | `count_normalized` | **0.2839** | `[0.1277, 0.4900]` | -0.0683 | -0.05 | **0/10** |
| `noaa20_2018` (2017→2019) | `total_frp` | **0.2972** | `[0.2713, 0.5739]` | +0.2165 | +0.09 | 2/10 |
| `noaa20_2018` (2017→2019) | `F_yıl` | **0.1600** | `[0.1062, 0.2083]` | +0.1600 | +0.41 | 4/10 |
| `noaa20_2018` (2017→2019) | `mean_frp_raw` | **0.1001** | `[0.0460, 0.1412]` | -0.1001 | +0.23 | 1/10 |

---

### 4.3 (c) Gürültü Referansına Göre Doğal Dalgalanmanın Dışında Kalanlar

Ülkelerin kendi dikişsiz yıl-içi dalgalanma profilleri büyük farklılık göstermektedir (ör. `Angola`'da `Δ=2` `count_normalized` medyanı `0.0588` ve `p90 = 0.1306` iken, `Indonesia`'da sırasıyla `0.8291` ve `1.6945`tir). Bu referansa göre `>P90` sınırını sistematik olarak aşan kombinasyonlar şunlardır:

1. **2012 SNPP'de `mean_frp_raw` (`10/10` ülke, hem `1y` hem `2y`):** Her ülkede doğal `p90` sınırının çok ötesindedir (medyan `z_benzeri = +9.28` ve `+12.70`; `Angola`'da `z = +40.76`).
2. **2012 SNPP'de ham sayımlar `raw` ve `count` (`8/10` `1y`, `9/10` `2y`):** 2011→2012 ve 2011→2013 geçişlerinde sırasıyla `≈ 6×` (`+1.80`) ve `≈ 4×` (`+1.39`) sıçramayla doğal gürültünün dışındadır.
3. **2002 Aqua'da `raw`, `count` ve `total_frp` (`6/10`–`7/10` ülke):** Tek MODIS'ten çift MODIS'e geçişte `Ω` uygulanamadığı için medyan `|log_step| = 1.05–1.30` (`≈ 2.85–3.66×`) ile çoğu ülkede `>P90` aşılır.
4. **2018 NOAA-20'de `raw` (`5/10`) ve `count` (`4/10`):** Özellikle düşük gürültülü veya yüksek yoğunluklu rejimlerde (`Angola`, `India`, `Russia`, `Australia`, `Indonesia`) doğal sınırın üzerine çıkar.
5. **2012 ve 2018'de `F_yıl` (`4/10` ülke):** Sayım bazlı metriklere kıyasla `F_yıl` yıl-içi doğal gürültüsü çok küçüktür (medyan `0.02–0.15`). Bu nedenle `Brazil`, `Angola`, `Indonesia` ve `India` dörtlüsünde hem 2012 hem 2018 dikişlerinde pozitif kayma `>P90` olarak belirgin şekilde yakalanmıştır (`z = +2.15` ile `+9.41`).

---

## 5. Değerlendirme

### 5.1 Hangi Metrik Dikişlerde En Sürekli?
- **Sayım sürekliliğinde `count_normalized` açık ara en iyi metriktir:**
  - `snpp_2012_1y`: `count` medyan `|log_step|` `1.7681` iken `count_normalized` **`0.1341`** (`%92.4` düşüş, yalnızca `1/10 >P90`: `Turkey`).
  - `snpp_2012_2y`: `1.3661` → **`0.2529`** (`%81.5` düşüş, yalnızca `1/10 >P90`: `Australia`).
  - `noaa20_2018`: `raw` `0.5031`, `count` `0.3591` iken `count_normalized` **`0.2839`** (`0/10 >P90`, medyan `z = -0.05`).
- **Enerji bütçesinde `total_frp` ham sayıma göre çok daha sürekli ancak 2012/2018'de yanlılıklara açıktır:** Medyan `|log_step|` `0.5873` (2012 1y), `0.3512` (2012 2y) ve `0.2972` (2018) düzeyindedir; medyan `z_benzeri` sıfıra yakındır (`+0.05`, `-0.04`, `+0.09`), ancak `India` (`+0.7299` 2012 1y) ve `Angola`/`Indonesia` (2018) gibi ülkelerde `>P90` pozitif sapma gösterir.
- **`F_yıl` (`total_frp / count_normalized`) mutlak adım olarak küçüktür (`0.142–0.160`), ancak sistematik bir yukarı kayma taşır:** Orta ve yüksek enlem ülkelerinde (`US`, `Canada`, `Russia`, `Kazakhstan`, `Australia`, `Turkey`) 2012 geçişinde neredeyse tamamen düzdür (`|log_step| ≤ 0.18`). Buna karşın tropikal/subtropikal ülkelerde (`Brazil`, `Angola`, `Indonesia`, `India`) hem 2012'de (`+0.17` ila `+0.66`) hem 2018'de (**10 ülkenin tamamında pozitif**, medyan `+0.1600`) istikrarlı biçimde yükselir.

### 5.2 `mean_frp_raw`'ın Beklenen Çöküşü
`mean_frp_raw` 2012 SNPP dikişinde **10 ülkenin 10'unda da** çökmüştür (`medyan log_step = -1.1484` 1y, `-1.1692` 2y; yani tespit başına ortalama ham FRP **~%68–%69 düşmüştür**). Bunun nedeni, 375 m VIIRS piksellerinin MODIS'in göremediği çok sayıdaki düşük enerjili yangını kayda alması ve `Matched` eşleşmelerinde VIIRS FRP'sinin tercih edilmesidir. Buna karşılık 2002 (MODIS+MODIS) ve 2018 (VIIRS+VIIRS) dikişlerinde aynı piksel sınıfı içinde kalındığı için çöküş yaşanmaz (2018 medyan `-0.1001`, `1/10 >P90`). Bu tablo, `mean_frp_raw` (ve doğal piksel `mean_frp`) metriğinin sensör geçişleri boyunca karşılaştırılamaz olduğunu ampirik olarak doğrular.

### 5.3 `Ω`'nın 2012 ve 2018 Sayım Sıçramasını Giderme Derecesi ve Yan Etkisi
- **Sayım tarafı:** `Ω`, bölgesel VIIRS/MODIS tespit çarpanını (`3.92–6.95` SNPP; `5.59–11.05` SNPP+NOAA20) doğrudan serinin kendi örtüşen aylarından tahmin ettiği için hem 2012 (`6× → 1.07×`) hem 2018 (`1.65× → 0.93×`) sayım sıçramalarını medyan ölçeğinde neredeyse tamamen ortadan kaldırır.
- **`F_yıl` üzerindeki yan etki (`R10` / `K11` için referans bulgu):** Mevcut pipeline'da `count_normalized = count / Ω` formülü eforu bölerken `total_frp` üzerinde eşdeğer bir `Ω` veya şiddet-rejimli düzeltme yoktur (yalnızca pre-2012 için `k_c(lat)` vardır). Sonuç olarak:
  1. `2012`'de tropikal/subtropikal `k_c(lat)` oranları (`1.11` / `1.18`) VIIRS'ın eklediği toplam FRP'yi tam karşılayamadığında (`Indonesia`, `India`, `Brazil`, `Angola`) `F_yıl` yukarı sıçrar; ayrıca `R10` henüz olmadığı için `Matched` kayıtlarda `1 MODIS` karşılığı `N VIIRS` alt-pikselinin FRP toplamı yerine tek bir VIIRS pikseli tutulur.
  2. `2018`'de `NOAA-20` eklendiğinde `Ω` yaklaşık `%23–%59` büyür (`count_normalized` sabit kalır), fakat ekstra VIIRS yörünge geçişinin eklediği yangın enerjisi `total_frp`'yi yukarı iter (`medyan +0.2165`). Bu yüzden `F_yıl = total_frp / count_normalized` **10 ülkenin 10'unda da `+0.082` ile `+0.425` arasında yukarı kayar**. `R10` ve `K11` sonrası yeniden ölçümde özellikle bu `2018` `F_yıl` pozitif kaymasının ve tropikal `2012` `F_yıl` sapmasının azalıp azalmadığı test edilecektir.

### 5.4 Sınırlamalar
1. **Gerçek yıllar-arası yangın değişkenliği:** Tek yıllık dikiş karşılaştırmaları gerçek iklim/yangın anomalilerinden etkilenir (ör. `Australia 2011` güçlü La Niña sonrası ot yangını zirvesi, `Indonesia 2019` şiddetli El Niño/IOD kuraklığı, `Canada 2017` büyük yangın yılı, `Turkey 2011→2012` düşüşü). Çok-yıllı blok ortalamaları ve placebo adım testi (`G12`) bu ayrımı keskinleştirecektir.
2. **2000 yılının kısmi yıl olması:** FIRMS MODIS arşivi Kasım 2000'de başladığı için `2000` yılı yalnızca 2 ay içerir. `(2000, 2001)` çifti `Δ=1` gürültü referansına dahil edildiğinden (`n=19`), `raw`/`count`/`total_frp` `p90` sınırını yukarı çekebilmektedir (medyanı ise 1/19 oranında minimal etkiler).
3. **2002 Aqua dikişinde `count_normalized` ve `F_yıl` yokluğu:** `FIRST_TERRA_AQUA_MONTH = (2002, 7)` gereği 2001 Terra-yalnız dönemi normalize edilmediğinden Aqua dikişi yalnızca `raw`, `count`, `total_frp` ve `mean_frp_raw` üzerinden değerlendirilebilmiştir.
4. **2012 yılının ilk 19 günü:** SNPP 20 Ocak 2012'de başladığı için `2012` yılının ilk 19 günü yalnızca MODIS içerir; güney yarımküre veya tropikal ocak yangınlarında küçük bir kısmi-ay etkisi yaratır (`2011→2013` 2 yıllık dikişi bunu atlamak için birlikte raporlanmıştır).
5. **Hacim katmanı kapsamı:** Seçilen 10 biyom-çeşitli ülkenin tamamı `>= 1000/yıl` yüksek hacim katmanındadır; `< 100/yıl` gürültü eşiği altındaki küçük ada/çöl ülkelerindeki davranış bu ilk referans çalışmada test edilmemiştir.
6. **MODIS=0 olan nadir kış ayları:** `Kazakhstan`'da 1 kış ayında MODIS sıfır iken yalnızca VIIRS tespiti olduğu için `Ω = NA` oluşmuştur; yıllık toplamı ihmal edilebilir düzeyde etkilese de aylık çözünürlükte düşük aktiviteli kuzey bölgelerde görülebilen bir uç durumdur.

---

## 6. Final Ölçüm (R10 + K11 sonrası, 2026-11-15)

Kod: `main` @ `a1e53b3` (R10 1 MODIS = N VIIRS toplamı + MODIS ayak izi sınırı, toplam kayıtlarda karesel belirsizlik, K11, K12; çoklu VIIRS FRP cetveli denenip geri alındı). Aynı komut ve aynı 10 ülke, 2000–2025; toplam süre 5489 sn. Ham CSV: `seam_continuity_results.csv` biçiminde yerel çıktı.

Ülkeler arası medyan `|log adım|` ve doğal dalgalanmanın (dikiş dışı yıl çiftlerinin p90'ı) dışına çıkan ülke sayısı:

| Dikiş | Metrik | Medyan `\|log\|` | Medyan yön | `> P90` |
|---|---|---:|---:|---:|
| 2012 SNPP (2011→2012) | ham sayım (`raw`) | 1.801 | +1.801 | 8/10 |
| | `count_normalized` | **0.133** | +0.042 | **1/10** |
| | `total_frp` | 0.629 | +0.414 | 1/10 |
| | `F` (`frp_per_detection_eq`) | 0.139 | +0.139 | 4/10 |
| | `mean_frp_raw` | 0.991 | −0.991 | 10/10 |
| 2012 SNPP (2011→2013) | ham sayım | 1.386 | +1.386 | 9/10 |
| | `count_normalized` | **0.249** | −0.222 | **1/10** |
| | `total_frp` | 0.316 | −0.016 | 3/10 |
| | `F` | 0.131 | +0.127 | 4/10 |
| 2018 NOAA-20 (2017→2019) | ham sayım | 0.503 | +0.503 | 5/10 |
| | `count_normalized` | **0.254** | −0.057 | **0/10** |
| | `total_frp` | 0.301 | +0.212 | 2/10 |
| | `F` | 0.163 | +0.163 | 4/10 |
| 2002 Aqua (2001→2003) | ham sayım | 1.190 | +1.190 | 7/10 |
| | `count_normalized`, `F` | NA | — | tasarım gereği (Aqua öncesi aylar normalize edilmez) |
| | `total_frp` | 1.297 | +1.297 | 6/10 |

**Sonuç:**
- Manşet metrik `count_normalized`, 2012 ve 2018 dikişlerinde 10 ülkenin 9'unda ve 10'unda doğal yıllar arası dalgalanmanın içinde kalır; ham sayımdaki adım (2012'de medyan ×6) ortadan kalkar.
- `total_frp` büyük ölçüde süreklidir (2012'de 1/10, 2018'de 2/10 ülke p90 dışında); 2012 1 yıllık pencerede medyan yön hâlâ yukarıdır.
- `F` iki dikişte de 4/10 ülkede p90 dışına çıkar ve yukarı yönlüdür; bu, VIIRS'ın tespit başına daha fazla küçük yangın enerjisi toplamasının kalıntısıdır. Çoklu VIIRS FRP cetveli bunu gidermek için denendi, ancak filo birleştirme aynı yangının S-NPP/NOAA-20 FRP'sinin ortalamasını zaten aldığından 2018 sonrası FRP'yi iki kez düzelttiği (Türkiye Ağustos toplamları yarıya indi, Manavgat sırası 2 → 8) görülüp geri alındı.
- Önceki referans ölçümle (R10 öncesi: `count_normalized` 0.13, `total_frp` 0.59, `F` 0.14) kıyaslandığında R10 süreklilik metriklerini anlamlı biçimde değiştirmemiştir.
- Sunumda ve arayüzde manşet metrik `count_normalized` olmalı; `total_frp` şiddet sorularında (bkz. `DOGRULAMA_PAKETI.md` Bölüm D) kullanılmalı, `F` ise kalıntı adım notuyla gösterilmelidir.
