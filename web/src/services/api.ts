import { decideNextDataMode, type ProbeOrFetchOutcome } from "../lib/dataMode";
import { buildDemoPoints, buildDemoRegions, buildDemoSeries } from "../lib/demoData";
import { parseContentDispositionFileName, type ExportFormat, type ExportViewKind } from "../lib/export";
import { DEFAULT_PIPELINE_CONFIG, overridesToQueryParams } from "../lib/pipelineOverrides";
import { useAppStore } from "../store/useAppStore";
import {
  Confidence,
  DayNight,
  HotspotSource,
  Sensor,
  DEFAULT_FUSION_MODEL,
  type ApiErrorKind,
  type BBox,
  type CalendarApiResponse,
  type ConfidenceFilterInfo,
  type CalendarCell,
  type CliffSummary,
  type CompareResponse,
  type EffortInfo,
  type FusionComparison,
  type FusionModel,
  type HarmonizedHotspot,
  type HarmonizedPoint,
  type HarmonizeResponse,
  type PipelineConfig,
  type PipelineOverrides,
  type MinConfidence,
  type PointStages,
  type RawHotspot,
  type RawPoint,
  type RegionSummary,
  type ReportInfo,
  type SensorFamily,
  type SeriesCoverage,
  type StatsResponse,
  type TropicalSwathGapInfo,
  type UncertaintyInfo,
  type YearlyPoint,
  type YearSpan,
} from "../types";

const BASE_URL = "/api";

export type ApiErrorCode = "unreachable" | "server" | "invalid_response" | "region_too_large";

export class ApiError extends Error {
  readonly status: number | null;
  readonly kind: ApiErrorKind;
  readonly code: ApiErrorCode;

  constructor(
    message: string,
    status: number | null = null,
    kind: ApiErrorKind = "server",
    code: ApiErrorCode = kind === "unreachable" ? "unreachable" : "server",
  ) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.kind = kind;
    this.code = code;
  }
}

export function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

// --- Wire types (snake_case) -------------------------------------------------

interface WireErrorBody {
  error?: unknown;
}

interface WireConfidenceFilter {
  min_confidence: string;
  excluded_detections: number;
}

interface WireAtmosphericBias {
  direction: string;
  low_pct: number;
  high_pct: number;
  applied: boolean;
  note: string;
  refs: string[];
}

interface WireUncertainty {
  confidence: number;
  note: string;
  atmospheric_bias?: WireAtmosphericBias | null;
}

interface WireRegionSummary {
  id: string;
  name: string;
  modis_years: [number, number] | null;
  viirs_years: [number, number] | null;
  size_mb?: number | null;
  too_large?: boolean;
}

interface WireRegionsResponse {
  source: string;
  regions: WireRegionSummary[];
}

interface WireSeriesFilters {
  fire_type: number[];
  confidence: string;
  area: string;
  sensors: string[];
}

interface WireSeriesYear {
  year: number;
  raw_modis: number;
  raw_viirs: number;
  raw_total: number;
  harmonized: number;
  matched: number;
  viirs_aggregated?: number;
  modis_only: number;
  viirs_only: number;
  static_masked?: number;
  sama_removed?: number;
  interscan_removed: number;
  fleet_merged?: number;
  first_date: string | null;
  last_date: string | null;
  partial: boolean;
  modis_first_date?: string | null;
  modis_last_date?: string | null;
  viirs_first_date?: string | null;
  viirs_last_date?: string | null;
}

interface WireSeriesMonth {
  year: number;
  month: number;
  raw_modis: number;
  raw_viirs: number;
  raw_total: number;
  harmonized: number;
  anomaly_raw: number | null;
  anomaly_harmonized: number | null;
  mean_frp?: number | null;
  mean_frp_lower?: number | null;
  mean_frp_upper?: number | null;
}

interface WireSeriesCliff {
  from_year: number;
  to_year: number;
  raw_from: number;
  raw_to: number;
  raw_change_pct: number | null;
  harmonized_from: number;
  harmonized_to: number;
  harmonized_change_pct: number | null;
}

interface WireYearSpan {
  first: string | number;
  last: string | number;
}

interface WireSeriesCoverage {
  modis?: WireYearSpan | null;
  viirs?: WireYearSpan | null;
}

interface WireSeriesResponse {
  region: string;
  filters: WireSeriesFilters;
  years: WireSeriesYear[];
  months: WireSeriesMonth[];
  cliff: WireSeriesCliff | null;
  coverage?: WireSeriesCoverage | null;
}

interface WireRawPoint {
  id: string;
  lat: number;
  lon: number;
  sensor: SensorFamily;
  frp: number;
  confidence: string;
  acquired: string;
  daynight: string;
  harmonized_id: string | null;
}

interface WireHarmonizedPoint {
  id: string;
  lat: number;
  lon: number;
  frp: number;
  frp_corrected: number;
  confidence: string;
  source: string;
  acquired: string;
  daynight: string;
  vza_factor?: number | null;
  n_obs?: number;
  platforms?: string[];
  frp_lower?: number | null;
  frp_upper?: number | null;
}

interface WirePointStages {
  raw_modis: number;
  raw_viirs: number;
  static_masked?: number;
  sama_removed?: number;
  interscan_removed: number;
  fleet_merged?: number;
  matched_pairs: number;
  viirs_aggregated?: number;
  harmonized: number;
}

interface WirePipelineConfig {
  dedup: {
    max_time_diff_secs: number;
    min_overlap_ratio: number;
  };
  fleet: {
    viirs_window_secs: number;
    modis_window_secs: number;
    radius_km: number;
  };
  matching: {
    radius_km: number;
    window_secs: number;
  };
  vza?: {
    a: number;
    b: number;
    c: number;
  };
}

