import assert from "node:assert/strict";
import fs from "node:fs";
import { performance } from "node:perf_hooks";
import type { GeoDataset, GeoFeature } from "../src/lib/geo/types.ts";
import { buildScenePoints, pointMatchesSensorFilter } from "../src/components/Map/engine/renderer.ts";
import { PointFlag, PointShape } from "../src/components/Map/engine/types.ts";
import {
  angularDistance,
  easeInOutCubic,
  facesCamera,
  fitDistance,
  frameForBBox,
  frameForFeatures,
  globeFitDistance,
  lonLatToVec,
  nearestScreenPoint,
  pointsInView,
  projectToScreen,
  vecToLonLat,
  viewHalfAngle,
  wrapLon,
  type ScreenProjector,
} from "../src/components/Map/globe/geoMath.ts";
import { buildPickFeatures, pickFeatureAt } from "../src/components/Map/globe/featurePick.ts";
import {
  buildGlobeLayer,
  drawOrderByKind,
  LONLAT_PROJECTION,
  POINT_RADIUS,
  sensorBitsFor,
  sensorBitsVisible,
} from "../src/components/Map/globe/pointAttributes.ts";

const t0 = performance.now();
const DEG = Math.PI / 180;

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

function close(a: number, b: number, eps = 1e-9): boolean {
  return Math.abs(a - b) <= eps;
}

check("lonLatToVec puts the poles, prime meridian and 90°E on the expected axes", () => {
  const north = lonLatToVec(0, 90);
  assert.ok(close(north[0], 0) && close(north[1], 1) && close(north[2], 0));
  const zero = lonLatToVec(0, 0);
  assert.ok(close(zero[0], 1) && close(zero[1], 0) && close(zero[2], 0, 1e-12));
  const east = lonLatToVec(90, 0);
  assert.ok(close(east[0], 0, 1e-12) && close(east[2], -1));
  const r = lonLatToVec(37, -12, 2.5);
  assert.ok(close(Math.hypot(...r), 2.5));
});

check("vecToLonLat inverts lonLatToVec across the globe, longitudes in [-180, 180)", () => {
  for (let lat = -85; lat <= 85; lat += 17) {
    for (let lon = -180; lon < 180; lon += 23) {
      const [x, y, z] = lonLatToVec(lon, lat, 1.006);
      const [lon2, lat2] = vecToLonLat(x, y, z);
      assert.ok(close(lat2, lat, 1e-9), `lat ${lat} -> ${lat2}`);
      assert.ok(close(wrapLon(lon2 - lon), 0, 1e-9), `lon ${lon} -> ${lon2}`);
      assert.ok(lon2 >= -180 && lon2 < 180);
    }
  }
  // Longitudes past the antimeridian (as in unwrapped rings) land back in range.
  const [lon] = vecToLonLat(...lonLatToVec(190.27, 60));
  assert.ok(close(lon, -169.73, 1e-9));
  assert.deepEqual(vecToLonLat(0, 0, 0), [0, 0]);
});

check("wrapLon and angularDistance handle the antimeridian", () => {
  assert.ok(close(wrapLon(190), -170));
  assert.ok(close(wrapLon(-190), 170));
  assert.ok(close(wrapLon(45), 45));
  assert.ok(close(angularDistance(179.5, 0, -179.5, 0), 1 * DEG, 1e-9));
  assert.ok(close(angularDistance(0, 90, 120, 90), 0, 1e-7));
});

check("easeInOutCubic is clamped, symmetric and monotonic", () => {
  assert.equal(easeInOutCubic(-1), 0);
  assert.equal(easeInOutCubic(0), 0);
  assert.equal(easeInOutCubic(1), 1);
  assert.equal(easeInOutCubic(2), 1);
  assert.ok(close(easeInOutCubic(0.5), 0.5));
  let prev = 0;
  for (let t = 0.05; t <= 1; t += 0.05) {
    const v = easeInOutCubic(t);
    assert.ok(v >= prev);
    assert.ok(close(v + easeInOutCubic(1 - t), 1, 1e-12));
    prev = v;
  }
});

