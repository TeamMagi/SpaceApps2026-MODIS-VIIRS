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
  countRawSummary: "{raw} detections",
  countRawSubline: "{modis} MODIS + {viirs} VIIRS",
  countSummary: "{raw} detections → {unique} harmonized detections",
  countSubline: "{dup} duplicate detections removed ({pct})",
  countOpenReceipt: "Show how we got this number",

  // Header
  navMethods: "Methods",
  modeBadgeArchive: "FIRMS archive",
  modeBadgeArchiveYears: "FIRMS archive · {first}–{last}",

  // Correction receipt
  receiptTitle: "From raw to harmonized detections",
  receiptRaw: "Raw detections",
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
  pipelineStepAngle: "Correct viewing angle",
  pipelineStepGrid: "Align to one grid",
  pipelineStepMatch: "Match across sensors",
  pipelineStepFuse: "Keep the best view",
  pipelineStepCalendar: "Build the calendar",
  pipelineTechCollect: "FIRMS",
  pipelineTechDedup: "Dedup",
  pipelineTechAngle: "VZA",
  pipelineTechGrid: "H3",
  pipelineTechMatch: "Cross-sensor",
  pipelineTechFuse: "Fusion",
  pipelineTechCalendar: "Monthly",
  pipelinePlannedBadge: "planned",
  pipelineCountIdle: "0/6 applied · 1 planned",
  pipelineCountDone: "6/6 applied · 1 planned",
  pipelineCount: "{n}/6 applied · 1 planned",
  pipelineProgressCollect: "Collecting detections from archive…",
  pipelineProgressDedup: "Removing double-counts…",
  pipelineProgressAngle: "Correcting viewing angle…",
  pipelineProgressGrid: "Aligning to one grid…",
  pipelineProgressMatch: "Matching across sensors…",
  pipelineProgressFuse: "Keeping the best view…",
  pipelineProgressCalendar: "Building the calendar…",
  pipelineProgressTotal: "All stages complete · {removed} total",
  pipelineSplit: "{dedup} interscan, {match} cross-sensor",

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
  calendarTitle: "Burn calendar · {years} years × 12 months",
  calendarLabel: "Burn calendar, detections per month",
  calendarTooltip: "{date} · {count} detections · {anomaly} ({sigma})",
  calendarNoAnomalyCell: "{date} · {count} detections",
  calendarEmptyCell: "{month} · 0 detections",
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
  cliffMarker2012: "{year} · VIIRS added",
  cliffLegendRaw: "Raw detections",
  cliffLegendHarmonized: "Harmonized detections",
  cliffLegendModis: "MODIS alone",
  cliffLegendPartial: "Partial year",
  cliffTableYear: "Year",
  cliffPartialYear: "{year} (partial)",

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
  exportCsv: "Download this view (CSV)",
  briefMe: "Brief me on this region",

  // Methods drawer
  methodsTitle: "How three satellites' detections are harmonized",
  methodsCliffDef:
    "Sensor cliff: a jump in detection counts that coincides with a new sensor joining the record; part of it can come from the sensor, not from more fires.",
  methodsSensors: "Satellites in archive: MODIS (Terra, Aqua) and VIIRS (Suomi NPP)",
  methodsStepsTitle: "Six applied steps · one planned",
  methodsStepCollect: "Every MODIS and VIIRS detection for the region, from the NASA FIRMS archive.",
  methodsStepDedup: "One sensor sometimes sees a fire in two overlapping scans. We keep one.",
  methodsStepAngle: "Fires seen at the edge of a scan look weaker. We correct FRP for the viewing angle.",
  methodsStepGrid: "Planned (not yet applied): place all detections on one shared hexagonal grid (H3) before matching.",
  methodsStepMatch: "When MODIS and VIIRS see the same fire at about the same time, we count it once.",
  methodsStepFuse: "From each matched pair we keep the sharpest observation.",
  methodsStepCalendar: "Harmonized detections are counted per month across the archive.",
  methodsLimitsTitle: "Limits",
  methodsSettingsTitle: "Display",
  themeLabel: "Theme",
  themeDark: "Dark",
  themeLight: "Light",
  presentationMode: "Presentation mode",
  presentationModeHint: "Press P to turn it on or off",
  angleDegrees: "{value}°",
  methodsSources: "NASA FIRMS archive · MODIS C6.1 · VIIRS S-NPP",
  methodsSourceCitation:
    "NASA FIRMS archive · MODIS C6.1 ({modisFirst}–{modisLast}) · VIIRS S-NPP ({viirsFirst}–{viirsLast})",
  methodsSourceCitationModis: "NASA FIRMS archive · MODIS C6.1 ({modisFirst}–{modisLast})",
  methodsSourceCitationFallback: "NASA FIRMS archive · MODIS C6.1 / VIIRS S-NPP",

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
  nrtLiveSummary: "Live: {raw} → {harmonized}",
  nrtTransactions: "{n} API calls",
} as const;

