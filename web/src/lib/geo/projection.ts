import type { GeoFeature, Position } from "./types.ts";

export interface RegionProjection {
  /** Central meridian; `project` wraps longitudes into centerLon ± 180. */
  readonly centerLon: number;
  project(lon: number, lat: number): readonly [x: number, y: number];
  /** Like `project` without wrapping, for longitudes already unwrapped around `centerLon`. */
  projectUnwrapped(lon: number, lat: number): readonly [x: number, y: number];
}

export type GeoBBox = readonly [west: number, south: number, east: number, north: number];

export interface PlaneBounds {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

export interface FitTransform {
  k: number;
  tx: number;
  ty: number;
}

const DEG_TO_RAD = Math.PI / 180;
const PLANE_SAMPLE_STEPS = 24;

function wrapDeltaLon(deltaDeg: number): number {
  if (deltaDeg >= -180 && deltaDeg <= 180) return deltaDeg;
  const wrapped = ((((deltaDeg + 180) % 360) + 360) % 360) - 180;
  return wrapped === -180 && deltaDeg > 0 ? 180 : wrapped;
}

export function createProjection(bbox: GeoBBox): RegionProjection {
  const [west, south, east, north] = bbox;
  const lon0Deg = (west + east) / 2;
  const lat0Deg = (south + north) / 2;
  const latSpan = north - south;
  const phi1 = (south + latSpan / 6) * DEG_TO_RAD;
  const phi2 = (south + (5 * latSpan) / 6) * DEG_TO_RAD;
  const phi0 = lat0Deg * DEG_TO_RAD;

  const sinPhi1 = Math.sin(phi1);
  const cosPhi1 = Math.cos(phi1);
  const n = 0.5 * (sinPhi1 + Math.sin(phi2));

  // Near-equatorial symmetric parallels degenerate to cylindrical equal-area.
  if (Math.abs(n) < 1e-7) {
    const safeCos = Math.abs(cosPhi1) < 1e-7 ? 1 : cosPhi1;
    const sinPhi0 = Math.sin(phi0);
    const projectDelta = (dLonDeg: number, lat: number): readonly [x: number, y: number] => {
      const x = dLonDeg * DEG_TO_RAD * safeCos;
      const y = -(Math.sin(lat * DEG_TO_RAD) - sinPhi0) / safeCos;
      return [x, y];
    };
    return {
      centerLon: lon0Deg,
      project: (lon, lat) => projectDelta(wrapDeltaLon(lon - lon0Deg), lat),
      projectUnwrapped: (lon, lat) => projectDelta(lon - lon0Deg, lat),
    };
  }

  const c = cosPhi1 * cosPhi1 + 2 * n * sinPhi1;
  const rho0 = Math.sqrt(Math.max(0, c - 2 * n * Math.sin(phi0))) / n;

  const projectDelta = (dLonDeg: number, lat: number): readonly [x: number, y: number] => {
    const theta = n * dLonDeg * DEG_TO_RAD;
    const rho = Math.sqrt(Math.max(0, c - 2 * n * Math.sin(lat * DEG_TO_RAD))) / n;
    const x = rho * Math.sin(theta);
    // Negate standard north-up Albers y so screen-like plane y grows downward.
    const y = rho * Math.cos(theta) - rho0;
    return [x, y];
  };
  return {
    centerLon: lon0Deg,
    project: (lon, lat) => projectDelta(wrapDeltaLon(lon - lon0Deg), lat),
    projectUnwrapped: (lon, lat) => projectDelta(lon - lon0Deg, lat),
  };
}

/**
 * Projects a ring as one continuous piece: longitudes are unwrapped vertex to vertex and the ring is
 * placed on the side of the projection seam (centerLon + 180) where most of it lies. Wrapping each
 * vertex on its own would draw a ring that straddles the seam (e.g. Alaska seen from Türkiye) as a
 * band across the whole map.
 */
export function projectRing(projection: RegionProjection, ring: readonly Position[]): Float32Array {
  const n = ring.length;
  const out = new Float32Array(n * 2);
  if (n === 0) return out;
  const lons = new Float64Array(n);
  let lon = projection.centerLon + wrapDeltaLon(ring[0][0] - projection.centerLon);
  let sum = 0;
  for (let i = 0; i < n; i++) {
    if (i > 0) lon += wrapDeltaLon(ring[i][0] - ring[i - 1][0]);
    lons[i] = lon;
    sum += lon;
  }
  const shift = -360 * Math.round((sum / n - projection.centerLon) / 360);
  for (let i = 0; i < n; i++) {
    const [x, y] = projection.projectUnwrapped(lons[i] + shift, ring[i][1]);
    out[i * 2] = x;
    out[i * 2 + 1] = y;
  }
  return out;
}

export function planeBounds(projection: RegionProjection, bbox: GeoBBox): PlaneBounds {
  const [west, south, east, north] = bbox;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  const visit = (lon: number, lat: number) => {
    const [x, y] = projection.project(lon, lat);
    if (x < minX) minX = x;
    if (y < minY) minY = y;
    if (x > maxX) maxX = x;
    if (y > maxY) maxY = y;
  };

  for (let i = 0; i <= PLANE_SAMPLE_STEPS; i++) {
    const t = i / PLANE_SAMPLE_STEPS;
    const lon = west + (east - west) * t;
    const lat = south + (north - south) * t;
    visit(lon, south);
    visit(lon, north);
    visit(west, lat);
    visit(east, lat);
  }

  return { minX, minY, maxX, maxY };
}

export function fitTransform(
  bounds: PlaneBounds,
  width: number,
  height: number,
  padding: number,
): FitTransform {
  const innerW = Math.max(0, width - 2 * padding);
  const innerH = Math.max(0, height - 2 * padding);
  const spanX = bounds.maxX - bounds.minX;
  const spanY = bounds.maxY - bounds.minY;

  if (innerW === 0 || innerH === 0) {
    return { k: 0, tx: width / 2, ty: height / 2 };
  }

  let k: number;
  if (spanX <= 0 && spanY <= 0) {
    k = 1;
  } else if (spanX <= 0) {
    k = innerH / spanY;
  } else if (spanY <= 0) {
    k = innerW / spanX;
  } else {
    k = Math.min(innerW / spanX, innerH / spanY);
  }

  const cx = (bounds.minX + bounds.maxX) / 2;
  const cy = (bounds.minY + bounds.maxY) / 2;
  return {
    k,
    tx: width / 2 - cx * k,
    ty: height / 2 - cy * k,
  };
}

export function regionExtent(features: readonly GeoFeature[]): GeoBBox {
  if (features.length === 0) {
    throw new Error("regionExtent requires at least one feature");
  }
  let west = Infinity;
  let south = Infinity;
  let east = -Infinity;
  let north = -Infinity;
  for (const feature of features) {
    const [w, s, e, n] = feature.bbox;
    if (w < west) west = w;
    if (s < south) south = s;
    if (e > east) east = e;
    if (n > north) north = n;
  }
  return [west, south, east, north];
}

export function padBBox(bbox: GeoBBox, fraction: number): GeoBBox {
  const [west, south, east, north] = bbox;
  const padLon = (east - west) * fraction;
  const padLat = (north - south) * fraction;
  return [
    west - padLon,
    Math.max(-90, south - padLat),
    east + padLon,
    Math.min(90, north + padLat),
  ];
}
