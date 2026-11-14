import type { CopyApi } from "../hooks/useCopy";
import type { MonthCell, MonthRef, YearlyPoint } from "../types";
import { anomalyColorVar } from "./scales";

export const RADIAL_VIEWBOX_SIZE = 340;
export const RADIAL_CENTER = 170;
export const RADIAL_INNER_RADIUS = 54;
export const RADIAL_MAX_RADIUS = 138;
export const RADIAL_LABEL_RADIUS = 154;
export const RADIAL_SLICE_GAP_RAD = 0.028;

export const RADIAL_NO_DATA_RADIUS = 57;
export const RADIAL_ZERO_COUNT_RADIUS = 59;
export const RADIAL_MIN_POSITIVE_RADIUS = 62;

const TWO_PI = Math.PI * 2;
const SLICE_STEP_RAD = TWO_PI / 12;

export interface MonthBaseline {
  readonly month: number;
  readonly sampleCount: number;
  readonly meanRaw: number | null;
  readonly meanHarmonized: number | null;
}

export interface SliceAngles {
  readonly month: number;
  readonly startAngle: number;
  readonly endAngle: number;
  readonly midAngle: number;
}

/**
 * Computes the historical mean for each calendar month (1..12) across all years where `hasData` is true.
 * Months without satellite coverage (`hasData === false`) are excluded; true zero months (`hasData === true`,
 * count === 0) are included in the baseline.
 */
export function computeMonthlyBaselines(months: readonly MonthCell[]): readonly MonthBaseline[] {
  const sumsRaw = new Float64Array(12);
  const sumsHarm = new Float64Array(12);
  const counts = new Uint32Array(12);

  for (const cell of months) {
    if (!cell.hasData || cell.month < 1 || cell.month > 12) continue;
    const idx = cell.month - 1;
    sumsRaw[idx] += cell.rawCount;
    sumsHarm[idx] += cell.harmonizedCount;
    counts[idx] += 1;
  }

  return Array.from({ length: 12 }, (_, idx) => {
    const sampleCount = counts[idx];
    return {
      month: idx + 1,
      sampleCount,
      meanRaw: sampleCount > 0 ? sumsRaw[idx] / sampleCount : null,
      meanHarmonized: sampleCount > 0 ? sumsHarm[idx] / sampleCount : null,
    };
  });
}

/**
 * Percentage difference from the calendar month's historical mean:
 *   round(((count - mean) / mean) * 100)
 * Returns null when no positive baseline exists.
 */
export function computeMonthNormalDeltaPct(count: number, mean: number | null): number | null {
  if (mean === null || !Number.isFinite(mean) || mean <= 0 || !Number.isFinite(count)) {
    return null;
  }
  return Math.round(((count - mean) / mean) * 100);
}

/** Maximum monthly count across recorded months and baselines (at least 1). */
export function computeMaxMonthCount(
  months: readonly MonthCell[],
  baselines: readonly MonthBaseline[],
  harmonized: boolean,
): number {
  let max = 1;
  for (const cell of months) {
    if (!cell.hasData) continue;
    const value = harmonized ? cell.harmonizedCount : cell.rawCount;
    if (value > max) max = value;
  }
  for (const b of baselines) {
    const mean = harmonized ? b.meanHarmonized : b.meanRaw;
    if (mean !== null && mean > max) max = mean;
  }
  return max;
}

/**
 * Polar angle (0 at 12 o'clock, increasing clockwise) to SVG Cartesian coordinates.
 */
export function polarToCartesian(
  cx: number,
  cy: number,
  radius: number,
  angleClockwiseFromTopRad: number,
): { x: number; y: number } {
  return {
    x: cx + radius * Math.sin(angleClockwiseFromTopRad),
    y: cy - radius * Math.cos(angleClockwiseFromTopRad),
  };
}

/**
 * Angles for a 1-based month (1 = January at top-right, advancing clockwise through 12 = December at top-left).
 */
export function computeSliceAngles(month: number, gapRad: number = RADIAL_SLICE_GAP_RAD): SliceAngles {
  const clampedMonth = Math.max(1, Math.min(12, Math.trunc(month)));
  const rawStart = (clampedMonth - 1) * SLICE_STEP_RAD;
  const rawEnd = clampedMonth * SLICE_STEP_RAD;
  const safeGap = Math.max(0, Math.min(SLICE_STEP_RAD * 0.4, gapRad));
  return {
    month: clampedMonth,
    startAngle: rawStart + safeGap,
    endAngle: rawEnd - safeGap,
    midAngle: (rawStart + rawEnd) / 2,
  };
}

