import assert from "node:assert/strict";
import { performance } from "node:perf_hooks";
import { angularDistance, lonLatToVec, vecToLonLat } from "../src/components/Map/globe/geoMath.ts";
import {
  angleToTile,
  surfacePxPerDegree,
  TILE_COLS,
  TILE_DEG,
  TILE_ROWS,
  tileBounds,
  tileKey,
  tilesInCap,
  tileSphereParams,
  visibleCapRadius,
  type TileId,
} from "../src/components/Map/globe/tileMath.ts";

const t0 = performance.now();
const DEG = Math.PI / 180;

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

/** Brute-force nearest angle from a lon/lat to a tile, sampling its outline and interior. */
function bruteAngle(lon: number, lat: number, tile: TileId): number {
  const { west, south } = tileBounds(tile);
  const steps = 90;
  let best = Infinity;
  for (let i = 0; i <= steps; i++) {
    for (let j = 0; j <= steps; j++) {
      const a = angularDistance(lon, lat, west + (TILE_DEG * i) / steps, south + (TILE_DEG * j) / steps);
      if (a < best) best = a;
    }
  }
  return best;
}

function keys(tiles: readonly TileId[]): string[] {
  return tiles.map(tileKey).sort();
}

/** Deterministic pseudo-random sequence so failures reproduce. */
function rng(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 2 ** 32;
  };
}

check("tileBounds tiles the whole globe from 180°W / 90°N", () => {
  assert.equal(TILE_COLS * TILE_DEG, 360);
  assert.equal(TILE_ROWS * TILE_DEG, 180);
  assert.deepEqual(tileBounds({ row: 0, col: 0 }), { west: -180, east: -157.5, south: 67.5, north: 90 });
  assert.deepEqual(tileBounds({ row: 7, col: 15 }), { west: 157.5, east: 180, south: -90, north: -67.5 });
  // Turkey (≈26–45°E, 36–42°N) sits in row 2, cols 9 and 10.
  assert.deepEqual(tileBounds({ row: 2, col: 9 }), { west: 22.5, east: 45, south: 22.5, north: 45 });
});

check("tileSphereParams corners land on the tile's lon/lat corners", () => {
  for (const tile of [{ row: 0, col: 0 }, { row: 2, col: 9 }, { row: 7, col: 15 }, { row: 4, col: 7 }]) {
    const p = tileSphereParams(tile);
    const b = tileBounds(tile);
    for (const [u, v, lon, lat] of [
      [0, 0, b.west, b.north],
      [1, 0, b.east, b.north],
      [0, 1, b.west, b.south],
      [1, 1, b.east, b.south],
      [0.5, 0.5, (b.west + b.east) / 2, (b.north + b.south) / 2],
    ] as const) {
      // three's SphereGeometry vertex formula.
      const phi = p.phiStart + u * p.phiLength;
      const theta = p.thetaStart + v * p.thetaLength;
      const vertex = [-Math.cos(phi) * Math.sin(theta), Math.cos(theta), Math.sin(phi) * Math.sin(theta)];
      const expected = lonLatToVec(lon, lat);
      for (let k = 0; k < 3; k++) assert.ok(Math.abs(vertex[k] - expected[k]) < 1e-12, `${tileKey(tile)} ${u},${v}`);
    }
  }
});

check("angleToTile matches a brute-force search, across the antimeridian and poles", () => {
  const random = rng(7);
  for (let n = 0; n < 400; n++) {
    const lon = -180 + random() * 360;
    const lat = -90 + random() * 180;
    const tile = { row: Math.floor(random() * TILE_ROWS), col: Math.floor(random() * TILE_COLS) };
    const exact = angleToTile(lon, lat, tile);
    const brute = bruteAngle(lon, lat, tile);
    // The exact distance is never above the sampled one, and within the grid spacing of it.
    assert.ok(exact <= brute + 1e-9, `${lon},${lat} ${tileKey(tile)}: ${exact} > ${brute}`);
    assert.ok(brute - exact < 0.2 * DEG, `${lon},${lat} ${tileKey(tile)}: ${exact} vs ${brute}`);
  }
  assert.equal(angleToTile(30, 40, { row: 2, col: 9 }), 0);
  // 179.5°E is half a degree from tile col 0 (starting at 180°W) along the equator.
  assert.ok(Math.abs(angleToTile(179.5, 0, { row: 3, col: 0 }) - 0.5 * DEG) < 1e-9);
  // Any longitude is on the pole, so the polar row is reached across it.
  assert.ok(Math.abs(angleToTile(0, 89, { row: 0, col: 8 + 4 }) - 1 * DEG) < 1e-9);
});

