# Bilimsel Referanslar, Yöntem Dokümantasyonu ve Sorumluluk Reddi (PyroSync References & Methods)

> Bu belge, PyroSync veri akışı (pipeline) için bilimsel dayanakları, aşama açıklamalarını, APA 7 formatındaki akademik referans envanterini ve model varsayımları ile veri kısıtlarını açıklayan bilimsel sorumluluk reddi (disclaimer) içeriğini tanımlar. Proje içindeki `TASARIM_METHODS.md`, `/methods` sayfası ve `web/src/content/references.ts` bu belgeyi esas alır.

---

## 1. Bilimsel Sorumluluk Reddi, Model Varsayımları ve Veri Güvenilirliği (Scientific Disclaimer & Limitations)

### 1.1 Temel İlke ve Sorumluluk Reddi (Disclaimer Statement)
- **TR:** Harmonize edilmiş değerler model tahminidir, yer gerçeği (ground truth) değildir. PyroSync, 2000 yılından günümüze farklı mekânsal çözünürlük, yörünge saati ve algılama hassasiyetine sahip beş uydu platformunun (MODIS: Terra, Aqua; VIIRS: Suomi NPP, NOAA-20, NOAA-21) aktif yangın gözlemlerini fiziksel ve istatistiksel modellerle ortak bir takvime harmonize eder. Üretilen Yangın Işınım Gücü (FRP) değerleri, tespit sayıları ve belirsizlik aralıkları doğrudan yer ölçümü yerine geçmez; bilimsel ve karşılaştırmalı analiz amacıyla sunulmaktadır.
- **EN:** Harmonized values are model estimates, not direct ground truth. PyroSync harmonizes active fire observations across five satellite platforms (MODIS on Terra and Aqua; VIIRS on Suomi NPP, NOAA-20, and NOAA-21) operating at differing spatial resolutions, overpass times, and detection limits since 2000. All Fire Radiative Power (FRP) estimates, detection counts, and uncertainty intervals are model outputs intended for research and comparative trend analysis rather than operational ground truth.

### 1.2 Model Varsayımları (Model Assumptions)
1. **Görüş Zenit Açısı (VZA) İkinci Dereceden Normalizasyonu:** MODIS pikselleri nadirden ($1.0\text{ km}$) tarama kenarına ($65.4^\circ\text{ VZA}$) doğru geometrik olarak yaklaşık $4.8\times 2.0\text{ km}$ ($9.7\text{ km}^2$) boyutuna genişler. Eksen dışı piksel büyümesini telafi etmek için uygulanan $R_{\text{rel}}(\theta) = \max(1.0, 0.2017\theta^2 + 0.0168\theta + 1.0)$ eğrisi Zheng et al. (2026) Aqua MYD14 uyumundan nadirde $1.0$ olacak biçimde normalize edilmiştir. Nadir ofseti ($1.19$) takvim aşamasındaki $k_c$ çarpanından ayrı tutularak çift sayım engellenmiştir.
2. **Terra MOD14 Geometrik Vekili:** Sabah geçişi yapan Terra uydusu için bağımsız bir VZA eğrisi literatürde yayımlanmadığından, aynı ikinci dereceden eğri geometrik vekil (proxy) olarak kullanılmaktadır. Bu durumdan kaynaklanan ek varyans, yalnız MODIS içeren kayıtlara $\sigma_{\text{scale}} = 0.10$ sistematik belirsizlik bileşeni eklenerek temsil edilir.
3. **Sensör Gürültüsü ve Hata Parametreleri:** Varsayılan tek-piksel sensör belirsizlikleri ($\sigma_{\text{modis}} = 0.266$, $\sigma_{\text{viirs}} = 0.08$) ve VZA hata katsayısı ($k_{\text{vza}} = 0.05$) doğrudan yayınlanmış tek bir tablodan değil, CEOS doğrulama çerçevesi (Wooster et al., 2021) ve ampirik gözlemler doğrultusunda seçilmiş model parametreleridir.
4. **$1\text{ MODIS} = N\text{ VIIRS}$ Zamansal ve Mekânsal Penceresi:** MODIS ve VIIRS kardeş uyduları aynı yangın bölgesinden en az 34 dakika (medyan ~158 dakika) arayla geçmektedir. Bu nedenle eşleştirme aşamasında kullanılan $2.0\text{ km}$ yarıçap ve $180\text{ dakika}$ zaman penceresi, aynı yangın cephesinin alt piksellerini toplamak ve mükerrer sayımı önlemek için seçilmiş fiziksel bir model varsayımıdır.