interface WirePointsResponse {
  region: string;
  year: number;
  month?: number | null;
  truncated: boolean;
  limit: number;
  raw: WireRawPoint[];
  harmonized: WireHarmonizedPoint[];
  stages: WirePointStages;
  bbox: BBox | null;
  config?: WirePipelineConfig;
  config_overridden?: boolean;
  filters?: WireConfidenceFilter | null;
  tropical_swath_gap?: unknown;
  fusion_model?: string;
  fusion_comparison?: WireFusionComparison | null;
}

export interface WireFusionComparison {
  model?: string;
  baseline_count?: number;
  baseline_total_frp?: number;
  count?: number;
  total_frp?: number;
  dropped_below_modis_limit?: number;
}

// --- NRT wire types ---

interface WireCalendarCell {
  year: number;
  month: number;
  count: number;
  count_normalized: number | null;
  effort_omega: number | null;
  mean_frp: number;
  mean_frp_raw: number;
  mean_frp_lower: number | null;
  mean_frp_upper: number | null;
  total_frp: number;
  total_frp_lower: number | null;
  total_frp_upper: number | null;
  frp_per_detection_eq: number | null;
  anomaly: number | null;
  quality_flags?: string[];
  count_recovered?: number;
  count_recovered_lower?: number;
  count_recovered_upper?: number;
}

interface WireEffortFactor {
  has_modis: boolean;
  viirs_platforms: string[];
  platforms: string[];
  months: number;
  total: number;
  modis_observable: number;
  omega: number | null;
  total_high_intensity?: number;
  modis_observable_high_intensity?: number;
  omega_high_intensity?: number | null;
  snpp_aqua_high_intensity_ratio?: number | null;
}

interface WireEffortInfo {
  reliable: boolean;
  reason: string | null;
  factors: WireEffortFactor[];
  tropical_swath_gap?: unknown;
}

interface WireReportInfo {
  per_source: [string, number][];
  failures: [string, string][];
  transactions_spent: number;
  uncovered: [string, string][];
}

interface WireRawHotspot {
  lat: number;
  lon: number;
  frp: number;
  sensor: string;
  acquired: string;
  daynight: string;
}

interface WireHarmonizedHotspot {
  lat: number;
  lon: number;
  frp: number;
  frp_corrected: number;
  vza_factor: number | null;
  n_obs: number;
  platforms: string[];
  confidence: string;
  source: string;
  acquired: string;
  daynight: string;
}

interface WireHarmonizeResponse {
  raw: {
    modis_count: number;
    viirs_count: number;
    total_raw: number;
    hotspots?: WireRawHotspot[] | null;
  };
  harmonized: {
    count: number;
    hotspots: WireHarmonizedHotspot[];
  };
  calendar: WireCalendarCell[];
  effort: WireEffortInfo;
  uncertainty?: WireUncertainty | null;
  filters?: WireConfidenceFilter | null;
  warnings: string[];
  report: WireReportInfo;
}

interface WireCompareResponse {
  counts: {
    raw_total: number;
    raw_modis: number;
    raw_viirs: number;
    harmonized_total: number;
    removed_total: number;
    removed_pct: number;
    by_source: {
      modis_only: number;
      viirs_only: number;
      matched: number;
    };
  };
  frp: {
    raw_sum: number;
    harmonized_raw_sum: number;
    harmonized_corrected_sum: number;
    reduction_pct: number;
  };
  merging: {
    records_with_multiple_observations: number;
    observations_merged: number;
    max_observations_in_one_record: number;
    platform_combinations: {
      platforms: string[];
      records: number;
    }[];
  };
  vza: {
    records_with_correction: number;
    mean_factor: number | null;
    max_factor: number | null;
    mean_frp_reduction_pct: number | null;
    mean_frp_adjustment_pct: number | null;
  };
  filters?: WireConfidenceFilter | null;
  warnings: string[];
  report: WireReportInfo;
}

interface WireStatsResponse {
  totals: {
    raw_detections: number;
    harmonized_records: number;
    calendar_months: number;
    distinct_platforms: number;
  };
  sensor_breakdown: {
    sensor: string;
    firms_source: string;
    raw_detections: number;
    share_pct: number;
  }[];
  confidence_breakdown: {
    low: number;
    nominal: number;
    high: number;
  };
  daynight_breakdown: {
    day: number;
    night: number;
  };
  frp: {
    harmonized_sum: number;
    harmonized_mean: number;
    harmonized_max: number;
    harmonized_median: number;
  };
  temporal: {
    first_detection: string | null;
    last_detection: string | null;
    months_covered: number;
  };
  pipeline: {
    transactions_spent: number;
    sources_queried: number;
    sources_failed: number;
  };
  filters?: WireConfidenceFilter | null;
  warnings: string[];
  report: WireReportInfo;
}

interface WireCalendarApiResponse {
  country: string;
  years: {
    requested_from: number;
    requested_to: number;
    effective_from: number;
    effective_to: number;
  };
  bbox: {
    west: number;
    south: number;
    east: number;
    north: number;
  } | null;
  coverage: {
    sensors_available: string[];
    sensors_unavailable: string[];
    note: string;
  };
  counts: {
    raw_detections: number;
    harmonized_records: number;
  };
  calendar: WireCalendarCell[];
  effort: WireEffortInfo;
  uncertainty?: WireUncertainty | null;
  filters?: WireConfidenceFilter | null;
  fetch: {
    per_sensor_year: {
      sensor: string;
      year: number;
      detections: number;
    }[];
    failures: {
      sensor: string;
      year: number;
      error: string;
    }[];
  };
  warnings: string[];
}