check("fitDistance frames a cap: grows with its radius, never past the whole-globe distance", () => {
  const half = 15 * DEG * 0.86;
  const whole = globeFitDistance(half);
  let prev = 1;
  for (let deg = 1; deg <= 89; deg += 4) {
    const d = fitDistance(deg * DEG, half);
    assert.ok(d >= prev, `monotonic at ${deg}°`);
    assert.ok(d <= whole + 1e-9);
    // The cap edge sits inside the view cone and in front of the horizon.
    const edgeAngle = Math.atan2(Math.sin(deg * DEG), d - Math.cos(deg * DEG));
    assert.ok(edgeAngle <= half + 1e-9 || close(d, whole, 1e-9));
    assert.ok(d * Math.cos(deg * DEG) >= 1 - 1e-9 || close(d, whole, 1e-9));
    prev = d;
  }
  assert.equal(fitDistance(Math.PI, half), whole);
  assert.equal(fitDistance(0, half), 1);
  // A narrower view (portrait) needs a farther camera.
  assert.ok(fitDistance(10 * DEG, viewHalfAngle(30 * DEG, 0.6)) > fitDistance(10 * DEG, viewHalfAngle(30 * DEG, 1.6)));
  assert.equal(viewHalfAngle(30 * DEG, 2), 15 * DEG);
});

function square(code: string, west: number, south: number, east: number, north: number): GeoFeature {
  return {
    code,
    name: code,
    regionId: code,
    bbox: [west, south, east, north],
    polygons: [[[
      [west, south],
      [east, south],
      [east, north],
      [west, north],
      [west, south],
    ]]],
  };
}

check("frameForBBox centres an antimeridian bbox and covers its vertices", () => {
  const frame = frameForBBox([170, -20, 190, -10], [
    [170, -20],
    [190, -10],
    [-175, -15],
  ]);
  const [lon, lat] = vecToLonLat(...frame.direction);
  assert.ok(close(lon, 180, 1e-9) || close(lon, -180, 1e-9));
  assert.ok(close(lat, -15, 1e-9));
  const expected = Math.max(
    angularDistance(180, -15, 170, -20),
    angularDistance(180, -15, 190, -10),
    angularDistance(180, -15, -175, -15),
  );
  assert.ok(close(frame.radius, expected, 1e-9));
  assert.equal(frameForFeatures([]), null);
  const sq = frameForFeatures([square("SQ", 10, 10, 20, 20)]);
  assert.ok(sq && sq.radius > 0 && sq.radius < 10 * DEG);
});

check("pickFeatureAt finds features, prefers the smaller one and skips rejected ones", () => {
  const features = [square("BIG", 0, 0, 20, 20), square("SMALL", 5, 5, 8, 8)];
  const pick = buildPickFeatures(features);
  assert.equal(pickFeatureAt(pick, 6, 6), 1);
  assert.equal(pickFeatureAt(pick, 15, 15), 0);
  assert.equal(pickFeatureAt(pick, 30, 15), -1);
  assert.equal(pickFeatureAt(pick, 6, 6, (i) => i !== 1), 0);
});

check("pickFeatureAt is antimeridian-safe for rings stored past ±180", () => {
  // A ring stored with longitudes 175..185 (as Natural Earth unwrapped rings are).
  const crossing = square("XNG", 175, -20, 185, -10);
  // A ring stored on the negative side, -179..-170.
  const negative = square("NEG", -179, 30, -170, 40);
  const pick = buildPickFeatures([crossing, negative]);
  assert.equal(pickFeatureAt(pick, 178, -15), 0);
  assert.equal(pickFeatureAt(pick, -178, -15), 0, "east of the antimeridian, reported as negative lon");
  assert.equal(pickFeatureAt(pick, -174, -15), -1);
  assert.equal(pickFeatureAt(pick, -175, 35), 1);
  assert.equal(pickFeatureAt(pick, 185, 35), 1, "unwrapped query longitude");
  // Raycast-style round trip: a surface hit converted with vecToLonLat still picks the feature.
  const [lon, lat] = vecToLonLat(...lonLatToVec(183, -12));
  assert.equal(pickFeatureAt(pick, lon, lat), 0);
});

