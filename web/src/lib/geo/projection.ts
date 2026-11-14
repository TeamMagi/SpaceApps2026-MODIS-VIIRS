import type { GeoFeature } from "./types.ts";

export interface RegionProjection {
  project(lon: number, lat: number): readonly [x: number, y: number];
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
    return {
      project(lon: number, lat: number): readonly [x: number, y: number] {
        const dLonRad = wrapDeltaLon(lon - lon0Deg) * DEG_TO_RAD;
        const phi = lat * DEG_TO_RAD;
        const x = dLonRad * safeCos;
        const y = -(Math.sin(phi) - sinPhi0) / safeCos;
        return [x, y];
      },
    };
  }

  const c = cosPhi1 * cosPhi1 + 2 * n * sinPhi1;
  const rho0 = Math.sqrt(Math.max(0, c - 2 * n * Math.sin(phi0))) / n;

  return {
    project(lon: number, lat: number): readonly [x: number, y: number] {
      const dLonRad = wrapDeltaLon(lon - lon0Deg) * DEG_TO_RAD;
      const phi = lat * DEG_TO_RAD;
      const theta = n * dLonRad;
      const rho = Math.sqrt(Math.max(0, c - 2 * n * Math.sin(phi))) / n;
      const x = rho * Math.sin(theta);
      // Negate standard north-up Albers y so screen-like plane y grows downward.
      const y = rho * Math.cos(theta) - rho0;
      return [x, y];
    },
  };
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