/**
 * Maps a monthly count onto [RADIAL_INNER_RADIUS, RADIAL_MAX_RADIUS].
 * Distinguishes no satellite record (`!hasData`), true zero (`hasData && count === 0`),
 * and positive counts (monotonically scaled so `count > mean` <=> `r(count) > r(mean)`).
 */
export function computeSliceRadius(hasData: boolean, count: number, maxCount: number): number {
  if (!hasData) return RADIAL_NO_DATA_RADIUS;
  if (count <= 0) return RADIAL_ZERO_COUNT_RADIUS;
  const safeMax = Math.max(1, maxCount);
  const ratio = Math.min(1, Math.max(0, count / safeMax));
  const scaled = Math.sqrt(ratio);
  const radius = RADIAL_MIN_POSITIVE_RADIUS + scaled * (RADIAL_MAX_RADIUS - RADIAL_MIN_POSITIVE_RADIUS);
  return Math.min(RADIAL_MAX_RADIUS, Math.max(RADIAL_MIN_POSITIVE_RADIUS, radius));
}

function fmtCoord(n: number): string {
  return Number(n.toFixed(2)).toString();
}

/**
 * SVG path `d` for an annular wedge between `innerRadius` and `outerRadius` across `[startAngle, endAngle]`.
 */
export function describeAnnularSector(
  cx: number,
  cy: number,
  innerRadius: number,
  outerRadius: number,
  startAngle: number,
  endAngle: number,
): string {
  const rIn = Math.max(0, Math.min(innerRadius, outerRadius));
  const rOut = Math.max(innerRadius, outerRadius);
  const span = Math.max(0.001, endAngle - startAngle);
  const largeArc = span > Math.PI ? 1 : 0;

  const outStart = polarToCartesian(cx, cy, rOut, startAngle);
  const outEnd = polarToCartesian(cx, cy, rOut, endAngle);
  const inEnd = polarToCartesian(cx, cy, rIn, endAngle);
  const inStart = polarToCartesian(cx, cy, rIn, startAngle);

  return [
    `M ${fmtCoord(outStart.x)} ${fmtCoord(outStart.y)}`,
    `A ${fmtCoord(rOut)} ${fmtCoord(rOut)} 0 ${largeArc} 1 ${fmtCoord(outEnd.x)} ${fmtCoord(outEnd.y)}`,
    `L ${fmtCoord(inEnd.x)} ${fmtCoord(inEnd.y)}`,
    `A ${fmtCoord(rIn)} ${fmtCoord(rIn)} 0 ${largeArc} 0 ${fmtCoord(inStart.x)} ${fmtCoord(inStart.y)}`,
    "Z",
  ].join(" ");
}

/**
 * SVG path `d` for a circular arc of `radius` across `[startAngle, endAngle]`.
 */
export function describeRadialArc(
  cx: number,
  cy: number,
  radius: number,
  startAngle: number,
  endAngle: number,
): string {
  const span = Math.max(0.001, endAngle - startAngle);
  const largeArc = span > Math.PI ? 1 : 0;
  const start = polarToCartesian(cx, cy, radius, startAngle);
  const end = polarToCartesian(cx, cy, radius, endAngle);
  return `M ${fmtCoord(start.x)} ${fmtCoord(start.y)} A ${fmtCoord(radius)} ${fmtCoord(radius)} 0 ${largeArc} 1 ${fmtCoord(end.x)} ${fmtCoord(end.y)}`;
}

/**
 * CSS color variable for a radial slice:
 * - `!hasData` -> `var(--anom-none)`
 * - `count === 0` -> `var(--anom-neg-3)` (or `var(--anom-0)` when historical mean is also 0)
 * - `z !== null` -> diverging anomaly scale (`var(--anom-neg-*)` purple, `var(--anom-0)` neutral, `var(--anom-pos-*)` orange)
 */
