/** Monthly FRP series model (Y1): per-sensor raw sums, harmonized totals, and pure helpers for the chart. */

export interface FrpSeriesPoint {
  year: number;
  month: number;
  modis: { count: number; frpSum: number } | null;
  viirs: { count: number; frpSum: number } | null;
  harmonized: {
    count: number;
    countNormalized: number | null;
    totalFrp: number;
    totalFrpLower: number | null;
    totalFrpUpper: number | null;
  } | null;
}

export type FrpMetric = "perDetection" | "total";

export interface SeriesValues {
  modis: number | null;
  viirs: number | null;
  harmonized: number | null;
  harmonizedLower: number | null;
  harmonizedUpper: number | null;
}

function mean(sensor: { count: number; frpSum: number } | null): number | null {
  return sensor === null || sensor.count <= 0 ? null : sensor.frpSum / sensor.count;
}

export function seriesValues(p: FrpSeriesPoint, metric: FrpMetric): SeriesValues {
  const h = p.harmonized;
  if (metric === "total") {
    return {
      modis: p.modis?.frpSum ?? null,
      viirs: p.viirs?.frpSum ?? null,
      harmonized: h?.totalFrp ?? null,
      harmonizedLower: h?.totalFrpLower ?? null,
      harmonizedUpper: h?.totalFrpUpper ?? null,
    };
  }
  const n = h?.countNormalized ?? null;
  const usable = h !== null && n !== null && n > 0;
  return {
    modis: mean(p.modis),
    viirs: mean(p.viirs),
    harmonized: usable ? h.totalFrp / n : null,
    harmonizedLower: usable && h.totalFrpLower !== null ? h.totalFrpLower / n : null,
    harmonizedUpper: usable && h.totalFrpUpper !== null ? h.totalFrpUpper / n : null,
  };
}

/** True when a covered month has harmonized records but no normalized count, so per-detection FRP is not comparable. */
export function isNotComparable(p: FrpSeriesPoint, metric: FrpMetric): boolean {
  const h = p.harmonized;
  return metric === "perDetection" && h !== null && h.count > 0 && (h.countNormalized === null || h.countNormalized <= 0);
}

export interface XY {
  x: number;
  y: number | null;
}

/** Polyline segments that break at every null; a lone point is kept as a one-point segment. */
export function pathSegments(points: readonly XY[]): { x: number; y: number }[][] {
  const segments: { x: number; y: number }[][] = [];
  let current: { x: number; y: number }[] = [];
  for (const point of points) {
    if (point.y === null || !Number.isFinite(point.y)) {
      if (current.length > 0) segments.push(current);
      current = [];
    } else {
      current.push({ x: point.x, y: point.y });
    }
  }
  if (current.length > 0) segments.push(current);
  return segments;
}

export interface ZoomRange {
  start: number;
  end: number;
}

export const MIN_ZOOM_MONTHS = 12;

/** Clamps an inclusive index range to [0, length-1], keeping at least MIN_ZOOM_MONTHS months when the data allows. */
export function clampRange(range: ZoomRange, length: number, minSpan = MIN_ZOOM_MONTHS): ZoomRange {
  if (length <= 0) return { start: 0, end: 0 };
  const last = length - 1;
  const span = Math.min(minSpan, length);
  let start = Math.max(0, Math.min(Math.round(range.start), last));
  let end = Math.max(0, Math.min(Math.round(range.end), last));
  if (end < start) [start, end] = [end, start];
  if (end - start + 1 < span) {
    end = start + span - 1;
    if (end > last) {
      end = last;
      start = last - span + 1;
    }
  }
  return { start, end };
}

export function fullRange(length: number): ZoomRange {
  return { start: 0, end: Math.max(0, length - 1) };
}

const monthOrdinal = (p: { year: number; month: number }): number => p.year * 12 + p.month - 1;

/** Fills every missing calendar month between the first and last point with an all-null point, sorted ascending. */
export function densify(points: readonly FrpSeriesPoint[]): FrpSeriesPoint[] {
  if (points.length === 0) return [];
  const byOrdinal = new Map(points.map(p => [monthOrdinal(p), p]));
  const ordinals = [...byOrdinal.keys()];
  const first = Math.min(...ordinals);
  const last = Math.max(...ordinals);
  const out: FrpSeriesPoint[] = [];
  for (let o = first; o <= last; o++) {
    out.push(byOrdinal.get(o) ?? { year: Math.floor(o / 12), month: (o % 12) + 1, modis: null, viirs: null, harmonized: null });
  }
  return out;
}

