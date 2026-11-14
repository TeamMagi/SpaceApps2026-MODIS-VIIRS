/**
 * Zoomed-out views draw simplified copies of the context borders: stroking every vertex of the
 * world at a few hundred pixels wide costs far more than the frame budget and adds nothing visible.
 * Tolerances are fractions of the projected map span, coarsest first.
 */
export const LOD_SPAN_DIVISORS = [1750, 4000] as const;
/** Largest on-screen deviation (CSS px) a simplified level may introduce; borders are under 1 px wide, so this stays invisible. */
export const LOD_MAX_ERROR_PX = 1.5;

/** Index of the coarsest level whose error stays under LOD_MAX_ERROR_PX at scale k, or -1 for full detail. */
export function pickLodLevel(tolerances: readonly number[], k: number): number {
  for (let i = 0; i < tolerances.length; i++) {
    if (tolerances[i] * k <= LOD_MAX_ERROR_PX) return i;
  }
  return -1;
}

function sqSegDist(px: number, py: number, ax: number, ay: number, bx: number, by: number): number {
  let x = ax;
  let y = ay;
  let dx = bx - x;
  let dy = by - y;
  if (dx !== 0 || dy !== 0) {
    const t = ((px - x) * dx + (py - y) * dy) / (dx * dx + dy * dy);
    if (t > 1) {
      x = bx;
      y = by;
    } else if (t > 0) {
      x += dx * t;
      y += dy * t;
    }
  }
  dx = px - x;
  dy = py - y;
  return dx * dx + dy * dy;
}

function markDouglasPeucker(
  coords: Float32Array,
  first: number,
  last: number,
  sqTol: number,
  keep: Uint8Array,
): void {
  const stack = [first, last];
  while (stack.length > 0) {
    const hi = stack.pop() as number;
    const lo = stack.pop() as number;
    let maxSq = 0;
    let maxIdx = -1;
    const ax = coords[lo * 2];
    const ay = coords[lo * 2 + 1];
    const bx = coords[hi * 2];
    const by = coords[hi * 2 + 1];
    for (let i = lo + 1; i < hi; i++) {
      const d = sqSegDist(coords[i * 2], coords[i * 2 + 1], ax, ay, bx, by);
      if (d > maxSq) {
        maxSq = d;
        maxIdx = i;
      }
    }
    if (maxIdx !== -1 && maxSq > sqTol) {
      keep[maxIdx] = 1;
      stack.push(lo, maxIdx, maxIdx, hi);
    }
  }
}

/** Douglas-Peucker on a closed projected ring (x,y pairs); null when nothing of it survives. */
export function simplifyRing(coords: Float32Array, tolerance: number): Float32Array | null {
  const n = coords.length >> 1;
  if (n < 4) return null;
  // Split at the vertex farthest from the start so a closed ring has a non-degenerate baseline.
  let split = 0;
  let maxSq = -1;
  for (let i = 1; i < n - 1; i++) {
    const dx = coords[i * 2] - coords[0];
    const dy = coords[i * 2 + 1] - coords[1];
    const d = dx * dx + dy * dy;
    if (d > maxSq) {
      maxSq = d;
      split = i;
    }
  }
  if (split === 0 || maxSq <= tolerance * tolerance) return null;

  const keep = new Uint8Array(n);
  keep[0] = 1;
  keep[split] = 1;
  keep[n - 1] = 1;
  const sqTol = tolerance * tolerance;
  markDouglasPeucker(coords, 0, split, sqTol, keep);
  markDouglasPeucker(coords, split, n - 1, sqTol, keep);

  let count = 0;
  for (let i = 0; i < n; i++) count += keep[i];
  if (count < 4) return null;
  const out = new Float32Array(count * 2);
  let o = 0;
  for (let i = 0; i < n; i++) {
    if (!keep[i]) continue;
    out[o++] = coords[i * 2];
    out[o++] = coords[i * 2 + 1];
  }
  return out;
}
