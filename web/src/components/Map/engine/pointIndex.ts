import type { PointSpatialIndex } from "./types";

export type { PointSpatialIndex };

/** Minimum tap/hover hit-test tolerance in CSS pixels; callers convert to plane units via `HIT_TOLERANCE_PX / transform.k`. */
export const HIT_TOLERANCE_PX = 12;

const MAX_GRID_SIDE = 256;

export function buildIndex(
  xs: Float32Array,
  ys: Float32Array,
  cellSize?: number,
): PointSpatialIndex {
  const count = Math.min(xs.length, ys.length);
  if (count === 0) {
    return {
      count: 0,
      xs,
      ys,
      minX: 0,
      minY: 0,
      maxX: 0,
      maxY: 0,
      cellSize: 1,
      cols: 1,
      rows: 1,
      heads: new Int32Array(0),
      next: new Int32Array(0),
    };
  }

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (let i = 0; i < count; i++) {
    const x = xs[i];
    const y = ys[i];
    if (!Number.isFinite(x) || !Number.isFinite(y)) continue;
    if (x < minX) minX = x;
    if (y < minY) minY = y;
    if (x > maxX) maxX = x;
    if (y > maxY) maxY = y;
  }

  if (!Number.isFinite(minX) || !Number.isFinite(minY)) {
    return {
      count: 0,
      xs,
      ys,
      minX: 0,
      minY: 0,
      maxX: 0,
      maxY: 0,
      cellSize: 1,
      cols: 1,
      rows: 1,
      heads: new Int32Array(0),
      next: new Int32Array(0),
    };
  }

  const spanX = Math.max(maxX - minX, 1e-6);
  const spanY = Math.max(maxY - minY, 1e-6);
  const maxSpan = Math.max(spanX, spanY);

  let resolvedCellSize: number;
  if (cellSize !== undefined && Number.isFinite(cellSize) && cellSize > 0) {
    resolvedCellSize = Math.max(cellSize, maxSpan / MAX_GRID_SIDE);
  } else {
    const targetSide = Math.max(8, Math.min(128, Math.ceil(Math.sqrt(count / 2))));
    resolvedCellSize = maxSpan / targetSide;
  }

  const cols = Math.max(1, Math.min(MAX_GRID_SIDE, Math.ceil(spanX / resolvedCellSize)));
  const rows = Math.max(1, Math.min(MAX_GRID_SIDE, Math.ceil(spanY / resolvedCellSize)));

  // Int32 intrusive linked lists avoid allocating per-cell JS arrays across 100k+ points.
  const heads = new Int32Array(cols * rows);
  heads.fill(-1);
  const next = new Int32Array(count);
  const invCell = 1 / resolvedCellSize;

  for (let i = count - 1; i >= 0; i--) {
    const x = xs[i];
    const y = ys[i];
    if (!Number.isFinite(x) || !Number.isFinite(y)) {
      next[i] = -1;
      continue;
    }
    const cx = Math.max(0, Math.min(cols - 1, Math.floor((x - minX) * invCell)));
    const cy = Math.max(0, Math.min(rows - 1, Math.floor((y - minY) * invCell)));
    const cell = cy * cols + cx;
    next[i] = heads[cell];
    heads[cell] = i;
  }

  return {
    count,
    xs,
    ys,
    minX,
    minY,
    maxX,
    maxY,
    cellSize: resolvedCellSize,
    cols,
    rows,
    heads,
    next,
  };
}

