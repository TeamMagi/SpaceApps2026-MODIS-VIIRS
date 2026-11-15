/** Copy for the cell diagnostic drawer (Y9) and the Orbital Drift strip; merged into the dictionaries in copy.ts. */

export const diagnosticsCopyEn = {
  cellOpenDetails: "Month details",
  cellOpenDetailsShort: "Details · {month}",
  cellDetailsKeyHint: "Press D on a focused month",
  cellDrawerTitle: "Month diagnostics · {month}",
  cellDrawerLead: "What is behind this calendar month: sensors, effort, completeness, uncertainty and flags.",
  cellLoading: "Loading the month breakdown…",
  cellError: "The month breakdown could not be loaded.",
  cellEmpty: "No record for this month.",
  cellUnavailable: "—",
  cellSectionSensors: "Sensors",
  cellRawModis: "Raw MODIS detections",
  cellRawViirs: "Raw VIIRS detections",
  cellRawTotal: "Raw total",
  cellPlatformNote:
    "The API reports MODIS and VIIRS per month, not per platform (Terra, Aqua, S-NPP, NOAA-20), so no platform split is shown.",
  cellHarmonized: "Harmonized records",
  cellMatched: "Matched (MODIS + VIIRS)",
  cellModisOnly: "MODIS only",
  cellViirsOnly: "VIIRS only",
  cellSourcesSampled:
    "The MODIS-only / VIIRS-only split needs every point of the month; this month is sampled, so only the matched count is exact.",
  cellRawInvite:
    "Switch to Harmonized (H) to see this month's effort, completeness, uncertainty and harmonized breakdown. Raw mode shows only what the satellites reported.",
  cellSectionEffort: "Observation effort",
  cellOmega: "Effort Ω",
  cellNormalized: "Effort-normalised count",
  cellPreAquaTitle: "Not comparable (Terra-only month)",
  cellPreAquaBody:
    "Before Aqua joined in 2002-07 only Terra watched. Ω cannot measure a change inside the MODIS fleet, so this month has no effort-normalised count and no anomaly; it must not be compared with later months. The detection counts above are real.",
  cellRecovered: "Recovered estimate (MODIS-observable detections)",
  cellRecoveredValue: "{value} (range {low}–{high}) · estimate",
  cellRecoveredNote:
    "An estimate, not an observation: the Terra count times a factor calibrated on 2002-07..2020-12 (adapted from Remy & Kaiser 2014). It is not comparable with effort-normalised counts.",
  cellSectionCompleteness: "Completeness k_c",
  cellCompletenessApplies:
    "This month has no VIIRS detections, so its total FRP is scaled by the pre-VIIRS completeness factor k_c (it depends on the latitude band, roughly 1.1 to 1.3). The API does not return the factor per month, so no value is shown.",
  cellCompletenessNotApplied: "This month includes VIIRS detections, so k_c is not applied to its total FRP.",
  cellCompletenessLink: "How k_c is derived (Methods)",
  cellSectionUncertainty: "Uncertainty",
  cellTotalFrp: "Total FRP",
  cellTotalFrpBand: "{value} MW (analytic range {low}–{high})",
  cellTotalFrpPlain: "{value} MW",
  cellFrpPerDetection: "FRP per detection (MODIS-equivalent)",
  cellConformal68: "Conformal band ±1σ (about 68 %)",
  cellConformal90: "Conformal band 90 %",
  cellConformalRange: "{low}–{high} MW",
  cellConformalEraPre: "pre-VIIRS calibration",
  cellConformalEraViirs: "VIIRS-era calibration",
  cellConformalNote:
    "A sensor and fleet reproducibility band, not ground-truth accuracy ({era}, {n} calibration residuals).",
  cellConformalExtrapolated: "Outside the calibrated range; the nearest bin was used.",
  cellConformalMissing:
    "No conformal band for this month (no calibrated table, a Terra-only month, or not a country-wide query).",
  cellAnomaly: "Anomaly vs the same month in other years",
  cellSectionFlags: "Quality and outage flags",
  cellFlagsNone: "No quality or outage flags for this month.",
  cellFlagsUnavailable: "Flags come from the live archive calendar; they are not available in demo mode or while it loads.",
  cellFlagTerraEarlyRecord: "Terra early record: incomplete first months of the Terra mission",
  cellFlagTerraOutage: "Terra outage: a known MOD14 data gap falls in this month",
  cellFlagSnppEarlyCalibration: "S-NPP early calibration: about 50 K nadir brightness bias before 2012-04-03",
  cellFlagSnppSpuriousScanLines: "S-NPP spurious scan lines before 2012-10-15",
  cellFlagModisOrbitalDrift: "MODIS orbital drift: Terra and Aqua drift from their nominal crossing times (2021 on)",
  cellFlagUnknown: "Unrecognised flag: {flag}",

  driftTitle: "Orbital drift",
  driftFlagBody:
    "From 2021 on, Terra and Aqua MODIS drift away from their nominal equator-crossing times (Terra 10:39 → 09:15, Aqua 13:33 → 15:24 by 2026), which the literature ties to a roughly −2.8 % to −5.5 % calibration deviation. Sensor change, not fire change.",
  driftDeviationBody:
    "The overpass times of this period differ from the nominal crossing times of the MODIS satellites. In the literature, MODIS orbital drift from 2021 on is tied to a roughly −2.8 % to −5.5 % calibration deviation.",
  driftSummaryFlag:
    "Terra and Aqua MODIS drift from their nominal crossing times from 2021 on (literature: −2.8 % to −5.5 % calibration deviation).",
  driftSummaryDeviation: "Overpass times differ from the MODIS nominal crossing times (literature: −2.8 % to −5.5 % calibration deviation).",
  driftShortCaveat: "Regional median, not the equator-crossing time.",
  driftMore: "Read more",
  driftLess: "Show less",
  driftRow: "{platform} {pass}: {time} vs {nominal} nominal ({diff} min)",
  driftPassDay: "day",
  driftPassNight: "night",
  driftNoNumbers: "Overpass medians come from the live archive and are not available in demo mode.",
  driftCaveat:
    "Each time is the median local solar time of all detections of this region and period. It is an indicator, not the equator-crossing time: detections away from the sub-satellite track spread around the crossing time and high-latitude passes shift it.",
  driftSource: "Source: fire-harmony and FireHarmonize (docs/REFERANS_ANALIZI.md, P4 and P7).",
  driftDismiss: "Dismiss orbital drift note",

  methodsBandsTitle: "Uncertainty bands and orbital drift",
  methodsBandsConformal:
    "The ±1σ conformal (Mondrian) band around a month's total FRP is a sensor and fleet reproducibility band, calibrated separately for the pre-VIIRS and VIIRS eras. It is not ground-truth accuracy, and it does not replace the analytic range.",
  methodsBandsDrift:
    "From 2021, MODIS Terra and Aqua drift from their nominal crossing times, which the literature ties to a −2.8 % to −5.5 % calibration deviation. The Orbital Drift strip on the dashboard compares each platform's median local overpass time with its nominal time. The medians are regional indicators, not equator-crossing times.",
} as const;

