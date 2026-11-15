/** Monthly fire-density grid model (Y3): parsing, cell geometry, mock gridding and the fixed colour scale. */

export type DensityMetric = "count" | "frp";
export type DensityMode = "raw" | "harmonized";

export interface DensityCell {
  row: number;
  col: number;
  rawCount: number;
  rawFrp: number;
  harmCount: number;
  harmFrp: number;
}

export interface DensityMonth {
  month: number;
  rawTotal: number;
  harmonizedTotal: number;
  cells: DensityCell[];
}

export interface DensityMax {
  rawCount: number;
  rawFrp: number;
  harmCount: number;
  harmFrp: number;
}

export interface DensityBBox {
  west: number;
  south: number;
  east: number;
  north: number;
}

export interface DensityYear {
  region: string;
  year: number;
  cellDeg: number;
  months: DensityMonth[];
  max: DensityMax;
  bbox: DensityBBox | null;
  coveredMonths: number[];
}

const EARTH_RADIUS_KM = 6371.0088;

function fail(message: string): never {
  throw new Error(`Invalid density response: ${message}`);
}

function record(value: unknown, what: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) fail(`${what} must be an object`);
  return value as Record<string, unknown>;
}

function array(value: unknown, what: string): unknown[] {
  if (!Array.isArray(value)) fail(`${what} must be an array`);
  return value;
}

function finite(value: unknown, what: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) fail(`${what} must be a finite number`);
  return value;
}

function nonNegative(value: unknown, what: string): number {
  const n = finite(value, what);
  if (n < 0) fail(`${what} must not be negative`);
  return n;
}

function integer(value: unknown, what: string, min: number, max = Number.MAX_SAFE_INTEGER): number {
  const n = finite(value, what);
  if (!Number.isInteger(n) || n < min || n > max) fail(`${what} must be an integer in ${min}..${max}`);
  return n;
}

function parseCell(value: unknown, where: string): DensityCell {
  const tuple = array(value, where);
  if (tuple.length !== 6) fail(`${where} must have 6 elements`);
  return {
    row: integer(tuple[0], `${where}[0] (row)`, 0),
    col: integer(tuple[1], `${where}[1] (col)`, 0),
    rawCount: nonNegative(tuple[2], `${where}[2] (raw_count)`),
    rawFrp: nonNegative(tuple[3], `${where}[3] (raw_frp)`),
    harmCount: nonNegative(tuple[4], `${where}[4] (harm_count)`),
    harmFrp: nonNegative(tuple[5], `${where}[5] (harm_frp)`),
  };
}

function parseMonth(value: unknown, index: number): DensityMonth {
  const m = record(value, `months[${index}]`);
  return {
    month: integer(m["month"], `months[${index}].month`, 1, 12),
    rawTotal: nonNegative(m["raw_total"], `months[${index}].raw_total`),
    harmonizedTotal: nonNegative(m["harmonized_total"], `months[${index}].harmonized_total`),
    cells: array(m["cells"], `months[${index}].cells`).map((cell, i) => parseCell(cell, `months[${index}].cells[${i}]`)),
  };
}

function parseBBox(value: unknown): DensityBBox | null {
  if (value === null || value === undefined) return null;
  const b = record(value, "bbox");
  return {
    west: finite(b["west"], "bbox.west"),
    south: finite(b["south"], "bbox.south"),
    east: finite(b["east"], "bbox.east"),
    north: finite(b["north"], "bbox.north"),
  };
}