export function nearest(
  index: PointSpatialIndex,
  planeX: number,
  planeY: number,
  maxDistPlane: number,
): number {
  const { count, xs, ys, minX, minY, maxX, maxY, cellSize, cols, rows, heads, next } = index;
  if (
    count === 0 ||
    heads.length === 0 ||
    !Number.isFinite(planeX) ||
    !Number.isFinite(planeY) ||
    !Number.isFinite(maxDistPlane) ||
    maxDistPlane < 0
  ) {
    return -1;
  }

  if (
    planeX + maxDistPlane < minX ||
    planeX - maxDistPlane > maxX ||
    planeY + maxDistPlane < minY ||
    planeY - maxDistPlane > maxY
  ) {
    return -1;
  }

  const invCell = 1 / cellSize;
  const c0 = Math.max(0, Math.min(cols - 1, Math.floor((planeX - maxDistPlane - minX) * invCell)));
  const c1 = Math.max(0, Math.min(cols - 1, Math.floor((planeX + maxDistPlane - minX) * invCell)));
  const r0 = Math.max(0, Math.min(rows - 1, Math.floor((planeY - maxDistPlane - minY) * invCell)));
  const r1 = Math.max(0, Math.min(rows - 1, Math.floor((planeY + maxDistPlane - minY) * invCell)));

  let bestDistSq = maxDistPlane * maxDistPlane;
  let bestIdx = -1;

  for (let r = r0; r <= r1; r++) {
    const rowOffset = r * cols;
    for (let c = c0; c <= c1; c++) {
      let i = heads[rowOffset + c];
      while (i !== -1) {
        const dx = xs[i] - planeX;
        const dy = ys[i] - planeY;
        const d2 = dx * dx + dy * dy;
        if (d2 < bestDistSq || (d2 === bestDistSq && (bestIdx === -1 || i < bestIdx))) {
          bestDistSq = d2;
          bestIdx = i;
        }
        i = next[i];
      }
    }
  }

  return bestIdx;
}

export function withinRadius(
  index: PointSpatialIndex,
  planeX: number,
  planeY: number,
  radiusPlane: number,
  maxResults: number = 32,
): number[] {
  const { count, xs, ys, minX, minY, maxX, maxY, cellSize, cols, rows, heads, next } = index;
  if (
    count === 0 ||
    heads.length === 0 ||
    maxResults <= 0 ||
    !Number.isFinite(planeX) ||
    !Number.isFinite(planeY) ||
    !Number.isFinite(radiusPlane) ||
    radiusPlane < 0
  ) {
    return [];
  }

  if (
    planeX + radiusPlane < minX ||
    planeX - radiusPlane > maxX ||
    planeY + radiusPlane < minY ||
    planeY - radiusPlane > maxY
  ) {
    return [];
  }

  const invCell = 1 / cellSize;
  const c0 = Math.max(0, Math.min(cols - 1, Math.floor((planeX - radiusPlane - minX) * invCell)));
  const c1 = Math.max(0, Math.min(cols - 1, Math.floor((planeX + radiusPlane - minX) * invCell)));
  const r0 = Math.max(0, Math.min(rows - 1, Math.floor((planeY - radiusPlane - minY) * invCell)));
  const r1 = Math.max(0, Math.min(rows - 1, Math.floor((planeY + radiusPlane - minY) * invCell)));

  const maxDistSq = radiusPlane * radiusPlane;
  const matches: { idx: number; d2: number }[] = [];

  for (let r = r0; r <= r1; r++) {
    const rowOffset = r * cols;
    for (let c = c0; c <= c1; c++) {
      let i = heads[rowOffset + c];
      while (i !== -1) {
        const dx = xs[i] - planeX;
        const dy = ys[i] - planeY;
        const d2 = dx * dx + dy * dy;
        if (d2 <= maxDistSq) {
          matches.push({ idx: i, d2 });
        }
        i = next[i];
      }
    }
  }

  matches.sort((a, b) => (a.d2 !== b.d2 ? a.d2 - b.d2 : a.idx - b.idx));
  const limit = Math.min(matches.length, maxResults);
  const out = new Array<number>(limit);
  for (let i = 0; i < limit; i++) {
    out[i] = matches[i].idx;
  }
  return out;
}

export function orderedForKeyboard(
  indexes: ArrayLike<number> | number,
  frpArray: ArrayLike<number>,
  ids?: readonly string[],
): number[] {
  let out: number[];
  if (typeof indexes === "number") {
    const len = Math.max(0, Math.min(indexes, frpArray.length));
    out = new Array<number>(len);
    for (let i = 0; i < len; i++) out[i] = i;
  } else {
    out = Array.from(indexes);
  }

  out.sort((a, b) => {
    const frpA = frpArray[a] ?? 0;
    const frpB = frpArray[b] ?? 0;
    if (frpB !== frpA) return frpB - frpA;
    if (ids) {
      const idA = ids[a] ?? "";
      const idB = ids[b] ?? "";
      if (idA < idB) return -1;
      if (idA > idB) return 1;
    }
    return a - b;
  });

  return out;
}
