# Tam Filo 2000–2025 Trend Serisi (K7)

Ölçüm tarihi: 2026-11-15. Veri: yerel FIRMS ülke arşivi (`make data`): MODIS (Terra + Aqua) 2000–2025, VIIRS S-NPP 2012–2025, VIIRS NOAA-20 2018–2025. NOAA-21 ülke arşivinde bulunmadığı için (`viirs-jpss2` dosyaları HTTP 404) 2023 geçişi ölçülemedi.

## 1. Amaç

G3'ün trend doğrulaması yalnızca MODIS + S-NPP serisinde yapılmıştı. K7, NOAA-20'yi de içeren tam filo serisinde iki şartı sınar:

1. **Trend:** 2001–2025 yıllık sayımında normalize Spearman ρ'nun mutlak değeri, ham ρ'nun yarısından küçük.
2. **Seviye kayması:** Her filo geçişinde `|log(normalize kayma)| < |log(ham kayma)| / 2`.

## 2. Yöntem

Araç: `crates/firms-client/examples/fleet_trend.rs`.

```bash
ARCHIVE_CACHE_DIR=.cache/firms-archive \
  cargo run --release -p pyrosync-firms --example fleet_trend -- \
  Turkey Greece Bulgaria Cyprus Georgia Armenia Azerbaijan Iran Iraq Syria --out fleet10.csv
```