export function sliceColorVar(
  hasData: boolean,
  count: number,
  z: number | null,
  mean: number | null = null,
): string {
  if (!hasData) return "var(--anom-none)";
  if (count <= 0) {
    return mean !== null && mean <= 0 ? "var(--anom-0)" : "var(--anom-neg-3)";
  }
  if (z !== null) {
    if (z <= -0.5 || z >= 0.5) {
      return anomalyColorVar(z);
    }
    if (mean !== null && mean > 0) {
      const rel = (count - mean) / mean;
      if (rel <= -0.12) return "var(--anom-neg-1)";
      if (rel >= 0.12) return "var(--anom-pos-1)";
    }
    return "var(--anom-0)";
  }
  if (mean !== null && mean > 0) {
    const rel = (count - mean) / mean;
    if (rel <= -0.4) return "var(--anom-neg-2)";
    if (rel <= -0.12) return "var(--anom-neg-1)";
    if (rel >= 0.4) return "var(--anom-pos-2)";
    if (rel >= 0.12) return "var(--anom-pos-1)";
  }
  return "var(--anom-0)";
}

/** Clamps or resolves an active year within `[firstYear, lastYear]`. */
export function resolveActiveYear(params: {
  viewYear: number | null;
  selectedMonth: MonthRef | null;
  defaultMapMonth: MonthRef | null;
  firstYear: number | null;
  lastYear: number | null;
}): number | null {
  const { viewYear, selectedMonth, defaultMapMonth, firstYear, lastYear } = params;
  if (firstYear === null || lastYear === null || firstYear > lastYear) return null;
  const clamp = (y: number) => Math.max(firstYear, Math.min(lastYear, y));
  if (viewYear !== null && Number.isFinite(viewYear)) return clamp(viewYear);
  if (selectedMonth) return clamp(selectedMonth.year);
  if (defaultMapMonth) return clamp(defaultMapMonth.year);
  return lastYear;
}

/** Steps the active year by `delta` within `[firstYear, lastYear]`. */
export function stepYear(currentYear: number, delta: number, firstYear: number, lastYear: number): number {
  return Math.max(firstYear, Math.min(lastYear, currentYear + delta));
}

export interface PlaybackToggleInput {
  readonly isPlaying: boolean;
  readonly canPlay: boolean;
  readonly activeYear: number | null;
  readonly firstYear: number | null;
  readonly lastYear: number | null;
}

export interface PlaybackToggleResult {
  readonly nextPlaying: boolean;
  /** Year to jump to before playing (restart from the archive start), or null to keep the current year. */
  readonly restartYear: number | null;
}

/** Pure decision for the play/pause button: pausing, starting, or restarting from `firstYear` at the end. */
export function resolvePlaybackToggle(input: PlaybackToggleInput): PlaybackToggleResult {
  const { isPlaying, canPlay, activeYear, firstYear, lastYear } = input;
  if (isPlaying) return { nextPlaying: false, restartYear: null };
  if (!canPlay || firstYear === null || lastYear === null) return { nextPlaying: false, restartYear: null };
  const restart = activeYear === null || activeYear >= lastYear;
  return { nextPlaying: true, restartYear: restart ? firstYear : null };
}

/** Next playback year (clamped to `lastYear`) and whether playback ends on it. */
export function resolvePlaybackStep(
  activeYear: number | null,
  firstYear: number,
  lastYear: number,
): { nextYear: number; finished: boolean } {
  const nextYear = Math.min(lastYear, (activeYear ?? firstYear) + 1);
  return { nextYear, finished: nextYear >= lastYear };
}

/** A tooltip anchor is only valid for the year and month it was measured for. */
export function isTooltipAnchorCurrent(
  anchor: { readonly year: number; readonly month: number } | null,
  activeYear: number | null,
  tooltipMonth: number | null,
): boolean {
  return anchor !== null && activeYear !== null && tooltipMonth !== null && anchor.year === activeYear && anchor.month === tooltipMonth;
}

/**
 * Picks which month (1..12) to highlight and summarize in the detail line for the active year.
 */
