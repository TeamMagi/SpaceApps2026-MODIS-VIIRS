import type { BBox } from "../types";

/** Upper bounds (MW) of FRP buckets 1..7; bucket 8 is everything above. Log-spaced. */
export const FRP_BREAKS = [5, 10, 20, 40, 80, 160, 320] as const;

export type FrpBucket = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;

export function frpBucket(frp: number): FrpBucket {
  const index = FRP_BREAKS.findIndex((upper) => frp < upper);
  return (index === -1 ? 8 : index + 1) as FrpBucket;
}

/** CSS colour for an FRP value, following the theme: `var(--frp-N)`. */
export function frpColorVar(frp: number): string {
  return `var(--frp-${frpBucket(frp)})`;
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