- Her ülke-yıl için üç sensör yüklenir, yangın tipi filtresi uygulanır, varsayılan `PipelineConfig` ile yıl bazında harmonize edilir; takvim ve Ω tüm seri üzerinden tek seferde kurulur (sunucuyla aynı).
- **Ham sayım:** yıllık harmonize kayıt sayısı (`CalendarCell::count`), G3 ile aynı tanım.
- **Normalize sayım:** yıllık `count_normalized` toplamı. Bir ayı normalize değeri olmayan yıl "eksik" sayılır ve normalize istatistiklerden çıkarılır; 2001 ve 2002'nin ilk yarısı Aqua öncesi kuralı (K8) nedeniyle her ülkede eksiktir.
- **Trend:** 2001–2025 tam yıllar, Spearman ρ.
- **2012 (S-NPP):** 2007–2011 ortalaması karşı 2013–2017 ortalaması (2012 kısmi yıl, dışarıda).
- **2018 (NOAA-20, 2018-04'ten itibaren):** iki ölçüm: aynı mevsim (Mayıs–Aralık 2017 karşı Mayıs–Aralık 2018) ve 3 yıllık blok (2015–2017 karşı 2019–2021).
- Ham kayma ±%10 içindeyse (`|log| < 0,0953`) ölçüt anlamsız sayılır ve "sınanamaz" yazılır.
- **Havuz:** Ülkeler önce kendi içinde harmonize edilir, sonra kayıtları birleştirilip Ω tek havuzdan hesaplanır. Komşu iki ülkenin sınırındaki ortak tespitler birleştirilmez.

İki ülke seti koşuldu: G3 bölgesini kapsayan 10 ülke (yaklaşık 56 sn) ve farklı biyomlardan 5 büyük ülke (Kazakhstan, Indonesia, India, United_States, Canada; yaklaşık 13 dk).

## 3. Sonuçlar — Havuzlanmış Seriler

| Set | Ölçüt | Ham | Normalize | Sonuç |
|---|---|---:|---:|---|
| 10 ülke | Trend ρ (2001–2025) | +0,928 | −0,020 | **Geçti** |
| 10 ülke | 2012 S-NPP `log` kayma | +1,382 (×3,98) | −0,027 | **Geçti** |
| 10 ülke | 2018 aynı mevsim | +0,275 | −0,227 | **Kaldı** |
| 10 ülke | 2018 3 yıllık blok | +0,368 | −0,134 | **Geçti** |
| 5 ülke | Trend ρ (2001–2025) | +0,862 | −0,023 | **Geçti** |
| 5 ülke | 2012 S-NPP `log` kayma | +1,600 | +0,092 | **Geçti** |
| 5 ülke | 2018 aynı mevsim | +0,288 | −0,138 | **Geçti** |
| 5 ülke | 2018 3 yıllık blok | +0,297 | −0,128 | **Geçti** |
| her ikisi | 2023 NOAA-21 | — | — | **Ölçülemedi** |

10 ülkelik havuzda Ω: MODIS-only `1,000` (114 ay), MODIS + S-NPP `4,092` (75 ay), MODIS + S-NPP + NOAA-20 `6,761` (93 ay).

## 4. Sonuçlar — Ülke Bazında

| Ülke | Trend | 2012 | 2018 aynı mevsim | 2018 3 yıl |
|---|---|---|---|---|
| Turkey | Geçti (+0,886 → −0,009) | Geçti | Kaldı (0,275 → 0,189) | Sınanamaz |
| Greece | Kaldı (+0,779 → −0,484) | Kaldı | Sınanamaz | Geçti |
| Bulgaria | Kaldı (+0,385 → −0,640) | Geçti | Kaldı | Kaldı |
| Cyprus | Geçti | Veri eksik | Veri eksik | Veri eksik |
| Georgia | Geçti | Geçti | Veri eksik | Kaldı |
| Armenia | Geçti | Geçti | Veri eksik | Kaldı |
| Azerbaijan | Geçti | Geçti | Geçti | Kaldı |
| Iran | Geçti | Geçti | Kaldı | Geçti |
| Iraq | Kaldı (+0,911 → +0,815) | Geçti | Geçti | Geçti |
| Syria | Geçti | Geçti | Kaldı | Kaldı |
| Kazakhstan | Kaldı (+0,287 → −0,769) | Geçti | Kaldı | Kaldı |
| Indonesia | Kaldı (+0,539 → −0,585) | Geçti | Kaldı | Kaldı |
| India | Kaldı (+0,967 → +0,577) | Geçti | Geçti | Geçti |
| United_States | Geçti | Geçti | Kaldı | Geçti |
| Canada | Geçti | Geçti | Sınanamaz | Sınanamaz |

## 5. Değerlendirme

- **2012 S-NPP geçişi** sağlam: 15 ülkenin 13'ünde ve iki havuzda da ölçüt sağlanıyor (Greece kaldı, Cyprus'ta veri eksik).
- **Trend** havuzlanmış serilerde neredeyse tamamen kalkıyor (ρ ≈ 0). Ülke bazında 6 ülkede kalıyor; bunların çoğunda (Greece, Bulgaria, Kazakhstan, Indonesia) normalize ρ belirgin **negatif**, yani düzeltme yalnız trendi kaldırmıyor, ters yönde bir eğim üretiyor. Iraq ve India'da ise pozitif trend kısmen kalıyor; bu ülkelerde tarımsal yanma gerçekten artmış olabilir, ölçüt bunu artefakttan ayıramaz.
- **2018 NOAA-20 geçişi** karışık. Normalize kayma çoğu ülkede negatif (×0,64–0,93), yani **aşırı düzeltme** eğilimi var. Olası açıklama (ölçülmedi, hipotez): MODIS + S-NPP + NOAA-20 havuzu 2018–2025'i tek havuzda topluyor ve bu dönem Terra/Aqua yörünge sürüklenmesi (2021+, bkz. K12 `modis_orbital_drift`) ile MODIS tespit veriminin düştüğü yılları içeriyor. MODIS-gözlenebilir sayım azalınca Ω büyüyor ve 2018 sonrası sayımlar fazla bölünüyor.
- **Küçük ülkelerde boş aylar:** Cyprus, Georgia, Armenia gibi düşük aktiviteli ülkelerde MODIS'in hiç tespit vermediği aylar `(MODIS yok, VIIRS kümesi)` havuzuna düşüyor; bu havuzda `modis_observable = 0` olduğu için Ω hesaplanamıyor ve yıl eksik sayılıyor. Bu, pipeline'ın bilinen bir sınırı; aylık seride boşluk olarak görünür.
- **2023 NOAA-21** ülke arşivinden ölçülemez; Area API veya NASA'nın arşive NOAA-21 eklemesi gerekir.

## 6. K7 Bitiş Şartı

| Şart | Durum |
|---|---|
| Normalize \|ρ\| < ham \|ρ\| / 2 | Havuzlanmış iki sette sağlanıyor; ülke bazında 15'in 9'unda |
| 2012 seviye kayması | Sağlanıyor |
| 2018 seviye kayması | 5 ülkelik havuzda iki ölçümde de, 10 ülkelik havuzda yalnız 3 yıllık blokta sağlanıyor |
| 2023 seviye kayması | Ölçülemedi (NOAA-21 arşivde yok) |

Şartın "üç geçişte de" kısmı karşılanmadığı için K7 açık kalır.
