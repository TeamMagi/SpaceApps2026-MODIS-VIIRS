# S-NPP ↔ NOAA-20 Köprü Katsayıları ve Terra Kısa-Aralık VZA Kontrolü (K13)

Ölçüm tarihi: 2026-11-15. Veri: yerel FIRMS ülke arşivi (`make data`), `viirs-snpp` ve `viirs-jpss1` (NOAA-20), 2018–2025. NOAA-21 ülke arşivinde bulunmadığı için ölçülmedi.

## 1. Amaç

- S-NPP ile NOAA-20 arasında tespit sayısı ve FRP köprü katsayılarını kendi arşivimizden ölçmek.
- Aqua (`MYD14`) üzerinden türetilen `R_rel(θ)` VZA eğrisinin sabah geçişli Terra'ya (`MOD14`) aktarılabilirliğini, `|Δt| ≤ 20 dk` Terra–VIIRS çiftleriyle dolaylı olarak sınamak (Zheng et al. 2026, Bölüm 5 uyarısı).

## 2. Yöntem

Araç: `crates/firms-client/examples/viirs_bridge.rs`.

```bash
ARCHIVE_CACHE_DIR=.cache/firms-archive \
  cargo run --release -p pyrosync-firms --example viirs_bridge -- \
  Turkey Greece Brazil Australia Angola Indonesia --from 2018 --to 2025 --out bridge.csv
```

- **Köprü (Bölüm 3):** Her iki sensörün de veri taşıdığı aylar (2018-04 ve sonrası, toplam 93 ay) alınır. Her sensöre ayrı ayrı yangın tipi filtresi ve interscan duplikat temizliği uygulanır (pipeline ile aynı). Oranlar `NOAA-20 / S-NPP` yönündedir.
- **Kısa-aralık kontrolü (Bölüm 4):** MODIS (Terra ve Aqua ayrı) ile VIIRS (S-NPP veya NOAA-20) tespitleri arasında `≤ 2 km` ve `|Δt| ≤ 20 dk` çiftleri sayılır; dağılımın nerede durduğunu göstermek için `≤ 180 dk` içindeki tüm eş-konumlu çiftlerin en küçük ve medyan `|Δt|` değeri de verilir. Çiftler bire-bir eşleştirilmez; bir MODIS pikseli birden çok VIIRS pikseliyle çift oluşturabilir.

**Kapsam sınırı:** On ülke hedeflenmişti; çift bulma adımı büyük ülkelerde uzun sürdüğü için (Avustralya tek başına ~26 dk) koşu altı ülkede kesildi. India, United_States, Canada ve Kazakhstan ölçülmedi.

## 3. S-NPP ↔ NOAA-20 Köprü Katsayıları (2018-04 … 2025-12, 93 ay)

| Ülke | Dilim | n S-NPP | n NOAA-20 | Sayım köprüsü | FRP toplam köprüsü | Tespit başına FRP oranı |
|---|---|---:|---:|---:|---:|---:|
| Turkey | tümü | 192.662 | 198.467 | 1,0301 | 1,0171 | 0,9873 |
| Turkey | gündüz | 138.670 | 142.896 | 1,0305 | 1,0088 | 0,9789 |
| Turkey | gece | 53.992 | 55.571 | 1,0292 | 1,0787 | 1,0480 |
| Greece | tümü | 35.202 | 35.735 | 1,0151 | 1,0912 | 1,0749 |
| Greece | gündüz | 21.737 | 22.946 | 1,0556 | 1,0728 | 1,0162 |
| Greece | gece | 13.465 | 12.789 | 0,9498 | 1,1401 | 1,2004 |
| Brazil | tümü | 9.154.487 | 9.186.198 | 1,0035 | 1,0127 | 1,0092 |
| Brazil | gündüz | 6.247.180 | 6.211.748 | 0,9943 | 1,0104 | 1,0162 |
| Brazil | gece | 2.907.307 | 2.974.450 | 1,0231 | 1,0391 | 1,0156 |
| Australia | tümü | 7.816.017 | 7.808.745 | 0,9991 | 1,0077 | 1,0087 |
| Australia | gündüz | 4.976.355 | 4.945.814 | 0,9939 | 1,0078 | 1,0140 |
| Australia | gece | 2.839.662 | 2.862.931 | 1,0082 | 1,0072 | 0,9990 |
| Angola | tümü | 8.579.109 | 8.792.777 | 1,0249 | 1,0401 | 1,0148 |
| Angola | gündüz | 8.004.720 | 8.188.143 | 1,0229 | 1,0398 | 1,0165 |
| Angola | gece | 574.389 | 604.634 | 1,0527 | 1,0725 | 1,0189 |
| Indonesia | tümü | 1.049.979 | 1.068.960 | 1,0181 | 1,0154 | 0,9973 |
| Indonesia | gündüz | 786.739 | 799.879 | 1,0167 | 1,0168 | 1,0001 |
| Indonesia | gece | 263.240 | 269.081 | 1,0222 | 1,0023 | 0,9806 |

**Okuma:**
- Sayım köprüsü (tümü) altı ülkede `0,999 – 1,030` aralığında; NOAA-20, S-NPP'den en fazla %3 fazla tespit veriyor.
- FRP toplam köprüsü (tümü) `1,008 – 1,091` aralığında. Yunanistan (`1,091`) en büyük sapma ve aynı zamanda en küçük örneklem (35 bin tespit); gece dilimi orada `1,14`.
- Tespit başına FRP oranı (tümü) `0,987 – 1,075`.

