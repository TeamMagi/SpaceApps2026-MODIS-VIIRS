import {
  DEFAULT_FUSION_MODEL,
  type DataMode,
  type FusionModel,
  type Lang,
  type MinConfidence,
  type MonthRef,
  type Palette,
  type PipelineConfig,
  type PipelineOverrides,
  type RegionId,
  type ViewMode,
} from "../types";
import { parsePanel, type PanelId } from "./dock";
import { monthKey } from "./format";
import {
  DEFAULT_PIPELINE_CONFIG,
  normalizeOverrides,
  parseBoundedNumber,
  PIPELINE_PARAM_SPECS,
} from "./pipelineOverrides";
import { isRouteHash } from "./route";

export const DEFAULT_REGION: RegionId = "Turkey";

let validRegionIds: ReadonlySet<string> | null = null;
let validYearRange: { firstYear: number; lastYear: number } | null = null;

export function readRawHashRegion(hash: string): string | null {
  const value = new URLSearchParams(hash.replace(/^#/, "")).get("region");
  const trimmed = value?.trim() ?? "";
  return trimmed.length > 0 ? trimmed : null;
}

let explicitHashRegion: string | null =
  typeof window === "undefined" ? null : readRawHashRegion(window.location.hash);

export function getExplicitHashRegion(): string | null {
  return explicitHashRegion;
}

export function setValidRegionIds(ids: readonly string[]): void {
  if (ids.length > 0) {
    validRegionIds = new Set<string>(ids);
  }
}

export function setValidYearRange(firstYear: number, lastYear: number): void {
  if (firstYear <= lastYear) {
    validYearRange = { firstYear, lastYear };
  }
}

export function isRegionId(value: string): value is RegionId {
  const trimmed = value.trim();
  if (trimmed.length === 0) return false;
  return validRegionIds === null ? true : validRegionIds.has(trimmed);
}

/** The slice of app state that is shareable through the URL hash. */
export interface UrlState {
  region: RegionId;
  mode: ViewMode;
  month: MonthRef | null;
  lang: Lang;
  palette?: Palette;
  data?: DataMode;
  overrides?: PipelineOverrides;
  minConfidence: MinConfidence;
  fusionModel?: FusionModel;
  panel?: PanelId | null;
}

const FUSION_HASH_VALUES: Readonly<Record<string, FusionModel>> = {
  eq: "equal_weight",
  mc: "modis_calibrated",
};

const FUSION_HASH_CODES: Readonly<Record<FusionModel, string | null>> = {
  viirs_preferred: null,
  equal_weight: "eq",
  modis_calibrated: "mc",
};

function parseMonth(value: string): MonthRef | null | undefined {
  const match = /^(\d{4})-(\d{2})$/.exec(value);
  if (!match) return undefined;
  const year = Number(match[1]);
  const month = Number(match[2]);
  if (year <= 0 || month < 1 || month > 12) {
    return undefined;
  }
  if (validYearRange && (year < validYearRange.firstYear || year > validYearRange.lastYear)) {
    return undefined;
  }
  return { year, month };
}

/**
 * Reads `#region=Turkey&mode=harmonized&month=2021-08&lang=tr&conf=nominal`. Unknown or invalid
 * values are ignored. A missing `conf` means the default `low`, since `serializeHash` omits it.
 */
export function parseHash(hash: string): Partial<UrlState> {
  const params = new URLSearchParams(hash.replace(/^#/, ""));
  const result: Partial<UrlState> = {};

  const region = params.get("region");
  if (region && isRegionId(region)) {
    const trimmed = region.trim();
    result.region = trimmed;
    explicitHashRegion = trimmed;
  }

  result.panel = parsePanel(params.get("panel"));

  const mode = params.get("mode");
  if (mode === "raw" || mode === "harmonized") result.mode = mode;

  const lang = params.get("lang");
  if (lang === "en" || lang === "tr") result.lang = lang;

  const month = params.get("month");
  if (month !== null) {
    const parsed = parseMonth(month);
    if (parsed !== undefined) result.month = parsed;
  }

  const palette = params.get("palette");
  if (palette === "default" || palette === "viridis" || palette === "cividis") {
    result.palette = palette;
  }

  const data = params.get("data");
  if (data === "demo" || data === "live") {
    result.data = data;
  }

  const overrides: PipelineOverrides = {};
  let hasOverrides = false;
  for (const spec of PIPELINE_PARAM_SPECS) {
    const parsed = parseBoundedNumber(
      params.get(spec.hashKey),
      spec.apiMin,
      spec.apiMax,
      spec.apiInteger,
    );
    if (parsed !== undefined) {
      overrides[spec.key] = parsed;
      hasOverrides = true;
    }
  }
  if (hasOverrides) {
    result.overrides = overrides;
  }

  const fm = params.get("fm");
  result.fusionModel = fm === "eq" || fm === "mc" ? FUSION_HASH_VALUES[fm] : DEFAULT_FUSION_MODEL;

  const conf = params.get("conf");
  if (conf === null || conf === "low") result.minConfidence = "low";
  else if (conf === "nominal") result.minConfidence = conf;
  return result;
}

export function serializeHash(
  state: UrlState,
  defaults: PipelineConfig = DEFAULT_PIPELINE_CONFIG,
): string {
  const params = new URLSearchParams();
  params.set("region", state.region);
  params.set("mode", state.mode);
  if (state.month) params.set("month", monthKey(state.month));
  params.set("lang", state.lang);
  if (state.palette && state.palette !== "default") {
    params.set("palette", state.palette);
  }
  if (state.data === "demo") {
    params.set("data", "demo");
  }
  const normalized = normalizeOverrides(state.overrides, defaults);
  for (const spec of PIPELINE_PARAM_SPECS) {
    const val = normalized[spec.key];
    if (val !== undefined) {
      const formatted = spec.apiInteger ? String(Math.round(val)) : String(Number(val.toFixed(4)));
      params.set(spec.hashKey, formatted);
    }
  }
  if (state.panel) params.set("panel", state.panel);
  const fmCode = FUSION_HASH_CODES[state.fusionModel ?? DEFAULT_FUSION_MODEL];
  if (fmCode) params.set("fm", fmCode);
  if (state.minConfidence && state.minConfidence !== "low") {
    params.set("conf", state.minConfidence);
  }
  return `#${params.toString()}`;
}

export function syncUrlHash(
  state: UrlState,
  defaults: PipelineConfig = DEFAULT_PIPELINE_CONFIG,
): void {
  if (typeof window === "undefined" || isRouteHash(window.location.hash)) return;
  const next = serializeHash(state, defaults);
  if (window.location.hash !== next) {
    window.history.replaceState(
      window.history.state,
      "",
      `${window.location.pathname}${window.location.search}${next}`,
    );
  }
}

export function readUrlState(): Partial<UrlState> {
  return typeof window === "undefined" ? {} : parseHash(window.location.hash);
}
