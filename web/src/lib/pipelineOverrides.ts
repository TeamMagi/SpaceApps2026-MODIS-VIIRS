import { DEFAULT_FUSION_MODEL, type FusionModel, type MinConfidence, type PipelineConfig, PipelineOverrideKey, PipelineOverrides } from "../types";

export const DEFAULT_PIPELINE_CONFIG: PipelineConfig = {
  dedup: {
    maxTimeDiffSecs: 8,
    minOverlapRatio: 0.3,
  },
  fleet: {
    viirsWindowSecs: 3600,
    modisWindowSecs: 10800,
    radiusKm: 2.0,
  },
  matching: {
    radiusKm: 2.0,
    windowSecs: 10800,
  },
  vza: {
    a: 0.2017,
    b: 0.0168,
    c: 1.0,
  },
};

export type PipelineUnitKind = "sec" | "pct" | "km" | "min";

export interface PipelineParamSpec {
  readonly key: PipelineOverrideKey;
  readonly hashKey: "dt" | "ov" | "mr" | "mw" | "fr";
  readonly queryKey:
    | "dedup_time_secs"
    | "dedup_overlap"
    | "match_radius_km"
    | "match_window_secs"
    | "fleet_radius_km";
  readonly unit: PipelineUnitKind;
  readonly uiMin: number;
  readonly uiMax: number;
  readonly uiStep: number;
  readonly uiInteger: boolean;
  readonly apiMin: number;
  readonly apiMax: number;
  readonly apiInteger: boolean;
  readonly getDefaultApi: (config: PipelineConfig) => number;
  readonly uiToApi: (uiValue: number) => number;
  readonly apiToUi: (apiValue: number) => number;
}

export const PIPELINE_PARAM_SPECS: readonly PipelineParamSpec[] = [
  {
    key: "dedupTimeSecs",
    hashKey: "dt",
    queryKey: "dedup_time_secs",
    unit: "sec",
    uiMin: 1,
    uiMax: 30,
    uiStep: 1,
    uiInteger: true,
    apiMin: 1,
    apiMax: 30,
    apiInteger: true,
    getDefaultApi: (c) => c.dedup.maxTimeDiffSecs,
    uiToApi: (v) => Math.round(v),
    apiToUi: (v) => v,
  },
  {
    key: "dedupOverlap",
    hashKey: "ov",
    queryKey: "dedup_overlap",
    unit: "pct",
    uiMin: 10,
    uiMax: 80,
    uiStep: 5,
    uiInteger: true,
    apiMin: 0.1,
    apiMax: 0.8,
    apiInteger: false,
    getDefaultApi: (c) => c.dedup.minOverlapRatio,
    uiToApi: (v) => Math.round(v) / 100,
    apiToUi: (v) => Math.round(v * 100),
  },
  {
    key: "matchRadiusKm",
    hashKey: "mr",
    queryKey: "match_radius_km",
    unit: "km",
    uiMin: 0.5,
    uiMax: 10,
    uiStep: 0.5,
    uiInteger: false,
    apiMin: 0.5,
    apiMax: 10,
    apiInteger: false,
    getDefaultApi: (c) => c.matching.radiusKm,
    uiToApi: (v) => v,
    apiToUi: (v) => v,
  },
  {
    key: "matchWindowSecs",
    hashKey: "mw",
    queryKey: "match_window_secs",
    unit: "min",
    uiMin: 30,
    uiMax: 360,
    uiStep: 15,
    uiInteger: true,
    apiMin: 1800,
    apiMax: 21600,
    apiInteger: true,
    getDefaultApi: (c) => c.matching.windowSecs,
    uiToApi: (v) => Math.round(v) * 60,
    apiToUi: (v) => Math.round(v / 60),
  },
  {
    key: "fleetRadiusKm",
    hashKey: "fr",
    queryKey: "fleet_radius_km",
    unit: "km",
    uiMin: 0.5,
    uiMax: 5,
    uiStep: 0.5,
    uiInteger: false,
    apiMin: 0.5,
    apiMax: 5,
    apiInteger: false,
    getDefaultApi: (c) => c.fleet.radiusKm,
    uiToApi: (v) => v,
    apiToUi: (v) => v,
  },
] as const;

const SPEC_BY_KEY: Readonly<Record<PipelineOverrideKey, PipelineParamSpec>> = {
  dedupTimeSecs: PIPELINE_PARAM_SPECS[0],
  dedupOverlap: PIPELINE_PARAM_SPECS[1],
  matchRadiusKm: PIPELINE_PARAM_SPECS[2],
  matchWindowSecs: PIPELINE_PARAM_SPECS[3],
  fleetRadiusKm: PIPELINE_PARAM_SPECS[4],
};

const FLOAT_EPSILON = 1e-6;

export function approxEqual(a: number, b: number): boolean {
  return Math.abs(a - b) <= FLOAT_EPSILON;
}

export function getParamSpec(key: PipelineOverrideKey): PipelineParamSpec {
  return SPEC_BY_KEY[key];
}

export function isDefaultOverrideValue(
  key: PipelineOverrideKey,
  apiValue: number,
  defaults: PipelineConfig = DEFAULT_PIPELINE_CONFIG,
): boolean {
  return approxEqual(apiValue, SPEC_BY_KEY[key].getDefaultApi(defaults));
}

export function normalizeOverrides(
  overrides: PipelineOverrides | undefined,
  defaults: PipelineConfig = DEFAULT_PIPELINE_CONFIG,
): PipelineOverrides {
  if (!overrides) return {};
  const result: PipelineOverrides = {};
  for (const spec of PIPELINE_PARAM_SPECS) {
    const val = overrides[spec.key];
    if (val === undefined || !Number.isFinite(val)) continue;
    if (!isDefaultOverrideValue(spec.key, val, defaults)) {
      result[spec.key] = val;
    }
  }
  return result;
}

