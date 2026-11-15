export enum Confidence {
  Low = "Low",
  Nominal = "Nominal",
  High = "High",
}

export enum DayNight {
  Day = "Day",
  Night = "Night",
}

export enum HotspotSource {
  ModisOnly = "ModisOnly",
  ViirsOnly = "ViirsOnly",
  Matched = "Matched",
}

export enum Sensor {
  ModisTerra = "ModisTerra",
  ModisAqua = "ModisAqua",
  ViirsSNPP = "ViirsSNPP",
  ViirsNOAA20 = "ViirsNOAA20",
  ViirsNOAA21 = "ViirsNOAA21",
}

export type ViewMode = "raw" | "harmonized";

/** Display input for summary cards; every value must describe the same scope. */
export interface AnalyticsSnapshot {
  scope: { region: string; year: number; month: number | null };
  rawCount: number;
  harmonizedCount: number;
  rawFrpSum: number;
  harmonizedFrpSum: number;
  platforms: readonly Sensor[];
  bySource: { modisOnly: number; viirsOnly: number; matched: number };
  generatedAt: string;
}

export type Lang = "en" | "tr";
export type Theme = "dark" | "light";

/** Phase of the Raw <-> Harmonized choreography. See hooks/useHarmonizeTransition. */
export type Phase = "idle" | "detect" | "merge" | "settle";

/** Phases that actually take time; `idle` is the resting state. */
export type ActivePhase = Exclude<Phase, "idle">;

export type PhaseDurations = Readonly<Record<ActivePhase, number>>;

/** Overlay panels. `receipt` is the correction receipt popover under the count. */
export type DrawerId = "methods" | "briefing" | "receipt";

/** Async fetch status exposed by data hooks. */
export type AsyncStatus = "loading" | "ready" | "empty" | "error";

export type ApiErrorKind = "unreachable" | "server";

export interface ApiErrorInfo {
  readonly kind: ApiErrorKind;
  readonly message: string;
}

export type MapState = AsyncStatus;

export type RegionId = string;

export interface BBox {
  west: number;
  south: number;
  east: number;
  north: number;
}

export interface RegionSummary {
  id: RegionId;
  name: string;
  modisYears: readonly [number, number] | null;
  viirsYears: readonly [number, number] | null;
  sizeMb: number | null;
  tooLarge: boolean;
}

export interface YearSpan {
  first: number;
  last: number;
}

export interface SeriesCoverage {
  modis: YearSpan | null;
  viirs: YearSpan | null;
}

export interface Region {
  id: RegionId;
  name: string;
  displayName: string;
  /** Short ASCII slug used in file names, e.g. `pyrosync_turkey_2021-08.csv`. */
  slug: string;
}

/** 1-based month reference. */
export interface MonthRef {
  year: number;
  month: number;
}

export type SensorFamily = "MODIS" | "VIIRS";

export interface RawPoint {
  id: string;
  lat: number;
  lon: number;
  sensor: SensorFamily;
  frp: number;
  confidence: Confidence;
  /** ISO 8601 / RFC3339 UTC timestamp. */
  acquired: string;
  daynight: DayNight;
  /** Id of the HarmonizedPoint this detection ended up in, or null when interscan dedup removed it. */
  harmonizedId: string | null;
}

export interface HarmonizedPoint {
  id: string;
  lat: number;
  lon: number;
  /** FRP of the kept observation before angle correction, MW. */
  frp: number;
  frpCorrected: number;
  source: HotspotSource;
  confidence: Confidence;
  acquired: string;
  daynight: DayNight;
  vzaFactor: number | null;
  nObs: number;
  platforms: string[];
}

export interface PointStages {
  rawModis: number;
  rawViirs: number;
  interscanRemoved: number;
  fleetMerged: number;
  matchedPairs: number;
  harmonized: number;
}

export interface MapPeriod {
  year: number | null;
  /** null = whole year. */
  month: number | null;
}

export interface MapPointSet {
  period: MapPeriod;
  raw: readonly RawPoint[];
  harmonized: readonly HarmonizedPoint[];
  stages: PointStages;
  bbox: BBox | null;
  truncated: boolean;
  limit: number;
  frpAngleCorrectionPct: number | null;
}

export interface MonthCell {
  year: number;
  month: number;
  /** False before the record starts or after the last recorded month. */
  hasData: boolean;
  rawModis: number;
  rawViirs: number;
  rawCount: number;
  harmonizedCount: number;
  /** z-score against the same calendar month in other years; null when fewer than 3 years or sd == 0. */
  anomalyRaw: number | null;
  anomalyHarmonized: number | null;
}