// --- Adapted camelCase types -------------------------------------------------

export interface RegionsResponse {
  source: string;
  regions: readonly RegionSummary[];
}

export interface SeriesFilters {
  fireType: readonly number[];
  confidence: string;
  area: string;
  sensors: readonly string[];
}

export interface SeriesMonth {
  year: number;
  month: number;
  rawModis: number;
  rawViirs: number;
  rawTotal: number;
  harmonized: number;
  anomalyRaw: number | null;
  anomalyHarmonized: number | null;
  meanFrp: number | null;
  meanFrpLower: number | null;
  meanFrpUpper: number | null;
}

export interface SeriesResponse {
  region: string;
  filters: SeriesFilters;
  years: readonly YearlyPoint[];
  months: readonly SeriesMonth[];
  cliff: CliffSummary | null;
  coverage: SeriesCoverage | null;
}

export interface PointsResponse {
  region: string;
  year: number;
  month: number | null;
  truncated: boolean;
  limit: number;
  raw: readonly RawPoint[];
  harmonized: readonly HarmonizedPoint[];
  stages: PointStages;
  bbox: BBox | null;
  config?: PipelineConfig;
  configOverridden?: boolean;
  filters: ConfidenceFilterInfo;
  tropicalSwathGap: TropicalSwathGapInfo | null;
  fusionModel?: FusionModel;
  fusionComparison?: FusionComparison | null;
}

// --- Core HTTP helper --------------------------------------------------------

async function requestResponse(
  endpoint: string,
  params?: Record<string, string>,
  signal?: AbortSignal,
): Promise<Response> {
  const url = new URL(`${BASE_URL}${endpoint}`, window.location.origin);
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      url.searchParams.set(k, v);
    }
  }

  let res: Response;
  try {
    res = await fetch(url.toString(), { signal });
  } catch (err: unknown) {
    if (isAbortError(err) || signal?.aborted) {
      throw err;
    }
    const message = err instanceof Error && err.message ? err.message : "Network request failed";
    throw new ApiError(message, null, "unreachable", "unreachable");
  }

  if (!res.ok) {
    let serverMessage: string | null = null;
    try {
      const body = (await res.json()) as WireErrorBody;
      if (isRecord(body) && typeof body.error === "string" && body.error.trim().length > 0) {
        serverMessage = body.error.trim();
      }
    } catch (err: unknown) {
      if (isAbortError(err) || signal?.aborted) {
        throw err;
      }
      // Response body was not JSON (e.g. dev proxy / gateway failure).
    }
    if (res.status === 413) {
      throw new ApiError(
        serverMessage ?? (res.statusText || "Region too large"),
        res.status,
        "server",
        "region_too_large",
      );
    }
    if (serverMessage !== null) {
      throw new ApiError(serverMessage, res.status, "server", "server");
    }
    throw new ApiError(res.statusText || `HTTP ${res.status}`, res.status, "server", "server");
  }

  return res;
}

async function requestJson<T>(
  endpoint: string,
  params?: Record<string, string>,
  signal?: AbortSignal,
): Promise<T> {
  const res = await requestResponse(endpoint, params, signal);
  try {
    return (await res.json()) as T;
  } catch (err: unknown) {
    if (isAbortError(err) || signal?.aborted) {
      throw err;
    }
    throw new ApiError("Invalid JSON response from server", res.status, "server", "invalid_response");
  }
}

// --- Adapters ----------------------------------------------------------------