export const diagnosticsCopyTr = {
  cellOpenDetails: "Ay ayrıntıları",
  cellOpenDetailsShort: "Ayrıntılar · {month}",
  cellDetailsKeyHint: "Odaktaki ayda D tuşuna bas",
  cellDrawerTitle: "Ay tanıları · {month}",
  cellDrawerLead: "Bu takvim ayının arkasında ne var: sensörler, efor, tamlık, belirsizlik ve bayraklar.",
  cellLoading: "Ay kırılımı yükleniyor…",
  cellError: "Ay kırılımı yüklenemedi.",
  cellEmpty: "Bu ay için kayıt yok.",
  cellUnavailable: "—",
  cellSectionSensors: "Sensörler",
  cellRawModis: "Ham MODIS tespitleri",
  cellRawViirs: "Ham VIIRS tespitleri",
  cellRawTotal: "Ham toplam",
  cellPlatformNote:
    "API aylık sayıları MODIS ve VIIRS olarak veriyor, platform (Terra, Aqua, S-NPP, NOAA-20) bazında vermiyor; bu yüzden platform kırılımı gösterilmiyor.",
  cellHarmonized: "Harmonize kayıtlar",
  cellMatched: "Eşleşen (MODIS + VIIRS)",
  cellModisOnly: "Yalnız MODIS",
  cellViirsOnly: "Yalnız VIIRS",
  cellSourcesSampled:
    "Yalnız MODIS / yalnız VIIRS ayrımı ayın tüm noktalarını gerektirir; bu ay örneklendiği için yalnızca eşleşen sayısı kesindir.",
  cellRawInvite:
    "Bu ayın eforunu, tamlığını, belirsizliğini ve harmonize kırılımını görmek için Harmonize'ye geç (H). Ham modu yalnızca uyduların bildirdiğini gösterir.",
  cellSectionEffort: "Gözlem eforu",
  cellOmega: "Efor Ω",
  cellNormalized: "Efor-normalize sayım",
  cellPreAquaTitle: "Karşılaştırılamaz (yalnız Terra ayı)",
  cellPreAquaBody:
    "2002-07'de Aqua katılmadan önce yalnız Terra izliyordu. Ω, MODIS filosunun kendi içindeki değişimi ölçemez; bu yüzden bu ayın efor-normalize sayımı ve anomalisi yoktur, sonraki aylarla karşılaştırılmamalıdır. Yukarıdaki tespit sayıları gerçektir.",
  cellRecovered: "Kurtarılmış tahmin (MODIS'in gözleyebileceği tespitler)",
  cellRecoveredValue: "{value} (aralık {low}–{high}) · tahmin",
  cellRecoveredNote:
    "Gözlem değil tahmindir: Terra sayısı, 2002-07..2020-12 aylarında kalibre edilen bir katsayıyla çarpılır (Remy & Kaiser 2014'ten uyarlama). Efor-normalize sayımlarla karşılaştırılamaz.",
  cellSectionCompleteness: "Tamlık k_c",
  cellCompletenessApplies:
    "Bu ayda VIIRS tespiti yok; bu yüzden toplam FRP'si VIIRS öncesi tamlık katsayısı k_c ile ölçeklenir (enlem bandına göre kabaca 1,1 ile 1,3 arası). API katsayıyı ay bazında döndürmediği için değer gösterilmiyor.",
  cellCompletenessNotApplied: "Bu ayda VIIRS tespitleri var; bu yüzden toplam FRP'ye k_c uygulanmaz.",
  cellCompletenessLink: "k_c nasıl türetilir (Yöntem)",
  cellSectionUncertainty: "Belirsizlik",
  cellTotalFrp: "Toplam FRP",
  cellTotalFrpBand: "{value} MW (analitik aralık {low}–{high})",
  cellTotalFrpPlain: "{value} MW",
  cellFrpPerDetection: "Tespit başına FRP (MODIS eşdeğeri)",
  cellConformal68: "Konformal bant ±1σ (yaklaşık %68)",
  cellConformal90: "Konformal bant %90",
  cellConformalRange: "{low}–{high} MW",
  cellConformalEraPre: "VIIRS öncesi kalibrasyon",
  cellConformalEraViirs: "VIIRS dönemi kalibrasyonu",
  cellConformalNote:
    "Sensör ve filo tekrarlanabilirlik bandıdır, gerçek (ground-truth) doğruluk değildir ({era}, {n} kalibrasyon kalıntısı).",
  cellConformalExtrapolated: "Kalibre edilen aralığın dışında; en yakın kutu kullanıldı.",
  cellConformalMissing:
    "Bu ay için konformal bant yok (kalibre tablo yok, yalnız Terra ayı veya ülke geneli olmayan sorgu).",
  cellAnomaly: "Diğer yılların aynı ayına göre anomali",
  cellSectionFlags: "Kalite ve kesinti bayrakları",
  cellFlagsNone: "Bu ay için kalite veya kesinti bayrağı yok.",
  cellFlagsUnavailable: "Bayraklar canlı arşiv takviminden gelir; demo modunda veya yüklenirken bulunmaz.",
  cellFlagTerraEarlyRecord: "Terra erken kayıt: Terra görevinin eksik ilk ayları",
  cellFlagTerraOutage: "Terra kesintisi: bilinen bir MOD14 veri boşluğu bu aya denk geliyor",
  cellFlagSnppEarlyCalibration: "S-NPP erken kalibrasyon: 2012-04-03 öncesinde nadirde yaklaşık 50 K parlaklık sapması",
  cellFlagSnppSpuriousScanLines: "S-NPP sahte tarama çizgileri (2012-10-15 öncesi)",
  cellFlagModisOrbitalDrift: "MODIS yörünge sürüklenmesi: Terra ve Aqua nominal geçiş saatlerinden uzaklaşıyor (2021'den itibaren)",
  cellFlagUnknown: "Tanınmayan bayrak: {flag}",

  driftTitle: "Yörünge sürüklenmesi",
  driftFlagBody:
    "2021'den itibaren Terra ve Aqua MODIS nominal ekvator geçiş saatlerinden uzaklaşıyor (2026'ya kadar Terra 10:39 → 09:15, Aqua 13:33 → 15:24); literatür bunu kabaca −%2,8 ile −%5,5 arasında bir kalibrasyon sapmasına bağlıyor. Değişen yangın değil, sensör.",
  driftDeviationBody:
    "Bu dönemin geçiş saatleri MODIS uydularının nominal geçiş saatlerinden farklı. Literatür, 2021'den itibaren MODIS yörünge sürüklenmesini kabaca −%2,8 ile −%5,5 arasında bir kalibrasyon sapmasına bağlıyor.",
  driftSummaryFlag:
    "Terra ve Aqua MODIS, 2021'den itibaren nominal geçiş saatlerinden uzaklaşıyor (literatür: −%2,8 ile −%5,5 kalibrasyon sapması).",
  driftSummaryDeviation: "Geçiş saatleri MODIS'in nominal geçiş saatlerinden farklı (literatür: −%2,8 ile −%5,5 kalibrasyon sapması).",
  driftShortCaveat: "Bölgesel medyan, ekvator geçiş saati değil.",
  driftMore: "Devamı",
  driftLess: "Daha az göster",
  driftRow: "{platform} {pass}: {time}, nominal {nominal} ({diff} dk)",
  driftPassDay: "gündüz",
  driftPassNight: "gece",
  driftNoNumbers: "Geçiş saati medyanları canlı arşivden gelir; demo modunda bulunmaz.",
  driftCaveat:
    "Her saat, bu bölge ve dönemdeki tüm tespitlerin yerel güneş saati medyanıdır. Ekvator geçiş saati değil, bir göstergedir: uydu izinden uzak tespitler geçiş saatinin etrafına yayılır, yüksek enlem geçişleri saati kaydırır.",
  driftSource: "Kaynak: fire-harmony ve FireHarmonize (docs/REFERANS_ANALIZI.md, P4 ve P7).",
  driftDismiss: "Yörünge sürüklenmesi notunu kapat",

  methodsBandsTitle: "Belirsizlik bantları ve yörünge sürüklenmesi",
  methodsBandsConformal:
    "Bir ayın toplam FRP'si çevresindeki ±1σ konformal (Mondrian) bant, VIIRS öncesi ve VIIRS dönemi için ayrı kalibre edilmiş bir sensör ve filo tekrarlanabilirlik bandıdır. Gerçek (ground-truth) doğruluk değildir ve analitik aralığın yerine geçmez.",
  methodsBandsDrift:
    "2021'den itibaren MODIS Terra ve Aqua nominal geçiş saatlerinden uzaklaşıyor; literatür bunu −%2,8 ile −%5,5 arasında bir kalibrasyon sapmasına bağlıyor. Panodaki Yörünge Sürüklenmesi şeridi her platformun medyan yerel geçiş saatini nominal saatiyle karşılaştırır. Medyanlar bölgesel göstergelerdir, ekvator geçiş saati değildir.",
} as const;