export function resolveFocusMonth(params: {
  yearCells: readonly MonthCell[];
  selectedMonth: MonthRef | null;
  defaultMapMonth: MonthRef | null;
  hoveredMonth: number | null;
  focusedMonth: number | null;
  harmonized: boolean;
}): number {
  const { yearCells, selectedMonth, defaultMapMonth, hoveredMonth, focusedMonth, harmonized } = params;
  if (hoveredMonth !== null && hoveredMonth >= 1 && hoveredMonth <= 12) return hoveredMonth;
  if (focusedMonth !== null && focusedMonth >= 1 && focusedMonth <= 12) return focusedMonth;
  if (selectedMonth && selectedMonth.month >= 1 && selectedMonth.month <= 12) return selectedMonth.month;
  if (defaultMapMonth && defaultMapMonth.month >= 1 && defaultMapMonth.month <= 12) return defaultMapMonth.month;

  let bestMonth = 7;
  let bestCount = -1;
  for (const cell of yearCells) {
    if (!cell.hasData) continue;
    const count = harmonized ? cell.harmonizedCount : cell.rawCount;
    if (count > bestCount) {
      bestCount = count;
      bestMonth = cell.month;
    }
  }
  return bestMonth;
}

/**
 * Total detections for `year` from the yearly series, falling back to summing the year's monthly cells.
 */
export function computeYearTotal(
  year: number,
  yearly: readonly YearlyPoint[],
  yearCells: readonly MonthCell[],
  harmonized: boolean,
): number {
  const found = yearly.find((y) => y.year === year);
  if (found) {
    return harmonized ? found.harmonized : found.raw;
  }
  let sum = 0;
  for (const c of yearCells) {
    if (!c.hasData || c.year !== year) continue;
    sum += harmonized ? c.harmonizedCount : c.rawCount;
  }
  return sum;
}

/**
 * Sparse tick set for the compact year histogram: first year, last year, cliff year, and active year.
 */
export function computeSparseYearTicks(
  firstYear: number,
  lastYear: number,
  cliffYear: number | null,
  activeYear: number | null,
): Set<number> {
  const ticks = new Set<number>([firstYear, lastYear]);
  if (cliffYear !== null && cliffYear >= firstYear && cliffYear <= lastYear) {
    ticks.add(cliffYear);
  }
  if (activeYear !== null && activeYear >= firstYear && activeYear <= lastYear) {
    ticks.add(activeYear);
  }
  return ticks;
}

/**
 * Prunes sparse histogram ticks so adjacent labels (within `minGap` years) never collide.
 * Priority: cliff year > active year > boundary years (`firstYear`, `lastYear`).
 */
export function resolveVisibleYearTicks(
  firstYear: number,
  lastYear: number,
  cliffYear: number | null,
  activeYear: number | null,
  minGap: number = 2,
): Set<number> {
  const ticks = computeSparseYearTicks(firstYear, lastYear, cliffYear, activeYear);
  const validCliff = cliffYear !== null && cliffYear >= firstYear && cliffYear <= lastYear ? cliffYear : null;
  const validActive = activeYear !== null && activeYear >= firstYear && activeYear <= lastYear ? activeYear : null;

  if (validActive !== null && validCliff !== null && validActive !== validCliff) {
    if (Math.abs(validActive - validCliff) <= minGap) {
      ticks.delete(validActive);
    }
  }

  for (const anchor of [validCliff, validActive]) {
    if (anchor === null || !ticks.has(anchor)) continue;
    for (const edge of [firstYear, lastYear]) {
      if (edge !== anchor && Math.abs(edge - anchor) <= minGap) {
        ticks.delete(edge);
      }
    }
  }
  return ticks;
}

/**
 * Formats the single-line summary below the year controls using the month's true historical average:
 * e.g. "Temmuz 2021 · normalinden %42 fazla · 3.261 tespit".
 */
export function formatSliceDetailLine(
  cell: MonthCell,
  mean: number | null,
  harmonized: boolean,
  copy: Pick<CopyApi, "t" | "fmt">,
): string {
  const { t, fmt } = copy;
  const date = fmt.monthYear(cell, "long");
  if (!cell.hasData) {
    return t("calendarNoRecord", { month: date });
  }
  const count = harmonized ? cell.harmonizedCount : cell.rawCount;
  const pctDiff = computeMonthNormalDeltaPct(count, mean);
  if (pctDiff === null) {
    return t("calendarDetailCountOnly", { date, count });
  }
  if (pctDiff > 0) {
    return t("calendarDetailAboveNormal", { date, pct: pctDiff, count });
  }
  if (pctDiff < 0) {
    return t("calendarDetailBelowNormal", { date, pct: Math.abs(pctDiff), count });
  }
  return t("calendarDetailAtNormal", { date, count });
}