function finiteOrNull(value: number | null | undefined): number | null {
  return value != null && Number.isFinite(value) ? value : null;
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

export function adaptPipelineConfig(wire: unknown): PipelineConfig {
  if (
    !isRecord(wire) ||
    !isRecord(wire.dedup) ||
    !isRecord(wire.fleet) ||
    !isRecord(wire.matching) ||
    !isFiniteNumber(wire.dedup.max_time_diff_secs) ||
    !isFiniteNumber(wire.dedup.min_overlap_ratio) ||
    !isFiniteNumber(wire.fleet.viirs_window_secs) ||
    !isFiniteNumber(wire.fleet.modis_window_secs) ||
    !isFiniteNumber(wire.fleet.radius_km) ||
    !isFiniteNumber(wire.matching.radius_km) ||
    !isFiniteNumber(wire.matching.window_secs)
  ) {
    throw new ApiError("Invalid config response from server", 200, "server", "invalid_response");
  }
  const adapted: PipelineConfig = {
    dedup: {
      maxTimeDiffSecs: wire.dedup.max_time_diff_secs,
      minOverlapRatio: wire.dedup.min_overlap_ratio,
    },
    fleet: {
      viirsWindowSecs: wire.fleet.viirs_window_secs,
      modisWindowSecs: wire.fleet.modis_window_secs,
      radiusKm: wire.fleet.radius_km,
    },
    matching: {
      radiusKm: wire.matching.radius_km,
      windowSecs: wire.matching.window_secs,
    },
  };
  if (
    isRecord(wire.vza) &&
    isFiniteNumber(wire.vza.a) &&
    isFiniteNumber(wire.vza.b) &&
    isFiniteNumber(wire.vza.c)
  ) {
    adapted.vza = {
      a: wire.vza.a,
      b: wire.vza.b,
      c: wire.vza.c,
    };
  }
  return adapted;
}

function adaptCliff(wire: WireSeriesCliff | null): CliffSummary | null {
  if (!wire) {
    return null;
  }
  const rawChangePct = wire.raw_change_pct != null && Number.isFinite(wire.raw_change_pct) ? wire.raw_change_pct : null;
  const harmonizedChangePct = wire.harmonized_change_pct != null && Number.isFinite(wire.harmonized_change_pct) ? wire.harmonized_change_pct : null;
  return {
    fromYear: wire.from_year,
    toYear: wire.to_year,
    year: wire.to_year,
    rawFrom: wire.raw_from,
    rawTo: wire.raw_to,
    rawChangePct,
    rawChangeRatio: rawChangePct != null ? rawChangePct / 100 : null,
    harmonizedFrom: wire.harmonized_from,
    harmonizedTo: wire.harmonized_to,
    harmonizedChangePct,
    harmonizedChangeRatio: harmonizedChangePct != null ? harmonizedChangePct / 100 : null,
  };
}

function adaptYearSpan(span: WireYearSpan | null | undefined): YearSpan | null {
  if (!span) return null;
  const first = typeof span.first === "string" ? parseInt(span.first, 10) : span.first;
  const last = typeof span.last === "string" ? parseInt(span.last, 10) : span.last;
  if (!Number.isFinite(first) || !Number.isFinite(last)) return null;
  return { first, last };
}

function adaptConfidence(value: string): Confidence {
  if (value === Confidence.High) return Confidence.High;
  if (value === Confidence.Low) return Confidence.Low;
  return Confidence.Nominal;
}

function adaptDayNight(value: string): DayNight {
  return value === DayNight.Night ? DayNight.Night : DayNight.Day;
}

function adaptSource(value: string): HotspotSource {
  if (value === HotspotSource.Matched) return HotspotSource.Matched;
  if (value === HotspotSource.ViirsOnly) return HotspotSource.ViirsOnly;
  return HotspotSource.ModisOnly;
}

function adaptSensor(value: string): Sensor {
  switch (value) {
    case "MODIS_Terra": return Sensor.ModisTerra;
    case "MODIS_Aqua": return Sensor.ModisAqua;
    case "VIIRS_SNPP": return Sensor.ViirsSNPP;
    case "VIIRS_NOAA20": return Sensor.ViirsNOAA20;
    case "VIIRS_NOAA21": return Sensor.ViirsNOAA21;
    default: return Sensor.ViirsSNPP;
  }
}

function adaptCalendarCell(w: WireCalendarCell): CalendarCell {
  return {
    year: w.year,
    month: w.month,
    count: w.count,
    countNormalized: w.count_normalized,
    effortOmega: w.effort_omega,
    meanFrp: w.mean_frp,
    meanFrpRaw: w.mean_frp_raw,
    meanFrpLower: w.mean_frp_lower,
    meanFrpUpper: w.mean_frp_upper,
    totalFrp: w.total_frp,
    totalFrpLower: w.total_frp_lower,
    totalFrpUpper: w.total_frp_upper,
    frpPerDetectionEq: w.frp_per_detection_eq,
    anomaly: w.anomaly,
    qualityFlags: w.quality_flags ?? [],
    ...(w.count_recovered !== undefined && { countRecovered: w.count_recovered }),
    ...(w.count_recovered_lower !== undefined && { countRecoveredLower: w.count_recovered_lower }),
    ...(w.count_recovered_upper !== undefined && { countRecoveredUpper: w.count_recovered_upper }),
  };
}

function adaptEffortInfo(w: WireEffortInfo): EffortInfo {
  return {
    reliable: w.reliable,
    reason: w.reason,
    factors: w.factors.map((f) => ({
      hasModis: f.has_modis,
      viirsPlatforms: f.viirs_platforms.map(adaptSensor),
      platforms: f.platforms.map(adaptSensor),
      months: f.months,
      total: f.total,
      modisObservable: f.modis_observable,
      omega: f.omega,
      totalHighIntensity: f.total_high_intensity,
      modisObservableHighIntensity: f.modis_observable_high_intensity,
      omegaHighIntensity: f.omega_high_intensity,
      snppAquaHighIntensityRatio: f.snpp_aqua_high_intensity_ratio,
    })),
    tropicalSwathGap: adaptTropicalSwathGap(w.tropical_swath_gap),
  };
}

function finiteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

/** Older servers send no flag, and a malformed one is dropped rather than shown half-read. */
export function adaptTropicalSwathGap(w: unknown): TropicalSwathGapInfo | null {
  if (!isRecord(w)) return null;
  const { belt_abs_lat, belt_share, basis, swath_gap_km, missed_frp_low_pct, missed_frp_high_pct } = w;
  if (
    !finiteNumber(belt_abs_lat) ||
    !finiteNumber(belt_share) ||
    belt_share <= 0 ||
    belt_share > 1 ||
    (basis !== "bbox" && basis !== "detections") ||
    !finiteNumber(swath_gap_km) ||
    !finiteNumber(missed_frp_low_pct) ||
    !finiteNumber(missed_frp_high_pct)
  ) {
    return null;
  }
  const comp = w.compensation;
  return {
    beltAbsLat: belt_abs_lat,
    beltShare: belt_share,
    basis,
    swathGapKm: swath_gap_km,
    missedFrpLowPct: missed_frp_low_pct,
    missedFrpHighPct: missed_frp_high_pct,
    compensation:
      isRecord(comp) && finiteNumber(comp.factor) && finiteNumber(comp.factor_sigma)
        ? { factor: comp.factor, factorSigma: comp.factor_sigma }
        : null,
  };
}

export function adaptFusionModel(value: string | null | undefined): FusionModel {
  return value === "equal_weight" || value === "modis_calibrated" ? value : DEFAULT_FUSION_MODEL;
}

/** Older servers send no `fusion_comparison`; the standard model sends null. Both read as null. */
export function adaptFusionComparison(w: WireFusionComparison | null | undefined): FusionComparison | null {
  if (typeof w !== "object" || w === null) return null;
  const num = (v: unknown): number => (typeof v === "number" && Number.isFinite(v) ? v : 0);
  return {
    model: adaptFusionModel(w.model),
    baselineCount: num(w.baseline_count),
    baselineTotalFrp: num(w.baseline_total_frp),
    count: num(w.count),
    totalFrp: num(w.total_frp),
    droppedBelowModisLimit: num(w.dropped_below_modis_limit),
  };
}

export function adaptMinConfidence(value: string | null | undefined): MinConfidence {
  return value === "nominal" || value === "high" ? value : "low";
}

/** Older servers send no `filters`; they never filtered, so that reads as `low` with nothing excluded. */
function adaptFilters(w: WireConfidenceFilter | null | undefined): ConfidenceFilterInfo {
  return {
    minConfidence: adaptMinConfidence(w?.min_confidence),
    excludedDetections: typeof w?.excluded_detections === "number" ? w.excluded_detections : 0,
  };
}

function adaptUncertainty(w: WireUncertainty | null | undefined): UncertaintyInfo | null {
  if (!isRecord(w)) return null;
  const bias = w.atmospheric_bias;
  return {
    confidence: w.confidence,
    note: w.note,
    atmosphericBias:
      isRecord(bias) && bias.direction === "downward"
        ? {
            direction: "downward",
            lowPct: bias.low_pct,
            highPct: bias.high_pct,
            applied: bias.applied,
            note: bias.note,
            refs: Array.isArray(bias.refs) ? bias.refs : [],
          }
        : null,
  };
}

/** `low` is the server default; omitting it keeps unfiltered URLs (and their caches) unchanged. */
function setMinConfidenceParam(query: Record<string, string>, minConfidence: MinConfidence | undefined): void {
  if (minConfidence && minConfidence !== "low") {
    query.min_confidence = minConfidence;
  }
}

function adaptReportInfo(w: WireReportInfo): ReportInfo {
  return {
    perSource: w.per_source,
    failures: w.failures,
    transactionsSpent: w.transactions_spent,
    uncovered: w.uncovered,
  };
}

// --- Health probe & demo fallback helpers ------------------------------------

export const HEALTH_PROBE_TIMEOUT_MS = 2500;

export type ProbeOutcome = Extract<ProbeOrFetchOutcome, { source: "probe" }>;

export async function probeHealth(
  signal?: AbortSignal,
  timeoutMs = HEALTH_PROBE_TIMEOUT_MS,
): Promise<ProbeOutcome> {
  if (signal?.aborted) {
    throw new DOMException("Aborted", "AbortError");
  }

  const controller = new AbortController();
  let timedOut = false;
  const timerId = globalThis.setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, timeoutMs);

  const onExternalAbort = () => {
    controller.abort();
  };
  signal?.addEventListener("abort", onExternalAbort, { once: true });

  try {
    const url = new URL(`${BASE_URL}/health`, window.location.origin);
    const res = await fetch(url.toString(), { signal: controller.signal });
    if (!res.ok) {
      return { source: "probe", ok: false, status: res.status };
    }
    const body: unknown = await res.json();
    if (isRecord(body) && typeof body.status === "string") {
      return { source: "probe", ok: true };
    }
    return { source: "probe", ok: false, status: null };
  } catch (err: unknown) {
    if (signal?.aborted && !timedOut) {
      throw err;
    }
    return { source: "probe", ok: false, status: null };
  } finally {
    globalThis.clearTimeout(timerId);
    signal?.removeEventListener("abort", onExternalAbort);
  }
}

