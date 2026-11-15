// Three-free sphere math so it can be unit tested under plain node.

import type { GeoFeature } from "../../../lib/geo/types";

const DEG = Math.PI / 180;

export type Vec3 = readonly [x: number, y: number, z: number];

/** Spherical position for a lon/lat pair at radius `r`, matching three's SphereGeometry UVs. */
export function lonLatToVec(lon: number, lat: number, r = 1): [x: number, y: number, z: number] {
  const phi = (lon + 180) * DEG;
  const theta = (90 - lat) * DEG;
  return [
    -r * Math.cos(phi) * Math.sin(theta),
    r * Math.cos(theta),
    r * Math.sin(phi) * Math.sin(theta),
  ];
}

/** Inverse of `lonLatToVec`; longitude comes back in [-180, 180). */
export function vecToLonLat(x: number, y: number, z: number): [lon: number, lat: number] {
  const r = Math.hypot(x, y, z);
  if (r === 0) return [0, 0];
  const lat = 90 - Math.acos(Math.max(-1, Math.min(1, y / r))) / DEG;
  let lon = Math.atan2(z, -x) / DEG - 180;
  if (lon < -180) lon += 360;
  if (lon >= 180) lon -= 360;
  return [lon, lat];
}

export function wrapLon(lon: number): number {
  const wrapped = ((((lon + 180) % 360) + 360) % 360) - 180;
  return wrapped;
}

/** Great-circle angle (radians) between two lon/lat positions. */
export function angularDistance(lon1: number, lat1: number, lon2: number, lat2: number): number {
  const a = lonLatToVec(lon1, lat1);
  const b = lonLatToVec(lon2, lat2);
  const dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  return Math.acos(Math.max(-1, Math.min(1, dot)));
}

/** Cubic ease-in-out used by the camera flight. */
export function easeInOutCubic(t: number): number {
  const v = t <= 0 ? 0 : t >= 1 ? 1 : t;
  return v < 0.5 ? 4 * v ** 3 : 1 - (-2 * v + 2) ** 3 / 2;
}

/** Camera distance (globe radius 1) at which the whole visible hemisphere just fits the half-angle. */
export function globeFitDistance(halfAngle: number): number {
  return 1 / Math.sin(halfAngle);
}

/**
 * Camera distance from the globe centre that frames a spherical cap of angular radius `capRadius`
 * (radians, centred on the view axis) inside a view cone of half-angle `halfAngle`.
 * The cap edge must also stay in front of the horizon, so the result is never closer than
 * 1 / cos(capRadius); caps past the horizon fall back to the whole-globe distance.
 */
export function fitDistance(capRadius: number, halfAngle: number): number {
  const whole = globeFitDistance(halfAngle);
  if (!(capRadius > 0)) return 1;
  if (capRadius >= Math.PI / 2) return whole;
  const cone = Math.cos(capRadius) + Math.sin(capRadius) / Math.tan(halfAngle);
  const horizon = 1 / Math.cos(capRadius);
  return Math.min(whole, Math.max(cone, horizon));
}

/** Half-angle of the narrower view axis for a vertical field of view (radians) and aspect ratio. */
export function viewHalfAngle(verticalFov: number, aspect: number): number {
  const halfV = verticalFov / 2;
  if (!(aspect > 0) || aspect >= 1) return halfV;
  return Math.atan(Math.tan(halfV) * aspect);
}

export interface GeoFrame {
  /** Unit vector from the globe centre toward the framed area. */
  readonly direction: Vec3;
  /** Angular radius (radians) of the smallest cap around `direction` holding every vertex. */
  readonly radius: number;
}

/**
 * Frame for an unwrapped lon/lat bbox (east may exceed 180) and the vertices that must be shown.
 * The centre is the bbox midpoint, matching how the flat map centres a region.
 */
export function frameForBBox(
  bbox: readonly [west: number, south: number, east: number, north: number],
  vertices: Iterable<readonly [number, number]>,
): GeoFrame {
  const [west, south, east, north] = bbox;
  const centerLon = wrapLon((west + east) / 2);
  const centerLat = Math.max(-80, Math.min(80, (south + north) / 2));
  const direction = lonLatToVec(centerLon, centerLat);
  let radius = 0;
  let any = false;
  for (const [lon, lat] of vertices) {
    any = true;
    const v = lonLatToVec(lon, lat);
    const dot = v[0] * direction[0] + v[1] * direction[1] + v[2] * direction[2];
    const angle = Math.acos(Math.max(-1, Math.min(1, dot)));
    if (angle > radius) radius = angle;
  }
  if (!any) {
    for (const [lon, lat] of [
      [west, south],
      [west, north],
      [east, south],
      [east, north],
    ] as const) {
      radius = Math.max(radius, angularDistance(centerLon, centerLat, lon, lat));
    }
  }
  return { direction, radius };
}

