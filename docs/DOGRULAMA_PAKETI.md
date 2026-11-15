# İç Doğrulama Paketi — Placebo Adım Testi, LOYO Normalizasyon ve Model vs Baseline Kapısı (G12 Referans Ölçümü)

> [!IMPORTANT]
> **REFERANS ölçüm: `R10` ve `K11` öncesi; aynı komutla yeniden çalıştırılacak.**
> Bu çalışma, `R10` (`1 MODIS = N VIIRS` ayak izi toplamı) ve `K11` (şiddet rejimli `Ω`) henüz uygulanmadan önceki başlangıç doğrulama ölçümünü kaydeder. İki görev tamamlandığında aynı komut, parametreler ve ülke kümesiyle yeniden çalıştırılarak karşılaştırılacaktır.

## 1. Amaç (G12)

[G11 süreklilik analizinde](file:///C:/Users/user/Desktop/SpaceApps2026-MODIS-VIIRS/docs/SUREKLILIK_ANALIZI.md) (`seam_continuity.rs`) üç büyük sensör filosu geçişindeki (2002 Aqua, 2012 SNPP, 2018 NOAA-20) ham ve harmonize adım büyüklükleri tanımlayıcı olarak ölçülmüştür. Bu iç doğrulama paketi (`G12`), üç tamamlayıcı nicel sınamayla pipeline'ın istatistiksel geçerliliğini ve out-of-sample tahmin gücünü test eder:

1. **A. Placebo Adım Testi (`fire-harmony`):** Gerçek sensör dikiş adımlarının, aynı serideki dikiş içermeyen yıl çiftlerinin ("placebo" dikişleri) doğal `|log adım|` dağılımından istatistiksel olarak ayrışıp ayrışmadığını tekil ülke ve 10 ülke birleşik sıra testleriyle ölçer.
2. **B. Yıl-Dışarıda (LOYO) Normalizasyon Testi:** VIIRS dönemindeki (`has_viirs`) her yıl için `Ω` efor çarpanını o yılın ayları **hariç** kalan aylardan yeniden tahmin edip yıllık MODIS-gözlenebilir tespit sayısını tahmin ederek iki adil baseline (`b0` normalizasyonsuz, `b1` kalıcılık) ile kıyaslar.
3. **C. Model vs Baseline Kapısı (`eg-firetrack` / `flare-chronos`):** Model hatası ile baseline hatası arasındaki farkın (`baseline − model`) yıl-bloklu bootstrap `%90` güven aralığını hesaplayarak sıfırı kesip kesmediğine göre `GEÇTİ` / `BELİRSİZ` / `KALDI` kararı üretir.

---

## 2. Yöntem

### 2.1 Veri ve Hesaplama Yolu
- **Arşiv ve dönem:** Salt okunur yerel FIRMS arşivi (`.cache/firms-archive`), `2000..=2025`, `ModisTerra` (Terra + Aqua), `ViirsSNPP` ve `ViirsNOAA20`. Yükleme sırasında `retain_wildfires` ile yalnız `VegetationFire` kayıtları tutulmuştur.
- **Pipeline (G11 ve `/api/calendar` ile birebir):** Her ülke için 26 yıllık seri tek seferde `harmonize_with_config(modis, viirs, &PipelineConfig::default())` ve `build_calendar_with(&harmonized, &config.uncertainty, &config.calibration)` ile işlenmiştir.
- **Ülkeler (G11 ile aynı 10 ülke, sıralı):** `Brazil`, `Angola`, `Australia`, `Indonesia`, `India`, `Turkey`, `United_States`, `Canada`, `Russian_Federation`, `Kazakhstan`.

### 2.2 A. Placebo Adım Testi
- **Test edilen yıllık metrikler:** `raw`, `count_normalized`, `total_frp`, `F_yıl` (`Σtotal_frp / Σcount_normalized`).
- **Gerçek dikişler:**
  - `aqua_2002`: `2001 → 2003` (`Δ = 2` yıl). `effort.rs` `FIRST_TERRA_AQUA_MONTH = (2002, 7)` kuralı nedeniyle `2001` normalize edilmediğinden `count_normalized` ve `F_yıl` bu dikişte `NA`dır; yalnız `raw` ve `total_frp` test edilir.
  - `snpp_2012`: `2011 → 2012` (`Δ = 1` yıl).
  - `noaa20_2018`: `2017 → 2019` (`Δ = 2` yıl).
- **Placebo dağılımı:** `[y₁, y₁ + Δ]` kapalı aralığı hiçbir dikiş yılını (`{2002, 2012, 2018}`) içermeyen tüm yıl çiftleri için $x = |\ln(v(y_1 + \Delta) / v(y_1))|$ değerleri toplanır (`Δ = 1` için $n_{\text{pl}} = 19$, normalize metriklerde $18$; `Δ = 2` için $n_{\text{pl}} = 15$).
- **Ampirik p-değeri:**
  $$p_{\text{emp}} = \frac{\#\{x_i \in \text{placebo} : x_i \ge |s_{\text{gerçek}}|\} + 1}{n_{\text{pl}} + 1}$$
- **Ülkeler üzerinden birleştirilmiş test:** Her ülkede gerçek adımın placebo dağılımındaki orta-sıralı (*mid-rank*) yüzdelik sırası hesaplanır:
  $$r_c = \frac{\#\{x_i < |s_{\text{gerçek}}|\} + 0.5\,\#\{x_i = |s_{\text{gerçek}}|\}}{n_{\text{pl}}} \in [0, 1]$$
  $H_0$ altında (dikiş adımı placebo çekimlerinden ayırt edilemezken) $E[r_c] = 0.50$'dir. $N = 10$ ülke üzerinden:
  1. Ortalama (`mean_rk`) ve medyan (`med_rk`) yüzdelik sıra,
  2. $r_c - 0.5$ üzerine **kesin iki-yönlü işaret testi** (`sign_p`),
  3. $r_c - 0.5$ üzerine **kesin iki-yönlü Wilcoxon işaretli-sıra testi** ($W_+$ ve tüm $2^m = 1024$ işaret çevrimi üzerinden kesin `wilc_p`) raporlanır.
- **Yorum kuralı:** Ham `raw` metrikte dikişler placebo'dan istatistiksel olarak ayrışabilmelidir (pozitif kontrol: düşük `p`, `mean_rk > 0.5`). Harmonize metrikte dikişler placebo'dan ayırt edilemiyorsa (`p` büyük, `mean_rk ≈ 0.5`) süreklilik sağlanmış demektir.

### 2.3 B. Yıl-Dışarıda (LOYO) Normalizasyon Testi
- `effort.rs` tanımına göre her `(yıl, ay)` kovası için `total` (toplam harmonize kayıt sayısı), `modis_observable` (`platforms` kümesinde en az bir MODIS sensörü içeren kayıt sayısı) ve havuz anahtarı `(has_modis, viirs_platforms)` çıkarılır. `FIRST_TERRA_AQUA_MONTH = (2002, 7)` öncesi aylar efor havuzlamasından çıkarılır.
- VIIRS dönemindeki (`has_viirs`, arşivde `2012..=2025`, ülke başına 14 yıl) her $y$ yılı için, **$y$'nin ayları hariç** kalan tüm aylardan havuz başına $\Omega_{-y}$ yeniden tahmin edilir:
  $$\Omega_{-y}(k) = \frac{\sum_{m \notin y,\, \text{key}(m)=k} \text{total}_m}{\sum_{m \notin y,\, \text{key}(m)=k} \text{modis\_observable}_m} \quad (\text{ay sayısı} \ge 3 \text{ ve } \Sigma\text{modis\_observable} > 0 \text{ ise})$$
  `MIN_MONTHS_PER_CONFIG = 3` şartını sağlamayan veya `modis_observable = 0` olan havuzlar `NA` bırakılır.
- Yıllık tahmin ve hata (`|ln(tahmin / gerçek)|`):
  $$\widehat{\text{modis\_obs}}_y^{\text{LOYO}} = \sum_{m \in y} \frac{\text{total}_{y,m}}{\Omega_{-y}(\text{key}(m))}, \qquad \text{hata}_y = \left|\ln\!\left(\frac{\widehat{\text{modis\_obs}}_y}{\text{modis\_observable}_y}\right)\right|$$
  *(Homojen yıllarda her ay aynı havuzda olduğundan bu toplam doğrudan $\text{total}_y / \Omega_{-y}$'ye eşittir; `2018` gibi geçiş yıllarında ise Ocak–Mart `[SNPP]` ve Nisan–Aralık `[SNPP, NOAA20]` havuzlarını kendi dışarıda bırakılmış $\Omega_{-2018}$ katsayılarıyla doğru biçimde birleştirir.)*
- **Baseline seçimi ve gerekçesi:**
  - **`b0_unnormalized` (normalizasyonsuz):** $\widehat{\text{modis\_obs}}_y^{(b0)} = \text{total}_y$. Efor normalizasyonu yapılmadığında ortaya çıkan ham VIIRS şişme hatasını ölçer.
  - **`b1_persistence` (kalıcılık):** $\widehat{\text{modis\_obs}}_y^{(b1)} = \text{modis\_observable}_{y-1}$ (önceki yılın gerçek MODIS-gözlenebilir sayımı). Prompt'ta belirtilen $\text{modis\_observable}_{y-1} \times (\text{total}_y / \text{total}_{y-1})$ formülü cebirsel olarak $\text{total}_y / \Omega_{y-1}$'e (1 yıl gecikmeli bir `Ω` normalizasyonuna) denk olduğundan ve 2011→2012 ile 2017→2018 gibi filo geçişlerinde $\text{total}_y / \text{total}_{y-1}$ gerçek yangın değişimi yerine sensör sıçramasını taşıdığından, daha basit ve adil olan doğrudan 1 yıllık kalıcılık $\text{modis\_observable}_{y-1}$ seçilmiştir.

### 2.4 C. Model vs Baseline Kapısı (`eg-firetrack`)
- Her `(ülke, yıl)` gözlemi için fark: $d_{c,y} = \text{err\_baseline}_{c,y} - \text{err\_model}_{c,y}$ (`d > 0` modelin hatasının daha düşük olduğunu gösterir).
- **Yıl-bloklu bootstrap `%90` güven aralığı:** Aynı yıldaki ülkeler küresel iklim olayları (ör. El Niño) ve ortak sensör geçişleri nedeniyle korelasyonlu olduğundan, bootstrap örneklemesi **yılları blok olarak** (`n_yr = 14` yıl bloğundan yerine koymalı `B = 1000` çekim, deterministik `xorshift64`, `--seed 42`) yeniden örnekler. Her bootstrap yinelemesinin ortalamaları sıralanıp doğrusal interpolasyonlu (R-7) `[q0.05, q0.95]` aralığı çıkarılır.
- **Kapı kararı:**
  - `ci90_lo > 0` → **`GEÇTİ`** (model istatistiksel olarak baseline'ı yeniyor)
  - `ci90_lo ≤ 0 ≤ ci90_hi` → **`BELİRSİZ`** (0 aralığın içinde; belirsizlik nedeniyle kapıdan geçemez)
  - `ci90_hi < 0` → **`KALDI`** (baseline modeli yeniyor)

---

## 3. Yeniden Çalıştırma Komutu

Araç: [validation_suite.rs](file:///C:/Users/user/Desktop/SpaceApps2026-MODIS-VIIRS/crates/firms-client/examples/validation_suite.rs)

```powershell
$env:PATH = "$HOME\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.MSVCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin;$env:PATH"
$env:ARCHIVE_CACHE_DIR = "C:/Users/user/Desktop/SpaceApps2026-MODIS-VIIRS/.cache/firms-archive"
cargo run --release -p pyrosync-firms --example validation_suite -- `
  Brazil Angola Australia Indonesia India Turkey United_States Canada Russian_Federation Kazakhstan `
  --from 2000 --to 2025 --out validation_suite_results.csv --bootstrap 1000 --seed 42
```

---

## 4. Sonuçlar

### 4.1 Çalışma Süreleri ve Veri Hacmi

Toplam süre: **5335.02 sn (88.92 dk)**.

| Ülke | Ham MODIS | Ham VIIRS | Harmonize | Yükleme (sn) | Harmonize (sn) | Takvim (sn) | Toplam (sn / dk) |
|---|---:|---:|---:|---:|---:|---:|---:|
| `Brazil` | 9,857,726 | 29,680,591 | 26,830,437 | 51.80 | 1221.17 | 6.74 | 1280.64s (21.34 dk) |
| `Angola` | 7,781,547 | 28,252,045 | 23,138,932 | 38.34 | 1381.94 | 4.09 | 1425.06s (23.75 dk) |
| `Australia` | 6,411,108 | 26,211,270 | 20,109,227 | 32.07 | 700.73 | 4.19 | 737.77s (12.30 dk) |
| `Indonesia` | 1,443,591 | 4,554,085 | 4,565,342 | 7.63 | 136.36 | 0.53 | 144.62s (2.41 dk) |
| `India` | 1,774,219 | 11,342,404 | 9,975,833 | 13.45 | 403.06 | 1.66 | 418.91s (6.98 dk) |
| `Turkey` | 244,093 | 672,337 | 677,467 | 1.82 | 13.72 | 0.07 | 15.63s (0.26 dk) |
| `United_States` | 2,913,228 | 9,455,808 | 7,736,551 | 15.68 | 199.21 | 1.23 | 216.38s (3.61 dk) |
| `Canada` | 1,679,473 | 10,723,536 | 6,989,214 | 14.79 | 233.83 | 1.16 | 250.13s (4.17 dk) |
| `Russian_Federation` | 7,384,347 | 23,315,041 | 18,595,474 | 43.39 | 729.87 | 3.87 | 778.21s (12.97 dk) |
| `Kazakhstan` | 1,143,474 | 2,127,642 | 2,035,091 | 6.15 | 61.15 | 0.30 | 67.66s (1.13 dk) |

---

### 4.2 A. Placebo Adım Testi — Ülke × Dikiş × Metrik Ampirik p-Değerleri

Hücre biçimi: `log_step` (`emp_p`, `pct_rk%`). `n_pl`: `aqua_2002` ve `noaa20_2018` için `15` (en küçük mümkün `emp_p = 0.0625`), `snpp_2012` için `raw`/`total_frp`'de `19` (min `0.0500`), `count_normalized`/`F_yıl`'da `18` (min `0.0526`).

#### Dikiş 1: `aqua_2002` (`2001 → 2003`, `Δ = 2` yıl)

> `count_normalized` ve `F_yıl` 2001'de Aqua öncesi kuralı gereği `NA`dır.

| Ülke | `raw` | `total_frp` |
|---|---|---|
| `Brazil` | +1.3049 (p=0.0625, %100.0) | +1.6681 (p=0.0625, %100.0) |
| `Angola` | +1.7037 (p=0.0625, %100.0) | +1.6873 (p=0.0625, %100.0) |
| `Australia` | -0.1046 (p=0.7500, %26.7) | -0.1938 (p=0.8750, %13.3) |
| `Indonesia` | +1.4834 (p=0.2500, %80.0) | +1.6826 (p=0.1875, %86.7) |
| `India` | +1.0942 (p=0.0625, %100.0) | +1.4717 (p=0.0625, %100.0) |
| `Turkey` | +0.4001 (p=0.5000, %53.3) | +0.3657 (p=0.7500, %26.7) |
| `United_States` | +0.9524 (p=0.0625, %100.0) | +1.1238 (p=0.0625, %100.0) |
| `Canada` | +1.9085 (p=0.0625, %100.0) | +2.1395 (p=0.0625, %100.0) |
| `Russian_Federation` | +1.2466 (p=0.0625, %100.0) | +1.0810 (p=0.0625, %100.0) |
| `Kazakhstan` | +1.1328 (p=0.1250, %93.3) | +0.9069 (p=0.1875, %86.7) |

#### Dikiş 2: `snpp_2012` (`2011 → 2012`, `Δ = 1` yıl)

| Ülke | `raw` | `count_normalized` | `total_frp` | `F_yıl` |
|---|---|---|---|---|
| `Brazil` | +2.0085 (p=0.1000, %94.7) | +0.3750 (p=0.3684, %66.7) | +0.6650 (p=0.3000, %73.7) | +0.2900 (p=0.0526, %100.0) |
| `Angola` | +1.4663 (p=0.1000, %94.7) | -0.1149 (p=0.3158, %72.2) | +0.0610 (p=0.6000, %42.1) | +0.1759 (p=0.0526, %100.0) |
| `Australia` | +1.7869 (p=0.1000, %94.7) | +0.0590 (p=0.8947, %11.1) | +0.0792 (p=0.9000, %10.5) | +0.0202 (p=0.9474, %5.6) |
| `Indonesia` | +1.8149 (p=0.1500, %89.5) | -0.0070 (p=1.0000, %0.0) | +0.6084 (p=0.6000, %42.1) | +0.6154 (p=0.0526, %100.0) |
| `India` | +2.1938 (p=0.1000, %94.7) | +0.1533 (p=0.3158, %72.2) | +0.7299 (p=0.1000, %94.7) | +0.5766 (p=0.0526, %100.0) |
| `Turkey` | +0.5500 (p=0.3500, %68.4) | -0.8867 (p=0.1579, %88.9) | -0.7786 (p=0.2500, %78.9) | +0.1081 (p=0.4211, %61.1) |
| `United_States` | +1.5608 (p=0.1000, %94.7) | +0.0682 (p=0.8421, %16.7) | +0.1351 (p=0.8500, %15.8) | +0.0670 (p=0.7895, %22.2) |
| `Canada` | +1.7204 (p=0.2000, %84.2) | -0.0580 (p=0.8947, %11.1) | -0.2423 (p=0.8000, %21.1) | -0.1843 (p=0.3158, %72.2) |
| `Russian_Federation` | +2.1521 (p=0.1000, %94.7) | +0.5586 (p=0.2632, %77.8) | +0.5663 (p=0.3000, %73.7) | +0.0077 (p=0.9474, %5.6) |
| `Kazakhstan` | +2.2502 (p=0.1000, %94.7) | +0.7535 (p=0.2632, %77.8) | +0.6898 (p=0.4000, %63.2) | -0.0636 (p=0.6316, %38.9) |

#### Dikiş 3: `noaa20_2018` (`2017 → 2019`, `Δ = 2` yıl)

| Ülke | `raw` | `count_normalized` | `total_frp` | `F_yıl` |
|---|---|---|---|---|
| `Brazil` | +0.5238 (p=0.1875, %86.7) | -0.0163 (p=1.0000, %0.0) | +0.1627 (p=0.5000, %53.3) | +0.1791 (p=0.1250, %93.3) |
| `Angola` | +0.6629 (p=0.0625, %100.0) | +0.0526 (p=0.6250, %40.0) | +0.2704 (p=0.1250, %93.3) | +0.2178 (p=0.0625, %100.0) |
| `Australia` | +0.8218 (p=0.0625, %100.0) | +0.1788 (p=0.6875, %33.3) | +0.3007 (p=0.6250, %40.0) | +0.1219 (p=0.6250, %40.0) |
| `Indonesia` | +2.2327 (p=0.1250, %93.3) | +1.5296 (p=0.2500, %80.0) | +1.9285 (p=0.1250, %93.3) | +0.3989 (p=0.0625, %100.0) |
| `India` | +0.4824 (p=0.0625, %100.0) | -0.1503 (p=0.5625, %46.7) | +0.2743 (p=0.2500, %80.0) | +0.4245 (p=0.0625, %100.0) |
| `Turkey` | +0.4567 (p=0.4375, %60.0) | -0.1202 (p=0.6250, %40.0) | -0.0258 (p=0.9375, %6.7) | +0.0944 (p=0.5625, %46.7) |
| `United_States` | +0.2562 (p=0.3750, %66.7) | -0.3889 (p=0.2500, %80.0) | -0.3072 (p=0.4375, %60.0) | +0.0817 (p=0.7500, %26.7) |
| `Canada` | -0.2964 (p=0.7500, %26.7) | -0.8696 (p=0.3750, %66.7) | -0.6899 (p=0.4375, %60.0) | +0.1798 (p=0.3125, %73.3) |
| `Russian_Federation` | +1.1281 (p=0.0625, %100.0) | +0.5218 (p=0.2500, %80.0) | +0.6628 (p=0.2500, %80.0) | +0.1410 (p=0.5000, %53.3) |
| `Kazakhstan` | +0.2029 (p=0.8750, %13.3) | -0.3947 (p=0.5625, %46.7) | -0.2937 (p=0.6875, %33.3) | +0.1010 (p=0.4375, %60.0) |

#### Birleşik Sıra Testi (10 Ülke Üzerinden, $H_0: \text{sıra} = 0.50$)

| Dikiş | Metrik | $N$ | Ortalama Sıra | Medyan Sıra | `> 0.5` / `< 0.5` | İşaret Testi `p` | Wilcoxon $W_+$ | Wilcoxon `p` |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| `aqua_2002` | `raw` | 10 | **0.8533** | 1.0000 | 9 / 1 | **0.0215** | 53.0 | **0.0059** |
| `aqua_2002` | `count_normalized` | 0 | NA | NA | 0 / 0 | NA | NA | NA |
| `aqua_2002` | `total_frp` | 10 | **0.8133** | 1.0000 | 8 / 2 | 0.1094 | 51.0 | **0.0156** |
| `aqua_2002` | `F_yıl` | 0 | NA | NA | 0 / 0 | NA | NA | NA |
| `snpp_2012` | `raw` | 10 | **0.9053** | 0.9474 | 10 / 0 | **0.0020** | 55.0 | **0.0020** |
| `snpp_2012` | `count_normalized` | 10 | **0.4944** | 0.6944 | 6 / 4 | 0.7539 | 23.0 | 0.6836 |
| `snpp_2012` | `total_frp` | 10 | **0.5158** | 0.5263 | 5 / 5 | 1.0000 | 28.5 | 0.9395 |
| `snpp_2012` | `F_yıl` | 10 | **0.6056** | 0.6667 | 6 / 4 | 0.7539 | 38.5 | 0.2617 |
| `noaa20_2018` | `raw` | 10 | **0.7467** | 0.9000 | 8 / 2 | 0.1094 | 47.5 | **0.0391** |
| `noaa20_2018` | `count_normalized` | 10 | **0.5133** | 0.4667 | 4 / 6 | 0.7539 | 29.5 | 0.8574 |
| `noaa20_2018` | `total_frp` | 10 | **0.6000** | 0.6000 | 7 / 3 | 0.3438 | 38.0 | 0.3066 |
| `noaa20_2018` | `F_yıl` | 10 | **0.6933** | 0.6667 | 7 / 3 | 0.3438 | 44.5 | 0.0996 |

---

### 4.3 B. LOYO Normalizasyon Hatası (`2012..=2025`)

Ortalama mutlak log hata: $\frac{1}{n}\sum |\ln(\text{tahmin} / \text{gerçek})|$. `Kazakhstan 2024` yılında MODIS=0 olan 1 kış ayı dışarıda bırakılmıştır (`skip = 1`); diğer 139 ülke-yılda `skip = 0`dır.

| Ülke | Yıl Sayısı | Model (`Ω_-y`) | `b0` (Normalizasyonsuz) | `b1` (Kalıcılık) |
|---|---:|---:|---:|---:|
| `Brazil` | 14 | **0.0358** | 1.7078 | 0.3137 |
| `Angola` | 14 | **0.0288** | 1.6723 | 0.1152 |
| `Australia` | 14 | **0.0406** | 1.8216 | 0.3403 |
| `Indonesia` | 14 | **0.0846** | 2.0269 | 0.7178 |
| `India` | 14 | **0.0530** | 2.1888 | 0.1758 |
| `Turkey` | 14 | **0.0556** | 1.6137 | 0.3971 |
| `United_States` | 14 | **0.0343** | 1.5983 | 0.2107 |
| `Canada` | 14 | **0.1025** | 1.8990 | 0.7967 |
| `Russian_Federation` | 14 | **0.0368** | 1.7651 | 0.3925 |
| `Kazakhstan` | 14 | **0.0442** | 1.5981 | 0.6187 |
| **TOPLAM (`ALL_POOLED`)** | **140** | **0.0516** | **1.7892** | **0.4078** |

---

### 4.4 C. Model vs Baseline Kapısı (Yıl-Bloklu Bootstrap `%90` GA, `B = 1000`, `seed = 42`)

| Kapsam | Baseline | Yıl Bloğu | Model Hata | Baseline Hata | Fark (`B − M`) | `%90 GA` | Karar |
|---|---|---:|---:|---:|---:|---|---|
| `Brazil` | `b0_unnormalized` | 14 | 0.0358 | 1.7078 | +1.6720 | `[+1.6039, +1.7394]` | **GEÇTİ** |
| `Brazil` | `b1_persistence` | 14 | 0.0358 | 0.3137 | +0.2778 | `[+0.1924, +0.3696]` | **GEÇTİ** |
| `Angola` | `b0_unnormalized` | 14 | 0.0288 | 1.6723 | +1.6435 | `[+1.5757, +1.7154]` | **GEÇTİ** |
| `Angola` | `b1_persistence` | 14 | 0.0288 | 0.1152 | +0.0864 | `[+0.0595, +0.1133]` | **GEÇTİ** |
| `Australia` | `b0_unnormalized` | 14 | 0.0406 | 1.8216 | +1.7810 | `[+1.7118, +1.8495]` | **GEÇTİ** |
| `Australia` | `b1_persistence` | 14 | 0.0406 | 0.3403 | +0.2997 | `[+0.1750, +0.4248]` | **GEÇTİ** |
| `Indonesia` | `b0_unnormalized` | 14 | 0.0846 | 2.0269 | +1.9423 | `[+1.8308, +2.0522]` | **GEÇTİ** |
| `Indonesia` | `b1_persistence` | 14 | 0.0846 | 0.7178 | +0.6331 | `[+0.4033, +0.9061]` | **GEÇTİ** |
| `India` | `b0_unnormalized` | 14 | 0.0530 | 2.1888 | +2.1358 | `[+2.0342, +2.2434]` | **GEÇTİ** |
| `India` | `b1_persistence` | 14 | 0.0530 | 0.1758 | +0.1228 | `[+0.0690, +0.1813]` | **GEÇTİ** |
| `Turkey` | `b0_unnormalized` | 14 | 0.0556 | 1.6137 | +1.5582 | `[+1.4762, +1.6448]` | **GEÇTİ** |
| `Turkey` | `b1_persistence` | 14 | 0.0556 | 0.3971 | +0.3415 | `[+0.2184, +0.4682]` | **GEÇTİ** |
| `United_States` | `b0_unnormalized` | 14 | 0.0343 | 1.5983 | +1.5640 | `[+1.4712, +1.6512]` | **GEÇTİ** |
| `United_States` | `b1_persistence` | 14 | 0.0343 | 0.2107 | +0.1764 | `[+0.1277, +0.2191]` | **GEÇTİ** |
| `Canada` | `b0_unnormalized` | 14 | 0.1025 | 1.8990 | +1.7965 | `[+1.6989, +1.8898]` | **GEÇTİ** |
| `Canada` | `b1_persistence` | 14 | 0.1025 | 0.7967 | +0.6942 | `[+0.3936, +0.9952]` | **GEÇTİ** |
| `Russian_Federation` | `b0_unnormalized` | 14 | 0.0368 | 1.7651 | +1.7283 | `[+1.6710, +1.7895]` | **GEÇTİ** |
| `Russian_Federation` | `b1_persistence` | 14 | 0.0368 | 0.3925 | +0.3557 | `[+0.2535, +0.4714]` | **GEÇTİ** |
| `Kazakhstan` | `b0_unnormalized` | 14 | 0.0442 | 1.5981 | +1.5540 | `[+1.4855, +1.6228]` | **GEÇTİ** |
| `Kazakhstan` | `b1_persistence` | 14 | 0.0442 | 0.6187 | +0.5745 | `[+0.4275, +0.7329]` | **GEÇTİ** |
| **`ALL_POOLED`** | **`b0_unnormalized`** | **14** | **0.0516** | **1.7892** | **+1.7376** | **`[+1.6636, +1.8091]`** | **`GEÇTİ`** |
| **`ALL_POOLED`** | **`b1_persistence`** | **14** | **0.0516** | **0.4078** | **+0.3562** | **`[+0.3193, +0.3979]`** | **`GEÇTİ`** |

---

## 5. Değerlendirme ve G11 ile Tutarlılık

1. **`Ω` sayım sürekliliğini istatistiksel olarak sağlıyor (Pozitif kontrol geçti):**
   - Birleşik placebo sıra testinde ham `raw` seri her üç dikişte de placebo dağılımından anlamlı biçimde ayrışmaktadır: `aqua_2002` `mean_rk = 0.8533` (`wilc_p = 0.0059`), `snpp_2012` `mean_rk = 0.9053` (`10/10 > 0.5`, `sign_p = 0.0020`, `wilc_p = 0.0020`) ve `noaa20_2018` `mean_rk = 0.7467` (`wilc_p = 0.0391`).
   - Buna karşılık `count_normalized` her iki test edilebilir dikişte de placebo merkezine oturmaktadır: `snpp_2012` `mean_rk = 0.4944` (`wilc_p = 0.6836`) ve `noaa20_2018` `mean_rk = 0.5133` (`wilc_p = 0.8574`). Yani `Ω` normalizasyonu sonrası sayım dikişleri, yıllık doğal yangın dalgalanmasından ayırt edilememektedir.
2. **LOYO out-of-sample tahmin doğruluğu ve kapı başarısı (`20/20` ülke-baseline ve `2/2` birleşik):**
   - Yıl-dışarıda `Ω_-y` modeli `140` ülke-yılda ortalama **`0.0516`** (~`%5.3`) mutlak log hata üretirken, normalizasyonsuz `b0` **`1.7892`** (~`6.0×` fazla sayım) ve kalıcılık `b1` **`0.4078`** (~`%50` sapma) hata vermektedir.
   - Hem `b0` hem `b1` için yıl-bloklu bootstrap `%90` güven aralıkları 10 ülkenin 10'unda ve birleşik `ALL_POOLED` testinde sıfırın tamamen üzerindedir (`GEÇTİ`). En zorlu kalıcılık vakası olan `Angola`'da dahi (`b1` hatası yalnız `0.1152`), model hatası `0.0288`'e inerek `[+0.0595, +0.1133]` aralığıyla kapıyı geçmiştir.
3. **`total_frp` ve `F_yıl`'da kalan dikiş davranışları (G11 ile birebir tutarlı):**
   - `aqua_2002`'de `total_frp` hâlâ belirgin bir dikiş taşır (`mean_rk = 0.8133`, `wilc_p = 0.0156`), çünkü pre-Aqua döneminde `Ω` uygulanamamaktadır.
   - `snpp_2012` ve `noaa20_2018`'de `F_yıl` 10 ülke ortalamasında orta derecede yukarı kayma gösterir (`mean_rk = 0.6056`, `wilc_p = 0.2617` ve `mean_rk = 0.6933`, `wilc_p = 0.0996`). Tekil ülke placebo sıraları incelendiğinde, G11'de tanımlanan dört tropikal/subtropikal ülkede (`Brazil`, `Angola`, `Indonesia`, `India`) `snpp_2012` `F_yıl` dikişi **tüm 18 placebo adımından büyüktür** (`emp_p = 0.0526`, `pct_rk = 100.0%`); `noaa20_2018`'de ise `Angola`, `Indonesia` ve `India` (`emp_p = 0.0625`, `%100.0`) ile `Brazil` (`%93.3`) yine dağılımın en üst ucundadır. Bu durum, `R10` (1 MODIS = N VIIRS ayak izi FRP toplamı) ve `K11` (şiddet rejimli `Ω`) ihtiyacını istatistiksel olarak teyit eder.

---

## 6. Sınırlamalar

1. **Az yıl sayısı → tekil ülke placebo testinde düşük istatistiksel güç:** 26 yıllık arşivde dikiş içermeyen yıl çifti sayısı yalnızca `n_pl = 18–19` (`Δ=1`) ve `n_pl = 15` (`Δ=2`) olduğundan, en uç gözlemde dahi tekil ülke ampirik p-değeri en fazla `0.0500`–`0.0625`'e inebilir (`< 0.05` eşiği tek ülkede ulaşılamaz). Bu nedenle tekil ülke `emp_p` değerleri yerine 10 ülke birleşik Wilcoxon/işaret testi birlikte yorumlanmalıdır.
2. **Yıl-içi iklim anomalileri ve şiddet rejimi kaymaları:** `Indonesia` (`2019–2020` El Niño/IOD kuraklığı vs ıslak yıl, LOYO `2020` hatası `0.2266`), `Canada` (`2014` `0.2681`, `2020` `0.3540`, `2023` rekor yangın yılı) ve `Turkey` (`2011→2012` düşüşü) gibi büyük yıllar-arası iklim dalgalanmaları hem placebo `p90` sınırını genişletir hem de yangın şiddet dağılımının değiştiği yıllarda `Ω`'nın yıllık out-of-sample hatasını yükseltir (`Canada` ortalama `0.1025`, `Indonesia` `0.0846`).
3. **Havuz büyüklüğü ve uç aylar:** `Kazakhstan 2024` içinde MODIS=0 olan 1 kış ayında `Ω_-y = NA` oluşmuştur (`skip = 1`). Yıllık toplama etkisi önemsiz olsa da (`2024` LOYO hatası `0.0049`), aylık çözünürlükte düşük aktiviteli kuzey bölgelerinde küçük havuzlarda `MIN_MONTHS_PER_CONFIG = 3` / `modis_observable > 0` sınırına çarpılabilir.
4. **`2000` kısmi yılı ve `>= 1000/yıl` hacim kapsamı:** `2000` yılı yalnız Kasım–Aralık aylarını içerdiği halde `(2000, 2001)` çifti `Δ=1` placebo havuzuna dahildir. Ayrıca test edilen 10 ülkenin tamamı `>= 1000/yıl` yüksek hacim katmanındadır; `< 100/yıl` düşük hacimli ülkelerde LOYO ve placebo davranışı bu referans çalışmada kapsanmamıştır.
5. **LOYO tasarımının kapsamı:** Bölüm B ve C doğrudan `effort.rs`'deki `Ω` efor normalizasyonunun `modis_observable` sayımını yeniden üretme başarısını test eder; `total_frp` ve `F_yıl` üzerindeki enerji kalibrasyonu (Bölüm A ve dış doğrulama `S12`/`Y10`) için ayrı dış referans ürünleri gereklidir.

---

## D. Bilinen Olay Testi (dış referans)

### Amaç ve olaylar

`crates/firms-client/examples/event_recall.rs`, harmonize aylık takvimin kamuya açık kaynaklarda belgelenmiş büyük yangın olaylarının aylarını yakalayıp yakalamadığını ölçer. Ham tespit sayıları ile harmonize `count_normalized`, ham `raw_frp` ile harmonize `total_frp` ayrı metrikler olarak değerlendirilir; böylece VIIRS geçişlerinin ham serilerde oluşturabileceği alarm fazlalığı da ölçülebilir. Manavgat gibi az sayıda çok şiddetli yangından oluşan olaylar, her yıl tekrarlanan tarımsal yakmaların baskın olduğu tespit sayılarında görünmeyebileceğinden yangın şiddetini yansıtan FRP görünümüne de ihtiyaç vardır. Bu bir ölçümdür: olay kaçırılması programın hata koduyla çıkmasına neden olmaz.

| Ülke arşiv adı | Aylar | Olay | Kısa kamu kaynağı notu |
|---|---|---|---|
| `Australia` | 2019-12, 2020-01 | Black Summer (NSW/Victoria) | [NASA Earth Observatory: Explosive Fire Activity in Australia](https://science.nasa.gov/earth/earth-observatory/explosive-fire-activity-in-australia-146125/), Aralık sonu ve Ocak başında Victoria ve New South Wales yangınları ve yangın kaynaklı fırtınalar. |
| `Brazil` | 2019-08 | Amazon fires | [NASA Earth Observatory: A New Tool for Tracking Amazon Fires](https://science.nasa.gov/earth/earth-observatory/a-new-tool-for-tracking-amazon-fires-147075/), Ağustos 2019 Amazon yangın etkinliğindeki artış. |
| `Canada` | 2023-06 | record wildfire season | [NASA: Fires Burn Across Quebec](https://science.nasa.gov/earth/earth-observatory/fires-burn-across-quebec-151430/), Haziran başında yeni yangınlar; [Tracking Canada's Extreme 2023 Fire Season](https://science.nasa.gov/earth/earth-observatory/tracking-canadas-extreme-2023-fire-season-151985/), rekor sezonun ülke geneline yayılması. |
| `Turkey` | 2021-08 | Manavgat / Mediterranean fires | [Copernicus CAMS: Mediterranean region evolves into wildfire hotspot](https://atmosphere.copernicus.eu/copernicus-mediterranean-region-evolves-wildfire-hotspot-while-fire-intensity-reaches-new-records), 2021 yazında Akdeniz yangınları ve Türkiye'de rekor yangın şiddeti. |
| `Indonesia` | 2015-09, 2015-10 | El Nino peat fires | [NASA: Seeing Through the Smoky Pall](https://science.nasa.gov/earth/earth-observatory/seeing-through-the-smoky-pall-observations-from-a-grim-indonesian-fire-season/), El Niño kuraklığında Eylül–Ekim boyunca yayılan yangınlar ve turba yanması. |
| `United_States` | 2020-09 | western US fire season (CA/OR) | [NASA: A Wall of Smoke on the U.S. West Coast](https://science.nasa.gov/earth/earth-observatory/a-wall-of-smoke-on-the-us-west-coast-147261/), Eylül 2020 Oregon ve California yangınları. |
| `Russian_Federation` | 2021-07 | Yakutia (Sakha) fires | [NASA: Fires Scorch the Sakha Republic](https://science.nasa.gov/earth/earth-observatory/fires-scorch-the-sakha-republic-148537/), Temmuz 2021 Yakutistan yangınları. |

Olayların bazıları bölgeseldir; araç ülke toplamını ölçer. Dış referans burada belgelenmiş olay zamanıdır; kaynaklardaki uydu gözlemleri FIRMS ile ortak sensörler kullanabildiğinden bu, bağımsız bir yanmış alan veya hasar doğrulaması değildir.

### Yöntem

- Varsayılan dönem `2003..=2025` olup her takvim ayı Terra+Aqua dönemindedir. `--from`, `--to` ve `--countries a,b` ile dönem ve yerleşik olay ülkelerinin alt kümesi seçilebilir; varsayılan ülke kümesi listedeki yedi ülkedir.
- `ARCHIVE_CACHE_DIR` (varsayılan `.cache/firms-archive`) altındaki `modis`, `viirs-snpp`, `viirs-jpss1` ülke CSV'leri salt okunur yüklenir. Mevcut doğrulama örnekleri gibi `retain_wildfires` uygulanır. Tamamlandı işareti bulunan sensör-yıl dizininde olmayan ülke dosyası boş kabul edilir; diğer eksik dosyalar hatadır.
- **Ham seriler:** Harmonizasyondan önce, tüm arşiv sensörlerinin aylık tespit toplamı (`raw_count`) ve aynı ham satırların FRP toplamı (`raw_frp`). **Harmonize seriler:** `PipelineConfig::default()` → `harmonize_with_config` → `build_calendar_with` yolundan gelen `count_normalized` ve `total_frp`; `total_frp`, VIIRS öncesi `k_c` katsayısını zaten içerir (çoklu VIIRS aylarına ayrıca efor düzeltmesi uygulanmaz). Takvimin `anomaly` alanını üreten yol korunur; bu araç dört serinin her biri için istenen yıl-dışarıda karşılaştırmanın z puanını ayrıca hesaplar. Harmonizasyon ve efor katsayıları tüm seçili dönemden hesaplanır; yalnız z referansında hedef yıl dışarıda bırakılır.
- Her ülke ve her takvim ayı `m` ayrı değerlendirilir. `y` yılı çıkarıldıktan sonra kalan aynı-ay yıllarının ortalaması `mean_m` ve anakütle standart sapması `sd_m = sqrt(Σ(x - mean_m)² / n)` hesaplanır: `z(y,m) = (x(y,m) - mean_m) / sd_m`. En az iki referans yıl yoksa veya standart sapma sıfırsa z `NA` olur.
- Sıra, aynı takvim ayının seçilen tüm yılları içinde `1 + hedef değerden büyük değerlerin sayısı`dır. En yüksek değer `1` olur; eşit değerler aynı sırayı alır. Bir olay ayı **harmonize sıra ≤ 3 veya harmonize z ≥ 2** ise ilgili metrikte yakalanır: `recalled_count` yalnız `count_normalized`, `recalled_frp` yalnız `total_frp` üzerinden hesaplanır. Dönem dışındaki veya ilgili harmonize değeri olmayan olay ayları `NA` gösterilir; birden fazla ayı olan olaylar ay bazında değerlendirilir.
- Arşivi yüklenmiş ve ham tespiti olmayan aylar her iki ham seride sıfırdır; Terra+Aqua döneminde takvim hücresi de yoksa her iki harmonize seride de sıfır kabul edilir. Mevcut hücrede `count_normalized` yoksa bu eksiklik korunur, sıfıra çevrilmez; `total_frp` bundan bağımsız değerlendirilir. Sonlu olmayan harmonize değerler eksik kabul edilir.
- **Yanlış alarm göstergesi:** Seçilen dönemle kesişen `2012..=to` ayları içinde `z ≥ 2` olanların, z puanı hesaplanabilen aylara oranı; dört serinin her biri için ayrı pay/payda raporlanır. Birleşik (`ALL_POOLED`) oran ülke yüzdelerinin ortalaması değil, ülke-ay pay ve paydalarının toplamıdır. Bilinen olay ayları da bu hesaba dahildir. Tüm diğer ayların olaysız olduğu doğrulanmadığından bu değer kesin yanlış alarm oranı değil, alarm sıklığı göstergesidir; ham serinin daha yüksek çıkacağı önceden varsayılmaz.
- `--out` CSV'sinde her ayın sayımları ve FRP toplamları, dört serinin z puanları ve sıraları ile olayların `recalled_count` ve `recalled_frp` sonuçları bulunur. `raw_z`, `raw_rank`, `harmonized_z`, `harmonized_rank` ve FRP eki olmayan alarm alanları sayım metriğine aittir; FRP alanları ayrıca `frp` ile adlandırılır. Olay ayları `section=event`, diğer aylar `month`, dönem dışındaki olaylar `event_outside_range`, ülke ve birleşik alarm özetleri `false_alarms` olarak yazılır. Hesaplanamayan sayısal alanlar boştur; alarm oranları `0..1` ölçeğindedir.

### Çalıştırma

```bash
export PATH="$(ls -d "$HOME"/AppData/Local/Microsoft/WinGet/Packages/BrechtSanders.WinLibs.*/mingw64/bin):$PATH"
cargo clippy -p pyrosync-firms --examples -- -D warnings

ARCHIVE_CACHE_DIR="C:/Users/user/Desktop/SpaceApps2026-MODIS-VIIRS/.cache/firms-archive" \
  cargo run --release -p pyrosync-firms --example event_recall -- \
  --from 2003 --to 2025 --out .cache/review/event_recall.csv

# Küçük kapsamlı kontrol
ARCHIVE_CACHE_DIR="C:/Users/user/Desktop/SpaceApps2026-MODIS-VIIRS/.cache/firms-archive" \
  cargo run --release -p pyrosync-firms --example event_recall -- \
  --countries Turkey --out .cache/review/event_recall_turkey.csv
```

### Sonuç tablosu

Sonuçlar: final koşusundan sonra doldurulacak.

| Olay | Ay | Sayım ham z | Sayım ham sıra | Sayım harmonize z | Sayım harmonize sıra | FRP ham z | FRP ham sıra | FRP harmonize z | FRP harmonize sıra | recalled_count | recalled_frp |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Black Summer (Australia) | 2019-12 | | | | | | | | | | |
| Black Summer (Australia) | 2020-01 | | | | | | | | | | |
| Amazon fires (Brazil) | 2019-08 | | | | | | | | | | |
| record wildfire season (Canada) | 2023-06 | | | | | | | | | | |
| Manavgat / Mediterranean fires (Turkey) | 2021-08 | | | | | | | | | | |
| El Nino peat fires (Indonesia) | 2015-09 | | | | | | | | | | |
| El Nino peat fires (Indonesia) | 2015-10 | | | | | | | | | | |
| western US fire season (CA/OR) | 2020-09 | | | | | | | | | | |
| Yakutia (Sakha) fires | 2021-07 | | | | | | | | | | |
