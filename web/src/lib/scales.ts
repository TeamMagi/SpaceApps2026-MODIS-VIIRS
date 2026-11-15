import type { BBox } from "../types";

/** Upper bounds (MW) of FRP buckets 1..7; bucket 8 is everything above. Log-spaced. */
export const FRP_BREAKS = [5, 10, 20, 40, 80, 160, 320] as const;

const MIN_QUANTILE_SAMPLE = 64;

export type FrpBucket = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;

/**
 * Computes `buckets - 1` strictly ascending break values from quantiles of finite positive
 * values. Returns `FRP_BREAKS` by reference when fewer than 64 usable values exist or when
 * duplicates collapse any break interval.
 */
export function computeQuantileBreaks(
  values: readonly number[],
  buckets = 8,
): readonly number[] {
  if (!Number.isInteger(buckets) || buckets < 2) return FRP_BREAKS;

  const usable: number[] = [];
  for (const v of values) {
    if (Number.isFinite(v) && v > 0) usable.push(v);
  }
  if (usable.length < MIN_QUANTILE_SAMPLE) return FRP_BREAKS;

  usable.sort((a, b) => a - b);

  const breaks: number[] = [];
  for (let i = 1; i < buckets; i += 1) {
    const pos = (i / buckets) * (usable.length - 1);
    const lo = Math.floor(pos);
    const hi = Math.ceil(pos);
    const weight = pos - lo;
    const value = usable[lo] * (1 - weight) + usable[hi] * weight;
    if (!(value > 0) || (breaks.length > 0 && !(value > breaks[breaks.length - 1]))) {
      return FRP_BREAKS;
    }
    breaks.push(value);
  }
  return breaks;
}

export function frpBucketFor(frp: number, breaks: readonly number[] = FRP_BREAKS): FrpBucket {
  const index = breaks.findIndex((upper) => frp < upper);
  const rawBucket = index === -1 ? breaks.length + 1 : index + 1;
  return Math.min(8, Math.max(1, rawBucket)) as FrpBucket;
}

export function frpBucket(frp: number): FrpBucket {
  return frpBucketFor(frp, FRP_BREAKS);
}

/** CSS colour for an FRP value, following the theme: `var(--frp-N)`. */
export function frpColorVar(frp: number, breaks: readonly number[] = FRP_BREAKS): string {
  return `var(--frp-${frpBucketFor(frp, breaks)})`;
}

export type AnomalyBucket = -3 | -2 | -1 | 0 | 1 | 2 | 3;

export function anomalyBucket(z: number): AnomalyBucket {
  if (z <= -2) return -3;
  if (z <= -1) return -2;
  if (z <= -0.5) return -1;
  if (z < 0.5) return 0;
  if (z < 1) return 1;
  if (z < 2) return 2;
  return 3;
}

/** CSS colour for an anomaly z-score (diverging scale); null = no record. */
export function anomalyColorVar(z: number | null): string {
  if (z === null) return "var(--anom-none)";
  const bucket = anomalyBucket(z);
  if (bucket === 0) return "var(--anom-0)";
  return bucket < 0 ? `var(--anom-neg-${-bucket})` : `var(--anom-pos-${bucket})`;
}

export type AnomalyCopyKey = "anomalyWellAbove" | "anomalyAbove" | "anomalyNear" | "anomalyBelow" | "anomalyWellBelow";

/** Plain-language anomaly phrase key; its template takes `{month}`. */
export function anomalyCopyKey(z: number): AnomalyCopyKey {
  if (z >= 2) return "anomalyWellAbove";
  if (z >= 1) return "anomalyAbove";
  if (z > -1) return "anomalyNear";
  if (z > -2) return "anomalyBelow";
  return "anomalyWellBelow";
}

/**
 * Equirectangular projection into the response bbox: x 0..1 west->east, y 0..1 north->south.
 */
export function projectToExtent(bbox: BBox, lat: number, lon: number): { x: number; y: number } {
  const lonSpan = bbox.east - bbox.west;
  const latSpan = bbox.north - bbox.south;
  return {
    x: lonSpan > 0 ? (lon - bbox.west) / lonSpan : 0.5,
    y: latSpan > 0 ? (bbox.north - lat) / latSpan : 0.5,
  };
}

/** Width / height of the extent with a cos(latitude) correction, for the SVG viewBox. */
export function extentAspect(bbox: BBox): number {
  const lonSpan = Math.max(bbox.east - bbox.west, 0.1);
  const latSpan = Math.max(bbox.north - bbox.south, 0.1);
  const midLat = ((bbox.north + bbox.south) / 2) * (Math.PI / 180);
  const rawAspect = (lonSpan * Math.max(Math.cos(midLat), 0.1)) / latSpan;
  return Math.min(Math.max(rawAspect, 0.5), 2.5);
}