export function hasPipelineOverrides(
  overrides: PipelineOverrides | undefined,
  defaults: PipelineConfig = DEFAULT_PIPELINE_CONFIG,
): boolean {
  if (!overrides) return false;
  for (const spec of PIPELINE_PARAM_SPECS) {
    const val = overrides[spec.key];
    if (val !== undefined && Number.isFinite(val) && !isDefaultOverrideValue(spec.key, val, defaults)) {
      return true;
    }
  }
  return false;
}

export function areOverridesEqual(a: PipelineOverrides | undefined, b: PipelineOverrides | undefined): boolean {
  for (const spec of PIPELINE_PARAM_SPECS) {
    const va = a?.[spec.key];
    const vb = b?.[spec.key];
    if (va === undefined && vb === undefined) continue;
    if (va === undefined || vb === undefined) return false;
    if (!approxEqual(va, vb)) return false;
  }
  return true;
}

function formatApiQueryValue(spec: PipelineParamSpec, apiValue: number): string {
  return spec.apiInteger ? String(Math.round(apiValue)) : String(Number(apiValue.toFixed(4)));
}

/** Converts API-unit overrides + server defaults into `/api/points` query parameters, omitting defaults. */
export function overridesToQueryParams(
  overrides: PipelineOverrides | undefined,
  defaults: PipelineConfig = DEFAULT_PIPELINE_CONFIG,
): Record<string, string> {
  const query: Record<string, string> = {};
  if (!overrides) return query;
  for (const spec of PIPELINE_PARAM_SPECS) {
    const val = overrides[spec.key];
    if (val === undefined || !Number.isFinite(val)) continue;
    if (!isDefaultOverrideValue(spec.key, val, defaults)) {
      query[spec.queryKey] = formatApiQueryValue(spec, val);
    }
  }
  return query;
}

/** UI-unit overrides: percent for `dedupOverlapPct` and minutes for `matchWindowMins`. */
export interface PipelineUiOverrides {
  dedupTimeSecs?: number;
  dedupOverlapPct?: number;
  matchRadiusKm?: number;
  matchWindowMins?: number;
  fleetRadiusKm?: number;
}

/** Converts UI-unit overrides (minutes & percent) into API-unit `PipelineOverrides`. */
export function uiOverridesToOverrides(ui: PipelineUiOverrides): PipelineOverrides {
  const out: PipelineOverrides = {};
  if (ui.dedupTimeSecs !== undefined && Number.isFinite(ui.dedupTimeSecs)) {
    out.dedupTimeSecs = SPEC_BY_KEY.dedupTimeSecs.uiToApi(ui.dedupTimeSecs);
  }
  if (ui.dedupOverlapPct !== undefined && Number.isFinite(ui.dedupOverlapPct)) {
    out.dedupOverlap = SPEC_BY_KEY.dedupOverlap.uiToApi(ui.dedupOverlapPct);
  }
  if (ui.matchRadiusKm !== undefined && Number.isFinite(ui.matchRadiusKm)) {
    out.matchRadiusKm = SPEC_BY_KEY.matchRadiusKm.uiToApi(ui.matchRadiusKm);
  }
  if (ui.matchWindowMins !== undefined && Number.isFinite(ui.matchWindowMins)) {
    out.matchWindowSecs = SPEC_BY_KEY.matchWindowSecs.uiToApi(ui.matchWindowMins);
  }
  if (ui.fleetRadiusKm !== undefined && Number.isFinite(ui.fleetRadiusKm)) {
    out.fleetRadiusKm = SPEC_BY_KEY.fleetRadiusKm.uiToApi(ui.fleetRadiusKm);
  }
  return out;
}

/** Converts UI-unit overrides + server defaults into `/api/points` query parameters, omitting defaults. */
export function uiOverridesToQueryParams(
  ui: PipelineUiOverrides,
  defaults: PipelineConfig = DEFAULT_PIPELINE_CONFIG,
): Record<string, string> {
  return overridesToQueryParams(uiOverridesToOverrides(ui), defaults);
}

/** Builds a canonical cache key for `/api/points` that includes confidence filter and normalized overrides. */
export function makePointsKey(
  region: string,
  year: number,
  month: number | null,
  overrides?: PipelineOverrides,
  defaults: PipelineConfig = DEFAULT_PIPELINE_CONFIG,
  minConfidence: MinConfidence = "low",
  fusionModel: FusionModel = DEFAULT_FUSION_MODEL,
): string {
  const modelPart = fusionModel === DEFAULT_FUSION_MODEL ? "" : `:${fusionModel}`;
  const base = `${region}:${year}:${month ?? "all"}:${minConfidence}${modelPart}`;
  const normalized = normalizeOverrides(overrides, defaults);
  const parts: string[] = [];
  for (const spec of PIPELINE_PARAM_SPECS) {
    const val = normalized[spec.key];
    if (val !== undefined) {
      parts.push(`${spec.hashKey}=${formatApiQueryValue(spec, val)}`);
    }
  }
  return parts.length > 0 ? `${base}|${parts.join("&")}` : base;
}

export function parseBoundedNumber(
  raw: string | null,
  min: number,
  max: number,
  integer = false,
): number | undefined {
  if (raw === null) return undefined;
  const trimmed = raw.trim();
  if (trimmed.length === 0) return undefined;
  const num = Number(trimmed);
  if (!Number.isFinite(num)) return undefined;
  if (integer && !Number.isInteger(num)) return undefined;
  if (num < min || num > max) return undefined;
  return num;
}
