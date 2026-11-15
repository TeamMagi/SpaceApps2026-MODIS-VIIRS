/**
 * Every user-facing string, EN + TR.
 *
 * - `en` defines the key set; `tr` must implement exactly the same keys (a missing or extra
 *   key is a compile error).
 * - Placeholders are `{name}`. Their names are inferred from the EN string, so
 *   `t(lang, "countSummary", { raw, unique })` is type-checked.
 * - Numbers passed as vars are formatted for the language (e.g. 1,200 / 1.200). Pass strings for
 *   anything already formatted (percentages, signed values, dates).
 * - TR templates may use `{name|abl}`, `{name|poss}`, or `{name|loc}` to append the correct Turkish case
 *   suffix to the interpolated number ("45,2'den", "2012'de").
 */

import type { ApiErrorInfo, Lang, PipelineStageKey } from "../types";
import { formatNumber } from "../lib/format";
import { trSuffix, type TurkishSuffix } from "../lib/turkish";
import { mapCopyEn, mapCopyTr } from "./copyMap";
import { regionCopyEn, regionCopyTr } from "./copyRegion";
import { stateCopyEn, stateCopyTr } from "./copyState";
import { analyticsCopyEn, analyticsCopyTr } from "./copyAnalytics";
import { methodsPageCopyEn, methodsPageCopyTr } from "./copyMethods";
import { tourCopyEn, tourCopyTr } from "./copyTour";
import { fusionCopyEn, fusionCopyTr } from "./copyFusion";
import { briefingCopyEn, briefingCopyTr } from "./copyBriefing";

export const TR_REGION_NAMES: Readonly<Record<string, string>> = {
  Turkey: "Türkiye",
  Greece: "Yunanistan",
  Bulgaria: "Bulgaristan",
  Cyprus: "Kıbrıs",
  Georgia: "Gürcistan",
  Armenia: "Ermenistan",
  Azerbaijan: "Azerbaycan",
  Iran: "İran",
  Iraq: "Irak",
  Syria: "Suriye",
  Russian_Federation: "Rusya Federasyonu",
};

export function formatRegionName(region: { id: string; name: string }, lang: Lang): string {
  if (lang === "tr") {
    const mapped = TR_REGION_NAMES[region.id] ?? TR_REGION_NAMES[region.name];
    if (mapped) return mapped;
  }
  return region.name.replace(/_/g, " ");
}

