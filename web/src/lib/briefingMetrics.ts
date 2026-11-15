import type {
  CalendarCell,
  MapPeriod,
  MapPointSet,
  MonthCell,
  RegionTotals,
  YearlyPoint,
} from "../types";

export const TREND_FLAT_THRESHOLD = 0.02;
export const TREND_YEARS = 5;
const MIN_TREND_YEARS = 4;
const MIN_BASELINE_YEARS = 3;
const DAY_MS = 86_400_000;

export interface Last7Days {
  /** ISO dates (YYYY-MM-DD, UTC), inclusive. */
  start: string;
  end: string;
  count: number;
}

export interface Deviation {
  /** (current - mean) / mean as a ratio. */
  ratio: number;
  mean: number;
  /** The month, or the whole year when `month` is null. */
  period: { year: number; month: number | null };
}

export interface SensorActivity {
  sensor: "MODIS" | "VIIRS";
  /** Share of raw MODIS + VIIRS detections, 0..1. */
  share: number;
}

export type TrendDirection = "rising" | "falling" | "flat";

export interface Trend {
  direction: TrendDirection;
  /** Least-squares slope per year as a ratio of the mean level. */
  perYear: number;
  fromYear: number;
  toYear: number;
  normalized: boolean;
}

function isoDay(ms: number): string {
  return new Date(ms).toISOString().slice(0, 10);
}

/**
 * Detections in the last 7 days of the points' period (the month's last day, or 31 Dec). Null when
 * the period is unknown or the point list was downsampled (a sample cannot be counted by day).
 */
export function lastSevenDays(
  points: Pick<MapPointSet, "period" | "raw" | "harmonized" | "truncated">,
  harmonized: boolean,
): Last7Days | null {
  const { year, month } = points.period;
  if (year === null || points.truncated) return null;
  const endMs = month === null ? Date.UTC(year, 11, 31) : Date.UTC(year, month, 0);
  const end = isoDay(endMs);
  const start = isoDay(endMs - 6 * DAY_MS);
  const source = harmonized ? points.harmonized : points.raw;
  let count = 0;
  for (const p of source) {
    const day = p.acquired.slice(0, 10);
    if (day >= start && day <= end) count += 1;
  }
  return { start, end, count };
}

/**
 * Harmonized count of the selected month (or year) against the mean of the same calendar month (or
 * of all other complete years). The selected year is left out of the baseline.
 */
export function historicalDeviation(
  period: { year: number | null; month: number | null },
  months: readonly MonthCell[],
  yearly: readonly YearlyPoint[],
): Deviation | null {
  const { year, month } = period;
  if (year === null) return null;
  let current: number | null = null;
  let baseline: number[] = [];
  if (month !== null) {
    const cells = months.filter((c) => c.hasData && c.month === month);
    const cell = cells.find((c) => c.year === year);
    if (!cell) return null;
    current = cell.harmonizedCount;
    baseline = cells.filter((c) => c.year !== year).map((c) => c.harmonizedCount);
  } else {
    const row = yearly.find((y) => y.year === year);
    if (!row) return null;
    current = row.harmonized;
    baseline = yearly.filter((y) => y.year !== year && !y.partial).map((y) => y.harmonized);
  }
  if (baseline.length < MIN_BASELINE_YEARS) return null;
  const mean = baseline.reduce((a, b) => a + b, 0) / baseline.length;
  if (!(mean > 0)) return null;
  return { ratio: (current - mean) / mean, mean, period: { year, month } };
}

/** MODIS vs VIIRS by raw counts; null when there are no raw detections or the two are equal. */
export function mostActiveSensor(counts: { modis: number; viirs: number }): SensorActivity | null {
  const total = counts.modis + counts.viirs;
  if (!(total > 0) || counts.modis === counts.viirs) return null;
  const sensor = counts.viirs > counts.modis ? "VIIRS" : "MODIS";
  return { sensor, share: Math.max(counts.modis, counts.viirs) / total };
}

/** The most frequent platform among harmonized detections; null when none carry platform names. */
export function topPlatform(points: Pick<MapPointSet, "harmonized">): string | null {
  const tally = new Map<string, number>();
  for (const p of points.harmonized) {
    for (const name of p.platforms) tally.set(name, (tally.get(name) ?? 0) + 1);
  }
  let best: string | null = null;
  let bestCount = 0;
  for (const [name, count] of tally) {
    if (count > bestCount) {
      best = name;
      bestCount = count;
    }
  }
  return best;
}

/** Raw counts of the loaded period; the whole record (`period: null`) when no points are loaded. */
export function sensorCounts(
  points: Pick<MapPointSet, "stages" | "period"> & { hasData: boolean },
  totals: Pick<RegionTotals, "modis" | "viirs">,
): { modis: number; viirs: number; period: MapPeriod | null } {
  return points.hasData
    ? { modis: points.stages.rawModis, viirs: points.stages.rawViirs, period: points.period }
    : { modis: totals.modis, viirs: totals.viirs, period: null };
}

const PLATFORM_LABELS: Readonly<Record<string, string>> = {
  modisterra: "Terra",
  modisaqua: "Aqua",
  viirssnpp: "S-NPP",
  viirsnoaa20: "NOAA-20",
  viirsnoaa21: "NOAA-21",
};

/** "ModisTerra" -> "Terra"; unknown names are shown as they are. */
export function platformLabel(name: string): string {
  return PLATFORM_LABELS[name.toLowerCase().replace(/[^a-z0-9]/g, "")] ?? name;
}

function slopeRatio(values: readonly number[]): number | null {
  const n = values.length;
  const meanX = (n - 1) / 2;
  const meanY = values.reduce((a, b) => a + b, 0) / n;
  if (!(meanY > 0)) return null;
  let num = 0;
  let den = 0;
  values.forEach((y, i) => {
    num += (i - meanX) * (y - meanY);
    den += (i - meanX) ** 2;
  });
  return num / den / meanY;
}

function effortNormalizedYear(year: number, calendar: ReadonlyMap<string, CalendarCell>): number | null {
  let sum = 0;
  for (let m = 1; m <= 12; m += 1) {
    const value = calendar.get(`${year}-${m}`)?.countNormalized;
    if (value === null || value === undefined) return null;
    sum += value;
  }
  return sum;
}

/**
 * Direction over the last 5 complete years: effort-normalised counts when the calendar covers all
 * of them, harmonized counts otherwise. |slope| under 2 %/yr counts as flat.
 */
export function yearlyTrend(
  yearly: readonly YearlyPoint[],
  calendar: ReadonlyMap<string, CalendarCell> = new Map(),
): Trend | null {
  const years = yearly
    .filter((y) => !y.partial)
    .slice(-TREND_YEARS);
  if (years.length < MIN_TREND_YEARS) return null;
  const normalizedValues = years.map((y) => effortNormalizedYear(y.year, calendar));
  const normalized = normalizedValues.every((v) => v !== null);
  const values = normalized ? (normalizedValues as number[]) : years.map((y) => y.harmonized);
  const perYear = slopeRatio(values);
  if (perYear === null) return null;
  const direction: TrendDirection =
    Math.abs(perYear) < TREND_FLAT_THRESHOLD ? "flat" : perYear > 0 ? "rising" : "falling";
  return { direction, perYear, fromYear: years[0].year, toYear: years[years.length - 1].year, normalized };
}
