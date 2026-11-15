// Three-free tile geometry for the high-resolution imagery, so it can be unit tested under plain node.
// Tiles come from tools/build-globe-tiles.sh: equirectangular, row 0 starts at 90°N, col 0 at 180°W.

import type { Vec3 } from "./geoMath";

const DEG = Math.PI / 180;

export const TILE_COLS = 16;
export const TILE_ROWS = 8;
export const TILE_DEG = 360 / TILE_COLS;
export const TILE_PX = 1024;
/** Texels per degree of the high-resolution tiles. */
export const TILE_TEXELS_PER_DEG = TILE_PX / TILE_DEG;

export interface TileId {
  readonly row: number;
  readonly col: number;
}

export interface TileBounds {
  readonly west: number;
  readonly east: number;
  readonly south: number;
  readonly north: number;
}

export interface TileSphereParams {
  readonly phiStart: number;
  readonly phiLength: number;
  readonly thetaStart: number;
  readonly thetaLength: number;
}

export function tileKey(tile: TileId): string {
  return `${tile.row}_${tile.col}`;
}

export function tileBounds(tile: TileId): TileBounds {
  const west = -180 + tile.col * TILE_DEG;
  const north = 90 - tile.row * TILE_DEG;
  return { west, east: west + TILE_DEG, south: north - TILE_DEG, north };
}

/** SphereGeometry angles covering a tile, matching `lonLatToVec` (phi = lon + 180°, theta = 90° − lat). */
export function tileSphereParams(tile: TileId): TileSphereParams {
  const { west, north } = tileBounds(tile);
  return {
    phiStart: (west + 180) * DEG,
    phiLength: TILE_DEG * DEG,
    thetaStart: (90 - north) * DEG,
    thetaLength: TILE_DEG * DEG,
  };
}

function lonLatOf(v: Vec3): [lon: number, lat: number] {
  const r = Math.hypot(v[0], v[1], v[2]) || 1;
  const lat = 90 - Math.acos(Math.max(-1, Math.min(1, v[1] / r))) / DEG;
  const lon = Math.atan2(v[2], -v[0]) / DEG - 180;
  return [lon < -180 ? lon + 360 : lon, lat];
}

/** Signed longitude difference a − b wrapped into [-180, 180). */
function lonDelta(a: number, b: number): number {
  return ((((a - b + 180) % 360) + 360) % 360) - 180;
}

/** Cosine of the angle between (lon0, lat0) and the closest point of a meridian arc. */
function meridianArcCos(lat0: number, dLon: number, south: number, north: number): number {
  const a = Math.sin(lat0 * DEG);
  const b = Math.cos(lat0 * DEG) * Math.cos(dLon * DEG);
  const at = (lat: number) => a * Math.sin(lat * DEG) + b * Math.cos(lat * DEG);
  let best = Math.max(at(south), at(north));
  const peak = Math.atan2(a, b) / DEG;
  if (peak > south && peak < north) best = Math.max(best, at(peak));
  return best;
}

/** Great-circle angle (radians) from a lon/lat position to the nearest point of a tile. */
export function angleToTile(lon: number, lat: number, tile: TileId): number {
  const { west, south, north } = tileBounds(tile);
  const fromWest = lonDelta(lon, west);
  if (fromWest >= 0 && fromWest <= TILE_DEG) {
    const off = lat > north ? lat - north : lat < south ? south - lat : 0;
    return off * DEG;
  }
  const dWest = Math.abs(fromWest);
  const dEast = Math.abs(lonDelta(lon, west + TILE_DEG));
  const cos = Math.max(meridianArcCos(lat, dWest, south, north), meridianArcCos(lat, dEast, south, north));
  return Math.acos(Math.max(-1, Math.min(1, cos)));
}

/**
 * Tiles touching the spherical cap of angular radius `capRadius` (radians) around `direction`,
 * nearest first. Handles the antimeridian and the poles.
 */
export function tilesInCap(direction: Vec3, capRadius: number): TileId[] {
  const [lon, lat] = lonLatOf(direction);
  const hits: { tile: TileId; angle: number }[] = [];
  for (let row = 0; row < TILE_ROWS; row++) {
    for (let col = 0; col < TILE_COLS; col++) {
      const tile = { row, col };
      const angle = angleToTile(lon, lat, tile);
      if (angle <= capRadius) hits.push({ tile, angle });
    }
  }
  hits.sort((a, b) => a.angle - b.angle || a.tile.row - b.tile.row || a.tile.col - b.tile.col);
  return hits.map((h) => h.tile);
}

/**
 * Angular radius (radians) of the globe area a camera at `distance` (globe radius 1) looking at the
 * centre can see: the frustum corner's footprint, or the horizon once the corner ray misses the globe.
 */
export function visibleCapRadius(distance: number, verticalFov: number, aspect: number): number {
  if (!(distance > 1)) return 0;
  const horizon = Math.acos(1 / distance);
  const tanHalf = Math.tan(verticalFov / 2);
  const corner = Math.atan(tanHalf * Math.sqrt(1 + aspect * aspect));
  const s = distance * Math.sin(corner);
  if (s >= 1) return horizon;
  return Math.min(horizon, Math.asin(s) - corner);
}

/** Screen pixels per degree of surface at the centre of the view, for a viewport `heightPx` tall. */
export function surfacePxPerDegree(distance: number, verticalFov: number, heightPx: number): number {
  const altitude = Math.max(1e-6, distance - 1);
  return (heightPx * DEG) / (2 * Math.tan(verticalFov / 2) * altitude);
}
