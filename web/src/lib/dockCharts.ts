import type { MonthCell } from "../types";

export interface XY {
  x: number;
  y: number | null;
}

export interface XBand {
  x: number;
  lo: number | null;
  hi: number | null;
}

const fix = (n: number): string => n.toFixed(1);

/** Linear map from a data domain to a pixel range (a flat domain maps to the range start). */
export function scaleLinear(d0: number, d1: number, r0: number, r1: number): (v: number) => number {
  const span = d1 - d0;
  return (v) => (span === 0 ? r0 : r0 + ((v - d0) / span) * (r1 - r0));
}

/** Polyline path that starts a new subpath after every null y. */
export function linePath(points: readonly XY[]): string {
  let d = "";
  let pen = false;
  for (const p of points) {
    if (p.y === null) {
      pen = false;
      continue;
    }
    d += `${pen ? "L" : "M"}${fix(p.x)} ${fix(p.y)}`;
    pen = true;
  }
  return d;
}

/** Closed band polygons (top edge forward, bottom edge back), one per run of complete points. */
export function bandPath(points: readonly XBand[]): string {
  const runs: XBand[][] = [];
  let run: XBand[] = [];
  for (const p of points) {
    if (p.lo === null || p.hi === null) {
      if (run.length > 0) runs.push(run);
      run = [];
    } else {
      run.push(p);
    }
  }
  if (run.length > 0) runs.push(run);

  return runs
    .map((r) => {
      const top = r.map((p, i) => `${i === 0 ? "M" : "L"}${fix(p.x)} ${fix(p.hi as number)}`).join("");
      const bottom = r
        .slice()
        .reverse()
        .map((p) => `L${fix(p.x)} ${fix(p.lo as number)}`)
        .join("");
      return `${top}${bottom}Z`;
    })
    .join("");
}

const NICE_MANTISSAS = [1, 1.5, 2, 2.5, 3, 4, 4.5, 5, 6, 8, 9, 10] as const;

/** Three ticks (0, mid, top) with a round step such that 2 * step covers `max`. */
export function niceTicks(max: number): { ticks: [number, number, number]; top: number } {
  if (!Number.isFinite(max) || max <= 0) return { ticks: [0, 0.5, 1], top: 1 };
  const exp = Math.pow(10, Math.floor(Math.log10(max / 2)));
  let step = 10 * exp;
  for (const m of NICE_MANTISSAS) {
    if (m * exp * 2 >= max - 1e-9) {
      step = m * exp;
      break;
    }
  }
  return { ticks: [0, step, step * 2], top: step * 2 };
}

/** Mean of each calendar month over every year except `excludeYear` that has data; null if none. */
export function monthAverages(
  months: readonly MonthCell[],
  excludeYear: number,
  harmonized: boolean,
): (number | null)[] {
  const sums = new Array<number>(12).fill(0);
  const counts = new Array<number>(12).fill(0);
  for (const c of months) {
    if (c.year === excludeYear || !c.hasData || c.month < 1 || c.month > 12) continue;
    sums[c.month - 1] += harmonized ? c.harmonizedCount : c.rawCount;
    counts[c.month - 1] += 1;
  }
  return sums.map((s, i) => (counts[i] > 0 ? s / counts[i] : null));
}

export type BarTone = "hot" | "normal";

/** Orange when the month's anomaly z-score is at least 1 sigma above normal. */
export function anomalyTone(z: number | null): BarTone {
  return z !== null && Number.isFinite(z) && z >= 1 ? "hot" : "normal";
}