/** Frame for a set of boundary features, every ring vertex included. */
export function frameForFeatures(features: readonly GeoFeature[]): GeoFrame | null {
  if (features.length === 0) return null;
  let west = Infinity;
  let south = Infinity;
  let east = -Infinity;
  let north = -Infinity;
  for (const f of features) {
    const [w, s, e, n] = f.bbox;
    if (w < west) west = w;
    if (s < south) south = s;
    if (e > east) east = e;
    if (n > north) north = n;
  }
  function* vertices(): Generator<readonly [number, number]> {
    for (const f of features) {
      for (const polygon of f.polygons) {
        for (const ring of polygon) {
          for (const p of ring) yield p;
        }
      }
    }
  }
  return frameForBBox([west, south, east, north], vertices());
}

/** True when a surface point faces the camera (lies on the visible hemisphere). */
export function facesCamera(camera: Vec3, x: number, y: number, z: number): boolean {
  return x * (camera[0] - x) + y * (camera[1] - y) + z * (camera[2] - z) > 0;
}

export interface ScreenProjector {
  /** Column-major view-projection matrix. */
  readonly matrix: ArrayLike<number>;
  readonly camera: Vec3;
  /** Viewport size in CSS px. */
  readonly width: number;
  readonly height: number;
}

/** Screen position (CSS px) of a world-space point, or null when it is behind the camera. */
export function projectToScreen(
  projector: ScreenProjector,
  x: number,
  y: number,
  z: number,
): [sx: number, sy: number] | null {
  const m = projector.matrix;
  const w = m[3] * x + m[7] * y + m[11] * z + m[15];
  if (w <= 0) return null;
  const nx = (m[0] * x + m[4] * y + m[8] * z + m[12]) / w;
  const ny = (m[1] * x + m[5] * y + m[9] * z + m[13]) / w;
  return [((nx + 1) / 2) * projector.width, ((1 - ny) / 2) * projector.height];
}

/**
 * Index of the visible-hemisphere point nearest to a screen position within `maxPx`, or -1.
 * `positions` holds xyz triples; `accept` filters (e.g. by sensor) before the distance test.
 */
export function nearestScreenPoint(
  positions: Float32Array,
  count: number,
  projector: ScreenProjector,
  sx: number,
  sy: number,
  maxPx: number,
  accept?: (index: number) => boolean,
): number {
  let best = -1;
  let bestD2 = maxPx * maxPx;
  for (let i = 0; i < count; i++) {
    const x = positions[i * 3];
    const y = positions[i * 3 + 1];
    const z = positions[i * 3 + 2];
    if (!facesCamera(projector.camera, x, y, z)) continue;
    if (accept && !accept(i)) continue;
    const p = projectToScreen(projector, x, y, z);
    if (!p) continue;
    const d2 = (p[0] - sx) ** 2 + (p[1] - sy) ** 2;
    if (d2 <= bestD2) {
      bestD2 = d2;
      best = i;
    }
  }
  return best;
}

/** Indices of accepted points on the visible hemisphere that land inside the viewport (± margin). */
export function pointsInView(
  positions: Float32Array,
  count: number,
  projector: ScreenProjector,
  marginPx: number,
  accept?: (index: number) => boolean,
): number[] {
  const out: number[] = [];
  for (let i = 0; i < count; i++) {
    const x = positions[i * 3];
    const y = positions[i * 3 + 1];
    const z = positions[i * 3 + 2];
    if (!facesCamera(projector.camera, x, y, z)) continue;
    if (accept && !accept(i)) continue;
    const p = projectToScreen(projector, x, y, z);
    if (!p) continue;
    if (
      p[0] >= -marginPx &&
      p[0] <= projector.width + marginPx &&
      p[1] >= -marginPx &&
      p[1] <= projector.height + marginPx
    ) {
      out.push(i);
    }
  }
  return out;
}