function handleLiveFetchError(err: unknown, signal?: AbortSignal): boolean {
  if (isAbortError(err) || signal?.aborted || !(err instanceof ApiError)) {
    return false;
  }
  const state = useAppStore.getState();
  const next = decideNextDataMode(
    { dataMode: state.dataMode, dataModeSource: state.dataModeSource },
    { source: "fetch", ok: false, kind: err.kind, status: err.status },
  );
  if (next.dataMode !== state.dataMode || next.dataModeSource !== state.dataModeSource) {
    state.setDataMode(next.dataMode, next.dataModeSource);
  }
  return next.dataMode === "demo";
}

// --- Public endpoint fetchers ------------------------------------------------

export async function fetchRegions(signal?: AbortSignal): Promise<RegionsResponse> {
  if (useAppStore.getState().dataMode === "demo") {
    return buildDemoRegions();
  }
  try {
    const wire = await requestJson<WireRegionsResponse>("/regions", undefined, signal);
    if (
      !isRecord(wire) ||
      typeof wire.source !== "string" ||
      !Array.isArray(wire.regions) ||
      wire.regions.some((r) => !isRecord(r) || typeof r.id !== "string" || typeof r.name !== "string")
    ) {
      throw new ApiError("Invalid regions response from server", 200, "server", "invalid_response");
    }
    return {
      source: wire.source,
      regions: wire.regions.map((r) => ({
        id: r.id,
        name: r.name,
        modisYears: r.modis_years ? [r.modis_years[0], r.modis_years[1]] : null,
        viirsYears: r.viirs_years ? [r.viirs_years[0], r.viirs_years[1]] : null,
        sizeMb: typeof r.size_mb === "number" && Number.isFinite(r.size_mb) ? r.size_mb : null,
        tooLarge: Boolean(r.too_large),
      })),
    };
  } catch (err: unknown) {
    if (handleLiveFetchError(err, signal)) {
      return buildDemoRegions();
    }
    throw err;
  }
}