export interface YearlyPoint {
  year: number;
  raw: number;
  rawModis: number;
  rawViirs: number;
  harmonized: number;
  matched: number;
  modisOnly: number;
  viirsOnly: number;
  interscanRemoved: number;
  fleetMerged: number;
  firstDate: string | null;
  lastDate: string | null;
  /** True when a sensor's data for that year starts after Jan 15 or ends before Dec 15. */
  partial: boolean;
  modisFirstDate: string | null;
  modisLastDate: string | null;
  viirsFirstDate: string | null;
  viirsLastDate: string | null;
}

export type YearlySeries = readonly YearlyPoint[];

export type PipelineStageKey = "collect" | "dedup" | "fleet" | "angle" | "grid" | "match" | "fuse" | "calendar";

export interface PipelineStage {
  key: PipelineStageKey;
  /** Detections removed by this stage. */
  removed?: number;
  /** Mean FRP change as a ratio, for the angle stage. */
  frpDeltaPct?: number;
}

export interface RegionTotals {
  modis: number;
  viirs: number;
  raw: number;
  harmonized: number;
  interscanRemoved: number;
  fleetMerged: number;
  matched: number;
  doubleCounts: number;
  /** doubleCounts / raw, as a ratio. */
  doubleCountShare: number;
}

export interface CliffSummary {
  fromYear: number;
  toYear: number;
  /** Alias for `toYear` (the year VIIRS was added). */
  year: number;
  rawFrom: number;
  rawTo: number;
  /** Percentage points (e.g. 180.4 means +180.4%). */
  rawChangePct: number | null;
  /** Ratio for Intl percent formatting (e.g. 1.804 means +180.4%). */
  rawChangeRatio: number | null;
  harmonizedFrom: number;
  harmonizedTo: number;
  /** Percentage points (e.g. 42.1 means +42.1%). */
  harmonizedChangePct: number | null;
  /** Ratio for Intl percent formatting (e.g. 0.421 means +42.1%). */
  harmonizedChangeRatio: number | null;
}

export interface BriefingMetrics {
  peakMonth: (MonthRef & { count: number }) | null;
  /** Mean harmonized detections per complete year since the cliff year. */
  meanAnnualHarmonized: number | null;
  /** Ratio for Intl percent formatting. */
  rawJumpPct: number | null;
  /** Ratio for Intl percent formatting. */
  harmonizedJumpPct: number | null;
  doubleCountShare: number;
  frpAngleCorrectionPct: number | null;
}

export interface RegionData {
  status: AsyncStatus;
  hasData: boolean;
  error: ApiErrorInfo | null;
  retry: () => void;
  region: Region;
  totals: RegionTotals;
  cliff: CliffSummary | null;
  coverage: SeriesCoverage;
  yearly: YearlySeries;
  /** Chronological, 12 cells per year from the first to the last year. */
  months: readonly MonthCell[];
  stages: readonly PipelineStage[];
  frpAngleCorrectionPct: number | null;
  briefing: BriefingMetrics;
  firstYear: number | null;
  lastYear: number | null;
  defaultMapMonth: MonthRef | null;
}

// --- NRT endpoint types (R4 hazırlık) ---

export interface CalendarCell {
  year: number;
  month: number;
  count: number;
  countNormalized: number | null;
  effortOmega: number | null;
  meanFrp: number;
  meanFrpRaw: number;
  anomaly: number | null;
}

export interface EffortFactor {
  hasModis: boolean;
  viirsPlatforms: Sensor[];
  platforms: Sensor[];
  months: number;
  total: number;
  modisObservable: number;
  omega: number | null;
}

export interface EffortInfo {
  reliable: boolean;
  reason: string | null;
  factors: EffortFactor[];
}

export interface ReportInfo {
  perSource: [string, number][];
  failures: [string, string][];
  transactionsSpent: number;
  uncovered: [string, string][];
}

export interface RawHotspot {
  lat: number;
  lon: number;
  frp: number;
  sensor: Sensor;
  acquired: string;
  daynight: DayNight;
}

export interface HarmonizedHotspot {
  lat: number;
  lon: number;
  frp: number;
  frpCorrected: number;
  vzaFactor: number | null;
  nObs: number;
  platforms: Sensor[];
  confidence: Confidence;
  source: HotspotSource;
  acquired: string;
  daynight: DayNight;
}

