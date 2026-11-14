import {
  Confidence,
  DayNight,
  HotspotSource,
  type ApiErrorKind,
  type BBox,
  type CliffSummary,
  type HarmonizedPoint,
  type PointStages,
  type RawPoint,
  type RegionSummary,
  type SensorFamily,
  type SeriesCoverage,
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
  first_date: string | null;
  last_date: string | null;
  partial: boolean;
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
  raw_change_pct: number;
  harmonized_from: number;
  harmonized_to: number;
  harmonized_change_pct: number;
}

interface WireYearSpan {
  first: number;
  last: number;
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
}

interface WirePointStages {
  raw_modis: number;
  raw_viirs: number;
  interscan_removed: number;
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
  const rawChangePct = Number.isFinite(wire.raw_change_pct) ? wire.raw_change_pct : 0;
  const harmonizedChangePct = Number.isFinite(wire.harmonized_change_pct) ? wire.harmonized_change_pct : 0;
  return {
    fromYear: wire.from_year,
    toYear: wire.to_year,
    year: wire.to_year,
    rawFrom: wire.raw_from,
    rawTo: wire.raw_to,
    rawChangePct,
    rawChangeRatio: rawChangePct / 100,
    harmonizedFrom: wire.harmonized_from,
    harmonizedTo: wire.harmonized_to,
    harmonizedChangePct,
    harmonizedChangeRatio: harmonizedChangePct / 100,
  };
}

function adaptYearSpan(span: WireYearSpan | null | undefined): YearSpan | null {
  if (!span || typeof span.first !== "number" || typeof span.last !== "number") {
    return null;
  }
  return { first: span.first, last: span.last };
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
      firstDate: y.first_date ?? null,
      lastDate: y.last_date ?? null,
      partial: y.partial,
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
    })),
    stages: {
      rawModis: wire.stages.raw_modis,
      rawViirs: wire.stages.raw_viirs,
      interscanRemoved: wire.stages.interscan_removed,
      matchedPairs: wire.stages.matched_pairs,
      harmonized: wire.stages.harmonized,
    },
    bbox: wire.bbox ?? null,
  };
}