const enBase = {
  // App
  appName: "PyroSync",
  appTagline: "MODIS + VIIRS fire record, harmonized",
  regionPickerLabel: "Region",
  langSwitchLabel: "Language",
  langEn: "EN",
  langTr: "TR",
  langEnName: "English",
  langTrName: "Türkçe",

  // Command row: headline
  headlineRaw: "Detections jumped {x}% in {year}. More fires, or more satellites?",
  headlineRawDropped: "Detections dropped {x}% in {year}. Fewer fires, or different satellites?",
  headlineHarmonized: "After harmonization: {pct} change from {from} to {to}",
  headlineNoCliff: "No sensor transition in this region's record",
  headlineNoBaseline: "No detections in {from} to compare with, so the change into {to} cannot be shown as a percentage",
  headlinePartialNote: "{year} is a partial year for {sensor}",
  headlineLoading: "Loading archive fire record…",
  headlineEmpty: "No fire detections in the archive for this region.",
  headlineError: "Could not load the fire archive.",

  // Command row: toggle
  toggleRaw: "Raw · as the satellites saw it",
  toggleHarmonized: "Harmonized · duplicates removed",
  toggleRawLabel: "Raw",
  toggleRawSub: "as the satellites saw it",
  toggleHarmonizedLabel: "Harmonized",
  toggleHarmonizedSub: "duplicates removed",
  toggleAriaLabel: "Harmonize the fire record",
  toggleHint: "Press H to harmonize",
  toggleHintRaw: "Press H to view raw",

  // Command row: count delta (Raw mode must not leak the harmonized answer)
  countScopeRange: "All years · {first}–{last}",
  countScopeAll: "All years",
  countRawSummary: "{raw} detections",
  countRawSubline: "{modis} MODIS + {viirs} VIIRS",
  countSummary: "{raw} detections → {unique} harmonized detections",
  countSubline: "{dup} duplicate detections removed ({pct})",
  countOpenReceipt: "Show how we got this number",

  // Header
  navMethods: "Methods",
  navSettings: "Settings",
  modeBadgeArchive: "FIRMS archive",
  modeBadgeArchiveYears: "FIRMS archive · {first}–{last}",
  modeBadgeDemo: "Demo data (synthetic)",
  modeBadgeToDemoAria: "Switch to synthetic demo data",
  modeBadgeToLiveAria: "Try switching to live server",
  modeBadgeChecking: "Checking server…",
  modeBadgeStayDemoNotice: "Server not reachable, staying on demo data",
  demoSourceShort: "Synthetic demo data",

  // Correction receipt
  receiptTitle: "From raw to harmonized detections",
  receiptRaw: "Raw detections",
  receiptStaticMasked: "Static sources / gas flares removed",
  receiptSamaRemoved: "SAMA night noise removed",
  receiptDedup: "− Duplicate detections removed",
  receiptUnique: "= Harmonized detections",
  receiptAngle: "FRP corrected for angle",
  receiptAngleValue: "{pct} avg",
  receiptStagesTitle: "By stage (archive total)",
  receiptPeriodTitle: "Shown period ({period})",

  // Pipeline
  pipelineSteps:
    "Collect · Remove double-counts · Correct viewing angle · Align to one grid (planned) · Match across sensors · Keep the best view · Build the calendar",
  pipelineLabel: "Harmonization steps",
  pipelineStepCollect: "Collect",
  pipelineStepDedup: "Remove double-counts",
  pipelineStepFleet: "Fleet consolidation",
  pipelineStepAngle: "Correct viewing angle",
  pipelineStepGrid: "Align to one grid",
  pipelineStepMatch: "Match across sensors",
  pipelineStepFuse: "Keep the best view",
  pipelineStepCalendar: "Build the calendar",
  pipelineTechCollect: "FIRMS",
  pipelineTechDedup: "Dedup",
  pipelineTechFleet: "Terra/Aqua · S-NPP/NOAA-20/21",
  pipelineTechAngle: "VZA",
  pipelineTechGrid: "H3",
  pipelineTechMatch: "Cross-sensor",
  pipelineTechFuse: "Fusion",
  pipelineTechCalendar: "Monthly",
  pipelinePlannedBadge: "planned",
  pipelineCountIdle: "No corrections applied yet",
  pipelineCountDone: "6 of 6 corrections applied",
  pipelineCount: "{n} of 6 corrections applied",
  pipelineCountShortDone: "6 of 6 corrections",
  pipelineCountShort: "{n} of 6 corrections",
  pipelineSummaryCompact: "{count} · {removed} detections",
  pipelineProgressCollect: "Collecting detections from archive…",
  pipelineProgressDedup: "Removing double-counts…",
  pipelineProgressFleet: "Merging platforms of the same sensor…",
  pipelineProgressAngle: "Correcting viewing angle…",
  pipelineProgressGrid: "Aligning to one grid…",
  pipelineProgressMatch: "Matching across sensors…",
  pipelineProgressFuse: "Keeping the best view…",
  pipelineProgressCalendar: "Building the calendar…",
  pipelineProgressTotal: "All stages complete · {removed} total",
  pipelineSplit: "{dedup} interscan, {fleet} fleet, {match} cross-sensor",

  // Map
  mapLabel: "Map of fire detections",
  mapPeriod: "Showing {period}",
  mapTruncatedNote: "showing a subset ({shown} of {total})",
  mapTooltip: "{frp} MW (corrected from {from}) · {sources} · {confidence} confidence · {date}",
  mapTooltipRaw: "{frp} MW · {sensor} · {confidence} confidence · {date}",
  mapLegendModis: "MODIS ({first}–{last})",
  mapLegendModisShort: "MODIS",
  mapLegendViirs: "VIIRS ({first}–{last})",
  mapLegendViirsShort: "VIIRS",
  mapLegendFrp: "Fire Radiative Power (MW)",
  mapLegendScaleQuantile: "Quantile",
  mapLegendScaleFixed: "Fixed",
  mapLegendFrpRange: "{min}–{max} MW",
  mapLegendFrpRangePlus: "{min}+ MW",
  mapLegendDuplicate: "Counted twice",
  mapMonthFilter: "Filtered: {month}",
  mapMonthFilterClear: "Clear",
  mapZoomIn: "Zoom in",
  mapZoomOut: "Zoom out",
  mapZoomReset: "Show whole region",
  mapZoomLabel: "Map zoom",
  latNorth: "{value}°N",
  latSouth: "{value}°S",
  lonEast: "{value}°E",
  lonWest: "{value}°W",
  sourceMatched: "MODIS + VIIRS",
  sourceModisOnly: "MODIS only",
  sourceViirsOnly: "VIIRS only",
  confidenceHigh: "high",
  confidenceNominal: "nominal",
  confidenceLow: "low",

  // Calendar
  calendarHeading: "Burn calendar",
  calendarClockRange: "year clock · {first}–{last}",
  calendarClockSub: "year clock",
  calendarTitle: "Burn calendar · {years} years × 12 months",
  calendarLabel: "Burn calendar, detections per month",
  calendarWheelLabel: "Monthly fire detections for {year}",
  calendarYearDetections: "{count} detections",
  calendarYearScope: "in {year}",
  calendarPrevYear: "Previous year",
  calendarNextYear: "Next year",
  calendarPlayYears: "Play years",
  calendarPauseYears: "Pause",
  calendarDetailAboveNormal: "{date} · {pct}% above normal · {count} detections",
  calendarDetailBelowNormal: "{date} · {pct}% below normal · {count} detections",
  calendarDetailAtNormal: "{date} · normal · {count} detections",
  calendarDetailCountOnly: "{date} · {count} detections",
  calendarNormalRange: "{first}–{last} normal",
  calendarNormalLabel: "Historical normal",
  calendarBelowNormalShort: "below normal",
  calendarAboveNormalShort: "above normal",
  calendarSliceHint: "Click a month to show it on the map",
  calendarSwathGapNote:
    "Tropical swath gap: {share}% of this region lies within 30°S–30°N, where gaps between MODIS orbits ({km} km at the equator) miss about {low}–{high}% of VIIRS fire energy ({low}% annual, {high}% monthly). Years with VIIRS fill those gaps; MODIS-only years before 2012 are not corrected for it.",
  calendarSwathGapNoteCompensated:
    "Tropical swath gap: {share}% of this region lies within 30°S–30°N, where gaps between MODIS orbits ({km} km at the equator) miss about {low}–{high}% of VIIRS fire energy ({low}% annual, {high}% monthly). Years with VIIRS fill those gaps; MODIS-only fire energy before 2012 is scaled up by {scale}% for it, with a wider uncertainty band.",
  calendarTooltip: "{date} · {count} detections · {anomaly} ({sigma})",
  calendarNoAnomalyCell: "{date} · {count} detections",
  calendarEmptyCell: "{month} · 0 detections",
  calendarFrpEq: "{value} MW per detection (MODIS-equivalent)",
  calendarFrpTotal: "total FRP {value} MW",
  calendarFrpTotalBand: "total FRP {value} MW (likely {low}–{high})",
  calendarFrpNative: "native-pixel mean {value} MW",
  calendarFrpNativeBand: "native-pixel mean {value} MW (likely {low}–{high})",
  calendarNoRecord: "{month} · no satellite record",
  calendarSelectHint: "Select a month to filter the map",
  calendarLegendBelow: "Below normal",
  calendarLegendAbove: "Above normal",
  anomalyWellAbove: "well above normal for {month}",
  anomalyAbove: "above normal for {month}",
  anomalyNear: "close to normal for {month}",
  anomalyBelow: "below normal for {month}",
  anomalyWellBelow: "well below normal for {month}",

  // Cliff chart
  cliffChartTitle: "Detections over time · {first}–{last}",
  cliffChartLabel: "Fire detections per year",
  cliffSliderLabel: "Select year",
  cliffSliderHint: "Use arrow keys to pick a year",
  cliffSliderValue: "{year} · {count} detections",
  cliffMarker2012: "{year} · VIIRS added",
  cliffLegendRaw: "Raw detections",
  cliffLegendHarmonized: "Harmonized detections",
  cliffLegendModis: "MODIS alone",
  cliffLegendPartial: "Partial year",
  cliffTableYear: "Year",
  cliffPartialYear: "{year} (partial)",

  // Chart panel & FRP band chart
  chartViewSwitchLabel: "Chart metric",
  chartTabDetections: "Detections",
  chartTabMeanFrp: "Mean FRP",
  frpChartTitle: "Mean FRP per year · {first}–{last}",
  frpChartLabel: "Mean fire radiative power per year",
  frpLegendMean: "Mean FRP",
  frpRawCaption: "Interval shown in the Harmonized view",
  frpTooltip: "{year} · Mean FRP {value}",
  frpTooltipNoData: "{year} · no data",
  frpTableMean: "Mean FRP (MW)",
  frpTableLower: "Lower bound (MW)",
  frpTableUpper: "Upper bound (MW)",

  // States
  loadingFirms: "Loading archive detections…",
  loadingRegions: "Loading regions…",
  emptyState: "No fires detected here in this period. That is a result, not an error.",
  errorBadge: "Archive error",
  errorLoading: "Could not load archive data from the server.",
  errorApiUnreachable: "The PyroSync data service is not reachable. Start the backend and try again.",
  regionTooLarge: "Too large for this server ({size} MB)",
  regionTooLargeNoSize: "Too large for this server",
  retryAction: "Retry",
  reloadAction: "Reload",
  closeAction: "Close",

  // Bottom bar
  disclaimer: "Harmonized values are model estimates, not ground truth.",
  disclaimerLink: "See how we calculate them →",
  validationGatePass: "Validation gate: beats both baselines (LOYO error {error})",
  validationGateInconclusive: "Validation gate: inconclusive vs baseline",
  validationGateFail: "Validation gate: does not beat baseline - read with caution",
  validationGateUnavailable: "Not yet validated for this region",
  exportCsv: "Download this view (CSV)",
  exportCsvShort: "CSV",
  exportCsvDescription: "Downloads {file}",
  briefMe: "Brief me on this region",
  confidenceFilterOff: "Exclude low-confidence",
  confidenceFilterOn: "Low-confidence excluded",
  confidenceFilterDescription:
    "Drops FIRMS low-confidence detections before harmonizing. Applies to the map, the CSV and the live counts; the yearly series is precomputed and stays unfiltered.",
  confidenceFilterExcluded: "{count} removed from this view",

  // Methods drawer
  methodsTitle: "How MODIS and VIIRS detections are harmonized",
  methodsCliffDef:
    "Sensor cliff: a jump in detection counts that coincides with a new sensor joining the record; part of it can come from the sensor, not from more fires.",
  methodsSensors: "Satellites in archive: MODIS (Terra, Aqua) and VIIRS (Suomi NPP, NOAA-20)",
  methodsStepsTitle: "Six applied steps · one planned",
  methodsStepCollect: "Every MODIS and VIIRS detection for the region, from the NASA FIRMS archive.",
  methodsStepDedup: "One sensor sometimes sees a fire in two overlapping scans. We keep one.",
  methodsStepFleet: "The same fire seen by several platforms of one sensor family (Terra and Aqua, or S-NPP, NOAA-20 and NOAA-21) is merged into one detection.",
  methodsStepAngle: "Fires seen at the edge of a scan look weaker. We correct FRP for the viewing angle.",
  methodsStepGrid: "Planned (not yet applied): place all detections on one shared hexagonal grid (H3) before matching.",
  methodsStepMatch: "When MODIS and VIIRS see the same fire at about the same time, we count it once.",
  methodsStepFuse: "From each matched pair we keep the sharpest observation.",
  methodsStepCalendar: "Harmonized detections are counted per month across the archive.",
  uncertaintyConfidenceLabel: "±1σ (~68% confidence interval)",
  uncertaintyWideTitle: "Wide interval",
  uncertaintyWideBody: "Wide interval = large sensor viewing angle or limited calibration data.",
  uncertaintyRawNote: "Raw observation: no harmonization model interval applies.",
  uncertaintyModelCaveat:
    "Harmonized values are model estimates, not ground truth. Default sensor noise and viewing-angle weights are modeling assumptions.",
  methodsLimitsTitle: "Limits",
  methodsAtmosphereNote:
    "Smoke and cloud can only push FRP down. FIRMS carries no smoke optical depth or cloud fraction, so FRP may be underestimated by roughly 7–22%: thin smoke dims the signal, and clouds hide about 22% (MODIS) and 12% (VIIRS) of fire energy globally, about 7% in regional studies. This one-sided bias is not part of the uncertainty bands.",
  methodsLowConfidenceNote:
    "Low-confidence detections include false alarms, such as up to 40% on bright roofs in eastern China and 10% at a South African power plant. “Exclude low-confidence” in the bottom bar drops them before harmonizing; the yearly series stays unfiltered.",
  methodsSettingsTitle: "Display",
  themeLabel: "Theme",
  themeDark: "Dark",
  themeLight: "Light",
  paletteLabel: "Colour palette",
  paletteDefault: "Inferno (Default)",
  paletteDefaultDesc: "High-contrast thermal ramp",
  paletteViridis: "Viridis (Colorblind-safe)",
  paletteViridisDesc: "Perceptually uniform purple-to-yellow",
  paletteCividis: "Cividis (High CVD-safe)",
  paletteCividisDesc: "Optimized for red-green & achromatopsia",
  paletteNameDefault: "Inferno",
  paletteNameViridis: "Viridis",
  paletteNameCividis: "Cividis",
  presentationMode: "Presentation mode",
  presentationModeHint: "Press P to turn it on or off",
  angleDegrees: "{value}°",
  methodsSources: "NASA FIRMS archive · MODIS C6.1 · VIIRS S-NPP / NOAA-20",
  methodsSourceCitation:
    "NASA FIRMS archive · MODIS C6.1 ({modisFirst}–{modisLast}) · VIIRS S-NPP / NOAA-20 ({viirsFirst}–{viirsLast})",
  methodsSourceCitationModis: "NASA FIRMS archive · MODIS C6.1 ({modisFirst}–{modisLast})",
  methodsSourceCitationFallback: "NASA FIRMS archive · MODIS C6.1 / VIIRS S-NPP / NOAA-20",
  methodsSourceCitationDemo:
    "Synthetic demo data · 2021 Mediterranean cluster + 2000–2025 synthetic series (not NASA FIRMS data)",

  // Settings
  settingsTitle: "Settings",
  settingsNote:
    "Adjustments recompute the selected month’s map points and stage counts. The yearly chart and calendar always use server defaults.",
  settingsDedupTimeLabel: "Dedup time window",
  settingsDedupOverlapLabel: "Dedup overlap ratio",
  settingsMatchRadiusLabel: "Cross-sensor match radius",
  settingsMatchWindowLabel: "Cross-sensor match window",
  settingsFleetRadiusLabel: "Fleet merge radius",
  settingsHintSec: "Default {default} s · Range {min}–{max} s",
  settingsHintPct: "Default {default}% · Range {min}–{max}%",
  settingsHintKm: "Default {default} km · Range {min}–{max} km",
  settingsHintMin: "Default {default} min · Range {min}–{max} min",
  settingsUnitSec: "s",
  settingsUnitPct: "%",
  settingsUnitKm: "km",
  settingsUnitMin: "min",
  settingsValueSec: "{value} s",
  settingsValuePct: "{value}%",
  settingsValueKm: "{value} km",
  settingsValueMin: "{value} min",
  settingsNumberInputAria: "{label} ({unit})",
  settingsReset: "Reset to defaults",
  settingsCustomBadge: "Custom thresholds",
  settingsCustomBadgeAria: "Custom thresholds active. Open settings",

  // Briefing
  briefingTitle: "Regional briefing",
  briefingFinding: "In {region}, {pct} of raw detections were duplicates ({removed} of {raw}).",
  briefingMetricRaw: "Raw detections",
  briefingMetricSensors: "By satellite",
  briefingRawInvite: "Harmonize the record to remove interscan and cross-sensor duplicate detections.",
  briefingMetricUnique: "Harmonized detections",
  briefingMetricDouble: "Duplicates removed",
  briefingMetricJumpRaw: "{from}–{to} change, raw",
  briefingMetricJumpHarmonized: "{from}–{to} change, harmonized",
  briefingMetricPeak: "Busiest month on record",
  briefingMetricMeanAnnual: "Harmonized detections per year since {year}",
  briefingPrint: "Print this briefing",
  briefingCopyLink: "Copy link to this view",
  briefingLinkCopied: "Link copied",
  briefingSourceLive: "Source: {source}",
  briefingSourceDemo:
    "Source: Synthetic demo data (2021 Mediterranean cluster + 2000–2025 synthetic series; not NASA FIRMS data)",
  nrtLiveSummary: "Live: {raw} → {harmonized}",
  nrtDemoUnavailable: "Live feed: not available in demo mode",
  nrtVzaAdjustment: "view-angle correction: mean FRP {pct}",
  nrtTransactions: "{n} API calls",
} as const;