const en = { ...enBase, ...mapCopyEn, ...regionCopyEn, ...stateCopyEn } as const;

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

  countRawSummary: "{raw} tespit",
  countRawSubline: "{modis} MODIS + {viirs} VIIRS",
  countSummary: "{raw} tespit → {unique} harmonize tespit",
  countSubline: "{dup} mükerrer tespit ayıklandı ({pct})",
  countOpenReceipt: "Bu sayıya nasıl ulaştığımızı göster",

  navMethods: "Yöntemler",
  modeBadgeArchive: "FIRMS arşivi",
  modeBadgeArchiveYears: "FIRMS arşivi · {first}–{last}",

  receiptTitle: "Ham tespitten harmonize tespite",
  receiptRaw: "Ham tespitler",
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
  pipelineStepAngle: "Bakış açısını düzelt",
  pipelineStepGrid: "Tek ızgaraya oturt",
  pipelineStepMatch: "Sensörleri eşleştir",
  pipelineStepFuse: "En iyi gözlemi tut",
  pipelineStepCalendar: "Takvimi kur",
  pipelineTechCollect: "FIRMS",
  pipelineTechDedup: "Dedup",
  pipelineTechAngle: "VZA",
  pipelineTechGrid: "H3",
  pipelineTechMatch: "Sensörler arası",
  pipelineTechFuse: "Füzyon",
  pipelineTechCalendar: "Aylık",
  pipelinePlannedBadge: "planlanan",
  pipelineCountIdle: "0/6 uygulandı · 1 planlanan",
  pipelineCountDone: "6/6 uygulandı · 1 planlanan",
  pipelineCount: "{n}/6 uygulandı · 1 planlanan",
  pipelineProgressCollect: "Arşivden tespitler toplanıyor…",
  pipelineProgressDedup: "Çift sayım ayıklanıyor…",
  pipelineProgressAngle: "Bakış açısı düzeltiliyor…",
  pipelineProgressGrid: "Tek ızgaraya oturtuluyor…",
  pipelineProgressMatch: "Sensörler eşleştiriliyor…",
  pipelineProgressFuse: "En iyi gözlem tutuluyor…",
  pipelineProgressCalendar: "Takvim kuruluyor…",
  pipelineProgressTotal: "Tüm aşamalar tamamlandı · toplam {removed}",
  pipelineSplit: "{dedup} tarama çakışması, {match} sensörler arası",

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

  calendarTitle: "Yanma takvimi · {years} yıl × 12 ay",
  calendarLabel: "Yanma takvimi, aylık tespit sayısı",
  calendarTooltip: "{date} · {count} tespit · {anomaly} ({sigma})",
  calendarNoAnomalyCell: "{date} · {count} tespit",
  calendarEmptyCell: "{month} · 0 tespit",
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
  cliffMarker2012: "{year} · VIIRS eklendi",
  cliffLegendRaw: "Ham tespitler",
  cliffLegendHarmonized: "Harmonize tespit",
  cliffLegendModis: "Yalnız MODIS",
  cliffLegendPartial: "Kısmi yıl",
  cliffTableYear: "Yıl",
  cliffPartialYear: "{year} (kısmi)",

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
  exportCsv: "Bu görünümü indir (CSV)",
  briefMe: "Bu bölgeyi özetle",

  methodsTitle: "Üç uydunun tespitleri nasıl harmonize edilir",
  methodsCliffDef:
    "Sensör uçurumu: kayda yeni bir sensörün katılmasıyla aynı döneme denk gelen tespit sayısı sıçraması; bunun bir kısmı daha fazla yangından değil, sensörden kaynaklanabilir.",
  methodsSensors: "Arşivdeki uydular: MODIS (Terra, Aqua) ve VIIRS (Suomi NPP)",
  methodsStepsTitle: "Altı uygulanan adım · bir planlanan",
  methodsStepCollect: "Bölgedeki tüm MODIS ve VIIRS tespitleri, NASA FIRMS arşivinden.",
  methodsStepDedup: "Bir sensör bazen tek yangını üst üste binen iki taramada görür. Birini tutarız.",
  methodsStepAngle: "Tarama kenarında görülen yangınlar daha zayıf görünür. FRP'yi bakış açısına göre düzeltiriz.",
  methodsStepGrid: "Planlanan (henüz uygulanmadı): eşleştirmeden önce tüm tespitleri ortak bir altıgen ızgaraya (H3) yerleştirmek.",
  methodsStepMatch: "MODIS ve VIIRS aynı yangını yaklaşık aynı anda görürse onu bir kez sayarız.",
  methodsStepFuse: "Eşleşen her çiftten en net gözlemi tutarız.",
  methodsStepCalendar: "Harmonize tespitler arşiv boyunca ay ay toplanır.",
  methodsLimitsTitle: "Sınırlar",
  methodsSettingsTitle: "Görünüm",
  themeLabel: "Tema",
  themeDark: "Koyu",
  themeLight: "Açık",
  presentationMode: "Sunum modu",
  presentationModeHint: "Açıp kapatmak için P'ye bas",
  angleDegrees: "{value}°",
  methodsSources: "NASA FIRMS arşivi · MODIS C6.1 · VIIRS S-NPP",
  methodsSourceCitation:
    "NASA FIRMS arşivi · MODIS C6.1 ({modisFirst}–{modisLast}) · VIIRS S-NPP ({viirsFirst}–{viirsLast})",
  methodsSourceCitationModis: "NASA FIRMS arşivi · MODIS C6.1 ({modisFirst}–{modisLast})",
  methodsSourceCitationFallback: "NASA FIRMS arşivi · MODIS C6.1 / VIIRS S-NPP",

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
  nrtLiveSummary: "Canlı: {raw} → {harmonized}",
  nrtTransactions: "{n} API çağrısı",
} as const;

const tr: CopyDictionary = { ...trBase, ...mapCopyTr, ...regionCopyTr, ...stateCopyTr };

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
  angle: { name: "pipelineStepAngle", tech: "pipelineTechAngle", progress: "pipelineProgressAngle", method: "methodsStepAngle" },
  grid: { name: "pipelineStepGrid", tech: "pipelineTechGrid", progress: "pipelineProgressGrid", method: "methodsStepGrid" },
  match: { name: "pipelineStepMatch", tech: "pipelineTechMatch", progress: "pipelineProgressMatch", method: "methodsStepMatch" },
  fuse: { name: "pipelineStepFuse", tech: "pipelineTechFuse", progress: "pipelineProgressFuse", method: "methodsStepFuse" },
  calendar: { name: "pipelineStepCalendar", tech: "pipelineTechCalendar", progress: "pipelineProgressCalendar", method: "methodsStepCalendar" },
} as const satisfies Record<PipelineStageKey, { name: CopyKey; tech: CopyKey; progress: CopyKey; method: CopyKey }>;
