# Referanslar (taslak)

> **Not:** Bu dosya S8 için taslak içeriktir. Disclaimer metni henüz yazılmamıştır; sınırlamalar, model varsayımları ve veri kaynağı güvenilirliği bölümü S8 kapsamında eklenecektir.

Kaynak: `docs/REFERANS_ANALIZI.md` (Makale Envanteri 01–32, Referans Projeler, Doğrulanması Gereken İddialar §1–3).

**Düzeltme (makale 04):** Envanterdeki 04 numarası, `academic-papers/04` dosyası olarak `rs10071091` MDPI kimliğiyle Aasen et al. (2018) İHA spektroskopisi derlemesine işaret ediyordu. Hedeflenen makale **Vadrevu, K. P. & Lasko, K. (2018)**'dir. Crossref kaydından doğrulanan bilgiler aşağıdadır. Dikkat: hedeflenen kaynakta belirtilen `Remote Sensing 10(6), 978` ve `10.3390/rs10060978` bilgileri **yanlıştır**; doğrusu `Remote Sensing 10(7)` ve DOI `10.3390/rs10070978`'dir (Bölüm 2'ye bakınız).

Yazar, yıl ve yayın yeri bilgileri analiz belgesinde yalnızca bazı makaleler için verildiği için, verilmeyen alanlar `—` ile işaretlenmiştir. Bu alanlar APA biçimine geçmeden önce ilgili PDF'lerden doğrulanmalıdır. Analiz belgesinde DOI yalnızca 04 için doğrulanmıştır; diğer girişlerde DOI bulunmadığı için boş bırakılmıştır.

## 1. Makale listesi (01–32)

Başlıklar analiz belgesindeki kısa başlıklardır (İngilizce, envanter metninden alınmıştır). Tam başlık ve yayın bilgisi PDF'ten doğrulanmalıdır.

| No | Yazarlar | Yıl | Başlık | Yayın yeri | DOI / URL | PyroSync'te kullanımı |
|---|---|---|---|---|---|---|
| 01 | — | 2026 | FRP Correction & Spatiotemporal Fusion (MYD14–VNP14IMG) | — | — | VZA çarpma eğrisi (Şek. 4), ≤5 dk footprint toplaması, 30 m Landsat doğrulaması |
| 02 | — | 2020 | Fire Detection & FRP in Northeast Asia (MODIS vs VIIRS) | — | — | Tek-piksel/ortak ve aylık toplam oran ikiliği; MODIS %65–83 omisyonu; orta enlem katsayısı |
| 03 | Li et al. | 2018 | Comparison of FRP From VIIRS and MODIS | — | — | Mevcut G8 katsayıları (1.16/0.97/0.82) ve 8 sn / >%30 dedup kaynağı; 750 m M-band nedeniyle 375 m'ye uymadığı uyarısı |
| 04 | Vadrevu, K. P. & Lasko, K. | 2018 | Intercomparison of MODIS AQUA and VIIRS I-Band Fires and Emissions in an Agricultural Landscape—Implications for Air Pollution Research | *Remote Sensing* 10(7), 978 | https://doi.org/10.3390/rs10070978 | Tarımsal 375 m I-band karşılaştırması. Analiz belgesinde tarımsal 2.5× oranı şu an yalnızca [21] ve [29] üzerinden dolaylı aktarılıyor; PDF indirilemediği için bu makaleden içerik ayrıntısı eklenmemiştir |
| 05 | — | 2022 | MODIS to VIIRS for Global Volcano Thermal Monitoring | — | — | MIR VRP/FRP standart hatası (±%30); VIIRS geçiş ve uyarı avantajı; belirsizlik (`uncertainty.rs`) ve volkanik filtre notu |
| 06 | — | 2017 | MODIS & VIIRS Gas Flaring in Khanty-Mansiysk | — | — | Borealde gece tespitlerinin %47–67'sinin gaz meşalesi olması; statik kaynak filtresi |
| 07 | — | 2026 | Comprehensive Survey of Wildfire Indicators | — | — | C6→C6.1 güncellemesi, Türkiye doğruluk aralığı, belirsizlik raporu gerekçesi |
| 08 | — | 2026 | Observation Operator & Detection Limits | EGUsphere 2026-89 (ön baskı) | — | Analitik D_p(A_p) ve OOp denklemleri; [17] ile bayt-özdeş |
| 09 | Schroeder et al. | 2014 | VIIRS 375 m Active Fire Algorithm | — | — | 3 zonlu kümeleme, bow-tie silme, I4 doygunluğu, SAMA ve tespit artışı; statik kaynak filtresi |
| 10 | — | 2015 | VIIRS 375 m Assessment for Direct Burned Area | — | — | 10 biyomda Landsat-8 doğrulaması; statik kaynak filtresi |
| 11 | Giglio et al. | 2016 | Collection 6 MODIS Active Fire Algorithm | — | — | C6 Wooster FRP, MODIS güven formülü, Terra başlangıç tarihi dışlaması |
| 12 | — | 2021 | Satellite Remote Sensing of Active Fires Review | — | — | MODIS tek-piksel belirsizliği σ = %26.6 (`uncertainty.rs`), <20 m² VIIRS duyarlılığı, CEOS doğrulama çerçevesi |
| 13 | — | 2014 | SNPP VIIRS Active Fire Status & First Evaluation | — | — | 2012 SNPP kırılma tarihi, ±31° dışı tespit artışı, off-nadir kayma |
| 14 | — | 2020 | Biomass Burning in Africa: FRP Missed by MODIS | — | — | VNP14IMG Afrika oranı 1.26; swath-gap; 1° VZA polinomu (Tablo 1) |
| 15 | — | 2020 | Pixel Size & Overpass Time Undercounting (Cerrado) | — | — | 50 m MAS yer gerçeği; güç-yasası eğimi α = 1.31; biyom/gece-gündüz kalibrasyonu |
| 16 | — | 2026 | Observation Operator for MODIS & VIIRS FRP | *GMD* 2026 (hakemli) | — | Küresel 177 vs 128 TW (1.38); D_p(A_p) Tablo 1–2; Aqua–SNPP ~%20–23 döngü farkı (`effort.rs`) |
| 17 | — | 2026 | Observation Operator & Detection Limits | EGUsphere 2026-89 (ön baskı) | — | [08] ile özdeş denklemler; SEVIRI 43 MW netleşmesi yalnızca [16]'da |
| 18 | Remy, D. & Kaiser, J. W. | 2014 | Daily Global FRP from 1 or 2 MODIS | — | — | 2°→12° uyarlanabilir regresyon; 2000–2002 Terra kurtarımı; Afrika 8× Aqua/Terra |
| 19 | — | 2017 | MODIS & VIIRS Intercomparison in Russia | Ön baskı | — | [06]'nın ön baskısı; VZA > 5° kaybı; statik kaynak filtresi |
| 20 | Rizza et al. | 2026 | Amazon FRP Evaluation with MODIS & VIIRS | Ön baskı (hakemsiz) | — | Uyarı kaynağı: VIIRS=high ve ≥100 MW filtresi oranı ters çevirir; kalibrasyonda kullanılmaz |
| 21 | Majumdar | 2025 | Decadal Biomass Fires on Indian Landmass | — | — | Hindistan 6.3–7.7× VIIRS/MODIS farkı; Fire-Year (Mart–Şubat); 20×20 km WUI |
| 22 | Li et al. | 2026 | EnsemFire V1.0 Global Ensemble Fire Emission | — | — | Envanterler arası 3–30× fark; MERRA-2 OC AOD doğrulaması (bağımsız doğrulama, P6) |
| 23 | — | 2025 | 3D Biomass Burning Emission Inventory for SE/E Asia | — | — | ±5 dk kalibrasyon ve ay × biyom tablosu; bulut kesri; TROPOMI CO doğrulaması (R = 0.97) |
| 24 | Alotaibi & Sorour | 2026 | Transformer Modeling of Fire Confidence | — | — | Yalnızca Low (4.42 MW) > Nominal (1.89 MW) bulgusu ve LOYO metrikleri; model kullanılmaz (bkz. §3) |
| 25 | — | 2026 | Wildfire Progression with Sentinel-2 & VIIRS (tez) | Tez | — | Çoklu VIIRS + 20 m Sentinel-2 füzyonu; savanda MODIS 500 m uyumu %41.1 (bağımsız doğrulama, P6) |
| 26 | — | 2017 | Fire Drivers in Chinese Forest Ecosystems | — | — | Boreal ve subtropikal sürücü ayrımı (AUC 0.865/0.733); MOD14A1 Kasım 2000 başlangıcı |
| 27 | — | 2021 | Humidity & Agriculture Ignitions in Yunnan | — | — | %38.06 tarımsal tutuşma; ~1.2 km köy eşiği; mevsimsel sürücü terslenmesi |
| 28 | — | 2021 | Effects of Straw Burning Bans in China | — | — | Yasaklarla −%46.9 PM2.5 ve 4256 kurtarılan hayat; politika/jüri anlatısı |
| 29 | — | 2020 | Post-Harvest Biomass Burning in NE China | — | — | Tarım ΣVIIRS/ΣMODIS = 2.5; I-band doygunluk faktörü f_FRP = 5; VIIRS %68 yakalama |
| 30 | — | 2021 | Open Straw Burning Variations in NE China | — | — | PCD ve PCP dairesel mevsim kayması (`calendar.rs`); MODIS eksikliği |
| 31 | — | 2020 | Late Pleistocene Fire in the Ili Basin (Loess) | — | — | **Kapsam dışı** (paleoklimat; aktif yangın ve FRP içermez) |
| 32 | — | 2005 | Land Cover Change in Twin Cities (Landsat) | — | — | **Kapsam dışı** (arazi örtüsü sınıflandırması; termal anomali içermez) |

**DOI sayısı:** 32 girişten 1'inde (04) DOI doğrulanmıştır.

## 2. Referans projeler

1. **fire-harmony:** 640.6M satırlık küresel FIRMS verisi; Mondrian split conformal aralıkları, dikiş yeri + placebo testi, Empirik Bayes VIIRS köprüsü ve canlı Methods sayfası.
2. **flare-chronos:** 1.61M satırlık FIRMS verisi; aynı geçiş küme toplamı, SAMA gece proton filtresi, Footprint Lens ve polar fenoloji saati.
3. **eg-firetrack:** 1.093M satırlık Borneo verisi; DBSCAN olay kümeleme, risk kural motoru ve Model vs Baseline Gate.
4. **FireHarmonize:** Hindistan NRT FastAPI + React uygulaması; Uydu Geçiş Saati / Yörünge Sürüklenmesi şeridi ve eşleşme duyarlılık önizlemesi.
5. **FireBridge:** Sentetik Kaliforniya prototipi; yalnızca UX fikirleri (60 sn tur, Viridis/Plasma paleti, ay tanı modalı).
6. **Chronofire:** Kodsuz tasarım dokümanı; RAW ↔ HARMONISED anahtarı ve Responder Brief konsepti.

## 3. Doğrulanması gereken iddialar ve açık boşluklar

Bu bölüm analiz belgesinin §1–3 bölümlerinden aktarılmıştır.

### 3.1 Eksik ve yinelenen kaynak dosyaları
- **[04]:** Dosya yanlış MDPI kimliğiyle (`rs10071091`, Aasen et al. İHA derlemesi) geldiği için hedeflenen Vadrevu & Lasko (2018) makalesi içermiyordu. Bu belge Bölüm 1'de düzeltilmiştir; PDF indirilememiştir (aşağıya bakınız).
- **[08] ve [17]:** Bayt-özdeş EGUsphere 2026-89 kopyalarıdır; [16] bunun hakemli GMD 2026 sürümüdür.
- **[06] ve [19]:** [19] ön baskısı [06]'nın aynı çalışmasıdır.
- **[18]:** Dosya adı `Zhang2014` olsa da yazarlar Remy & Kaiser (2014)'tür.
- **[07]:** Dosya adı `_2025` olsa da yayın yılı 2026'dır.

### 3.2 Metinde bulunmayan (`metinde yok`) parametreler
- VIIRS 375 m tek-piksel FRP 1σ belirsizliği (`uncertainty.rs`): MODIS için σ = %26.6 [12] bulunmuştur; VIIRS 375 m için bir değer hiçbir makalede tablo olarak verilmemiştir.
- Terra MOD14 VZA düzeltme katsayıları: [01] Şekil 4 katsayıları (0.24, 0.02, 1.19) yalnızca Aqua MYD14 içindir; Terra için ayrı fit gerekir.
- FIRMS ayrık güven eşikleri ve `type` kodları: MODIS güven kesimleri [11]'de bulunur; VIIRS `h` sınıfının kesin algoritmik kuralı [09]'da bulunur; FIRMS `type ∈ {0..3}` ve `scan/track` yuvarlama tanımları kaynak makalelerde yoktur.
- C6 ↔ C6.1 ve SNPP ↔ NOAA-20/21 FRP farkları: [11] C6'ya aittir; C6.1 ve SNPP–NOAA-20/21 FRP farkları arşivden ölçülmelidir.

### 3.3 Kaynak içi ve kaynaklar arası çelişkiler
- **[20] Rizza et al. (2026), hakemsiz:** Metin FRP < 100 MW verilerini attığını söylerken Tablo 2–3 0–10 dahil tüm sınıfları gösterir; metindeki "2–3 kat" ifadesi, tablodaki oran 3.55–4.23× ile çelişir. VIIRS=high filtresi nedeniyle kalibrasyonda kullanılmamalıdır.
- **[24] Alotaibi & Sorour (2026):** Döngüsel hedef (VNP14 girdilerinden VNP14 güven etiketi tahmini), sınıf-koşullu doldurma/Winsorisation kaynaklı olası target leakage ve metin–tablo sayısal çelişkileri (özet 0.956–0.964 ile Tablo 11 0.929–0.936; −Thermal düşüşü %25.9 ile Tablo 12 %48.6–48.9). Model kullanılmaz; yalnızca Low > Nominal bulgusu ve LOYO metrikleri referans alınır.
- **[21] Majumdar (2025):** Özet (10.794/yıl) ile Sonuçlar (6.022/yıl) arasında yıllık yangın sayısı OLS eğiminde tutarsızlık vardır.
- **[22] Li et al. (2026):** Methods (Eq. 1–2, basit ortalama) ile Results s. 9 ("bias-minimization weighting") arasında ağırlıklandırma çelişkisi vardır.
- **Küçük yazım ve sürüm farkları:** [10] Tablo 3 başlığındaki `>0.9 ha` (baskı hatası; Bölüm 2.4 `0.09 ha`); [16] Bölüm 4.2 (88 vs 106 TW) ile Şekil 11 başlığı (104 vs 86 TW, 1.2× diurnal çarpan farkı); [19] boylam yazım hatası (65°W–75°W yerine Doğu).

## 4. Pipeline aşaması → kaynak

Eşleme yalnızca analiz belgesindeki yol haritası satırlarından (`REFERANS_ANALIZI.md`, Önceliklendirilmiş Yol Haritası) alınmıştır.

| Pipeline aşaması (modül) | Dayanak sorun | Destekleyen kaynaklar |
|---|---|---|
| VZA (bakış açısı) düzeltmesi (`vza.rs`) | P2 | 01, 14, 16 |
| FRP kalibrasyonu (`calibration.rs`) | P1 | 02, 03, 14, 15, 16, 23 |
| Eşleştirme ve birleştirme (`harmonize.rs`) | P5, P1 | 01, 02, 14 |
| Statik kaynak ve gaz meşalesi filtresi (`dedup.rs`) | P1, P4 | 06, 09, 10, 19 |
| Efor normalizasyonu (`effort.rs`) | P4, P1 | 16, 17 / 08 |
| Belirsizlik modeli (`uncertainty.rs`) | P3 | 05, 12 |
| Bağımsız doğrulama (`tests/`) | P6 | 22, 23, 25 |
| Takvim ve mevsimsellik (`calendar.rs`) | P7, P4 | 21, 30 |

Not: Eşleştirme aşaması için analizde doğrudan bir kaynak satırı yoktur; yalnızca `harmonize.rs` satırındaki [01, 02, 14] bağlantısı kullanılmıştır.