check("pickFeatureAt works on the real dataset including Fiji and Russia", () => {
  const dataset = JSON.parse(fs.readFileSync("public/geo/regions.json", "utf8")) as GeoDataset;
  const pick = buildPickFeatures(dataset.features);
  const codeAt = (lon: number, lat: number) => {
    const idx = pickFeatureAt(pick, lon, lat);
    return idx >= 0 ? dataset.features[idx].code : null;
  };
  assert.equal(codeAt(35, 39), "TUR");
  assert.equal(codeAt(-178.4, -19.1) ?? codeAt(178.4, -17.8), "FJI");
  assert.equal(codeAt(178.4, -17.8), "FJI");
  assert.equal(codeAt(-175, 66.5), "RUS", "Chukotka east of the antimeridian");
  assert.equal(codeAt(100, 60), "RUS");
  assert.equal(codeAt(-30, 0), null, "open Atlantic");
});

function lookAtProjector(lon: number, lat: number, distance: number, width: number, height: number): ScreenProjector {
  // Perspective camera at `distance` looking at the origin, 30° vertical FOV, column-major matrices.
  const eye = lonLatToVec(lon, lat, distance);
  const f = eye.map((v) => -v / distance) as [number, number, number];
  const up = [0, 1, 0];
  const s = [f[1] * up[2] - f[2] * up[1], f[2] * up[0] - f[0] * up[2], f[0] * up[1] - f[1] * up[0]];
  const sl = Math.hypot(s[0], s[1], s[2]);
  for (let i = 0; i < 3; i++) s[i] /= sl;
  const u = [s[1] * f[2] - s[2] * f[1], s[2] * f[0] - s[0] * f[2], s[0] * f[1] - s[1] * f[0]];
  const dot = (a: readonly number[], b: readonly number[]) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const view = [
    s[0], u[0], -f[0], 0,
    s[1], u[1], -f[1], 0,
    s[2], u[2], -f[2], 0,
    -dot(s, eye), -dot(u, eye), dot(f, eye), 1,
  ];
  const near = 0.01;
  const far = 100;
  const t = 1 / Math.tan(15 * DEG);
  const aspect = width / height;
  const proj = [
    t / aspect, 0, 0, 0,
    0, t, 0, 0,
    0, 0, (far + near) / (near - far), -1,
    0, 0, (2 * far * near) / (near - far), 0,
  ];
  const m = new Array<number>(16).fill(0);
  for (let c = 0; c < 4; c++) {
    for (let r = 0; r < 4; r++) {
      let sum = 0;
      for (let k = 0; k < 4; k++) sum += proj[k * 4 + r] * view[c * 4 + k];
      m[c * 4 + r] = sum;
    }
  }
  return { matrix: m, camera: eye, width, height };
}

check("projection and point picking ignore the far hemisphere", () => {
  const projector = lookAtProjector(35, 39, 3, 800, 600);
  const centre = projectToScreen(projector, ...lonLatToVec(35, 39, POINT_RADIUS));
  assert.ok(centre && close(centre[0], 400, 1e-6) && close(centre[1], 300, 1e-6));
  const north = projectToScreen(projector, ...lonLatToVec(35, 45, POINT_RADIUS));
  assert.ok(north && north[1] < 300, "north is up on screen");
  const east = projectToScreen(projector, ...lonLatToVec(41, 39, POINT_RADIUS));
  assert.ok(east && east[0] > 400, "east is right on screen");
  assert.ok(facesCamera(projector.camera, ...lonLatToVec(35, 39, POINT_RADIUS)));
  assert.ok(!facesCamera(projector.camera, ...lonLatToVec(-145, -39, POINT_RADIUS)));

  // Two points sharing a screen position: the visible one wins, the antipodal one never does.
  const positions = new Float32Array([
    ...lonLatToVec(-145, -39, POINT_RADIUS),
    ...lonLatToVec(35, 39, POINT_RADIUS),
  ]);
  assert.equal(nearestScreenPoint(positions, 2, projector, 401, 301, 12), 1);
  assert.equal(nearestScreenPoint(positions, 2, projector, 401, 301, 12, (i) => i === 0), -1);
  assert.equal(nearestScreenPoint(positions, 2, projector, 500, 301, 12), -1);
  assert.deepEqual(pointsInView(positions, 2, projector, 12), [1]);
});

