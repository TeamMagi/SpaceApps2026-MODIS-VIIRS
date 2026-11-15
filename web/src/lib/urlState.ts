import type { Lang, MonthRef, Palette, RegionId, ViewMode } from "../types";
import { monthKey } from "./format";

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
}

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

/** Reads `#region=Turkey&mode=harmonized&month=2021-08&lang=tr`. Unknown or invalid values are ignored. */
export function parseHash(hash: string): Partial<UrlState> {
  const params = new URLSearchParams(hash.replace(/^#/, ""));
  const result: Partial<UrlState> = {};

  const region = params.get("region");
  if (region && isRegionId(region)) {
    const trimmed = region.trim();
    result.region = trimmed;
    explicitHashRegion = trimmed;
  }

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
  return result;
}

export function serializeHash(state: UrlState): string {
  const params = new URLSearchParams();
  params.set("region", state.region);
  params.set("mode", state.mode);
  if (state.month) params.set("month", monthKey(state.month));
  params.set("lang", state.lang);
  if (state.palette && state.palette !== "default") {
    params.set("palette", state.palette);
  }
  return `#${params.toString()}`;
}

export function syncUrlHash(state: UrlState): void {
  if (typeof window === "undefined") return;
  const next = serializeHash(state);
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