export function parseDensity(json: unknown): DensityYear {
  const root = record(json, "response");
  if (typeof root["region"] !== "string") fail("region must be a string");
  const cellDeg = finite(root["cell_deg"], "cell_deg");
  if (cellDeg <= 0 || cellDeg > 90) fail("cell_deg must be in (0, 90]");
  const max = record(root["max"], "max");
  const months = array(root["months"], "months").map(parseMonth);
  const seen = new Set<number>();
  for (const m of months) {
    if (seen.has(m.month)) fail(`month ${m.month} appears twice`);
    seen.add(m.month);
  }
  return {
    region: root["region"],
    year: integer(root["year"], "year", 1900, 3000),
    cellDeg,
    months: months.sort((a, b) => a.month - b.month),
    max: {
      rawCount: nonNegative(max["raw_count"], "max.raw_count"),
      rawFrp: nonNegative(max["raw_frp"], "max.raw_frp"),
      harmCount: nonNegative(max["harm_count"], "max.harm_count"),
      harmFrp: nonNegative(max["harm_frp"], "max.harm_frp"),
    },
    bbox: parseBBox(root["bbox"]),
    coveredMonths: array(root["covered_months"], "covered_months").map((v, i) => integer(v, `covered_months[${i}]`, 1, 12)),
  };
}

export function cellBounds(row: number, col: number, cellDeg: number): DensityBBox {
  const south = row * cellDeg - 90;
  const west = col * cellDeg - 180;
  return { west, south, east: west + cellDeg, north: south + cellDeg };
}

/** Spherical-zone area of an equal-angle cell; shrinks with cos(latitude), so cells are not equal-area. */
export function cellAreaKm2(row: number, cellDeg: number): number {
  const rad = Math.PI / 180;
  const phi1 = (row * cellDeg - 90) * rad;
  const phi2 = phi1 + cellDeg * rad;
  return EARTH_RADIUS_KM ** 2 * (cellDeg * rad) * (Math.sin(phi2) - Math.sin(phi1));
}

export const cellRow = (lat: number, cellDeg: number): number => Math.floor((lat + 90) / cellDeg);
export const cellCol = (lon: number, cellDeg: number): number => Math.floor((lon + 180) / cellDeg);

export interface RawLike { lat: number; lon: number; frp: number; acquired: string }
export interface HarmonizedLike { lat: number; lon: number; frpCorrected: number; acquired: string }
export interface PointsLike {
  region: string;
  year: number;
  raw: readonly RawLike[];
  harmonized: readonly HarmonizedLike[];
  bbox: DensityBBox | null;
}

/** Mock-only: the synthetic hotspot set is complete (not sampled), so it grids exactly like the backend. */
export function gridFromPoints(points: PointsLike, cellDeg: number): DensityYear {
  const buckets = Array.from({ length: 12 }, () => new Map<string, DensityCell>());
  const totals = Array.from({ length: 12 }, () => ({ raw: 0, harm: 0 }));
  const cellOf = (month: number, lat: number, lon: number): DensityCell => {
    const row = cellRow(lat, cellDeg);
    const col = cellCol(lon, cellDeg);
    const key = `${row}:${col}`;
    let cell = buckets[month].get(key);
    if (cell === undefined) {
      cell = { row, col, rawCount: 0, rawFrp: 0, harmCount: 0, harmFrp: 0 };
      buckets[month].set(key, cell);
    }
    return cell;
  };
  for (const p of points.raw) {
    const month = new Date(p.acquired).getUTCMonth();
    if (Number.isNaN(month)) continue;
    const cell = cellOf(month, p.lat, p.lon);
    cell.rawCount += 1;
    cell.rawFrp += p.frp;
    totals[month].raw += 1;
  }
  for (const p of points.harmonized) {
    const month = new Date(p.acquired).getUTCMonth();
    if (Number.isNaN(month)) continue;
    const cell = cellOf(month, p.lat, p.lon);
    cell.harmCount += 1;
    cell.harmFrp += p.frpCorrected;
    totals[month].harm += 1;
  }
  const max: DensityMax = { rawCount: 0, rawFrp: 0, harmCount: 0, harmFrp: 0 };
  const months = buckets.map((bucket, i) => {
    const cells = [...bucket.values()].sort((a, b) => a.row - b.row || a.col - b.col);
    for (const c of cells) {
      max.rawCount = Math.max(max.rawCount, c.rawCount);
      max.rawFrp = Math.max(max.rawFrp, c.rawFrp);
      max.harmCount = Math.max(max.harmCount, c.harmCount);
      max.harmFrp = Math.max(max.harmFrp, c.harmFrp);
    }
    return { month: i + 1, rawTotal: totals[i].raw, harmonizedTotal: totals[i].harm, cells };
  });
  return {
    region: points.region,
    year: points.year,
    cellDeg,
    months,
    max,
    bbox: points.bbox,
    coveredMonths: months.filter(m => m.cells.length > 0).map(m => m.month),
  };
}

