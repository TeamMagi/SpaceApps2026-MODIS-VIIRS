import type { MapPointSet } from "../types";

export interface PointFeatureCollection {
  type: "FeatureCollection";
  /** Present only on client-built demo files, which hold synthetic data. */
  synthetic?: true;
  data_source?: "synthetic_demo";
  view: "harmonized" | "raw";
  features: Array<{
    type: "Feature";
    geometry: { type: "Point"; coordinates: [number, number] };
    properties: Record<string, string | number | null>;
  }>;
}

/**
 * GeoJSON of what the map currently shows, mirroring the CSV columns. Coordinates are [lon, lat]
 * (RFC 7946); lat/lon are not repeated in properties. The raw file omits the merge links.
 */
export function mapPointsToGeoJson(points: MapPointSet, harmonized: boolean, isDemo = false): PointFeatureCollection {
  const features: PointFeatureCollection["features"] = harmonized
    ? points.harmonized.map((p) => ({
        type: "Feature",
        geometry: { type: "Point", coordinates: [p.lon, p.lat] },
        properties: {
          id: p.id,
          frp_mw: p.frp,
          frp_corrected_mw: p.frpCorrected,
          source: p.source,
          confidence: p.confidence,
          daynight: p.daynight,
          acquired_utc: p.acquired,
        },
      }))
    : points.raw.map((p) => ({
        type: "Feature",
        geometry: { type: "Point", coordinates: [p.lon, p.lat] },
        properties: {
          id: p.id,
          sensor: p.sensor,
          frp_mw: p.frp,
          confidence: p.confidence,
          daynight: p.daynight,
          acquired_utc: p.acquired,
        },
      }));
  return {
    type: "FeatureCollection",
    ...(isDemo ? { synthetic: true as const, data_source: "synthetic_demo" as const } : {}),
    view: harmonized ? "harmonized" : "raw",
    features,
  };
}