export interface HarmonizeRawSummary {
  modisCount: number;
  viirsCount: number;
  totalRaw: number;
  hotspots: RawHotspot[] | null;
}

export interface HarmonizeResponse {
  raw: HarmonizeRawSummary;
  harmonized: {
    count: number;
    hotspots: HarmonizedHotspot[];
  };
  calendar: CalendarCell[];
  effort: EffortInfo;
  warnings: string[];
  report: ReportInfo;
}

export interface CompareCountsBySource {
  modisOnly: number;
  viirsOnly: number;
  matched: number;
}

export interface CompareCounts {
  rawTotal: number;
  rawModis: number;
  rawViirs: number;
  harmonizedTotal: number;
  removedTotal: number;
  removedPct: number;
  bySource: CompareCountsBySource;
}

export interface CompareFrp {
  rawSum: number;
  harmonizedRawSum: number;
  harmonizedCorrectedSum: number;
  reductionPct: number;
}

export interface PlatformCombination {
  platforms: Sensor[];
  records: number;
}

export interface CompareMerging {
  recordsWithMultipleObservations: number;
  observationsMerged: number;
  maxObservationsInOneRecord: number;
  platformCombinations: PlatformCombination[];
}

export interface CompareVza {
  recordsWithCorrection: number;
  meanFactor: number | null;
  maxFactor: number | null;
  meanFrpReductionPct: number | null;
}

export interface CompareResponse {
  counts: CompareCounts;
  frp: CompareFrp;
  merging: CompareMerging;
  vza: CompareVza;
  warnings: string[];
  report: ReportInfo;
}

export interface StatsTotals {
  rawDetections: number;
  harmonizedRecords: number;
  calendarMonths: number;
  distinctPlatforms: number;
}

export interface SensorBreakdownEntry {
  sensor: Sensor;
  firmsSource: string;
  rawDetections: number;
  sharePct: number;
}

export interface ConfidenceBreakdown {
  low: number;
  nominal: number;
  high: number;
}

export interface DayNightBreakdown {
  day: number;
  night: number;
}

export interface StatsFrp {
  harmonizedSum: number;
  harmonizedMean: number;
  harmonizedMax: number;
  harmonizedMedian: number;
}

export interface StatsTemporal {
  firstDetection: string | null;
  lastDetection: string | null;
  monthsCovered: number;
}

export interface StatsPipeline {
  transactionsSpent: number;
  sourcesQueried: number;
  sourcesFailed: number;
}

export interface StatsResponse {
  totals: StatsTotals;
  sensorBreakdown: SensorBreakdownEntry[];
  confidenceBreakdown: ConfidenceBreakdown;
  daynightBreakdown: DayNightBreakdown;
  frp: StatsFrp;
  temporal: StatsTemporal;
  pipeline: StatsPipeline;
  warnings: string[];
  report: ReportInfo;
}

export interface CalendarYears {
  requestedFrom: number;
  requestedTo: number;
  effectiveFrom: number;
  effectiveTo: number;
}

export interface ArchiveCoverage {
  sensorsAvailable: Sensor[];
  sensorsUnavailable: Sensor[];
  note: string;
}

export interface CalendarCounts {
  rawDetections: number;
  harmonizedRecords: number;
}

export interface SensorYearFetch {
  sensor: Sensor;
  year: number;
  detections: number;
}

export interface SensorYearFailure {
  sensor: Sensor;
  year: number;
  error: string;
}

export interface ArchiveFetchSummary {
  perSensorYear: SensorYearFetch[];
  failures: SensorYearFailure[];
}

export interface CalendarApiResponse {
  country: string;
  years: CalendarYears;
  bbox: BBox | null;
  coverage: ArchiveCoverage;
  counts: CalendarCounts;
  calendar: CalendarCell[];
  effort: EffortInfo;
  fetch: ArchiveFetchSummary;
  warnings: string[];
}

export interface HealthResponse {
  status: string;
  version: string;
}

export interface AvailabilityEntry {
  source: string;
  minDate: string;
  maxDate: string;
}

/** Proposed model for Y1 (monthly FRP series); not a current API field. */
export interface FrpMonthlyPoint {
  year: number;
  month: number;
  rawModisCount: number;
  rawModisFrpSum: number;
  rawViirsCount: number;
  rawViirsFrpSum: number;
  harmonizedCount: number;
  harmonizedFrpSum: number;
  meanFrpRaw: number | null;
  meanFrpHarmonized: number | null;
}