export function cellValue(cell: DensityCell, metric: DensityMetric, mode: DensityMode): number {
  if (mode === "raw") return metric === "count" ? cell.rawCount : cell.rawFrp;
  return metric === "count" ? cell.harmCount : cell.harmFrp;
}

/** Busiest month by raw detections; the starting month when the shared selection is for another year. */
export function busiestMonth(data: DensityYear): number {
  let best = data.months[0]?.month ?? 1;
  let peak = -1;
  for (const m of data.months) {
    if (m.rawTotal > peak) { peak = m.rawTotal; best = m.month; }
  }
  return best;
}

export const CLASS_COUNT = 8;
const MIN_CELLS_FOR_QUANTILES = 64;
const FIXED_BREAKS: Record<DensityMetric, readonly number[]> = {
  count: [2, 4, 8, 16, 32, 64, 128],
  frp: [5, 10, 20, 40, 80, 160, 320],
};

export interface DensityScale {
  /** Seven ascending lower edges of classes 2..8; class 1 starts at the smallest non-zero value. */
  breaks: readonly number[];
  method: "quantile" | "fixed";
  /** Colour class 1..8 for a non-zero value; 0 for an empty cell (never drawn). */
  classOf: (value: number) => number;
  min: number;
  max: number;
}

function strictlyIncreasing(values: readonly number[]): boolean {
  return values.every((v, i) => i === 0 || v > values[i - 1]);
}

/**
 * One scale per (year, metric): 8-class quantiles of every non-empty cell of the whole year, raw and
 * harmonized pooled (C7 §5.1, falling back to fixed log breaks for < 64 cells or tied quantiles).
 * No month or mode argument by design, so the scale never renormalizes when either changes.
 */
export function densityScale(data: DensityYear, metric: DensityMetric): DensityScale {
  const values: number[] = [];
  for (const month of data.months) {
    for (const cell of month.cells) {
      for (const mode of ["raw", "harmonized"] as const) {
        const v = cellValue(cell, metric, mode);
        if (v > 0) values.push(v);
      }
    }
  }
  values.sort((a, b) => a - b);
  let breaks: readonly number[] = FIXED_BREAKS[metric];
  let method: DensityScale["method"] = "fixed";
  if (values.length >= MIN_CELLS_FOR_QUANTILES) {
    const quantiles = Array.from({ length: CLASS_COUNT - 1 }, (_, i) => values[Math.floor(((i + 1) / CLASS_COUNT) * values.length)]);
    if (strictlyIncreasing(quantiles)) { breaks = quantiles; method = "quantile"; }
  }
  const classOf = (value: number): number => {
    if (!(value > 0)) return 0;
    let k = 1;
    while (k < CLASS_COUNT && value >= breaks[k - 1]) k += 1;
    return k;
  };
  return { breaks, method, classOf, min: values[0] ?? 0, max: values[values.length - 1] ?? 0 };
}

export type Direction = "left" | "right" | "up" | "down";

/** Nearest non-empty cell in an arrow direction (up = north), for keyboard navigation. */
export function neighbourCell(cells: readonly DensityCell[], from: { row: number; col: number }, direction: Direction): DensityCell | null {
  let best: DensityCell | null = null;
  let bestScore = Infinity;
  for (const c of cells) {
    const dr = c.row - from.row;
    const dc = c.col - from.col;
    const along = direction === "right" ? dc : direction === "left" ? -dc : direction === "up" ? dr : -dr;
    if (along <= 0) continue;
    const across = direction === "left" || direction === "right" ? Math.abs(dr) : Math.abs(dc);
    const score = along + 2 * across;
    if (score < bestScore) { best = c; bestScore = score; }
  }
  return best;
}
