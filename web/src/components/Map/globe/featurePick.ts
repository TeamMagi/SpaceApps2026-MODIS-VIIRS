import type { GeoFeature } from "../../../lib/geo/types";
import { pointInRings } from "../engine/featureHit";

/** One feature's rings as flat lon/lat pairs. Rings are continuous, so a ring may run past ±180. */
export interface PickFeature {
  readonly rings: readonly Float32Array[];
  /** minLon, minLat, maxLon, maxLat per ring, index-aligned with `rings`. */
  readonly ringBounds: Float32Array;
  readonly minLon: number;
  readonly minLat: number;
  readonly maxLon: number;
  readonly maxLat: number;
  readonly area: number;
}

export function buildPickFeatures(features: readonly GeoFeature[]): PickFeature[] {
  return features.map((feature) => {
    const rings: Float32Array[] = [];
    for (const polygon of feature.polygons) {
      for (const ring of polygon) {
        const coords = new Float32Array(ring.length * 2);
        for (let i = 0; i < ring.length; i++) {
          coords[i * 2] = ring[i][0];
          coords[i * 2 + 1] = ring[i][1];
        }
        rings.push(coords);
      }
    }
    const ringBounds = new Float32Array(rings.length * 4);
    let minLon = Infinity;
    let minLat = Infinity;
    let maxLon = -Infinity;
    let maxLat = -Infinity;
    rings.forEach((coords, r) => {
      let a = Infinity;
      let b = Infinity;
      let c = -Infinity;
      let d = -Infinity;
      for (let i = 0; i < coords.length; i += 2) {
        if (coords[i] < a) a = coords[i];
        if (coords[i] > c) c = coords[i];
        if (coords[i + 1] < b) b = coords[i + 1];
        if (coords[i + 1] > d) d = coords[i + 1];
      }
      ringBounds.set([a, b, c, d], r * 4);
      if (a < minLon) minLon = a;
      if (b < minLat) minLat = b;
      if (c > maxLon) maxLon = c;
      if (d > maxLat) maxLat = d;
    });
    const [w, s, e, n] = feature.bbox;
    return { rings, ringBounds, minLon, minLat, maxLon, maxLat, area: (e - w) * (n - s) };
  });
}

const LON_SHIFTS = [0, 360, -360] as const;

/** Even-odd containment that also tries the point one world-width east and west of itself. */
export function pickFeatureContains(feature: PickFeature, lon: number, lat: number): boolean {
  if (lat < feature.minLat || lat > feature.maxLat) return false;
  for (const shift of LON_SHIFTS) {
    const x = lon + shift;
    if (x < feature.minLon || x > feature.maxLon) continue;
    if (pointInRings(feature.rings, x, lat, feature.ringBounds)) return true;
  }
  return false;
}

/**
 * Index of the feature under a lon/lat position, or -1. Like the flat map's hit test, `accept`
 * filters first and the smallest feature wins so enclaves stay reachable.
 */
export function pickFeatureAt(
  features: readonly PickFeature[],
  lon: number,
  lat: number,
  accept?: (index: number) => boolean,
): number {
  let best = -1;
  let bestArea = Infinity;
  for (let i = 0; i < features.length; i++) {
    const f = features[i];
    if (f.area >= bestArea) continue;
    if (accept && !accept(i)) continue;
    if (pickFeatureContains(f, lon, lat)) {
      best = i;
      bestArea = f.area;
    }
  }
  return best;
}
