/** Copy for the full methods page (`#/methods`); merged into the main dictionaries in copy.ts. */

export const methodsPageCopyEn = {
  methodsPageTitle: "Methods and data flow",
  methodsPageLead:
    "From five satellites (MODIS on Terra and Aqua; VIIRS on S-NPP, NOAA-20 and NOAA-21) to one consistent fire calendar.",
  methodsBackToMap: "← Back to map",
  methodsPrint: "Print methods",
  methodsNavLabel: "Methods sections",
  methodsNavPipeline: "Pipeline",
  methodsNavVza: "Viewing angle",
  methodsNavH3: "Spatial grid",
  methodsNavReferences: "References",
  methodsNavLimits: "Limits",

  methodsDisclaimerTitle: "Model estimates and assumptions",
  methodsDisclaimerBody:
    "For MODIS detections, VZA scales raw FRP to MODIS footprint equivalents; VIIRS-era calendar total_frp adds empirically measured pre-VIIRS completeness (k_c) when no VIIRS detections are present in a cell. mean_frp remains native-pixel and is discontinuous at sensor transitions. Default sensor-noise, VZA error, and MODIS scale uncertainty parameters are assumptions, not published values.",
  methodsDisclaimerShow: "Show assumptions",
  methodsDisclaimerHide: "Hide assumptions",

  methodsPipelineTitle: "Seven-stage pipeline",
  methodsPipelineIntro: "Open a stage to see what it does, why, and which parameters it uses.",
  methodsFlowLabel: "Pipeline diagram: seven stages in order",
  methodsFlowNoteFleet:
    "Fleet consolidation merges sibling platforms of one sensor family right after stage 2. It runs inside the pipeline but is not a separate stage.",
  methodsFlowNoteGrid: "Stage 4 is planned and not applied yet. Today H3 only narrows the candidates for stages 2, 3 and 5.",
  methodsFlowNoteModules:
    "Uncertainty intervals are added in stage 6; the effort factor Ω, the pre-VIIRS completeness factor k_c and monthly intervals in stage 7.",
  methodsExpandAll: "Expand all steps",
  methodsCollapseAll: "Collapse all steps",
  methodsLabelWhat: "What it does",
  methodsLabelWhy: "Why",
  methodsLabelParams: "Parameters",
  methodsLabelIo: "Input → output",
  methodsConfigLive: "Showing the active server settings.",
  methodsConfigFallback: "Live server thresholds unavailable; showing default parameters.",
  methodsConfigRetry: "Retry",

  methodsDetailCollectWhat:
    "Reads vegetation-fire detections (FIRMS type 0) from the NASA FIRMS near-real-time API or from the local country archive. Volcanoes, static sources and offshore flares are dropped.",
  methodsDetailCollectWhy: "Detections from different sensors and date ranges need one common schema before they can be compared.",
  methodsDetailCollectIo: "FIRMS CSV rows → one list of MODIS and VIIRS detections",
  methodsDetailDedupWhat:
    "Within one sensor, detections that are close in time and whose pixel footprints overlap strongly are grouped. The highest-confidence record is kept with the group's mean FRP. Sibling platforms of one sensor family (Terra and Aqua, or S-NPP, NOAA-20 and NOAA-21) are then merged by fleet consolidation.",
  methodsDetailDedupWhy: "A single satellite can count the same fire twice where consecutive scans overlap.",
  methodsDetailDedupIo: "Detections → de-duplicated detections → fleet observations",
  methodsDetailAngleWhat:
    "For MODIS only, the viewing zenith angle (VZA) is derived from the scan pixel width when FIRMS gives none. Raw FRP is multiplied by R_rel(θ), a quadratic in θ that equals 1 at nadir, so FRP rises toward the scan edge. VIIRS is left unchanged.",
  methodsDetailAngleWhy:
    "A MODIS pixel grows toward the scan edge, so the same fire is measured on a different footprint. The multiplier brings raw FRP to a footprint-equivalent scale. The nadir offset is kept out of this step so it is not counted twice with the pre-VIIRS completeness factor in stage 7.",
  methodsDetailAngleIo: "FRP and scan geometry → FRP × R_rel(θ)",
  methodsDetailGridWhat:
    "Planned, not applied yet: place all detections on one shared H3 hexagonal grid before matching. Today H3 resolution 8 only indexes candidates for de-duplication, fleet consolidation and matching; coordinates are not changed.",
  methodsDetailGridWhy: "MODIS pixels (about 1 km) and VIIRS pixels (375 m) would be aligned in the same spatial cell.",
  methodsDetailGridIo: "Fleet observations (indexed by H3, coordinates unchanged)",
  methodsDetailMatchWhat:
    "Each VIIRS observation is assigned to its nearest MODIS observation inside the matching radius and time window. The best-scoring VIIRS observation (spatial closeness 0.6, temporal closeness 0.4) seeds the match, and every VIIRS pixel of that pass within 10 minutes of the seed is summed into one matched record, so FRP and observation counts add up (1 MODIS = N VIIRS).",
  methodsDetailMatchWhy:
    "Terra (about 10:30) never passes together with VIIRS, and S-NPP and NOAA-20 are about 50 minutes apart, so the same fire is often seen hours apart: in live samples from 4 regions, co-located pairs were at least 34 minutes apart (median about 158 minutes). A wide window avoids counting one fire twice. Aqua and S-NPP both pass around 13:30, so the archive also holds many pairs within 20 minutes; those near-simultaneous matches are flagged separately for radiometric comparison and the completeness factor k_c was measured from such Aqua/S-NPP pairs.",
  methodsDetailMatchIo: "Fleet observations → matched records, plus MODIS-only and VIIRS-only remainders",
  methodsDetailFuseWhat:
    "For matched records, the VIIRS (375 m) position and the summed VIIRS FRP are kept, with the higher confidence of the records involved. Every record gets an analytic FRP interval from sensor noise and VZA error; MODIS-only records also carry a MODIS scale uncertainty.",
  methodsDetailFuseWhy:
    "When both sensors see a fire, the finer sensor is the better witness. The interval shows how far each remaining record can be trusted.",
  methodsDetailFuseIo: "Matched, MODIS-only and VIIRS-only records → harmonized hotspots with frp_lower and frp_upper",
  methodsDetailCalendarWhat:
    "Harmonized hotspots are counted per UTC month. An effort factor Ω per platform configuration normalizes counts for how many satellites were watching, and a z-score marks anomalies. In months with no VIIRS detections, total FRP is scaled by a latitude-band completeness factor k_c; mean FRP stays at native pixel scale. Uncertainty intervals are computed per month.",
  methodsDetailCalendarWhy:
    "Satellites added in 2012 (S-NPP), 2018 (NOAA-20) and 2023 (NOAA-21) create artificial jumps in counts, the sensor cliff, and pre-VIIRS months miss FRP. Normalizing makes real fire anomalies visible.",
  methodsDetailCalendarIo:
    "Harmonized hotspots → calendar cells (count, Ω, normalized count, FRP with intervals, anomaly)",

  methodsVzaTitle: "Viewing-angle correction",
  methodsVzaIntro:
    "R_rel(θ) multiplies MODIS FRP. It equals 1 at nadir and grows toward the scan edge, so it raises off-nadir FRP rather than lowering it.",
  methodsVzaFigureLabel: "Correction multiplier R_rel(θ) against viewing zenith angle, with the σ_vza band",
  methodsVzaAngleLabel: "Viewing zenith angle",
  methodsVzaBandToggle: "Show σ_vza band",
  methodsVzaNadir: "Nadir · no correction",
  methodsVzaEdge: "Scan edge",
  methodsVzaReadout:
    "θ = {angle}° · R_rel(θ) = {ratio} ({gain}) · σ_vza ±{sigmaVza} · MODIS random ±{sigmaRand} · plus scale ±{sigmaScale}",
  methodsVzaNote:
    "R_rel(θ) is normalized to 1 at nadir from the fit in Zheng et al. (2026). Sensor noise, the VZA error coefficient and the MODIS scale uncertainty are model assumptions. Terra has no separate fit, so it uses the same curve as a proxy.",

  methodsH3Title: "Spatial grid (H3)",
  methodsH3Intro:
    "Candidate search runs on H3 hexagons at resolution 8, whose cells are about the size of a MODIS pixel at nadir.",
  methodsH3FigureLabel: "Hexagonal H3 cells with a MODIS detection (hollow circle) and a VIIRS detection (filled square)",
  methodsH3Modis: "MODIS detection (about 1 km pixel)",
  methodsH3Viirs: "VIIRS detection (375 m pixel)",
  methodsH3Applied: "Applied today: candidate index. Coordinates are not changed.",
  methodsH3Planned: "Planned stage 4: merge detections in shared cells.",
  methodsH3Facts:
    "Resolution 8 · about 0.737 km² per cell · centres about 0.80 km apart · search ring k = {k} for the matching radius",
  methodsH3Why:
    "Hexagons give all six neighbours the same centre distance and follow a circular search radius more closely than square windows.",

  methodsReferencesTitle: "References",
  methodsReferencesIntro:
    "Works named in the project documentation. Entries marked as pending still need a verified full citation.",
  methodsReferencePending: "Full citation pending",
  methodsReferenceUsedIn: "Used in",
} as const;

