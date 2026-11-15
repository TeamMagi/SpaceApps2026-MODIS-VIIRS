import {
  Confidence,
  DayNight,
  HotspotSource,
  Sensor,
  type ApiErrorKind,
  type BBox,
  type CalendarApiResponse,
  type CalendarCell,
  type CliffSummary,
  type CompareResponse,
  type EffortInfo,
  type HarmonizedHotspot,
  type HarmonizedPoint,
  type HarmonizeResponse,
  type PointStages,
  type RawHotspot,
  type RawPoint,
  type RegionSummary,
  type ReportInfo,
  type SensorFamily,
  type SeriesCoverage,
  type StatsResponse,
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
  modis_only: number;
  viirs_only: number;
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
}

interface WirePointStages {
  raw_modis: number;
  raw_viirs: number;
  interscan_removed: number;
  fleet_merged?: number;
  matched_pairs: number;
  harmonized: number;
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
}

interface WireEffortFactor {
  has_modis: boolean;
  viirs_platforms: string[];
  platforms: string[];
  months: number;
  total: number;
  modis_observable: number;
  omega: number | null;
}

interface WireEffortInfo {
  reliable: boolean;
  reason: string | null;
  factors: WireEffortFactor[];
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
}

// --- Core HTTP helper --------------------------------------------------------

async function requestJson<T>(
  endpoint: string,
  params?: Record<string, string>,
  signal?: AbortSignal,
): Promise<T> {
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
    })),
  };
}

function adaptReportInfo(w: WireReportInfo): ReportInfo {
  return {
    perSource: w.per_source,
    failures: w.failures,
    transactionsSpent: w.transactions_spent,
    uncovered: w.uncovered,
  };
}

// --- Public endpoint fetchers ------------------------------------------------

export async function fetchRegions(signal?: AbortSignal): Promise<RegionsResponse> {
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
}

export async function fetchSeries(region: string, signal?: AbortSignal): Promise<SeriesResponse> {
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
      modisOnly: y.modis_only,
      viirsOnly: y.viirs_only,
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
    })),
    cliff: adaptCliff(wire.cliff),
    coverage: wire.coverage
      ? {
          modis: adaptYearSpan(wire.coverage.modis),
          viirs: adaptYearSpan(wire.coverage.viirs),
        }
      : null,
  };
}

export interface FetchPointsParams {
  region: string;
  year: number;
  month?: number | null;
}

export async function fetchPoints(params: FetchPointsParams, signal?: AbortSignal): Promise<PointsResponse> {
  const query: Record<string, string> = {
    region: params.region,
    year: String(params.year),
  };
  if (params.month !== undefined && params.month !== null) {
    query.month = String(params.month);
  }
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
    })),
    stages: {
      rawModis: wire.stages.raw_modis,
      rawViirs: wire.stages.raw_viirs,
      interscanRemoved: wire.stages.interscan_removed,
      fleetMerged: wire.stages.fleet_merged ?? 0,
      matchedPairs: wire.stages.matched_pairs,
      harmonized: wire.stages.harmonized,
    },
    bbox: wire.bbox ?? null,
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

export async function fetchHarmonize(
  params: HarmonizeParams,
  signal?: AbortSignal,
): Promise<HarmonizeResponse> {
  const wire = await requestJson<WireHarmonizeResponse>("/harmonize", harmonizeQuery(params), signal);
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
    warnings: wire.warnings,
    report: adaptReportInfo(wire.report),
  };
}

export async function fetchCompare(
  params: HarmonizeParams,
  signal?: AbortSignal,
): Promise<CompareResponse> {
  const wire = await requestJson<WireCompareResponse>("/compare", harmonizeQuery(params), signal);
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
    warnings: wire.warnings,
    report: adaptReportInfo(wire.report),
  };
}

export async function fetchStats(
  params: HarmonizeParams,
  signal?: AbortSignal,
): Promise<StatsResponse> {
  const wire = await requestJson<WireStatsResponse>("/stats", harmonizeQuery(params), signal);
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
  const wire = await requestJson<WireCalendarApiResponse>("/calendar", q, signal);
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