check("tilesInCap equals the brute-force set and lists the nearest tile first", () => {
  const random = rng(11);
  for (let n = 0; n < 60; n++) {
    const lon = -180 + random() * 360;
    const lat = -90 + random() * 180;
    const cap = (2 + random() * 40) * DEG;
    const got = tilesInCap(lonLatToVec(lon, lat), cap);
    const expected: TileId[] = [];
    for (let row = 0; row < TILE_ROWS; row++) {
      for (let col = 0; col < TILE_COLS; col++) {
        // Skip tiles within a hair of the cap edge, where sampling resolution decides.
        const brute = bruteAngle(lon, lat, { row, col });
        if (Math.abs(brute - cap) < 0.25 * DEG) continue;
        if (brute <= cap) expected.push({ row, col });
      }
    }
    const gotKeys = new Set(keys(got));
    for (const key of keys(expected)) assert.ok(gotKeys.has(key), `${lon},${lat} cap ${cap / DEG}: missing ${key}`);
    const [home] = got;
    const [hLon, hLat] = vecToLonLat(...lonLatToVec(lon, lat));
    assert.equal(angleToTile(hLon, hLat, home), 0);
  }
});

check("tilesInCap wraps the antimeridian and covers the poles", () => {
  assert.deepEqual(keys(tilesInCap(lonLatToVec(179.9, 10), 2 * DEG)), ["3_0", "3_15"]);
  assert.deepEqual(keys(tilesInCap(lonLatToVec(-179.9, -10), 2 * DEG)), ["4_0", "4_15"]);
  const north = tilesInCap(lonLatToVec(0, 90), 5 * DEG);
  assert.equal(north.length, TILE_COLS);
  assert.ok(north.every((t) => t.row === 0));
  const south = tilesInCap(lonLatToVec(123, -88), 5 * DEG);
  assert.equal(south.length, TILE_COLS);
  assert.ok(south.every((t) => t.row === TILE_ROWS - 1));
  assert.equal(tilesInCap(lonLatToVec(10, 0), Math.PI).length, TILE_ROWS * TILE_COLS);
});

check("visibleCapRadius grows with distance and stops at the horizon", () => {
  const fov = 30 * DEG;
  assert.equal(visibleCapRadius(1, fov, 1.5), 0);
  let prev = 0;
  for (const d of [1.01, 1.05, 1.2, 1.6, 2, 3, 6]) {
    const cap = visibleCapRadius(d, fov, 1.5);
    assert.ok(cap > prev, `cap at ${d}`);
    assert.ok(cap <= Math.acos(1 / d) + 1e-12);
    prev = cap;
  }
  // Far away the frustum corner misses the globe, so the whole visible hemisphere counts.
  assert.ok(Math.abs(visibleCapRadius(6, fov, 1.5) - Math.acos(1 / 6)) < 1e-12);
  // Close up the corner footprint is tiny: about altitude × tan(corner half-angle).
  const corner = Math.atan(Math.tan(fov / 2) * Math.sqrt(1 + 1.5 * 1.5));
  assert.ok(Math.abs(visibleCapRadius(1.01, fov, 1.5) - 0.01 * Math.tan(corner)) < 2e-4);
});

check("surfacePxPerDegree halves when the altitude doubles", () => {
  const fov = 30 * DEG;
  const a = surfacePxPerDegree(1.2, fov, 600);
  const b = surfacePxPerDegree(1.4, fov, 600);
  assert.ok(Math.abs(a / b - 2) < 1e-9);
  // Altitude 0.2 with a 30° field: 600 px span 2·0.2·tan(15°) ≈ 0.107 rad ≈ 6.14°.
  assert.ok(Math.abs(a - 600 / ((2 * 0.2 * Math.tan(fov / 2)) / DEG)) < 1e-9);
});

console.log(`globe tile tests done in ${(performance.now() - t0).toFixed(1)} ms`);