export async function fetchSeries(region: string, signal?: AbortSignal): Promise<SeriesResponse> {
  if (useAppStore.getState().dataMode === "demo") {
    return buildDemoSeries(region);
  }
  try {
    const wire = await requestJson<WireSeriesResponse>("/series", { region }, signal);
    if (
      !isRecord(wire) ||
      typeof wire.region !== "string" ||
      !isRecord(wire.filters) ||
      !Array.isArray(wire.years) ||
      !Array.isArray(wire.months)
    ) {
      throw new ApiError("Invalid series response from server", 200, "server", "invalid_response");
    }
    return {
      region: wire.region,
      filters: {
        fireType: wire.filters.fire_type,
        confidence: wire.filters.confidence,
        area: wire.filters.area,
        sensors: wire.filters.sensors,
      },
      years: wire.years.map((y) => ({
        year: y.year,
        raw: y.raw_total,
        rawModis: y.raw_modis,
        rawViirs: y.raw_viirs,
        harmonized: y.harmonized,
        matched: y.matched,
        viirsAggregated: y.viirs_aggregated ?? 0,
        modisOnly: y.modis_only,
        viirsOnly: y.viirs_only,
        staticMasked: y.static_masked ?? 0,
        samaRemoved: y.sama_removed ?? 0,
        interscanRemoved: y.interscan_removed,
        fleetMerged: y.fleet_merged ?? 0,
        firstDate: y.first_date ?? null,
        lastDate: y.last_date ?? null,
        partial: y.partial,
        modisFirstDate: y.modis_first_date ?? null,
        modisLastDate: y.modis_last_date ?? null,
        viirsFirstDate: y.viirs_first_date ?? null,
        viirsLastDate: y.viirs_last_date ?? null,
      })),
      months: wire.months.map((m) => ({
        year: m.year,
        month: m.month,
        rawModis: m.raw_modis,
        rawViirs: m.raw_viirs,
        rawTotal: m.raw_total,
        harmonized: m.harmonized,
        anomalyRaw: m.anomaly_raw,
        anomalyHarmonized: m.anomaly_harmonized,
        meanFrp: finiteOrNull(m.mean_frp),
        meanFrpLower: finiteOrNull(m.mean_frp_lower),
        meanFrpUpper: finiteOrNull(m.mean_frp_upper),
      })),
      cliff: adaptCliff(wire.cliff),
      coverage: wire.coverage
        ? {
            modis: adaptYearSpan(wire.coverage.modis),
            viirs: adaptYearSpan(wire.coverage.viirs),
          }
        : null,
    };
  } catch (err: unknown) {
    if (handleLiveFetchError(err, signal)) {
      return buildDemoSeries(region);
    }
    throw err;
  }
}

export async function fetchConfig(signal?: AbortSignal): Promise<PipelineConfig> {
  if (useAppStore.getState().dataMode === "demo") {
    return DEFAULT_PIPELINE_CONFIG;
  }
  try {
    const wire = await requestJson<WirePipelineConfig>("/config", undefined, signal);
    return adaptPipelineConfig(wire);
  } catch (err: unknown) {
    if (handleLiveFetchError(err, signal)) {
      return DEFAULT_PIPELINE_CONFIG;
    }
    throw err;
  }
}

export interface FetchPointsParams {
  region: string;
  year: number;
  month?: number | null;
  overrides?: PipelineOverrides;
  defaults?: PipelineConfig;
  minConfidence?: MinConfidence;
  fusionModel?: FusionModel;
}

/** Query shared by /api/points and /api/export; non-default parameters only, so default URLs stay stable. */
function buildPointsQuery(params: FetchPointsParams): Record<string, string> {
  const defaults = params.defaults ?? useAppStore.getState().serverConfig ?? DEFAULT_PIPELINE_CONFIG;
  const query: Record<string, string> = {
    region: params.region,
    year: String(params.year),
    ...overridesToQueryParams(params.overrides, defaults),
  };
  if (params.month !== undefined && params.month !== null) {
    query.month = String(params.month);
  }
  setMinConfidenceParam(query, params.minConfidence);
  if (params.fusionModel && params.fusionModel !== DEFAULT_FUSION_MODEL) {
    query.fusion_model = params.fusionModel;
  }
  return query;
}

