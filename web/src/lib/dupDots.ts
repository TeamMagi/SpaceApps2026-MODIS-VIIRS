export const DOT_GRID_COLS = 10;
export const DOT_GRID_ROWS = 2;
export const DOT_GRID_TOTAL = DOT_GRID_COLS * DOT_GRID_ROWS;

/**
 * Computes how many of the 20 summary dots disappear in Harmonized mode:
 * `round(20 * removed / raw)`, clamped to `1..19` whenever duplicates were removed.
 */
export function computeDisappearingDotCount(removed: number, raw: number, total = DOT_GRID_TOTAL): number {
  if (!Number.isFinite(removed) || !Number.isFinite(raw) || removed <= 0 || raw <= 0 || total <= 1) {
    return 0;
  }
  const rounded = Math.round((total * removed) / raw);
  return Math.min(total - 1, Math.max(1, rounded));
}

function spreadIndicesInRow(rowStart: number, cols: readonly number[], count: number): number[] {
  if (count <= 0) return [];
  const out: number[] = [];
  for (let i = 0; i < count; i++) {
    const pos = Math.floor(((i + 0.5) * cols.length) / count);
    out.push(rowStart + cols[pos]);
  }
  return out;
}

/**
 * Picks `count` deterministic, evenly spread dot indices in `0..19`.
 * For `count <= 18`, column 0 is skipped so every disappearing dot slides left onto a real neighbour.
 */
export function selectDisappearingDotIndices(count: number): Set<number> {
  const clamped = Math.max(0, Math.min(DOT_GRID_TOTAL - 1, Math.round(count)));
  if (clamped === 0) return new Set();

  const innerCols = [1, 2, 3, 4, 5, 6, 7, 8, 9] as const;
  const allCols = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9] as const;

  const row0Count = clamped === DOT_GRID_TOTAL - 1 ? innerCols.length : Math.ceil(clamped / 2);
  const row1Count = clamped - row0Count;

  const row0 = spreadIndicesInRow(0, innerCols, row0Count);
  const row1 = spreadIndicesInRow(DOT_GRID_COLS, row1Count > innerCols.length ? allCols : innerCols, row1Count);

  return new Set([...row0, ...row1]);
}