const en = { ...enBase, ...mapCopyEn, ...regionCopyEn, ...stateCopyEn, ...analyticsCopyEn, ...methodsPageCopyEn, ...tourCopyEn, ...fusionCopyEn, ...briefingCopyEn } as const;

export type CopyKey = keyof typeof en;
export type CopyDictionary = { readonly [K in CopyKey]: string };

const trBase = {
  appName: "PyroSync",
  appTagline: "MODIS + VIIRS yangın kaydı, harmonize",
  regionPickerLabel: "Bölge",
  langSwitchLabel: "Dil",
  langEn: "EN",
  langTr: "TR",
  langEnName: "English",
  langTrName: "Türkçe",

  headlineRaw: "{year|loc} tespitler %{x} arttı. Yangınlar mı çoğaldı, uydular mı?",
  headlineRawDropped: "{year|loc} tespitler %{x} düştü. Yangınlar mı azaldı, uydular mı değişti?",
  headlineHarmonized: "Harmonizasyon sonrası: {from}–{to} arasında {pct} değişim",
  headlineNoCliff: "Bu bölgenin kaydında sensör geçişi yok",
  headlineNoBaseline: "{from|loc} karşılaştırılacak tespit yok; {to} yılındaki değişim yüzde olarak gösterilemez",
  headlinePartialNote: "{year}, {sensor} için kısmi bir yıldır",
  headlineLoading: "Arşiv yangın kaydı yükleniyor…",
  headlineEmpty: "Bu bölge için arşivde yangın tespiti yok.",
  headlineError: "Yangın arşivi yüklenemedi.",

  toggleRaw: "Ham · uyduların gördüğü",
  toggleHarmonized: "Harmonize · mükerrerler ayıklanmış",
  toggleRawLabel: "Ham",
  toggleRawSub: "uyduların gördüğü",
  toggleHarmonizedLabel: "Harmonize",
  toggleHarmonizedSub: "mükerrerler ayıklanmış",
  toggleAriaLabel: "Yangın kaydını harmonize et",
  toggleHint: "Harmonize etmek için H'ye bas",
  toggleHintRaw: "Ham veriyi görmek için H'ye bas",

  countScopeRange: "Tüm yıllar · {first}–{last}",
  countScopeAll: "Tüm yıllar",
  countRawSummary: "{raw} tespit",
  countRawSubline: "{modis} MODIS + {viirs} VIIRS",
  countSummary: "{raw} tespit → {unique} harmonize tespit",
  countSubline: "{dup} mükerrer tespit ayıklandı ({pct})",
  countOpenReceipt: "Bu sayıya nasıl ulaştığımızı göster",

  navMethods: "Yöntemler",
  navSettings: "Ayarlar",
  modeBadgeArchive: "FIRMS arşivi",
  modeBadgeArchiveYears: "FIRMS arşivi · {first}–{last}",
  modeBadgeDemo: "Demo veri (sentetik)",
  modeBadgeToDemoAria: "Sentetik demo veriye geç",
  modeBadgeToLiveAria: "Canlı sunucuya geçmeyi dene",
  modeBadgeChecking: "Sunucu kontrol ediliyor…",
  modeBadgeStayDemoNotice: "Sunucuya ulaşılamıyor, demo veride kalınıyor",
  demoSourceShort: "Sentetik demo verisi",

  receiptTitle: "Ham tespitten harmonize tespite",
  receiptRaw: "Ham tespitler",
  receiptStaticMasked: "Sabit kaynaklar / gaz meşaleleri çıkarıldı",
  receiptSamaRemoved: "SAMA gece gürültüsü çıkarıldı",
  receiptDedup: "− Ayıklanan mükerrer tespitler",
  receiptUnique: "= Harmonize tespit",
  receiptAngle: "Bakış açısına göre düzeltilmiş FRP",
  receiptAngleValue: "ort. {pct}",
  receiptStagesTitle: "Aşamalara göre (arşiv toplamı)",
  receiptPeriodTitle: "Gösterilen dönem ({period})",

  pipelineSteps:
    "Topla · Çift sayımı ayıkla · Bakış açısını düzelt · Tek ızgaraya oturt (planlanan) · Sensörleri eşleştir · En iyi gözlemi tut · Takvimi kur",
  pipelineLabel: "Harmonizasyon adımları",
  pipelineStepCollect: "Topla",
  pipelineStepDedup: "Çift sayımı ayıkla",
  pipelineStepFleet: "Filo birleştirme",
  pipelineStepAngle: "Bakış açısını düzelt",
  pipelineStepGrid: "Tek ızgaraya oturt",
  pipelineStepMatch: "Sensörleri eşleştir",
  pipelineStepFuse: "En iyi gözlemi tut",
  pipelineStepCalendar: "Takvimi kur",
  pipelineTechCollect: "FIRMS",
  pipelineTechFleet: "Terra/Aqua · S-NPP/NOAA-20/21",
  pipelineTechDedup: "Dedup",
  pipelineTechAngle: "VZA",
  pipelineTechGrid: "H3",
  pipelineTechMatch: "Sensörler arası",
  pipelineTechFuse: "Füzyon",
  pipelineTechCalendar: "Aylık",
  pipelinePlannedBadge: "planlanan",
  pipelineCountIdle: "Henüz düzeltme uygulanmadı",
  pipelineCountDone: "6 düzeltmenin 6'sı uygulandı",
  pipelineCount: "6 düzeltmenin {n|poss} uygulandı",
  pipelineCountShortDone: "6/6 düzeltme",
  pipelineCountShort: "{n}/6 düzeltme",
  pipelineSummaryCompact: "{count} · {removed} tespit",
  pipelineProgressCollect: "Arşivden tespitler toplanıyor…",
  pipelineProgressDedup: "Çift sayım ayıklanıyor…",
  pipelineProgressFleet: "Aynı sensörün platformları birleştiriliyor…",
  pipelineProgressAngle: "Bakış açısı düzeltiliyor…",
  pipelineProgressGrid: "Tek ızgaraya oturtuluyor…",
  pipelineProgressMatch: "Sensörler eşleştiriliyor…",
  pipelineProgressFuse: "En iyi gözlem tutuluyor…",
  pipelineProgressCalendar: "Takvim kuruluyor…",
  pipelineProgressTotal: "Tüm aşamalar tamamlandı · toplam {removed}",
  pipelineSplit: "{dedup} tarama çakışması, {fleet} filo, {match} sensörler arası",

  mapLabel: "Yangın tespitleri haritası",
  mapPeriod: "Gösterilen dönem: {period}",
  mapTruncatedNote: "alt küme gösteriliyor ({total} tespitten {shown})",
  mapTooltip: "{frp} MW ({from|abl} düzeltildi) · {sources} · {confidence} güven · {date}",
  mapTooltipRaw: "{frp} MW · {sensor} · {confidence} güven · {date}",
  mapLegendModis: "MODIS ({first}–{last})",
  mapLegendModisShort: "MODIS",
  mapLegendViirs: "VIIRS ({first}–{last})",
  mapLegendViirsShort: "VIIRS",
  mapLegendFrp: "Yangın Işınım Gücü (MW)",
  mapLegendScaleQuantile: "Kuantil",
  mapLegendScaleFixed: "Sabit",
  mapLegendFrpRange: "{min}–{max} MW",
  mapLegendFrpRangePlus: "{min}+ MW",
  mapLegendDuplicate: "İki kez sayılan",
  mapMonthFilter: "Filtre: {month}",
  mapMonthFilterClear: "Temizle",
  mapZoomIn: "Yakınlaştır",
  mapZoomOut: "Uzaklaştır",
  mapZoomReset: "Tüm bölgeyi göster",
  mapZoomLabel: "Harita yakınlaştırma",
  latNorth: "{value}°K",
  latSouth: "{value}°G",
  lonEast: "{value}°D",
  lonWest: "{value}°B",
  sourceMatched: "MODIS + VIIRS",
  sourceModisOnly: "Yalnız MODIS",
  sourceViirsOnly: "Yalnız VIIRS",
  confidenceHigh: "yüksek",
  confidenceNominal: "normal",
  confidenceLow: "düşük",

  calendarHeading: "Yanma takvimi",
  calendarClockRange: "yıl saati · {first}–{last}",
  calendarClockSub: "yıl saati",
  calendarTitle: "Yanma takvimi · {years} yıl × 12 ay",
  calendarLabel: "Yanma takvimi, aylık tespit sayısı",
  calendarWheelLabel: "{year} yılı aylık yangın tespitleri",
  calendarYearDetections: "{count} tespit",
  calendarYearScope: "{year|loc}",
  calendarPrevYear: "Önceki yıl",
  calendarNextYear: "Sonraki yıl",
  calendarPlayYears: "Yılları oynat",
  calendarPauseYears: "Duraklat",
  calendarDetailAboveNormal: "{date} · normalinden %{pct} fazla · {count} tespit",
  calendarDetailBelowNormal: "{date} · normalinden %{pct} az · {count} tespit",
  calendarDetailAtNormal: "{date} · normalinde · {count} tespit",
  calendarDetailCountOnly: "{date} · {count} tespit",
  calendarNormalRange: "{first}–{last} normali",
  calendarNormalLabel: "Tarihsel normal",
  calendarBelowNormalShort: "normal altı",
  calendarAboveNormalShort: "normal üstü",
  calendarSliceHint: "Haritada görmek için bir aya tıkla",
  calendarSwathGapNote:
    "Tropikal swath boşluğu: Bu bölgenin %{share|poss} 30°G–30°K kuşağında; burada MODIS yörüngeleri arasındaki boşluklar (ekvatorda {km} km) VIIRS yangın enerjisinde yaklaşık %{low}–{high} (yıllık %{low}, aylık %{high}) kayba yol açar. VIIRS'li yıllar bu boşlukları doldurur; 2012 öncesi yalnız MODIS yılları bunun için düzeltilmez.",
  calendarSwathGapNoteCompensated:
    "Tropikal swath boşluğu: Bu bölgenin %{share|poss} 30°G–30°K kuşağında; burada MODIS yörüngeleri arasındaki boşluklar (ekvatorda {km} km) VIIRS yangın enerjisinde yaklaşık %{low}–{high} (yıllık %{low}, aylık %{high}) kayba yol açar. VIIRS'li yıllar bu boşlukları doldurur; 2012 öncesi yalnız MODIS yangın enerjisi bunun için %{scale} artırılır ve belirsizlik bandı genişletilir.",
  calendarTooltip: "{date} · {count} tespit · {anomaly} ({sigma})",
  calendarNoAnomalyCell: "{date} · {count} tespit",
  calendarEmptyCell: "{month} · 0 tespit",
  calendarFrpEq: "tespit başına {value} MW (MODIS eşdeğeri)",
  calendarFrpTotal: "toplam FRP {value} MW",
  calendarFrpTotalBand: "toplam FRP {value} MW (olası aralık {low}–{high})",
  calendarFrpNative: "yerel piksel ortalaması {value} MW",
  calendarFrpNativeBand: "yerel piksel ortalaması {value} MW (olası aralık {low}–{high})",
  calendarNoRecord: "{month} · uydu kaydı yok",
  calendarSelectHint: "Haritayı süzmek için bir ay seç",
  calendarLegendBelow: "Normalin altında",
  calendarLegendAbove: "Normalin üstünde",
  anomalyWellAbove: "{month} normalinin çok üstünde",
  anomalyAbove: "{month} normalinin üstünde",
  anomalyNear: "{month} için normale yakın",
  anomalyBelow: "{month} normalinin altında",
  anomalyWellBelow: "{month} normalinin çok altında",

  cliffChartTitle: "Yıllara göre tespitler · {first}–{last}",
  cliffChartLabel: "Yıllık yangın tespitleri",
  cliffSliderLabel: "Yıl seç",
  cliffSliderHint: "Yıl seçmek için ok tuşlarını kullan",
  cliffSliderValue: "{year} · {count} tespit",
  cliffMarker2012: "{year} · VIIRS eklendi",
  cliffLegendRaw: "Ham tespitler",
  cliffLegendHarmonized: "Harmonize tespit",
  cliffLegendModis: "Yalnız MODIS",
  cliffLegendPartial: "Kısmi yıl",
  cliffTableYear: "Yıl",
  cliffPartialYear: "{year} (kısmi)",

  chartViewSwitchLabel: "Grafik metriği",
  chartTabDetections: "Tespitler",
  chartTabMeanFrp: "Ortalama FRP",
  frpChartTitle: "Yıllık ortalama FRP · {first}–{last}",
  frpChartLabel: "Yıllık ortalama yangın ışınım gücü",
  frpLegendMean: "Ortalama FRP",
  frpRawCaption: "Aralık Harmonize görünümde gösterilir",
  frpTooltip: "{year} · Ortalama FRP {value}",
  frpTooltipNoData: "{year} · veri yok",
  frpTableMean: "Ortalama FRP (MW)",
  frpTableLower: "Alt sınır (MW)",
  frpTableUpper: "Üst sınır (MW)",

  loadingFirms: "Arşiv tespitleri yükleniyor…",
  loadingRegions: "Bölgeler yükleniyor…",
  emptyState: "Bu dönemde burada yangın tespit edilmedi. Bu bir hata değil, bir sonuç.",
  errorBadge: "Arşiv hatası",
  errorLoading: "Sunucudan arşiv verisi yüklenemedi.",
  errorApiUnreachable: "PyroSync veri servisine ulaşılamıyor. Backend'i başlatıp tekrar deneyin.",
  regionTooLarge: "Bu sunucu için çok büyük ({size} MB)",
  regionTooLargeNoSize: "Bu sunucu için çok büyük",
  retryAction: "Tekrar dene",
  reloadAction: "Yenile",
  closeAction: "Kapat",

  disclaimer: "Harmonize değerler model tahminidir, yer gerçeği değildir.",
  disclaimerLink: "Nasıl hesapladığımızı gör →",
  validationGatePass: "Doğrulama sonucu: her iki temel yöntemden daha başarılı (LOYO hatası {error})",
  validationGateInconclusive: "Doğrulama sonucu: temel yöntemlere göre üstünlük belirsiz",
  validationGateFail: "Doğrulama sonucu: temel yöntemden daha başarılı değil - dikkatle yorumlayın",
  validationGateUnavailable: "Bu bölge için henüz doğrulama yapılmadı",
  exportCsv: "Bu görünümü indir (CSV)",
  exportCsvShort: "CSV",
  exportCsvDescription: "{file} dosyasını indirir",
  briefMe: "Bu bölgeyi özetle",
  confidenceFilterOff: "Düşük güveni hariç tut",
  confidenceFilterOn: "Düşük güven hariç",
  confidenceFilterDescription:
    "FIRMS düşük güvenli tespitlerini harmonizasyondan önce çıkarır. Haritaya, CSV'ye ve canlı sayılara uygulanır; yıllık seri önceden hesaplandığı için filtresiz kalır.",
  confidenceFilterExcluded: "Bu görünümden {count} tespit çıkarıldı",

  methodsTitle: "MODIS ve VIIRS tespitleri nasıl harmonize edilir",
  methodsCliffDef:
    "Sensör uçurumu: kayda yeni bir sensörün katılmasıyla aynı döneme denk gelen tespit sayısı sıçraması; bunun bir kısmı daha fazla yangından değil, sensörden kaynaklanabilir.",
  methodsSensors: "Arşivdeki uydular: MODIS (Terra, Aqua) ve VIIRS (Suomi NPP, NOAA-20)",
  methodsStepsTitle: "Altı uygulanan adım · bir planlanan",
  methodsStepCollect: "Bölgedeki tüm MODIS ve VIIRS tespitleri, NASA FIRMS arşivinden.",
  methodsStepDedup: "Bir sensör bazen tek yangını üst üste binen iki taramada görür. Birini tutarız.",
  methodsStepFleet: "Aynı sensör ailesinin birden çok platformunun (Terra ve Aqua ya da S-NPP, NOAA-20 ve NOAA-21) gördüğü aynı yangın tek tespitte birleştirilir.",
  methodsStepAngle: "Tarama kenarında görülen yangınlar daha zayıf görünür. FRP'yi bakış açısına göre düzeltiriz.",
  methodsStepGrid: "Planlanan (henüz uygulanmadı): eşleştirmeden önce tüm tespitleri ortak bir altıgen ızgaraya (H3) yerleştirmek.",
  methodsStepMatch: "MODIS ve VIIRS aynı yangını yaklaşık aynı anda görürse onu bir kez sayarız.",
  methodsStepFuse: "Eşleşen her çiftten en net gözlemi tutarız.",
  methodsStepCalendar: "Harmonize tespitler arşiv boyunca ay ay toplanır.",
  uncertaintyConfidenceLabel: "±1σ (~%68 güven aralığı)",
  uncertaintyWideTitle: "Geniş aralık",
  uncertaintyWideBody: "Geniş aralık = sensör açısı büyük veya kalibrasyon verisi az.",
  uncertaintyRawNote: "Ham gözlem: uyumlaştırma modeli aralığı uygulanmaz.",
  uncertaintyModelCaveat:
    "Harmonize değerler model tahminidir, yer gerçeği değildir. Varsayılan sensör gürültüsü ve bakış açısı ağırlıkları modelleme varsayımlarıdır.",
  methodsLimitsTitle: "Sınırlar",
  methodsAtmosphereNote:
    "Duman ve bulut FRP'yi yalnızca aşağı çekebilir. FIRMS duman optik derinliği veya bulut kesri taşımadığından FRP kabaca %7–22 düşük tahmin edilebilir: ince duman sinyali zayıflatır, bulutlar küresel ölçekte yangın enerjisinin yaklaşık %22'sini (MODIS) ve %12'sini (VIIRS), bölgesel çalışmalarda yaklaşık %7'sini gizler. Bu tek yönlü sapma belirsizlik bantlarına dahil değildir.",
  methodsLowConfidenceNote:
    "Düşük güvenli tespitler yanlış alarmlar içerir: Doğu Çin'deki parlak çatılarda %40'a, Güney Afrika'daki bir termik santralde %10'a varan oranlarda. Alt çubuktaki “Düşük güveni hariç tut” bunları harmonizasyondan önce çıkarır; yıllık seri filtresiz kalır.",
  methodsSettingsTitle: "Görünüm",
  themeLabel: "Tema",
  themeDark: "Koyu",
  themeLight: "Açık",
  paletteLabel: "Renk paleti",
  paletteDefault: "Inferno (Varsayılan)",
  paletteDefaultDesc: "Yüksek kontrastlı ısıl rampa",
  paletteViridis: "Viridis (Renk Körlüğü Uyumlu)",
  paletteViridisDesc: "Algısal olarak eşit mor–sarı rampa",
  paletteCividis: "Cividis (Yüksek CVD Uyumlu)",
  paletteCividisDesc: "Kırmızı-yeşil ve akromatopsi için optimize",
  paletteNameDefault: "Inferno",
  paletteNameViridis: "Viridis",
  paletteNameCividis: "Cividis",
  presentationMode: "Sunum modu",
  presentationModeHint: "Açıp kapatmak için P'ye bas",
  angleDegrees: "{value}°",
  methodsSources: "NASA FIRMS arşivi · MODIS C6.1 · VIIRS S-NPP / NOAA-20",
  methodsSourceCitation:
    "NASA FIRMS arşivi · MODIS C6.1 ({modisFirst}–{modisLast}) · VIIRS S-NPP / NOAA-20 ({viirsFirst}–{viirsLast})",
  methodsSourceCitationModis: "NASA FIRMS arşivi · MODIS C6.1 ({modisFirst}–{modisLast})",
  methodsSourceCitationFallback: "NASA FIRMS arşivi · MODIS C6.1 / VIIRS S-NPP / NOAA-20",
  methodsSourceCitationDemo:
    "Sentetik demo verisi · 2021 Akdeniz kümesi + 2000–2025 sentetik seri (NASA FIRMS verisi değildir)",

  settingsTitle: "Ayarlar",
  settingsNote:
    "Değişiklikler seçili ayın harita noktalarını ve aşama sayılarını yeniden hesaplar. Yıllık grafik ve takvim her zaman sunucu varsayılanlarını kullanır.",
  settingsDedupTimeLabel: "Çakışma zaman penceresi",
  settingsDedupOverlapLabel: "Çakışma örtüşme oranı",
  settingsMatchRadiusLabel: "Sensörler arası eşleme yarıçapı",
  settingsMatchWindowLabel: "Sensörler arası eşleme penceresi",
  settingsFleetRadiusLabel: "Filo birleştirme yarıçapı",
  settingsHintSec: "Varsayılan {default} sn · Aralık {min}–{max} sn",
  settingsHintPct: "Varsayılan %{default} · Aralık %{min}–%{max}",
  settingsHintKm: "Varsayılan {default} km · Aralık {min}–{max} km",
  settingsHintMin: "Varsayılan {default} dk · Aralık {min}–{max} dk",
  settingsUnitSec: "sn",
  settingsUnitPct: "%",
  settingsUnitKm: "km",
  settingsUnitMin: "dk",
  settingsValueSec: "{value} sn",
  settingsValuePct: "%{value}",
  settingsValueKm: "{value} km",
  settingsValueMin: "{value} dk",
  settingsNumberInputAria: "{label} ({unit})",
  settingsReset: "Varsayılana dön",
  settingsCustomBadge: "Özel eşikler",
  settingsCustomBadgeAria: "Özel eşikler etkin. Ayarları aç",

  briefingTitle: "Bölge brifingi",
  briefingFinding: "{region} bölgesinde ham tespitlerin {pct|poss} mükerrer tespitti ({raw} tespitten {removed}).",
  briefingMetricRaw: "Ham tespitler",
  briefingMetricSensors: "Uyduya göre",
  briefingRawInvite: "Tarama çakışmalarını ve sensörler arası mükerrer tespitleri ayıklamak için kaydı harmonize et.",
  briefingMetricUnique: "Harmonize tespit",
  briefingMetricDouble: "Ayıklanan mükerrer tespit",
  briefingMetricJumpRaw: "{from}–{to} değişimi, ham",
  briefingMetricJumpHarmonized: "{from}–{to} değişimi, harmonize",
  briefingMetricPeak: "Kayıttaki en yoğun ay",
  briefingMetricMeanAnnual: "{year}'den bu yana yıllık harmonize tespit",
  briefingPrint: "Bu brifingi yazdır",
  briefingCopyLink: "Bu görünümün bağlantısını kopyala",
  briefingLinkCopied: "Bağlantı kopyalandı",
  briefingSourceLive: "Kaynak: {source}",
  briefingSourceDemo:
    "Kaynak: Sentetik demo verisi (2021 Akdeniz kümesi + 2000–2025 sentetik seri; NASA FIRMS verisi değildir)",
  nrtLiveSummary: "Canlı: {raw} → {harmonized}",
  nrtDemoUnavailable: "Canlı akış: demo modunda kullanılamıyor",
  nrtVzaAdjustment: "bakış açısı düzeltmesi: ortalama FRP {pct}",
  nrtTransactions: "{n} API çağrısı",
} as const;