export async function fetchPoints(params: FetchPointsParams, signal?: AbortSignal): Promise<PointsResponse> {
  if (useAppStore.getState().dataMode === "demo") {
    return buildDemoPoints(params);
  }
  const query = buildPointsQuery(params);
  try {
    const wire = await requestJson<WirePointsResponse>("/points", query, signal);
    if (
      !isRecord(wire) ||
      typeof wire.region !== "string" ||
      !Array.isArray(wire.raw) ||
      !Array.isArray(wire.harmonized) ||
      !isRecord(wire.stages)
    ) {
      throw new ApiError("Invalid points response from server", 200, "server", "invalid_response");
    }
    let adaptedConfig: PipelineConfig | undefined;
    if (wire.config !== undefined) {
      try {
        adaptedConfig = adaptPipelineConfig(wire.config);
      } catch {
        // Ignore malformed optional config metadata on points response.
      }
    }
    return {
      region: wire.region,
      year: wire.year,
      month: wire.month ?? null,
      truncated: wire.truncated,
      limit: wire.limit,
      raw: wire.raw.map((r) => ({
        id: r.id,
        lat: r.lat,
        lon: r.lon,
        sensor: r.sensor === "VIIRS" ? "VIIRS" : "MODIS",
        frp: r.frp,
        confidence: adaptConfidence(r.confidence),
        acquired: r.acquired,
        daynight: adaptDayNight(r.daynight),
        harmonizedId: r.harmonized_id,
      })),
      harmonized: wire.harmonized.map((h) => ({
        id: h.id,
        lat: h.lat,
        lon: h.lon,
        frp: h.frp,
        frpCorrected: h.frp_corrected,
        confidence: adaptConfidence(h.confidence),
        source: adaptSource(h.source),
        acquired: h.acquired,
        daynight: adaptDayNight(h.daynight),
        vzaFactor: h.vza_factor ?? null,
        nObs: h.n_obs ?? 1,
        platforms: h.platforms ?? [],
        frpLower: finiteOrNull(h.frp_lower),
        frpUpper: finiteOrNull(h.frp_upper),
      })),
      stages: {
        rawModis: wire.stages.raw_modis,
        rawViirs: wire.stages.raw_viirs,
        staticMasked: wire.stages.static_masked ?? 0,
        samaRemoved: wire.stages.sama_removed ?? 0,
        interscanRemoved: wire.stages.interscan_removed,
        fleetMerged: wire.stages.fleet_merged ?? 0,
        matchedPairs: wire.stages.matched_pairs,
        viirsAggregated: wire.stages.viirs_aggregated ?? 0,
        harmonized: wire.stages.harmonized,
      },
      bbox: wire.bbox ?? null,
      config: adaptedConfig,
      configOverridden: Boolean(wire.config_overridden),
      filters: adaptFilters(wire.filters),
      tropicalSwathGap: adaptTropicalSwathGap(wire.tropical_swath_gap),
      fusionModel: adaptFusionModel(wire.fusion_model),
      fusionComparison: adaptFusionComparison(wire.fusion_comparison),
    };
  } catch (err: unknown) {
    if (handleLiveFetchError(err, signal)) {
      return buildDemoPoints(params);
    }
    throw err;
  }
}

export interface ExportFile {
  readonly blob: Blob;
  readonly fileName: string;
}

/** Archive export from the backend (`GET /api/export`); the file name comes from Content-Disposition. */
export async function fetchExport(
  params: FetchPointsParams,
  format: ExportFormat,
  view: ExportViewKind,
  fallbackFileName: string,
  signal?: AbortSignal,
): Promise<ExportFile> {
  const res = await requestResponse("/export", { ...buildPointsQuery(params), format, view }, signal);
  const blob = await res.blob();
  return {
    blob,
    fileName: parseContentDispositionFileName(res.headers.get("Content-Disposition")) ?? fallbackFileName,
  };
}

// --- NRT endpoint fetchers ---

export interface HarmonizeParams {
  west: number;
  south: number;
  east: number;
  north: number;
  days?: number;
  start?: string;
  end?: string;
  includeRaw?: boolean;
  minConfidence?: MinConfidence;
}

function harmonizeQuery(params: HarmonizeParams): Record<string, string> {
  const q: Record<string, string> = {
    west: String(params.west),
    south: String(params.south),
    east: String(params.east),
    north: String(params.north),
  };
  if (params.days != null) q.days = String(params.days);
  if (params.start != null) q.start = params.start;
  if (params.end != null) q.end = params.end;
  if (params.includeRaw != null) q.include_raw = String(params.includeRaw);
  setMinConfidenceParam(q, params.minConfidence);
  return q;
}

function adaptRawHotspot(w: WireRawHotspot): RawHotspot {
  return {
    lat: w.lat,
    lon: w.lon,
    frp: w.frp,
    sensor: adaptSensor(w.sensor),
    acquired: w.acquired,
    daynight: adaptDayNight(w.daynight),
  };
}

function adaptHarmonizedHotspot(w: WireHarmonizedHotspot): HarmonizedHotspot {
  return {
    lat: w.lat,
    lon: w.lon,
    frp: w.frp,
    frpCorrected: w.frp_corrected,
    vzaFactor: w.vza_factor,
    nObs: w.n_obs,
    platforms: w.platforms.map(adaptSensor),
    confidence: adaptConfidence(w.confidence),
    source: adaptSource(w.source),
    acquired: w.acquired,
    daynight: adaptDayNight(w.daynight),
  };
}

async function requestLiveJson<T>(
  endpoint: string,
  params?: Record<string, string>,
  signal?: AbortSignal,
): Promise<T> {
  if (useAppStore.getState().dataMode === "demo") {
    throw new ApiError("Not available in demo mode", null, "unreachable", "unreachable");
  }
  try {
    return await requestJson<T>(endpoint, params, signal);
  } catch (err: unknown) {
    handleLiveFetchError(err, signal);
    throw err;
  }
}

export async function fetchHarmonize(
  params: HarmonizeParams,
  signal?: AbortSignal,
): Promise<HarmonizeResponse> {
  const wire = await requestLiveJson<WireHarmonizeResponse>("/harmonize", harmonizeQuery(params), signal);
  return {
    raw: {
      modisCount: wire.raw.modis_count,
      viirsCount: wire.raw.viirs_count,
      totalRaw: wire.raw.total_raw,
      hotspots: wire.raw.hotspots ? wire.raw.hotspots.map(adaptRawHotspot) : null,
    },
    harmonized: {
      count: wire.harmonized.count,
      hotspots: wire.harmonized.hotspots.map(adaptHarmonizedHotspot),
    },
    calendar: wire.calendar.map(adaptCalendarCell),
    effort: adaptEffortInfo(wire.effort),
    uncertainty: adaptUncertainty(wire.uncertainty),
    filters: adaptFilters(wire.filters),
    warnings: wire.warnings,
    report: adaptReportInfo(wire.report),
  };
}