### 1.3 Veri Kaynağı Güvenilirliği ve Çevresel Kısıtlar (Data Reliability & Environmental Limits)
1. **Duman ve Bulut Kaynaklı Tek Taraflı Negatif Sapma:** FIRMS veri setleri duman optik derinliği (AOD) veya anlık bulut kesri taşımaz. Kalın duman ve bulut örtüsü yangından yayılan orta kızılötesi (MIR) ışımasını zayıflatır veya tamamen engeller. Literatür bulgularına göre (Li et al., 2026; Sofiev & Kouznetsov, 2026), bulutlar küresel ölçekte MODIS yangın enerjisinin yaklaşık $\%22$'sini, VIIRS enerjisinin $\%12$'sini, bölgesel çalışmalarda ise yaklaşık $\%7$'sini gizler. Bu asimetrik ve tek yönlü eksik tahmin (underestimation) durumu simetrik normal belirsizlik bantlarına ($\pm 1\sigma$) dahil edilememektedir.
2. **Düşük Güven Sınıfı ve Sahte Alarmlar:** Düşük güvenilirlikli (`low confidence`) aktif yangın pikselleri yüksek oranda sahte alarm (false alarm) içerebilir (örneğin Doğu Çin'deki parlak metalik/cam çatılarda $\%40$'a, endüstriyel termik santrallerde $\%10$'a varan oranlarda). Arayüzdeki "Düşük güveni hariç tut" filtresi bu gürültüyü ayıklar; ancak uzun vadeli yıllık seriler ham karşılaştırılabilirlik için filtresiz muhafaza edilir.
3. **Yörünge Sürüklenmesi (Orbital Drift):** 2020 yılından itibaren yakıt tasarrufu nedeniyle yörünge düzeltme manevraları sınırlandırılan Terra uydusunun geçiş saati $10:30$'dan $09:15$'e, Aqua uydusunun geçiş saati ise $13:30$'dan $15:24$'e kaymıştır. Bu kayma gün içi yangın döngüsünde $\%2.8\dots\%5.5$ civarında sapmaya yol açabilmektedir.

### 1.4 Sensör Uçurumu ve Süreksizlik Ayrımı (Sensor Cliff & Completeness Factor)
- **Aylık Toplam FRP (`total_frp`):** VIIRS uydularının bulunmadığı 2000–2012 (pre-VIIRS) döneminde, küçük yangınların MODIS tarafından algılanamamasından kaynaklanan eksiklik, enlem bandına göre ölçülen ampirik $k_c(\text{lat})$ bütünlük çarpanı ($[0–30^\circ]: 1.11$, $[30–50^\circ]: 1.18$, $[55–70^\circ]: 1.28$) uygulanarak dengelenir.
- **Piksel Başına Ortalama FRP (`mean_frp`):** $k_c$ çarpanı piksel başına ortalama FRP değerine **uygulanmaz**. 375 m VIIRS piksellerinin algıladığı düşük şiddetli küçük yangınlar ortalama piksel FRP'sini matematiksel olarak düşürdüğü için, 2012 geçişinde `mean_frp` serisinde sensör çözünürlüğünden kaynaklanan kaçınılmaz bir süreksizlik (uçurum) bulunur. Gerçek yangın rejimini incelemek için normalize edilmiş toplam aktivite (`count_normalized`), `total_frp` ve $z$-skoru anomali değerleri kullanılmalıdır.

---

## 2. Pipeline Aşamaları Teknik Açıklamaları (Pipeline Stage Descriptions)

### Aşama 1: Topla (Collect)
- **TR:** NASA FIRMS NRT API'sinden ve yerel ülke arşivlerinden yalnızca bitki örtüsü yangını (`type == 0`) tespitlerini okur. Volkanik anomaliler, açık deniz gaz meşaleleri ve sabit endüstriyel termal kaynaklar filtrelenir; farklı dönem ve uydulardan gelen veriler tek bir standart şemada birleştirilir.
- **EN:** Ingests vegetation fire detections (FIRMS type 0) from the NASA FIRMS near-real-time API and local country archives. Volcanic thermal anomalies, offshore gas flares, and static industrial sources are excluded, standardizing multi-sensor records into a unified data structure.

### Aşama 2: Çift Sayımı Ayıkla & Filo Birleştir (Dedup & Fleet Consolidation)
- **TR:** Aynı sensör içinde ardışık taramalarda ortaya çıkan örtüşmeler ($|\Delta t| \le 8\text{ sn}$, örtüşme $>0.30$) tespit edilerek mükerrer kayıtlar elenir ve en yüksek güvenli tespit korunur. Ardından kardeş platformlar (Terra ile Aqua veya S-NPP, NOAA-20 ve NOAA-21) filo mantığıyla birleştirilerek sensör içi mükerrerlikler giderilir.
- **EN:** Removes duplicate detections within the same sensor family caused by overlapping consecutive scan swaths ($|\Delta t| \le 8\text{ s}$, overlap $>0.30$), retaining the observation with highest confidence. Fleet consolidation subsequently merges sibling satellites (Terra with Aqua, or S-NPP with NOAA-20 and NOAA-21) to eliminate multi-satellite redundant counts.

### Aşama 3: Bakış Açısını Düzelt (Viewing Zenith Angle Correction — VZA)
- **TR:** MODIS tarama geometrisinden türetilen görüş zenit açısı (VZA) kullanılarak, tarama kenarında genişleyen piksellerdeki geometrik zayıflama $R_{\text{rel}}(\theta)$ ikinci dereceden fonksiyonu ile düzeltilir. Düzeltme çarpanı nadirde $1.0$'dır ve tarama kenarına doğru büyüyerek ham MODIS FRP'sini VIIRS piksel ölçeği eşdeğerine yükseltir; VIIRS kayıtları sabit tutulur.
- **EN:** Corrects MODIS footprint distortion across the scan swath using the viewing zenith angle (VZA) derived from pixel dimensions via a quadratic function $R_{\text{rel}}(\theta)$. The multiplier is normalized to $1.0$ at nadir and increases toward the swath edge to scale MODIS FRP to footprint-equivalent values, leaving VIIRS measurements unaltered.

### Aşama 4: Ortak Izgara (Spatial Grid — H3 Resolution 8) [Planlanan / Planned]
- **TR:** Çözünürlük 8 H3 altıgen ızgarası (~$0.737\text{ km}^2$, merkezler arası ~$0.80\text{ km}$), bugünkü uygulamada 2., 3. ve 5. aşamalar için mekânsal aday aramasını hızlandıran bir uzamsal indeks olarak çalışmaktadır. Gelecek aşamada farklı çözünürlükteki tüm tespitlerin eşleştirme öncesinde bu altıgen hücrelerde birleştirilmesi planlanmaktadır.
- **EN:** An H3 hexagonal spatial grid at Resolution 8 (~$0.737\text{ km}^2$, ~$0.80\text{ km}$ spacing) currently serves as an efficient candidate search index for stages 2, 3, and 5. Full spatial aggregation of heterogeneous footprint detections into unified hexagonal cells prior to matching is scheduled as a planned extension.

### Aşama 5: Sensörleri Eşleştir (Sensor Matching — $1\text{ MODIS} = N\text{ VIIRS}$)
- **TR:** MODIS ve VIIRS filo gözlemleri $2.0\text{ km}$ yarıçap ve $180\text{ dakika}$ zaman penceresinde eşleştirilir. En yüksek mekânsal-zamansal skora sahip VIIRS gözlemi merkez alınarak, aynı uydu geçişindeki tüm VIIRS alt pikselleri tek bir MODIS ayak izi altında kümülatif olarak toplanır ($1\text{ MODIS} = N\text{ VIIRS}$).
- **EN:** Spatiotemporally pairs MODIS and VIIRS fleet observations within a $2.0\text{ km}$ radius and a $180\text{ minute}$ window. The highest-scoring VIIRS observation anchors the match, and all concurrent VIIRS sub-pixels within the MODIS footprint are aggregated into a single paired observation ($1\text{ MODIS} = N\text{ VIIRS}$).

### Aşama 6: En İyi Gözlemi Tut & Analitik Belirsizlik (FRP Fusion & Analytic Uncertainty)
- **TR:** Eşleşen kayıtlarda $375\text{ m}$ çözünürlüklü VIIRS konumu ve toplanmış VIIRS FRP değeri korunur, en yüksek algılama güveni atanır. Sensör gürültüsü, VZA hatası ve yalnız MODIS kalan noktalar için ölçek belirsizliğini birleştiren analitik hata yayılımı formülleriyle her tespite $\pm 1\sigma$ belirsizlik sınırları atanır.
- **EN:** Retains the high-resolution $375\text{ m}$ VIIRS coordinates and aggregated FRP for matched pairs, assigning the maximum confidence score. Analytic error propagation combining sensor noise, VZA error, and MODIS scale uncertainty is evaluated to assign explicit $\pm 1\sigma$ confidence intervals to every record.

### Aşama 7: Yangın Takvimini Kur (Calendar Aggregation, Effort $\Omega$, Completeness $k_c$, Conformal Bands)
- **TR:** Harmonize tespitler UTC aylık hücrelerine toplanır. Uydu filosu değişimlerinden kaynaklanan yapay sıçramalar platform çaba faktörü $\Omega$ ile dengelenir; VIIRS öncesi aylara enlem bazlı $k_c$ bütünlük çarpanı uygulanır. Mondrian split conformal aralıkları ile garantili örtüşme sınırları ve uzun vadeli $z$-skoru anomali değerleri hesaplanır.
- **EN:** Aggregates harmonized records into monthly UTC calendar cells. Artificial sensor cliff jumps are corrected via the fleet observation effort factor $\Omega$, while pre-VIIRS months receive latitude-dependent completeness scaling $k_c$. Distribution-free Mondrian split conformal prediction intervals and standardized $z$-score anomalies are generated for long-term climatology.

---

## 3. Pipeline Aşaması ↔ Akademik Referans Eşleme Tablosu

Aşağıdaki tablo, PyroSync veri akışındaki her bir adımın dayandığı bilimsel literatürü, APA 7 formatındaki künyesini ve koddaki karşılığını gösterir:

| # | Pipeline Aşaması | İlgili Çekirdek Modül | Temel Akademik Kaynak | APA 7 Referansı & DOI | Koddaki Dayanak / Parametre |
|---|---|---|---|---|---|
| **1** | **Topla (Collect)** | `crates/firms-client/src/archive.rs`<br>`crates/firms-client/src/client.rs` | NASA FIRMS<br>Giglio et al. (2016)<br>Schroeder et al. (2014) | NASA LANCE FIRMS (2024)<br>Giglio, L., et al. (2016), *RSE*, 178, 31–41. https://doi.org/10.1016/j.rse.2016.02.054<br>Schroeder, W., et al. (2014), *RSE*, 143, 85–96. https://doi.org/10.1016/j.rse.2013.12.008 | `FireType::VegetationFire`<br>`type == 0` filtreleme, NRT API ve ülke arşiv CSV parser |
| **2** | **Çift Sayımı Ayıkla & Filo (Dedup & Fleet)** | `crates/core/src/dedup.rs`<br>`crates/core/src/fleet.rs` | Li et al. (2018)<br>Csiszar et al. (2014) | Li, F., et al. (2018), *JGR: Atmospheres*, 123(9), 4545–4563. https://doi.org/10.1029/2017JD027823<br>Csiszar, I., et al. (2014), *JGR: Atmospheres*, 119(2), 803–816. https://doi.org/10.1002/2013JD020453 | `max_time_diff_secs = 8`<br>`min_overlap_ratio = 0.30`<br>`fleet.viirs_window_secs = 3600`<br>`fleet.modis_window_secs = 10800` |
| **3** | **Bakış Açısını Düzelt (Angle / VZA)** | `crates/core/src/vza.rs` | Zheng et al. (2026)<br>Freeborn et al. (2014) | Zheng, B., et al. (2026), *Remote Sensing of Environment*, 114620. https://doi.org/10.1016/j.rse.2025.114620<br>Freeborn, P. H., et al. (2014), *RSE*, 147, 88–100. | Eq. 9, Fig. 4 VZA eğrisi ($a=0.2017, b=0.0168, c=1.0$), nadir normalizasyonu, küresel geometri tarama açısı kestirimi |
| **4** | **Ortak Izgara (Grid - H3 Res 8)** | `crates/core/src/h3index.rs` | Brodsky (2018)<br>Sahr et al. (2003) | Brodsky, I. (2018). *H3: A Hexagonal Hierarchical Spatial Index*. Uber Technologies. https://h3geo.org<br>Sahr, K., et al. (2003), *CaGIS*, 30(2), 121–134. | `INDEX_RESOLUTION = Resolution::Eight`, aday arama halkası `k_ring_for_radius(radius_km)` |
| **5** | **Sensörleri Eşleştir (Match 1:N)** | `crates/core/src/harmonize.rs` | Vadrevu & Lasko (2018)<br>Zhang et al. (2020) | Vadrevu, K. P., & Lasko, K. (2018), *Remote Sensing*, 10(7), 978. https://doi.org/10.3390/rs10070978<br>Zhang, T., et al. (2020), *RSE*, 247, 111867. https://doi.org/10.1016/j.rse.2020.111867 | `radius_km = 2.0`, `window_secs = 10800`<br>$0.6\times\text{spatial} + 0.4\times\text{temporal}$ skorlama, toplanmış VIIRS alt pikselleri (`1 MODIS = N VIIRS`) |
| **6** | **En İyi Gözlem & Belirsizlik (Fuse)** | `crates/core/src/harmonize.rs`<br>`crates/core/src/uncertainty.rs` | Abramowitz & Stegun (1964)<br>Wooster et al. (2021)<br>Oliva & Schroeder (2015) | Abramowitz, M., & Stegun, I. A. (1964). *Handbook of Mathematical Functions* (AMS 55). NBS.<br>Wooster, M. J., et al. (2021), *Remote Sensing*, 13(3), 448. https://doi.org/10.3390/rs13030448<br>Oliva, P., & Schroeder, W. (2015), *RSE*, 160, 144–155. | VIIRS FRP & konum önceliği, formül 7.1.26 `erf_approx`, `modis_sensor_sigma = 0.266`, `viirs_sensor_sigma = 0.08`, `vza_error_coeff = 0.05`, `modis_scale_sigma = 0.10` |
| **7** | **Yangın Takvimi & Anomali (Calendar)** | `crates/core/src/calendar.rs`<br>`crates/core/src/effort.rs`<br>`crates/core/src/conformal.rs` | Sofiev & Kouznetsov (2026)<br>Remy & Kaiser (2014)<br>Angelopoulos & Bates (2021)<br>Yang et al. (2020) | Sofiev, M., & Kouznetsov, R. (2026), *GMD*, 19, 1–25. https://doi.org/10.5194/gmd-19-1-2026<br>Remy, S., & Kaiser, J. W. (2014), *ACP*, 14, 13291–13307. https://doi.org/10.5194/acp-14-13291-2014<br>Angelopoulos, A. N., & Bates, S. (2021), *arXiv:2107.07511*<br>Yang, G., et al. (2020), *Atmos. Environ.*, 223, 117287. | Çaba normalizasyonu $\Omega$, $k_c(\text{lat})$ bütünlük katsayıları ($1.11, 1.18, 1.28$), Mondrian split conformal aralıkları (`fit_mondrian`), $z$-skoru anomalisi |

---

## 4. Akademik Referanslar Bibliyografyası (APA 7 Formatında)

1. **Abramowitz, M., & Stegun, I. A. (Eds.). (1964).** *Handbook of mathematical functions with formulas, graphs, and mathematical tables* (Applied Mathematics Series 55). National Bureau of Standards, U.S. Government Printing Office.
2. **Angelopoulos, A. N., & Bates, S. (2021).** A gentle introduction to conformal prediction and distribution-free uncertainty quantification. *arXiv preprint arXiv:2107.07511*. https://doi.org/10.48550/arXiv.2107.07511
3. **Brodsky, I. (2018).** *H3: A hexagonal hierarchical spatial index* [Computer software]. Uber Technologies. https://h3geo.org
4. **Csiszar, I., Schroeder, W., Giglio, L., Ellicott, E., Vadrevu, K. P., Justice, C. O., & Wind, B. (2014).** Active fires from the Suomi NPP Visible Infrared Imaging Radiometer Suite: Product status and first evaluation results. *Journal of Geophysical Research: Atmospheres*, 119(2), 803–816. https://doi.org/10.1002/2013JD020453
5. **Freeborn, P. H., Wooster, M. J., Roberts, G., & Xu, W. (2014).** The MODIS fire radiative power (FRP) algorithm: A review and update. *Remote Sensing of Environment*, 147, 88–100. https://doi.org/10.1016/j.rse.2014.02.014
6. **Giglio, L., Schroeder, W., & Justice, C. O. (2016).** The Collection 6 MODIS active fire detection algorithm and fire products. *Remote Sensing of Environment*, 178, 31–41. https://doi.org/10.1016/j.rse.2016.02.054
7. **Li, F., Zhang, X., Kondragunta, S., & Csiszar, I. (2018).** Comparison of fire radiative power estimates from VIIRS and MODIS observations. *Journal of Geophysical Research: Atmospheres*, 123(9), 4545–4563. https://doi.org/10.1029/2017JD027823
8. **Li, M., et al. (2026).** EnsemFire V1.0: Global ensemble fire emission dataset with uncertainty quantification. *Geoscientific Model Development*, 19, 120–145.
9. **NASA LANCE FIRMS. (2024).** *MODIS Collection 6.1 and VIIRS Active Fire Detections* [Data set]. NASA Land, Atmosphere Near real-time Capability for EOS (LANCE) Fire Information for Resource Management System. https://doi.org/10.5067/FIRMS/WGS84
10. **Oliva, P., & Schroeder, W. (2015).** Assessment of VIIRS 375 m active fire detection product for direct burned area mapping. *Remote Sensing of Environment*, 160, 144–155. https://doi.org/10.1016/j.rse.2015.01.010
11. **Remy, S., & Kaiser, J. W. (2014).** Daily global fire radiative power from 1 or 2 MODIS instruments. *Atmospheric Chemistry and Physics*, 14(24), 13291–13307. https://doi.org/10.5194/acp-14-13291-2014
12. **Sahr, K., White, D., & Kimerling, A. J. (2003).** Geodesic discrete global grid systems. *Cartography and Geographic Information Science*, 30(2), 121–134. https://doi.org/10.1559/152304003100011090
13. **Schroeder, W., Oliva, P., Giglio, L., & Csiszar, I. A. (2014).** The New VIIRS 375 m active fire detection product: Algorithm description and initial assessment. *Remote Sensing of Environment*, 143, 85–96. https://doi.org/10.1016/j.rse.2013.12.008
14. **Sofiev, M., & Kouznetsov, R. (2026).** Observation operator for MODIS and VIIRS fire radiative power. *Geoscientific Model Development*, 19, 1–25. https://doi.org/10.5194/gmd-19-1-2026
15. **Vadrevu, K. P., & Lasko, K. (2018).** Intercomparison of MODIS AQUA and VIIRS I-Band fires and emissions in an agricultural landscape—Implications for air pollution research. *Remote Sensing*, 10(7), 978. https://doi.org/10.3390/rs10070978
16. **Wooster, M. J., Roberts, G., Freeborn, P. H., Xu, W., Govaerts, Y., Beeby, R., ... & Mullen, R. (2021).** Satellite remote sensing of active fires review and CEOS validation framework. *Remote Sensing*, 13(3), 448. https://doi.org/10.3390/rs13030448
17. **Yang, G., Zhao, H., Vadrevu, K. P., et al. (2020).** Post-harvest biomass burning in Northeast China: Fire dynamics, emissions and impacts on air quality. *Atmospheric Environment*, 223, 117287. https://doi.org/10.1016/j.atmosenv.2020.117287
18. **Zhang, T., Wooster, M. J., de Jong, M. C., & Xu, W. (2020).** How well does the 'small fire boost' of the new VIIRS I-Band active fire product characterise biomass burning in Africa?. *Remote Sensing of Environment*, 247, 111867. https://doi.org/10.1016/j.rse.2020.111867
19. **Zheng, B., et al. (2026).** Fire Radiative Power correction and spatiotemporal fusion between Aqua MODIS and VIIRS 375 m active fire products. *Remote Sensing of Environment*, 114620. https://doi.org/10.1016/j.rse.2025.114620

---

## 5. 32 Makalelik Araştırma Envanteri (01–32)

| No | Yazarlar | Yıl | Başlık | Yayın Yeri | DOI / URL | PyroSync'te Kullanımı |
|---|---|---|---|---|---|---|
| 01 | Zheng, B., et al. | 2026 | FRP Correction & Spatiotemporal Fusion (MYD14–VNP14IMG) | *RSE* 114620 | https://doi.org/10.1016/j.rse.2025.114620 | VZA çarpma eğrisi (Şek. 4: $0.2017\theta^2 + 0.0168\theta + 1.0$), $\le 5\text{ dk}$ footprint toplaması, 30 m Landsat doğrulaması |
| 02 | — | 2020 | Fire Detection & FRP in Northeast Asia (MODIS vs VIIRS) | — | — | Tek-piksel/ortak ve aylık toplam oran ikiliği; MODIS $\%65–83$ omisyonu; orta enlem katsayısı |
| 03 | Li, F., et al. | 2018 | Comparison of FRP From VIIRS and MODIS | *JGR: Atmos.* 123(9) | https://doi.org/10.1029/2017JD027823 | Mevcut G8 katsayıları ($1.16/0.97/0.82$) ve $8\text{ sn}$ / $>\%30$ dedup kaynağı; 750 m M-band uyarısı |
| 04 | Vadrevu, K. P. & Lasko, K. | 2018 | Intercomparison of MODIS AQUA and VIIRS I-Band Fires and Emissions in an Agricultural Landscape—Implications for Air Pollution Research | *Remote Sensing* 10(7), 978 | https://doi.org/10.3390/rs10070978 | Tarımsal 375 m I-band karşılaştırması (Pencap). VIIRS tespit sayısı MODIS Aqua'ya göre $6.5\times$, Aqua+Terra'ya göre $4.8\times$ yüksek; kümülatif toplam FRP VIIRS'te $2.5\times$ fazla ($\Sigma\text{VIIRS}/\Sigma\text{MODIS} = 2.5$); $r^2 = 0.98$ korelasyon |
| 05 | — | 2022 | MODIS to VIIRS for Global Volcano Thermal Monitoring | — | — | MIR VRP/FRP standart hatası ($\pm\%30$); VIIRS geçiş ve uyarı avantajı; belirsizlik (`uncertainty.rs`) ve volkanik filtre notu |
| 06 | — | 2017 | MODIS & VIIRS Gas Flaring in Khanty-Mansiysk | — | — | Borealde gece tespitlerinin $\%47–67$'sinin gaz meşalesi olması; statik kaynak filtresi |
| 07 | — | 2026 | Comprehensive Survey of Wildfire Indicators | — | — | C6 $\to$ C6.1 güncellemesi, Türkiye doğruluk aralığı, belirsizlik raporu gerekçesi |
| 08 | — | 2026 | Observation Operator & Detection Limits | EGUsphere 2026-89 (ön baskı) | — | Analitik $D_p(A_p)$ ve OOp denklemleri; [17] ile bayt-özdeş |
| 09 | Schroeder, W., Oliva, P., Giglio, L., & Csiszar, I. A. | 2014 | The New VIIRS 375 m active fire detection product: Algorithm description and initial assessment | *RSE* 143, 85–96 | https://doi.org/10.1016/j.rse.2013.12.008 | 3 zonlu kümeleme, bow-tie silme ($\sim\%50$), $367\text{ K}$ I4 doygunluğu, SAMA ve tespit artışı; statik kaynak filtresi |
| 10 | Oliva, P. & Schroeder, W. | 2015 | Assessment of VIIRS 375 m active fire detection product for direct burned area mapping | *RSE* 160, 144–155 | https://doi.org/10.1016/j.rse.2015.01.010 | 10 biyomda Landsat-8 doğrulaması; statik kaynak filtresi |
| 11 | Giglio, L., Schroeder, W., & Justice, C. O. | 2016 | The collection 6 MODIS active fire detection algorithm and fire products | *RSE* 178, 31–41 | https://doi.org/10.1016/j.rse.2016.02.054 | C6 Wooster FRP, MODIS güven formülü, Terra başlangıç tarihi dışlaması |
| 12 | Wooster, M. J., et al. | 2021 | Satellite Remote Sensing of Active Fires Review and CEOS Validation Framework | *Remote Sensing* 13(3), 448 | https://doi.org/10.3390/rs13030448 | MODIS tek-piksel belirsizliği $\sigma = \%26.6$ (`uncertainty.rs`), $<20\text{ m}^2$ VIIRS duyarlılığı, CEOS doğrulama çerçevesi |
| 13 | Csiszar, I., Schroeder, W., Giglio, L., et al. | 2014 | Active fires from the Suomi NPP Visible Infrared Imaging Radiometer Suite: Product status and first evaluation results | *JGR: Atmos.* 119(2) | https://doi.org/10.1002/2013JD020453 | 2012 SNPP kırılma tarihi, $\pm 31^\circ$ dışı tespit artışı, off-nadir kayma |
| 14 | Zhang, T., Wooster, M. J., de Jong, M. C., & Xu, W. | 2020 | How well does the 'small fire boost' of the new VIIRS I-Band active fire product characterise biomass burning in Africa? | *RSE* 247, 111867 | https://doi.org/10.1016/j.rse.2020.111867 | VNP14IMG Afrika oranı 1.26; swath-gap $\%10.7–12.5$; $1^\circ$ VZA polinomu |
| 15 | — | 2020 | Pixel Size & Overpass Time Undercounting (Cerrado) | — | — | 50 m MAS yer gerçeği; güç-yasası eğimi $\alpha = 1.31$; biyom/gece-gündüz kalibrasyonu |
| 16 | Sofiev, M., & Kouznetsov, R. | 2026 | Observation Operator for MODIS & VIIRS FRP | *GMD* 19, 1–25 | https://doi.org/10.5194/gmd-19-1-2026 | Küresel 177 vs 128 TW (1.38); $D_p(A_p)$ Tablo 1–2; Aqua–SNPP $\sim\%20–23$ döngü farkı (`effort.rs`) |
| 17 | — | 2026 | Observation Operator & Detection Limits | EGUsphere 2026-89 (ön baskı) | — | [08] ile özdeş denklemler; SEVIRI 43 MW netleşmesi yalnızca [16]'da |
| 18 | Remy, S. & Kaiser, J. W. | 2014 | Daily Global FRP from 1 or 2 MODIS | *ACP* 14, 13291–13307 | https://doi.org/10.5194/acp-14-13291-2014 | $2^\circ\to 12^\circ$ uyarlanabilir regresyon; 2000–2002 Terra kurtarımı; Afrika $8\times$ Aqua/Terra |
| 19 | — | 2017 | MODIS & VIIRS Intercomparison in Russia | Ön baskı | — | [06]'nın ön baskısı; VZA > 5° kaybı; statik kaynak filtresi |
| 20 | Rizza et al. | 2026 | Amazon FRP Evaluation with MODIS & VIIRS | Ön baskı (hakemsiz) | — | Uyarı kaynağı: VIIRS=high ve $\ge 100\text{ MW}$ filtresi oranı ters çevirir; kalibrasyonda kullanılmaz |
| 21 | Majumdar | 2025 | Decadal Biomass Fires on Indian Landmass | — | — | Hindistan $6.3–7.7\times$ VIIRS/MODIS farkı; Fire-Year (Mart–Şubat); $20\times 20\text{ km}$ WUI |
| 22 | Li, M., et al. | 2026 | EnsemFire V1.0 Global Ensemble Fire Emission | *GMD* 19, 120–145 | — | Envanterler arası $3–30\times$ fark; MERRA-2 OC AOD doğrulaması (bağımsız doğrulama, P6) |
| 23 | — | 2025 | 3D Biomass Burning Emission Inventory for SE/E Asia | — | — | $\pm 5\text{ dk}$ kalibrasyon ve ay $\times$ biyom tablosu; bulut kesri; TROPOMI CO doğrulaması ($R = 0.97$) |
| 24 | Alotaibi & Sorour | 2026 | Transformer Modeling of Fire Confidence | — | — | Yalnızca Low ($4.42\text{ MW}$) > Nominal ($1.89\text{ MW}$) bulgusu ve LOYO metrikleri; model kullanılmaz |
| 25 | — | 2026 | Wildfire Progression with Sentinel-2 & VIIRS (tez) | Tez | — | Çoklu VIIRS + 20 m Sentinel-2 füzyonu; savanda MODIS 500 m uyumu $\%41.1$ |
| 26 | — | 2017 | Fire Drivers in Chinese Forest Ecosystems | — | — | Boreal ve subtropikal sürücü ayrımı (AUC 0.865/0.733); MOD14A1 Kasım 2000 başlangıcı |
| 27 | — | 2021 | Humidity & Agriculture Ignitions in Yunnan | — | — | $\%38.06$ tarımsal tutuşma; $\sim 1.2\text{ km}$ köy eşiği; mevsimsel sürücü terslenmesi |
| 28 | — | 2021 | Effects of Straw Burning Bans in China | — | — | Yasaklarla $-\%46.9$ PM2.5 ve 4256 kurtarılan hayat; politika/jüri anlatısı |
| 29 | Yang, G., Zhao, H., Vadrevu, K. P., et al. | 2020 | Post-Harvest Biomass Burning in NE China | *Atmos. Environ.* 223 | https://doi.org/10.1016/j.atmosenv.2020.117287 | Tarım $\Sigma\text{VIIRS}/\Sigma\text{MODIS} = 2.5$; I-band doygunluk faktörü $f_{\text{FRP}} = 5$; VIIRS $\%68$ yakalama |
| 30 | — | 2021 | Open Straw Burning Variations in NE China | — | — | PCD ve PCP dairesel mevsim kayması (`calendar.rs`); MODIS eksikliği |
| 31 | — | 2020 | Late Pleistocene Fire in the Ili Basin (Loess) | — | — | **Kapsam dışı** (paleoklimat; aktif yangın ve FRP içermez) |
| 32 | — | 2005 | Land Cover Change in Twin Cities (Landsat) | — | — | **Kapsam dışı** (arazi örtüsü sınıflandırması; termal anomali içermez) |

---

## 6. İncelenen Referans Projeler

1. **fire-harmony:** 640.6M satırlık küresel FIRMS verisi; Mondrian split conformal aralıkları, dikiş yeri + placebo testi, Empirik Bayes VIIRS köprüsü ve canlı Methods sayfası.
2. **flare-chronos:** 1.61M satırlık FIRMS verisi; aynı geçiş küme toplamı, SAMA gece proton filtresi, Footprint Lens ve polar fenoloji saati.
3. **eg-firetrack:** 1.093M satırlık Borneo verisi; DBSCAN olay kümeleme, risk kural motoru ve Model vs Baseline Gate.
4. **FireHarmonize:** Hindistan NRT FastAPI + React uygulaması; Uydu Geçiş Saati / Yörünge Sürüklenmesi şeridi ve eşleşme duyarlılık önizlemesi.
5. **FireBridge:** Sentetik Kaliforniya prototipi; yalnızca UX fikirleri (60 sn tur, Viridis/Plasma paleti, ay tanı modalı).
6. **Chronofire:** Kodsuz tasarım dokümanı; RAW $\leftrightarrow$ HARMONISED anahtarı ve Responder Brief konsepti.

---

## 7. Doğrulanması Gereken İddialar ve Açık Boşluklar

### 7.1 Eksik ve Yinelenen Kaynak Dosyaları
- **[04]:** Dosya başlangıçta yanlış MDPI kimliğiyle (`rs10071091`, Aasen et al. İHA derlemesi) etiketlenmişti. Hedeflenen asıl makale olan Vadrevu & Lasko (2018), *Remote Sens.* 10(7), 978 tam metin PDF olarak temin edilmiş ve `academic-papers/04.pdf` olarak arşive yerleştirilmiştir. Makalenin bulguları (VIIRS vs MODIS 6.5× tespit, 2.5× toplam FRP farkı, $r^2=0.98$ korelasyonu) Bölüm 1'e ve projenin bilimsel temeline doğrudan işlenmiştir.
- **[08] ve [17]:** Bayt-özdeş EGUsphere 2026-89 kopyalarıdır; [16] bunun hakemli GMD 2026 sürümüdür.
- **[06] ve [19]:** [19] ön baskısı [06]'nın aynı çalışmasıdır.
- **[18]:** Dosya adı `Zhang2014` olsa da yazarlar Remy & Kaiser (2014)'tür.
- **[07]:** Dosya adı `_2025` olsa da yayın yılı 2026'dır.

### 7.2 Metinde Bulunmayan (`metinde yok`) Parametreler
- **VIIRS 375 m tek-piksel FRP $1\sigma$ belirsizliği (`uncertainty.rs`):** MODIS için $\sigma = \%26.6$ [12] bulunmuştur; VIIRS 375 m için açık bir parametrik değer hiçbir makalede doğrudan verilmediğinden model varsayımı olarak $\sigma_{\text{viirs}} = 0.08$ atanmış ve disclaimer metnine eklenmiştir.
- **Terra MOD14 VZA düzeltme katsayıları:** [01] Şekil 4 katsayıları ($0.24, 0.02, 1.19$) yalnızca Aqua MYD14 içindir; Terra için ayrı fit literatürde yoktur, geometrik vekil olarak aynı eğri kullanılır ve $\sigma_{\text{scale}} = 0.10$ belirsizliğiyle desteklenir.
- **FIRMS ayrık güven eşikleri ve `type` kodları:** MODIS güven kesimleri [11]'de bulunur; VIIRS `h` sınıfının kesin algoritmik kuralı [09]'da bulunur; FIRMS `type \in {0..3}` tanımları operasyonel dokümantasyondan alınmıştır.
- **C6 $\leftrightarrow$ C6.1 ve SNPP $\leftrightarrow$ NOAA-20/21 FRP farkları:** [11] C6'ya aittir; C6.1 aktif yangın FRP farkı ile SNPP vs NOAA-20/21 sayım köprüleri ampirik olarak ele alınmaktadır.

### 7.3 Kaynak İçi ve Kaynaklar Arası Çelişkiler
- **[20] Rizza et al. (2026), hakemsiz:** Metin FRP < 100 MW verilerini attığını söylerken Tablo 2–3 0–10 dahil tüm sınıfları gösterir; metindeki "2–3 kat" ifadesi, tablodaki oran 3.55–4.23× ile çelişir. VIIRS=high filtresi nedeniyle kalibrasyonda kullanılmamalıdır.
- **[24] Alotaibi & Sorour (2026):** Döngüsel hedef (VNP14 girdilerinden VNP14 güven etiketi tahmini), sınıf-koşullu doldurma kaynaklı target leakage ve metin–tablo sayısal çelişkileri taşır. Model kullanılmaz; yalnızca Low > Nominal bulgusu referans alınır.
- **[21] Majumdar (2025):** Özet (10.794/yıl) ile Sonuçlar (6.022/yıl) arasında yıllık yangın sayısı OLS eğiminde tutarsızlık vardır.
- **[22] Li et al. (2026):** Methods (Eq. 1–2, basit ortalama) ile Results s. 9 ("bias-minimization weighting") arasında ağırlıklandırma çelişkisi vardır.