const tr: CopyDictionary = { ...trBase, ...mapCopyTr, ...regionCopyTr, ...stateCopyTr, ...analyticsCopyTr, ...methodsPageCopyTr, ...tourCopyTr, ...fusionCopyTr, ...briefingCopyTr };

export const copy: Readonly<Record<Lang, CopyDictionary>> = { en, tr };

type Placeholders<S extends string> = S extends `${string}{${infer Name}}${infer Rest}`
  ? Name | Placeholders<Rest>
  : never;

export type CopyVars<K extends CopyKey> = Readonly<Record<Placeholders<(typeof en)[K]>, string | number>>;

/** Rest-args tuple: no second argument for static strings, a typed vars object otherwise. */
export type CopyArgs<K extends CopyKey> = [Placeholders<(typeof en)[K]>] extends [never] ? [] : [vars: CopyVars<K>];

export type Translate = <K extends CopyKey>(key: K, ...args: CopyArgs<K>) => string;

const PLACEHOLDER = /\{(\w+)(?:\|(abl|poss|loc))?\}/g;

function interpolate(lang: Lang, template: string, vars: Readonly<Record<string, string | number>> | undefined): string {
  if (!vars) return template;
  return template.replace(PLACEHOLDER, (match, name: string, suffix: TurkishSuffix | undefined) => {
    const value = vars[name];
    if (value === undefined) return match;
    const text = typeof value === "number" ? formatNumber(lang, value, Number.isInteger(value) ? 0 : 1) : value;
    return suffix && lang === "tr" ? `${text}'${trSuffix(text, suffix)}` : text;
  });
}