export async function fetchCompare(
  params: HarmonizeParams,
  signal?: AbortSignal,
): Promise<CompareResponse> {
  const wire = await requestLiveJson<WireCompareResponse>("/compare", harmonizeQuery(params), signal);
  return {
    counts: {
      rawTotal: wire.counts.raw_total,
      rawModis: wire.counts.raw_modis,
      rawViirs: wire.counts.raw_viirs,
      harmonizedTotal: wire.counts.harmonized_total,
      removedTotal: wire.counts.removed_total,
      removedPct: wire.counts.removed_pct,
      bySource: {
        modisOnly: wire.counts.by_source.modis_only,
        viirsOnly: wire.counts.by_source.viirs_only,
        matched: wire.counts.by_source.matched,
      },
    },
    frp: {
      rawSum: wire.frp.raw_sum,
      harmonizedRawSum: wire.frp.harmonized_raw_sum,
      harmonizedCorrectedSum: wire.frp.harmonized_corrected_sum,
      reductionPct: wire.frp.reduction_pct,
    },
    merging: {
      recordsWithMultipleObservations: wire.merging.records_with_multiple_observations,
      observationsMerged: wire.merging.observations_merged,
      maxObservationsInOneRecord: wire.merging.max_observations_in_one_record,
      platformCombinations: wire.merging.platform_combinations.map((pc) => ({
        platforms: pc.platforms.map(adaptSensor),
        records: pc.records,
      })),
    },
    vza: {
      recordsWithCorrection: wire.vza.records_with_correction,
      meanFactor: wire.vza.mean_factor,
      maxFactor: wire.vza.max_factor,
      meanFrpAdjustmentPct: wire.vza.mean_frp_adjustment_pct,
    },
    filters: adaptFilters(wire.filters),
    warnings: wire.warnings,
    report: adaptReportInfo(wire.report),
  };
}

export async function fetchStats(
  params: HarmonizeParams,
  signal?: AbortSignal,
): Promise<StatsResponse> {
  const wire = await requestLiveJson<WireStatsResponse>("/stats", harmonizeQuery(params), signal);
  return {
    totals: {
      rawDetections: wire.totals.raw_detections,
      harmonizedRecords: wire.totals.harmonized_records,
      calendarMonths: wire.totals.calendar_months,
      distinctPlatforms: wire.totals.distinct_platforms,
    },
    sensorBreakdown: wire.sensor_breakdown.map((s) => ({
      sensor: adaptSensor(s.sensor),
      firmsSource: s.firms_source,
      rawDetections: s.raw_detections,
      sharePct: s.share_pct,
    })),
    confidenceBreakdown: wire.confidence_breakdown,
    daynightBreakdown: wire.daynight_breakdown,
    frp: {
      harmonizedSum: wire.frp.harmonized_sum,
      harmonizedMean: wire.frp.harmonized_mean,
      harmonizedMax: wire.frp.harmonized_max,
      harmonizedMedian: wire.frp.harmonized_median,
    },
    temporal: {
      firstDetection: wire.temporal.first_detection,
      lastDetection: wire.temporal.last_detection,
      monthsCovered: wire.temporal.months_covered,
    },
    pipeline: {
      transactionsSpent: wire.pipeline.transactions_spent,
      sourcesQueried: wire.pipeline.sources_queried,
      sourcesFailed: wire.pipeline.sources_failed,
    },
    filters: adaptFilters(wire.filters),
    warnings: wire.warnings,
    report: adaptReportInfo(wire.report),
  };
}

export interface CalendarParams {
  country: string;
  from?: number;
  to?: number;
  west?: number;
  south?: number;
  east?: number;
  north?: number;
  minConfidence?: MinConfidence;
}

export async function fetchCalendar(
  params: CalendarParams,
  signal?: AbortSignal,
): Promise<CalendarApiResponse> {
  const q: Record<string, string> = { country: params.country };
  if (params.from != null) q.from = String(params.from);
  if (params.to != null) q.to = String(params.to);
  if (params.west != null) q.west = String(params.west);
  if (params.south != null) q.south = String(params.south);
  if (params.east != null) q.east = String(params.east);
  if (params.north != null) q.north = String(params.north);
  setMinConfidenceParam(q, params.minConfidence);
  const wire = await requestLiveJson<WireCalendarApiResponse>("/calendar", q, signal);
  return {
    country: wire.country,
    years: {
      requestedFrom: wire.years.requested_from,
      requestedTo: wire.years.requested_to,
      effectiveFrom: wire.years.effective_from,
      effectiveTo: wire.years.effective_to,
    },
    bbox: wire.bbox,
    coverage: {
      sensorsAvailable: wire.coverage.sensors_available.map(adaptSensor),
      sensorsUnavailable: wire.coverage.sensors_unavailable.map(adaptSensor),
      note: wire.coverage.note,
    },
    counts: {
      rawDetections: wire.counts.raw_detections,
      harmonizedRecords: wire.counts.harmonized_records,
    },
    calendar: wire.calendar.map(adaptCalendarCell),
    effort: adaptEffortInfo(wire.effort),
    uncertainty: adaptUncertainty(wire.uncertainty),
    filters: adaptFilters(wire.filters),
    fetch: {
      perSensorYear: wire.fetch.per_sensor_year.map((s) => ({
        sensor: adaptSensor(s.sensor),
        year: s.year,
        detections: s.detections,
      })),
      failures: wire.fetch.failures.map((f) => ({
        sensor: adaptSensor(f.sensor),
        year: f.year,
        error: f.error,
      })),
    },
    warnings: wire.warnings,
  };
}