export const methodsPageCopyTr: { readonly [K in keyof typeof methodsPageCopyEn]: string } = {
  methodsPageTitle: "Yöntem ve veri akışı",
  methodsPageLead:
    "Beş uydudan (Terra ve Aqua üzerinde MODIS; S-NPP, NOAA-20 ve NOAA-21 üzerinde VIIRS) tek tutarlı yangın takvimine.",
  methodsBackToMap: "← Haritaya dön",
  methodsPrint: "Yöntemleri yazdır",
  methodsNavLabel: "Yöntem bölümleri",
  methodsNavPipeline: "Akış",
  methodsNavVza: "Bakış açısı",
  methodsNavH3: "Mekânsal ızgara",
  methodsNavReferences: "Referanslar",
  methodsNavLimits: "Sınırlar",

  methodsDisclaimerTitle: "Model tahminleri ve varsayımlar",
  methodsDisclaimerBody:
    "MODIS tespitlerinde VZA düzeltmesi ham FRP'yi MODIS ayak izi eşdeğerine ölçekler; VIIRS olmayan (VIIRS öncesi) takvim hücrelerinde total_frp üzerine ölçülmüş bütünlük çarpanı (k_c) eklenir. mean_frp yerel piksel ölçeğinde kalır ve sensör geçişlerinde süreksizdir. Varsayılan sensör gürültüsü, VZA hata katsayısı ve MODIS ölçek belirsizliği parametreleri yayımlanmış değerler değil, varsayımlardır.",
  methodsDisclaimerShow: "Varsayımları göster",
  methodsDisclaimerHide: "Varsayımları gizle",

  methodsPipelineTitle: "Yedi aşamalı akış",
  methodsPipelineIntro: "Bir aşamayı açarak ne yaptığını, neden yaptığını ve hangi parametreleri kullandığını görün.",
  methodsFlowLabel: "Akış diyagramı: sırayla yedi aşama",
  methodsFlowNoteFleet:
    "Filo birleştirme, aynı sensör ailesinin kardeş uydularını 2. aşamadan hemen sonra birleştirir. Akışın içinde çalışır ama ayrı bir aşama değildir.",
  methodsFlowNoteGrid:
    "4. aşama planlanmıştır, henüz uygulanmamıştır. Bugün H3 yalnızca 2., 3. ve 5. aşamaların adaylarını daraltır.",
  methodsFlowNoteModules:
    "Belirsizlik aralıkları 6. aşamada; çaba faktörü Ω, VIIRS öncesi bütünlük çarpanı k_c ve aylık aralıklar 7. aşamada eklenir.",
  methodsExpandAll: "Tüm adımları aç",
  methodsCollapseAll: "Tüm adımları kapat",
  methodsLabelWhat: "Ne yapılıyor",
  methodsLabelWhy: "Neden",
  methodsLabelParams: "Parametreler",
  methodsLabelIo: "Girdi → çıktı",
  methodsConfigLive: "Sunucunun etkin ayarları gösteriliyor.",
  methodsConfigFallback: "Canlı sunucu eşikleri alınamadı; varsayılan parametreler gösteriliyor.",
  methodsConfigRetry: "Tekrar dene",

  methodsDetailCollectWhat:
    "Bitki örtüsü yangını tespitlerini (FIRMS type 0) NASA FIRMS yakın gerçek zamanlı API'sinden veya yerel ülke arşivinden okur. Yanardağlar, sabit kaynaklar ve deniz üstü meşaleler ayıklanır.",
  methodsDetailCollectWhy:
    "Farklı sensörlerin ve tarih aralıklarının tespitleri karşılaştırılmadan önce ortak bir şemaya ihtiyaç duyar.",
  methodsDetailCollectIo: "FIRMS CSV satırları → MODIS ve VIIRS tespitlerinin tek listesi",
  methodsDetailDedupWhat:
    "Aynı sensörde zamanca yakın ve piksel ayak izi güçlü biçimde örtüşen tespitler gruplanır. En yüksek güvenli kayıt, grubun ortalama FRP'siyle tutulur. Aynı sensör ailesinin kardeş uyduları (Terra ve Aqua ya da S-NPP, NOAA-20 ve NOAA-21) ardından filo birleştirmeyle tek gözleme indirilir.",
  methodsDetailDedupWhy: "Tek bir uydu, ardışık taramaların örtüştüğü yerde aynı yangını iki kez sayabilir.",
  methodsDetailDedupIo: "Tespitler → tekilleştirilmiş tespitler → filo gözlemleri",
  methodsDetailAngleWhat:
    "Yalnızca MODIS için, FIRMS vermediğinde görüş zenit açısı (VZA) tarama piksel genişliğinden türetilir. Ham FRP, nadirde 1'e eşit olan ikinci dereceden R_rel(θ) ile çarpılır; böylece FRP tarama kenarına doğru artar. VIIRS değişmeden kalır.",
  methodsDetailAngleWhy:
    "MODIS pikseli tarama kenarına doğru büyür; aynı yangın farklı bir ayak izinde ölçülür. Çarpan, ham FRP'yi ayak izi eşdeğeri bir ölçeğe taşır. Nadir ofseti bu adımdan ayrı tutulur; böylece 7. aşamadaki VIIRS öncesi bütünlük çarpanıyla iki kez sayılmaz.",
  methodsDetailAngleIo: "FRP ve tarama geometrisi → FRP × R_rel(θ)",
  methodsDetailGridWhat:
    "Planlanan, henüz uygulanmayan adım: eşleştirmeden önce tüm tespitleri ortak bir H3 altıgen ızgarasına yerleştirmek. Bugün H3 çözünürlük 8 yalnızca çift sayım ayıklama, filo birleştirme ve eşleştirme için aday indeksi olarak çalışır; koordinatlar değişmez.",
  methodsDetailGridWhy: "MODIS pikselleri (yaklaşık 1 km) ile VIIRS pikselleri (375 m) aynı mekânsal hücrede hizalanırdı.",
  methodsDetailGridIo: "Filo gözlemleri (H3 ile indekslenir, koordinatlar değişmez)",
  methodsDetailMatchWhat:
    "Her VIIRS gözlemi, eşleştirme yarıçapı ve zaman penceresi içindeki en yakın MODIS gözlemine atanır. En yüksek skorlu VIIRS gözlemi (mekânsal yakınlık 0,6, zamansal yakınlık 0,4) eşleşmeyi başlatır; aynı geçişin başlangıç gözleminden en fazla 10 dakika uzaktaki tüm VIIRS pikselleri tek bir eşleşmiş kayıtta toplanır, böylece FRP ve gözlem sayıları toplanır (1 MODIS = N VIIRS).",
  methodsDetailMatchWhy:
    "Terra (yaklaşık 10:30) hiçbir zaman VIIRS ile birlikte geçmez, S-NPP ile NOAA-20 arasında da yaklaşık 50 dakika vardır; bu yüzden aynı yangın çoğu zaman saatler arayla görülür: 4 bölgedeki canlı örneklerde aynı konumdaki çiftler en az 34 dakika aralıklıydı (medyan yaklaşık 158 dakika). Geniş pencere, tek bir yangını iki kez saymayı önler. Aqua ve S-NPP ise ikisi de yaklaşık 13:30'da geçtiği için arşivde 20 dakika içinde gözlenen çok sayıda çift vardır; bu neredeyse eşzamanlı eşleşmeler radyometrik karşılaştırma için ayrıca işaretlenir ve tamlık katsayısı k_c bu tür Aqua/S-NPP çiftlerinden ölçülmüştür.",
  methodsDetailMatchIo: "Filo gözlemleri → eşleşmiş kayıtlar, artı yalnız MODIS ve yalnız VIIRS kalanları",
  methodsDetailFuseWhat:
    "Eşleşen kayıtlarda VIIRS (375 m) konumu ve toplanan VIIRS FRP'si, ilgili kayıtların yüksek güveniyle birlikte tutulur. Her kayda sensör gürültüsü ve VZA hatasından analitik bir FRP aralığı verilir; yalnız MODIS kayıtları ayrıca MODIS ölçek belirsizliği taşır.",
  methodsDetailFuseWhy:
    "İki sensör de yangını gördüğünde ince çözünürlüklü sensör daha iyi tanıktır. Aralık, kalan her kayda ne kadar güvenilebileceğini gösterir.",
  methodsDetailFuseIo:
    "Eşleşen, yalnız MODIS ve yalnız VIIRS kayıtları → frp_lower ve frp_upper içeren harmonize tespitler",
  methodsDetailCalendarWhat:
    "Harmonize tespitler UTC aylarına göre sayılır. Platform yapılandırması başına çaba faktörü Ω, kaç uydunun izlediğine göre sayıları normalize eder; z-skoru anomalileri işaretler. VIIRS tespiti olmayan aylarda toplam FRP, enlem bandı bütünlük çarpanı k_c ile ölçeklenir; ortalama FRP yerel piksel ölçeğinde kalır. Belirsizlik aralıkları aylık hesaplanır.",
  methodsDetailCalendarWhy:
    "2012 (S-NPP), 2018 (NOAA-20) ve 2023'te (NOAA-21) eklenen uydular sayılarda yapay sıçramalar yaratır; buna sensör uçurumu denir, VIIRS öncesi aylarda ise FRP eksiktir. Normalizasyon gerçek yangın anomalilerini görünür kılar.",
  methodsDetailCalendarIo:
    "Harmonize tespitler → takvim hücreleri (sayı, Ω, normalize sayı, aralıklı FRP, anomali)",

  methodsVzaTitle: "Bakış açısı düzeltmesi",
  methodsVzaIntro:
    "R_rel(θ) MODIS FRP'sini çarpar. Nadirde 1'dir ve tarama kenarına doğru büyür; yani eksen dışı FRP'yi düşürmez, yükseltir.",
  methodsVzaFigureLabel: "Görüş zenit açısına göre düzeltme çarpanı R_rel(θ) ve σ_vza bandı",
  methodsVzaAngleLabel: "Görüş zenit açısı",
  methodsVzaBandToggle: "σ_vza bandını göster",
  methodsVzaNadir: "Nadir · düzeltme yok",
  methodsVzaEdge: "Tarama kenarı",
  methodsVzaReadout:
    "θ = {angle}° · R_rel(θ) = {ratio} ({gain}) · σ_vza ±{sigmaVza} · MODIS rastgele ±{sigmaRand} · ayrıca ölçek ±{sigmaScale}",
  methodsVzaNote:
    "R_rel(θ), Zheng et al. (2026) uyumundan nadirde 1 olacak biçimde normalize edilmiştir. Sensör gürültüsü, VZA hata katsayısı ve MODIS ölçek belirsizliği model varsayımlarıdır. Terra için ayrı bir uyum yoktur; aynı eğri vekil olarak kullanılır.",

  methodsH3Title: "Mekânsal ızgara (H3)",
  methodsH3Intro:
    "Aday araması, hücreleri nadirdeki bir MODIS pikseli büyüklüğünde olan çözünürlük 8 H3 altıgenleri üzerinde yapılır.",
  methodsH3FigureLabel: "Bir MODIS tespiti (içi boş daire) ve bir VIIRS tespiti (dolu kare) içeren altıgen H3 hücreleri",
  methodsH3Modis: "MODIS tespiti (yaklaşık 1 km piksel)",
  methodsH3Viirs: "VIIRS tespiti (375 m piksel)",
  methodsH3Applied: "Bugün uygulanan: aday indeksi. Koordinatlar değişmez.",
  methodsH3Planned: "Planlanan 4. aşama: tespitleri ortak hücrelerde birleştirmek.",
  methodsH3Facts:
    "Çözünürlük 8 · hücre başına yaklaşık 0,737 km² · merkezler yaklaşık 0,80 km arayla · eşleştirme yarıçapı için arama halkası k = {k}",
  methodsH3Why:
    "Altıgenler altı komşuya da eşit merkez uzaklığı verir ve dairesel arama yarıçapını kare pencerelerden daha yakın izler.",

  methodsReferencesTitle: "Referanslar",
  methodsReferencesIntro:
    "Proje dokümantasyonunda adı geçen çalışmalar. Beklemede olarak işaretli kayıtların doğrulanmış tam künyesi henüz eklenmedi.",
  methodsReferencePending: "Tam künye bekleniyor",
  methodsReferenceUsedIn: "Kullanıldığı aşama",
};
