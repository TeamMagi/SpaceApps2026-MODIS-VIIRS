import type { SceneFeature } from "./types";

/** Even-odd test across every ring of a feature, so MultiPolygon parts and holes need no special casing. */
export function pointInRings(rings: readonly Float32Array[], x: number, y: number): boolean {
  let inside = false;
  for (let r = 0; r < rings.length; r++) {
    const coords = rings[r];
    const n = coords.length >> 1;
    if (n < 3) continue;
    let jx = coords[(n - 1) * 2];
    let jy = coords[(n - 1) * 2 + 1];
    for (let i = 0; i < n; i++) {
      const ix = coords[i * 2];
      const iy = coords[i * 2 + 1];
      if (iy > y !== jy > y && x < ((jx - ix) * (y - iy)) / (jy - iy) + ix) {
        inside = !inside;
      }
      jx = ix;
      jy = iy;
    }
  }
  return inside;
}

/**
 * Index of the feature containing the plane point, or -1. `accept` filters before the polygon test.
 * When simplified borders overlap, the smallest feature wins so enclaves stay reachable.
 */
export function hitTestFeatures(
  features: readonly SceneFeature[],
  x: number,
  y: number,
  accept?: (index: number) => boolean,
): number {
  let best = -1;
  let bestArea = Infinity;
  for (let i = 0; i < features.length; i++) {
    const f = features[i];
    if (x < f.minX || x > f.maxX || y < f.minY || y > f.maxY) continue;
    const area = (f.maxX - f.minX) * (f.maxY - f.minY);
    if (area >= bestArea) continue;
    if (accept && !accept(i)) continue;
    if (pointInRings(f.rings, x, y)) {
      best = i;
      bestArea = area;
    }
  }
  return best;
}
