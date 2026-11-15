# Tam Filo 2000–2025 Trend Serisi (K7)

Ölçüm tarihi: 2026-11-15. Veri: yerel FIRMS ülke arşivi (`make data`): MODIS (Terra + Aqua) 2000–2025, VIIRS S-NPP 2012–2025, VIIRS NOAA-20 2018–2025. NOAA-21 ülke arşivinde yoktur (`viirs-jpss2`, `viirs-noaa21`, `viirs-n21`, `viirs-jpss-2`, `viirs-noaa-21` slug'larının tümü 2023 ve 2024 için HTTP 404 verir; `viirs-jpss1` 200 verir). NOAA-21 yalnız 10 ülkelik sette, FIRMS area API'sinden (`VIIRS_NOAA21_SP`, 2023-03-01..2025-12-31) alındı. 5 büyük ülke yalnız arşivle ölçüldü.

## 1. Amaç

G3'ün trend doğrulaması yalnızca MODIS + S-NPP serisinde yapılmıştı. K7, NOAA-20 ve NOAA-21'i de içeren tam filo serisinde iki şartı sınar:

1. **Trend:** 2001–2025 yıllık sayımında normalize Spearman ρ'nun mutlak değeri, ham ρ'nun yarısından küçük.
2. **Seviye kayması:** Her filo geçişinde (2012 S-NPP, 2018 NOAA-20, 2023 NOAA-21) `|log(normalize kayma)| < |log(ham kayma)| / 2`.

## 2. Yöntem

Araç: `crates/firms-client/examples/fleet_trend.rs`.

```bash
# 10 ülke, tam filo (NOAA-21 area API ile; FIRMS_API_KEY ortam değişkeni gerekir)
ARCHIVE_CACHE_DIR=.cache/firms-archive \
  cargo run --release -p pyrosync-firms --example fleet_trend -- \
  Turkey Greece Bulgaria Cyprus Georgia Armenia Azerbaijan Iran Iraq Syria \
  --noaa21 --noaa21-union --yearly-omega --out fleet10.csv

# 5 büyük ülke, yalnız arşiv
ARCHIVE_CACHE_DIR=.cache/firms-archive \
  cargo run --release -p pyrosync-firms --example fleet_trend -- \
  Kazakhstan Indonesia India United_States Canada --yearly-omega --out big5.csv
```

Bayraklar: `--noaa21` NOAA-21'i area API'den ekler, `--noaa21-union` tek bir birleşik sınır kutusu için tek istek dizisi yapar, `--regions-geo` çokgen dosyasını seçer (varsayılan `web/public/geo/regions.json`), `--yearly-omega` yıl bazında Ω tanısını yazar.

- Her ülke-yıl için sensörler yüklenir, yangın tipi filtresi uygulanır, varsayılan `PipelineConfig` ile yıl bazında harmonize edilir; takvim ve Ω tüm seri üzerinden tek seferde kurulur (sunucuyla aynı).
- **Ham sayım:** yıllık harmonize kayıt sayısı (`CalendarCell::count`), G3 ile aynı tanım.
- **Normalize sayım:** yıllık `count_normalized` toplamı. Bir ayı normalize değeri olmayan yıl "eksik" sayılır ve normalize istatistiklerden çıkarılır; 2001 ve 2002'nin ilk yarısı Aqua öncesi kuralı (K8) nedeniyle her ülkede eksiktir.
- **Trend:** 2001–2025 tam yıllar, Spearman ρ.
- **2012 (S-NPP):** 2007–2011 ortalaması karşı 2013–2017 ortalaması (2012 kısmi yıl, dışarıda).
- **2018 (NOAA-20, 2018-04'ten itibaren):** iki ölçüm: aynı mevsim (Mayıs–Aralık 2017 karşı Mayıs–Aralık 2018) ve 3 yıllık blok (2015–2017 karşı 2019–2021).
- **2023 (NOAA-21, 2023-03'ten itibaren):** iki ölçüm: aynı mevsim (Mayıs–Aralık 2022 karşı Mayıs–Aralık 2023) ve blok (2020–2022 karşı 2024–2025; 2023 dışarıda).
- Ham kayma ±%10 içindeyse (`|log| < 0,0953`) ölçüt anlamsız sayılır ve "sınanamaz" yazılır.
- **Havuz:** Ülkeler önce kendi içinde harmonize edilir, sonra kayıtları birleştirilip Ω tek havuzdan hesaplanır. Komşu iki ülkenin sınırındaki ortak tespitler birleştirilmez.

### NOAA-21 kaynağı

NOAA-21 area API'sinden `VIIRS_NOAA21_SP` ile, 2023-03-01..2025-12-31 aralığında, 10 ülkenin birleşik sınır kutusu `[19,65; 25,10; 63,30; 44,24]` için tek seferde çekilir (1.660.614 satır). Satırlar ülke başına Natural Earth 1:50m çokgenleriyle (`web/public/geo/regions.json`) kırpılır. Pencereler `.cache/firms-area` altında kalıcı olarak saklanır (K5 `AreaCache`); Docker bu dizini bind-mount eder (`AREA_CACHE_HOST_DIR`), bu yüzden yeniden koşular 0 transaction harcar.

Maliyet: FIRMS, 10 ülkelik birleşik kutu için 5 günlük istek başına yaklaşık 37 transaction saydı. İlk koşu 135 pencereden sonra `Exceeding allowed transaction limit` hatası verdi; önbelleğe alınan pencereler yeniden kullanıldı ve ikinci koşu 370 transaction daha harcadı. Bu maliyet yüzünden 5 büyük ülke 2023 için ölçülmedi. FIRMS ülke API uç noktası artık yok (`Invalid API call`); area API tek kaynaktır.

### MODIS referansı ile düzeltilmiş ölçüt (karar ölçütü)

Tek sezonluk kıyas, gerçek yıllar arası yangın aktivitesini cihaz basamağıyla karıştırır. Bu yüzden her kontrol için aynı pencerelerde MODIS referansı kurulur: aylık `modis_observable` sayımı (platformları arasında Terra veya Aqua olan kayıtlar). MODIS filosu bu geçişlerde değişmediği için referansın kayması gerçek aktivite değişimini ölçer.

- `ref_shift` = referans sonrası / referans öncesi.
- `raw_adj = raw_shift / ref_shift`, `norm_adj = norm_shift / ref_shift`.
- **Geçti:** `|ln norm_adj| < |ln raw_adj| / 2`. Ham kayma küçüklüğü koruması aynıdır: `|ln raw_adj| < 0,0953` ise "sınanamaz".

Kullanıcı kararı (2026-11-15): düzeltilmiş ölçüt karar ölçütüdür; düzeltilmemiş ölçüt de raporlanır.

### K7 ölçütü (MET / NOT MET)

Bir bölge için K7 ölçütü karşılanır (MET), ancak ve ancak: trend geçti; 2012 düzeltilmiş ölçüt geçti; 2018 3 yıllık blok düzeltilmiş ölçüt geçti ve 2018 aynı mevsim düzeltilmiş ölçüt geçti ya da sınanamaz; 2023 blok düzeltilmiş ölçüt geçti ve 2023 aynı mevsim düzeltilmiş ölçüt geçti ya da sınanamaz. Eksik veri ve ölçülmemiş geçiş hiçbir zaman geçmiş sayılmaz.

## 3. Sonuçlar — Havuzlanmış Seriler

Sütunlar: `ham` ve `norm` istatistikleri (trend satırında Spearman ρ, diğerlerinde araç çıktısındaki istatistik), `ref_shift`, MODIS referansı ile düzeltilmiş `ham_dz` ve `norm_dz`, düzeltilmemiş sonuç, karar sonucu (düzeltilmiş).

### 10 ülke (Turkey, Greece, Bulgaria, Cyprus, Georgia, Armenia, Azerbaijan, Iran, Iraq, Syria), MODIS + S-NPP + NOAA-20 + NOAA-21

| Ölçüt | Ham | Norm | ref_shift | ham_dz | norm_dz | Düzeltilmemiş | Karar |
|---|---:|---:|---:|---:|---:|---|---|
| Trend ρ (2001–2025) | +0,9377 | −0,0188 | — | — | — | Geçti | **Geçti** |
| 2012 S-NPP | +1,3821 | −0,0270 | 0,976 | +1,4061 | −0,0029 | Geçti | **Geçti** |
| 2018 aynı mevsim | +0,2751 | −0,2539 | 0,776 | +0,5282 | −0,0008 | Kaldı | **Geçti** |
| 2018 3 yıllık blok | +0,3681 | −0,1610 | 0,858 | +0,5214 | −0,0076 | Geçti | **Geçti** |
| 2023 aynı mevsim | +0,7681 | +0,5951 | 1,826 | +0,1660 | −0,0070 | Kaldı | **Geçti** |
| 2023 blok | +0,4941 | +0,3210 | 1,447 | +0,1245 | −0,0485 | Kaldı | **Geçti** |

Havuz K7 ölçütü: **MET**.

Ω (10 ülke havuzu, `estimate_effort`):

| Platform kümesi | Ay | Toplam kayıt | MODIS-gözlenebilir | Ω |
|---|---:|---:|---:|---:|
| MODIS (Terra + Aqua) | 114 | 235.776 | 235.776 | 1,0000 |
| MODIS + S-NPP | 75 | 624.640 | 152.646 | 4,0921 |
| MODIS + S-NPP + NOAA-20 | 59 | 741.713 | 106.792 | 6,9454 |
| MODIS + S-NPP + NOAA-20 + NOAA-21 | 34 | 611.192 | 74.019 | 8,2572 |

NOAA-21 eklenince 2023 sonrası aylar dördüncü kümeye geçer; üçüncü küme 93 aydan 59 aya iner. NOAA-21'siz önceki ölçümde bu kümenin Ω'sı 6,761 idi.

### 5 büyük ülke (Kazakhstan, Indonesia, India, United_States, Canada), yalnız arşiv

| Ölçüt | Ham | Norm | ref_shift | ham_dz | norm_dz | Düzeltilmemiş | Karar |
|---|---:|---:|---:|---:|---:|---|---|
| Trend ρ (2001–2025) | +0,8623 | −0,0227 | — | — | — | Geçti | **Geçti** |
| 2012 S-NPP | +1,5996 | +0,0917 | 1,085 | +1,5177 | +0,0098 | Geçti | **Geçti** |
| 2018 aynı mevsim | +0,2876 | −0,1376 | 0,786 | +0,5279 | +0,1027 | Geçti | **Geçti** |
| 2018 3 yıllık blok | +0,2969 | −0,1283 | 0,906 | +0,3952 | −0,0300 | Geçti | **Geçti** |
| 2023 NOAA-21 | — | — | — | — | — | — | **Ölçülmedi** |

Havuz K7 ölçütü: **NOT MET** (yalnız 2023 ölçülmediği için).

Ω (5 ülke havuzu): MODIS `1,0000` (114 ay), MODIS + S-NPP `4,5173` (75 ay), MODIS + S-NPP + NOAA-20 `6,9110` (93 ay).

## 4. Sonuçlar — Ülke Bazında

Hücreler karar (düzeltilmiş) sonucunu gösterir. "Eksik": normalize değer üretilemedi. "Sınanamaz": ham kayma çok küçük. Trend sütununda ρ ham → norm verilir.

| Ülke | Trend (ρ ham → norm) | 2012 | 2018 aynı mevsim | 2018 3 yıl | 2023 aynı mevsim | 2023 blok | K7 |
|---|---|---|---|---|---|---|---|
| Turkey | Geçti (+0,9100 → −0,0415) | Geçti | Geçti | Geçti | Geçti | Geçti | MET |
| Greece | Kaldı (+0,8215 → −0,4496) | Geçti | Geçti | Geçti | Sınanamaz | Kaldı | NOT MET |
| Bulgaria | Kaldı (+0,5008 → −0,6404) | Geçti | Geçti | Kaldı | Kaldı | Kaldı | NOT MET |
| Cyprus | Geçti (+0,8482 → −0,0303) | Eksik | Eksik | Sınanamaz | Sınanamaz | Eksik | NOT MET |
| Georgia | Geçti (+0,8608 → +0,0045) | Geçti | Eksik | Geçti | Eksik | Kaldı | NOT MET |
| Armenia | Geçti (+0,8800 → +0,2721) | Geçti | Eksik | Geçti | Kaldı | Kaldı | NOT MET |
| Azerbaijan | Geçti (+0,7700 → −0,3662) | Geçti | Geçti | Geçti | Geçti | Geçti | MET |
| Iran | Geçti (+0,9377 → +0,0741) | Geçti | Geçti | Geçti | Sınanamaz | Sınanamaz | NOT MET |
| Iraq | Kaldı (+0,9108 → +0,8142) | Geçti | Geçti | Geçti | Sınanamaz | Sınanamaz | NOT MET |
| Syria | Geçti (+0,7254 → +0,1914) | Geçti | Geçti | Geçti | Kaldı | Kaldı | NOT MET |
| Kazakhstan | Kaldı (+0,2869 → −0,7685) | Geçti | Geçti | Geçti | Ölçülmedi | Ölçülmedi | NOT MET |
| Indonesia | Kaldı (+0,5385 → −0,5850) | Geçti | Geçti | Geçti | Ölçülmedi | Ölçülmedi | NOT MET |
| India | Kaldı (+0,9669 → +0,5771) | Geçti | Geçti | Geçti | Ölçülmedi | Ölçülmedi | NOT MET |
| United_States | Geçti (+0,8977 → −0,1393) | Geçti | Geçti | Geçti | Ölçülmedi | Ölçülmedi | NOT MET |
| Canada | Geçti (+0,8369 → +0,4170) | Geçti | Geçti | Geçti | Ölçülmedi | Ölçülmedi | NOT MET |

10 ülkeden 2'si (Turkey, Azerbaijan) K7 ölçütünü tam karşılıyor. Ülke bazında trend 15 ülkenin 9'unda geçiyor.

## 5. Değerlendirme

- **Yörünge sürüklenmesi hipotezi çürütüldü.** Önceki taslakta "MODIS yörünge sürüklenmesi havuzlanmış Ω'yı şişiriyor ve 2018'i aşırı düzeltiyor" hipotezi vardı. `--yearly-omega` tanısı bunu desteklemiyor: MODIS + S-NPP + NOAA-20 yapılandırmasında yıl bazında `toplam / modis_observable` oranı 2022–2025'te, 2018–2020'ye göre 10 ülke havuzunda `x0,973` (NOAA-21'siz koşu), NOAA-21 ile `x0,992`. Oran 1'in belirgin üstüne çıkmıyor; sürüklenme kaynaklı bir Ω şişmesi görülmüyor. 5 ülkelik havuzda aynı oran `x1,062`.
- **2018 aynı mevsim düşüşü gerçek aktivitedir.** Havuzda normalize kayma `x0,776`, MODIS referansı da `x0,776` (`ref_shift` 0,776). Düzeltilmemiş ölçüt (0,2751 → 0,2539) bu yüzden kalıyor; düzeltilmiş ölçüt geçiyor (0,5282 → 0,0008). Yani normalizasyon cihaz basamağını kaldırmış, kalan düşüş MODIS'in kendi gördüğü düşüştür.
- **2023'te tersi olur.** Referans yükselir: aynı mevsim `ref_shift` 1,826, blok 1,447. 2023 bölgede aktif bir yıldı. Düzeltilmemiş ölçüt iki kontrolde de kalıyor (aynı mevsim 0,7681 → 0,5951; blok 0,4941 → 0,3210), düzeltilmiş ölçüt geçiyor (0,1660 → 0,0070; 0,1245 → 0,0485).
- **Ülke düzeyi zayıf.** Havuz MET, ama ülke bazında 10'da 2 MET. Trend: Greece, Bulgaria, Iraq kalıyor (normalize ρ Greece ve Bulgaria'da belirgin negatif, Iraq'ta +0,8142; Iraq'ta tarımsal yanma gerçekten artmış olabilir, ölçüt bunu artefaktan ayıramaz). 2023: Greece blok, Bulgaria her iki kontrol, Georgia blok, Armenia her iki kontrol, Syria her iki kontrol kalıyor; Iran ve Iraq'ta düzeltilmiş ham kayma çok küçük olduğu için 2023 blok sınanamıyor ve K7 kuralı bunu geçmiş saymıyor. Bulgaria'nın 2018 3 yıllık bloğu da kalıyor (norm_dz +0,6457).
- **Küçük ülkelerde boş aylar.** Cyprus, Georgia, Armenia'da MODIS'in hiç tespit vermediği aylar `(MODIS yok, VIIRS kümesi)` havuzuna düşer; `modis_observable = 0` olduğu için Ω hesaplanamaz ve ilgili yıl ya da pencere "Eksik" kalır. Bu pipeline'ın bilinen sınırıdır.
- **Sınır kırpma uyarısı.** NOAA-21 satırları Natural Earth 1:50m çokgenleriyle kırpılıyor; arşivdeki MODIS, S-NPP ve NOAA-20 satırları ise FIRMS'in kendi ülke atamasıyla geliyor. Sınır bölgelerinde iki yöntem farklı sonuç verebilir. Birleşik kutuda çekilen 1.660.614 satırdan ülkelere 1.171.116 satır atandı (Turkey 119.386, Greece 18.406, Bulgaria 10.764, Cyprus 1.369, Georgia 6.409, Armenia 4.098, Azerbaijan 9.738, Iran 577.277, Iraq 387.036, Syria 36.633); kalan 489.498 satır başka ülkelere ya da denize düştü. Küçük ülkelerde kıyı ve sınır payı yüksek olduğundan, bu fark küçük ülkelerin 2023 sonuçlarını en çok etkileyebilecek kaynaktır.
- **5 büyük ülke 2023 için ölçülmedi.** NOAA-21 yalnız area API'den geliyor ve 10 ülkelik birleşik kutu bile transaction limitine çarptı (bölüm 2). 5 büyük ülke için aynı maliyet karşılanmadı; bu yüzden 5 ülkelik set 2018'e kadar ölçüldü ve K7 ölçütü orada NOT MET.

## 6. K7 Bitiş Şartı

| Şart | Durum |
|---|---|
| Normalize \|ρ\| < ham \|ρ\| / 2 | 2000–2025 tam filo havuzlanmış seride sağlanıyor (10 ülke −0,0188; 5 ülke −0,0227); ülke bazında 15'in 9'unda |
| 2012 seviye kayması | Karar ölçütüyle her iki havuzda sağlanıyor |
| 2018 seviye kayması | Karar ölçütüyle her iki havuzda iki ölçümde de sağlanıyor |
| 2023 seviye kayması | 10 ülke havuzunda karar ölçütüyle iki ölçümde de sağlanıyor |

G3 bölgesinin 2000–2025 tam filo havuzlanmış serisinde, karar ölçütüyle (MODIS referansı ile düzeltilmiş), tüm koşullar sağlanıyor.

Açık kalan sınırlamalar:

- Ülke düzeyinde yalnız 2 ülke (Turkey, Azerbaijan) K7 ölçütünü tam karşılıyor.
- 5 büyük ülke için 2023 geçişi ölçülmedi (area API maliyeti).
- Düzeltilmemiş ölçüt 2018 aynı mevsim ve 2023'ün iki kontrolünde havuzda kalıyor; bu, gerçek yıllar arası aktivite farkından kaynaklanıyor (bölüm 5).