type Json = Record<string, unknown>;

function isRecord(value: unknown): value is Json {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function reqNumber(row: Json, key: string, where: string): number {
  const value = row[key];
  if (typeof value !== "number" || !Number.isFinite(value)) throw new Error(`Invalid /api/calendar: ${where}.${key} must be a number`);
  return value;
}

function optNumber(row: Json, key: string, where: string): number | null {
  const value = row[key];
  if (value === null || value === undefined) return null;
  return reqNumber(row, key, where);
}

function reqBool(row: Json, key: string, where: string): boolean {
  const value = row[key];
  if (typeof value !== "boolean") throw new Error(`Invalid /api/calendar: ${where}.${key} must be a boolean`);
  return value;
}

function rows(json: Json, key: string): Json[] {
  const value = json[key];
  if (!Array.isArray(value)) throw new Error(`Invalid /api/calendar: ${key} must be an array`);
  return value.map((row, i): Json => {
    if (!isRecord(row)) throw new Error(`Invalid /api/calendar: ${key}[${i}] must be an object`);
    return row;
  });
}

type Harmonized = NonNullable<FrpSeriesPoint["harmonized"]>;

/** Parses the /api/calendar JSON (calendar cells + raw_monthly) into a month-keyed series. */
export function fromLiveCalendar(json: unknown): FrpSeriesPoint[] {
  if (!isRecord(json)) throw new Error("Invalid /api/calendar: response must be an object");
  if (!("raw_monthly" in json) || json.raw_monthly === undefined) {
    throw new Error("/api/calendar has no raw_monthly: the backend is too old, update and restart the server");
  }
  const cells = new Map<number, Harmonized>();
  rows(json, "calendar").forEach((row, i) => {
    const where = `calendar[${i}]`;
    const ordinal = monthOrdinal({ year: reqNumber(row, "year", where), month: reqNumber(row, "month", where) });
    cells.set(ordinal, {
      count: reqNumber(row, "count", where),
      countNormalized: optNumber(row, "count_normalized", where),
      totalFrp: reqNumber(row, "total_frp", where),
      totalFrpLower: optNumber(row, "total_frp_lower", where),
      totalFrpUpper: optNumber(row, "total_frp_upper", where),
    });
  });

  const byOrdinal = new Map<number, FrpSeriesPoint>();
  rows(json, "raw_monthly").forEach((row, i) => {
    const where = `raw_monthly[${i}]`;
    const year = reqNumber(row, "year", where);
    const month = reqNumber(row, "month", where);
    const modisCovered = reqBool(row, "modis_covered", where);
    const viirsCovered = reqBool(row, "viirs_covered", where);
    const ordinal = monthOrdinal({ year, month });
    const cell = cells.get(ordinal);
    byOrdinal.set(ordinal, {
      year, month,
      modis: modisCovered ? { count: reqNumber(row, "modis_count", where), frpSum: reqNumber(row, "modis_frp_sum", where) } : null,
      viirs: viirsCovered ? { count: reqNumber(row, "viirs_count", where), frpSum: reqNumber(row, "viirs_frp_sum", where) } : null,
      harmonized: cell ?? (modisCovered || viirsCovered
        ? { count: 0, countNormalized: null, totalFrp: 0, totalFrpLower: null, totalFrpUpper: null }
        : null),
    });
  });
  for (const [ordinal, harmonized] of cells) {
    if (!byOrdinal.has(ordinal)) {
      byOrdinal.set(ordinal, { year: Math.floor(ordinal / 12), month: (ordinal % 12) + 1, modis: null, viirs: null, harmonized });
    }
  }
  return [...byOrdinal.entries()].sort((a, b) => a[0] - b[0]).map(([, p]) => p);
}

/** Left edge of a tooltip centred on `center`, kept fully inside [0, containerWidth] (left wins when it cannot fit). */
export function clampTooltipLeft(center: number, tooltipWidth: number, containerWidth: number): number {
  return Math.max(0, Math.min(center - tooltipWidth / 2, containerWidth - tooltipWidth));
}