check("harmonized points draw low FRP first and intense fires last, stably", () => {
  const kinds = new Float32Array([9, 2, 5, 2, 9, 3]);
  assert.deepEqual([...drawOrderByKind(kinds)], [1, 3, 5, 2, 0, 4]);
  assert.deepEqual([...drawOrderByKind(new Float32Array(0))], []);
});

check("globe layer attributes mirror the flat renderer's shapes, buckets and sensor filter", () => {
  const raw = [
    { id: "r1", sensor: "MODIS", lon: 30, lat: 40, frp: 12, harmonizedId: "h1" },
    { id: "r2", sensor: "VIIRS", lon: 30.01, lat: 40, frp: 3, harmonizedId: "h1" },
    { id: "r3", sensor: "VIIRS", lon: 31, lat: 41, frp: 400, harmonizedId: null },
  ];
  const harmonized = [{ id: "h1", source: "Matched", lon: 30, lat: 40, frpCorrected: 15 }];
  const scene = buildScenePoints(
    { raw, harmonized } as unknown as Parameters<typeof buildScenePoints>[0],
    LONLAT_PROJECTION,
  );
  assert.equal(scene.raw.xs[0], 30);
  assert.equal(scene.raw.ys[0], 40);

  const rawLayer = buildGlobeLayer(scene.raw, false);
  assert.equal(rawLayer.count, 3);
  assert.deepEqual([...rawLayer.kinds], [0, 1, 1]);
  assert.deepEqual([...rawLayer.duplicates], [1, 1, 1]);
  const [lon, lat] = vecToLonLat(rawLayer.positions[0], rawLayer.positions[1], rawLayer.positions[2]);
  assert.ok(close(lon, 30, 1e-4) && close(lat, 40, 1e-4));
  assert.ok(close(Math.hypot(rawLayer.positions[0], rawLayer.positions[1], rawLayer.positions[2]), POINT_RADIUS, 1e-6));

  const harmLayer = buildGlobeLayer(scene.harmonized, true);
  assert.equal(harmLayer.kinds[0], 1 + scene.harmonized.buckets[0]);
  assert.equal(rawLayer.drawOrder, null);
  assert.deepEqual([...(harmLayer.drawOrder ?? [])], [0]);

  for (const filter of [
    { modis: true, viirs: true },
    { modis: true, viirs: false },
    { modis: false, viirs: true },
    { modis: false, viirs: false },
  ]) {
    for (const layer of [scene.raw, scene.harmonized]) {
      for (let i = 0; i < layer.count; i++) {
        assert.equal(
          sensorBitsVisible(sensorBitsFor(layer.flags[i], layer.shapes[i]), filter),
          pointMatchesSensorFilter(layer.flags[i], layer.shapes[i], filter),
        );
      }
    }
  }
  // Flagless points fall back to their shape, as in pointMatchesSensorFilter.
  for (const shape of [PointShape.HollowCircle, PointShape.Square, PointShape.Circle]) {
    for (const filter of [{ modis: true, viirs: false }, { modis: false, viirs: true }]) {
      assert.equal(
        sensorBitsVisible(sensorBitsFor(PointFlag.None, shape), filter),
        pointMatchesSensorFilter(PointFlag.None, shape, filter),
      );
    }
  }
});

console.log(`globe tests done in ${(performance.now() - t0).toFixed(1)} ms`);