**fire-harmony ile karşılaştırma:** O projenin Empirik Bayes sayım köprüleri (`S-NPP 0,990`, `NOAA-21 1,046`) filo ortalamasına göre tanımlıdır; buradaki `n_NOAA-20 / n_S-NPP` ile aynı büyüklük değildir ve NOAA-20 için bir referans değer vermez. Bu yüzden uyum ya da uyumsuzluk iddia edilmiyor; yalnızca iki sayının da 1'e birkaç yüzde yakın olduğu not ediliyor.

## 4. Terra Kısa-Aralık (`|Δt| ≤ 20 dk`, `≤ 2 km`) Kontrolü

| Ülke | Terra–VIIRS çift (≤20 dk) | Terra eş-konumlu (≤180 dk) | Terra min / medyan `Δt` (dk) | Aqua–VIIRS çift (≤20 dk) | Aqua min / medyan `Δt` (dk) |
|---|---:|---:|---:|---:|---:|
| Turkey | 191 | 38.863 | 1 / 129 | 54.623 | 0 / 31 |
| Greece | 57 | 27.913 | 3 / 124 | 31.393 | 0 / 31 |
| Brazil | 0 | 3.312.798 | 43 / 156 | 4.329.676 | 0 / 29 |
| Australia | 1.384 | 4.339.986 | 0 / 147 | 5.138.893 | 0 / 29 |
| Angola | 0 | 838.601 | 57 / 158 | 4.323.567 | 0 / 28 |
| Indonesia | 0 | 272.065 | 77 / 158 | 388.534 | 0 / 23 |

**Bulgular:**
- Aqua ile VIIRS aynı öğleden sonra yörüngesinde olduğu için kısa aralıklı çift boldur (medyan ~23–31 dk).
- Terra için durum enleme bağlıdır. Tropik ülkelerde (Brazil, Angola, Indonesia) `≤ 20 dk` çifti **hiç yoktur**; en küçük fark 43–77 dakikadır. Orta enlemlerde (Turkey, Greece, Australia) az sayıda çift vardır ve bunlar tarama kenarına yığılır.
- Bu, v1:G2'deki "30 dakikanın altında sıfır çift" gözlemini düzeltir: o ölçüm tropik/alt-tropik NRT bölgelerinde yapılmıştı; orta enlem arşiv verisinde Aqua için dakikalar mertebesinde, Terra için de az sayıda kısa aralıklı çift bulunuyor.

### 4.1 VZA dilimine göre `VIIRS_FRP / MODIS_FRP` (yalnız Australia; 200 çift eşiğini aşan tek ülke)

| MODIS VZA (°) | Terra n | Terra Σ oranı | Terra medyan | Aqua n | Aqua Σ oranı | Aqua medyan |
|---|---:|---:|---:|---:|---:|---:|
| 0–10 | 0 | — | — | 1.583.438 | 0,2826 | 0,3097 |
| 10–20 | 0 | — | — | 932.696 | 0,2739 | 0,2938 |
| 20–30 | 127 | 0,2263 | 0,1851 | 839.087 | 0,2510 | 0,2617 |
| 30–40 | 5 | 0,1072 | 0,1032 | 685.821 | 0,2164 | 0,2243 |
| 40–50 | 131 | 0,0833 | 0,0767 | 631.091 | 0,1783 | 0,1866 |
| 50–60 | 531 | 0,0790 | 0,0679 | 353.415 | 0,1537 | 0,1401 |
| 60–70 | 590 | 0,0613 | 0,0525 | 113.345 | 0,1156 | 0,1012 |

**Okuma ve sınırlar:**
- Her iki platformda oran VZA arttıkça düşüyor; yön Aqua ile Terra'da aynı.
- Terra oranı aynı VZA diliminde Aqua'nın yaklaşık yarısı (ör. 50–60°: `0,079` karşı `0,154`).
- Bu fark `R_rel(θ)`'nın Terra'ya aktarılamadığının kanıtı **sayılmamalıdır**: Terra çiftleri 0–20° dilimini hiç içermiyor (nadir normalizasyonu yapılamıyor), 1.384 çiftin 1.261'i tek bir yıla (2019) ait, ve çiftler bire-bir olmadığı için oran tek VIIRS pikselini tüm MODIS pikseline bölüyor. Sabah–öğleden sonra yangın şiddeti farkı da orana karışıyor.
- Sonuç: Terra'ya aktarım bu veriyle ne doğrulanabildi ne de çürütülebildi. Terra oranının sistematik olarak düşük çıkması, eğrinin Terra'da ayrıca kalibre edilmesi gerekebileceğine işaret eden bir ön bulgudur.

## 5. Sonuç

1. S-NPP ↔ NOAA-20 köprü katsayıları ölçüldü: sayımda `1,00–1,03`, FRP toplamında `1,01–1,09`. Filo birleştirmesinde iki platformu eşdeğer saymak sayım açısından %3 içinde tutarlı.
2. NOAA-21 köprüsü ülke arşivinden ölçülemez; Area API (K5 cache'i) gerektirir.
3. Terra `MOD14` için `≤ 20 dk` VZA kontrolü yalnız Avustralya'da yapılabildi ve yukarıdaki sınırlar nedeniyle kesin sonuç vermedi.
4. Ölçülmeyen dört ülke (India, United_States, Canada, Kazakhstan) için araç yeniden koşturulabilir; çift bulma adımı büyük ülkelerde dakikalar sürer.
