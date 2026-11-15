import type { RegionProjection } from "../../../lib/geo/projection";
import { PointFlag, PointShape, type ScenePointLayer, type SensorFilter } from "../engine/types";
import { lonLatToVec } from "./geoMath";

export const POINT_RADIUS = 1.006;

/** Identity "projection" so the shared scene builder keeps lon/lat in its xs/ys arrays. */
export const LONLAT_PROJECTION: RegionProjection = {
  centerLon: 0,
  project: (lon, lat) => [lon, lat],
  projectUnwrapped: (lon, lat) => [lon, lat],
};

export const PointKind = {
  ModisRing: 0,
  ViirsSquare: 1,
  /** Harmonized FRP bucket b (1..8) is stored as FrpBase + b. */
  FrpBase: 1,
} as const;

export const SensorBit = { Modis: 1, Viirs: 2 } as const;

export interface GlobePointLayer {
  readonly count: number;
  /** xyz per point on the globe surface. */
  readonly positions: Float32Array;
  readonly kinds: Float32Array;
  /** Sensor bits matching `pointMatchesSensorFilter`, precomputed so the shader can filter. */
  readonly sensors: Float32Array;
  readonly duplicates: Float32Array;
  /** Point indices in draw order (low FRP first, intense fires on top), or null for layer order. */
  readonly drawOrder: Uint32Array | null;
}

/** Stable counting sort of point indices by kind, so higher FRP buckets draw last. */
export function drawOrderByKind(kinds: Float32Array): Uint32Array {
  const slots = PointKind.FrpBase + 9;
  const slot = (kind: number) => Math.min(slots - 1, Math.max(0, Math.round(kind)));
  const starts = new Uint32Array(slots + 1);
  for (let i = 0; i < kinds.length; i++) starts[slot(kinds[i]) + 1]++;
  for (let k = 1; k <= slots; k++) starts[k] += starts[k - 1];
  const order = new Uint32Array(kinds.length);
  for (let i = 0; i < kinds.length; i++) order[starts[slot(kinds[i])]++] = i;
  return order;
}

export function sensorBitsFor(flag: number, shape: number): number {
  const bits = flag & (PointFlag.Modis | PointFlag.Viirs);
  if (bits !== 0) {
    return ((bits & PointFlag.Modis) !== 0 ? SensorBit.Modis : 0) |
      ((bits & PointFlag.Viirs) !== 0 ? SensorBit.Viirs : 0);
  }
  return shape === PointShape.HollowCircle ? SensorBit.Modis : SensorBit.Viirs;
}

/** Same predicate as `pointMatchesSensorFilter`, expressed on the precomputed bits. */
export function sensorBitsVisible(bits: number, filter: SensorFilter): boolean {
  return (
    ((bits & SensorBit.Modis) !== 0 && filter.modis) ||
    ((bits & SensorBit.Viirs) !== 0 && filter.viirs)
  );
}

/** `layer` must come from `buildScenePoints(…, LONLAT_PROJECTION)`, so xs/ys hold lon/lat. */
export function buildGlobeLayer(layer: ScenePointLayer, harmonized: boolean): GlobePointLayer {
  const { count, xs, ys, buckets, shapes, flags } = layer;
  const positions = new Float32Array(count * 3);
  const kinds = new Float32Array(count);
  const sensors = new Float32Array(count);
  const duplicates = new Float32Array(count);
  for (let i = 0; i < count; i++) {
    const [x, y, z] = lonLatToVec(xs[i], ys[i], POINT_RADIUS);
    positions[i * 3] = x;
    positions[i * 3 + 1] = y;
    positions[i * 3 + 2] = z;
    kinds[i] = harmonized
      ? PointKind.FrpBase + Math.min(8, Math.max(1, buckets[i] || 1))
      : shapes[i] === PointShape.HollowCircle
        ? PointKind.ModisRing
        : PointKind.ViirsSquare;
    sensors[i] = sensorBitsFor(flags[i], shapes[i]);
    duplicates[i] = (flags[i] & PointFlag.Duplicate) !== 0 ? 1 : 0;
  }
  return { count, positions, kinds, sensors, duplicates, drawOrder: harmonized ? drawOrderByKind(kinds) : null };
}