export function t<K extends CopyKey>(lang: Lang, key: K, ...args: CopyArgs<K>): string {
  const vars = args[0] as Readonly<Record<string, string | number>> | undefined;
  return interpolate(lang, copy[lang][key], vars);
}

export function createTranslator(lang: Lang): Translate {
  return (key, ...args) => t(lang, key, ...args);
}

export function formatApiError(
  error: ApiErrorInfo | null | undefined,
  translate: Translate,
  fallbackKey: "errorLoading" | "headlineError" = "errorLoading",
): string {
  if (!error) return translate(fallbackKey);
  if (error.kind === "unreachable") return translate("errorApiUnreachable");
  return error.message || translate(fallbackKey);
}

/** Copy keys per pipeline stage, in pipeline order. None of them take vars. */
export const PIPELINE_COPY = {
  collect: { name: "pipelineStepCollect", tech: "pipelineTechCollect", progress: "pipelineProgressCollect", method: "methodsStepCollect" },
  dedup: { name: "pipelineStepDedup", tech: "pipelineTechDedup", progress: "pipelineProgressDedup", method: "methodsStepDedup" },
  fleet: { name: "pipelineStepFleet", tech: "pipelineTechFleet", progress: "pipelineProgressFleet", method: "methodsStepFleet" },
  angle: { name: "pipelineStepAngle", tech: "pipelineTechAngle", progress: "pipelineProgressAngle", method: "methodsStepAngle" },
  grid: { name: "pipelineStepGrid", tech: "pipelineTechGrid", progress: "pipelineProgressGrid", method: "methodsStepGrid" },
  match: { name: "pipelineStepMatch", tech: "pipelineTechMatch", progress: "pipelineProgressMatch", method: "methodsStepMatch" },
  fuse: { name: "pipelineStepFuse", tech: "pipelineTechFuse", progress: "pipelineProgressFuse", method: "methodsStepFuse" },
  calendar: { name: "pipelineStepCalendar", tech: "pipelineTechCalendar", progress: "pipelineProgressCalendar", method: "methodsStepCalendar" },
} as const satisfies Record<PipelineStageKey, { name: CopyKey; tech: CopyKey; progress: CopyKey; method: CopyKey }>;
