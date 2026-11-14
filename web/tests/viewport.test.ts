import assert from "node:assert/strict";
import { performance } from "node:perf_hooks";
import {
  DEFAULT_FIT_PADDING,
  MAX_ZOOM_FACTOR,
  MIN_ZOOM_FACTOR,
  clampTransform,
  fitTo,
  panBy,
  planeToScreen,
  screenToPlane,
  zoomAt,
  type PlaneBounds,
  type ViewTransform,
} from "../src/components/Map/engine/viewport.ts";

const t0 = performance.now();

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const sampleBounds: PlaneBounds = { minX: -0.5, minY: -0.25, maxX: 0.5, maxY: 0.25 };
const sampleSize = { width: 800, height: 600 };

check("fitTo normal bounds fits and centres within padding", () => {
  const t = fitTo(sampleBounds, sampleSize, 20);
  assert.ok(Number.isFinite(t.k) && t.k > 0);
  const [sx0, sy0] = planeToScreen(t, sampleBounds.minX, sampleBounds.minY);
  const [sx1, sy1] = planeToScreen(t, sampleBounds.maxX, sampleBounds.maxY);
  assert.ok(sx0 >= 20 - 1e-9 && sx1 <= 800 - 20 + 1e-9);
  assert.ok(sy0 >= 20 - 1e-9 && sy1 <= 600 - 20 + 1e-9);
  assert.ok(Math.abs(sx0 - (800 - sx1)) < 1e-9);
  assert.ok(Math.abs(sy0 - (600 - sy1)) < 1e-9);
});

check("fitTo handles zero size, negative size, and NaN/Infinity size", () => {
  assert.deepEqual(fitTo(sampleBounds, { width: 0, height: 600 }), { k: 1, tx: 0, ty: 0 });
  assert.deepEqual(fitTo(sampleBounds, { width: 800, height: 0 }), { k: 1, tx: 0, ty: 0 });
  assert.deepEqual(fitTo(sampleBounds, { width: -10, height: 600 }), { k: 1, tx: 0, ty: 0 });
  assert.deepEqual(fitTo(sampleBounds, { width: Number.NaN, height: 600 }), { k: 1, tx: 0, ty: 0 });
  assert.deepEqual(fitTo(sampleBounds, { width: 800, height: Number.POSITIVE_INFINITY }), {
    k: 1,
    tx: 0,
    ty: 0,
  });
});

check("fitTo handles NaN and degenerate bounds", () => {
  assert.deepEqual(
    fitTo({ minX: Number.NaN, minY: 0, maxX: 1, maxY: 1 }, sampleSize),
    { k: 1, tx: 400, ty: 300 },
  );
  assert.deepEqual(
    fitTo({ minX: 2, minY: 3, maxX: 2, maxY: 3 }, sampleSize),
    { k: 1, tx: 400, ty: 300 },
  );
  assert.deepEqual(
    fitTo({ minX: 5, minY: 5, maxX: 1, maxY: 1 }, sampleSize),
    { k: 1, tx: 400, ty: 300 },
  );
  const lineX = fitTo({ minX: 0, minY: 2, maxX: 10, maxY: 2 }, sampleSize, 20);
  assert.ok(Number.isFinite(lineX.k) && lineX.k > 0);
  const lineY = fitTo({ minX: 2, minY: 0, maxX: 2, maxY: 10 }, sampleSize, 20);
  assert.ok(Number.isFinite(lineY.k) && lineY.k > 0);
});

check("zoomAt keeps the plane point under the cursor fixed within 1e-6 px", () => {
  const base = fitTo(sampleBounds, sampleSize, DEFAULT_FIT_PADDING);
  const factors = [0.25, 0.5, 1.25, 2.5, 8];
  const cursors = [
    [0, 0],
    [400, 300],
    [723.5, 189.25],
    [12.75, 588.125],
  ] as const;

  for (const factor of factors) {
    for (const [cx, cy] of cursors) {
      const [px, py] = screenToPlane(base, cx, cy);
      const zoomed = zoomAt(base, factor, cx, cy);
      assert.ok(Math.abs(zoomed.k - base.k * factor) < 1e-12);
      const [sx, sy] = planeToScreen(zoomed, px, py);
      assert.ok(
        Math.abs(sx - cx) < 1e-6 && Math.abs(sy - cy) < 1e-6,
        `cursor drifted at factor=${factor}, cursor=(${cx},${cy}): got (${sx},${sy})`,
      );
    }
  }

  // Non-positive or non-finite factor leaves transform unchanged.
  assert.deepEqual(zoomAt(base, 0, 100, 100), base);
  assert.deepEqual(zoomAt(base, -2, 100, 100), base);
  assert.deepEqual(zoomAt(base, Number.NaN, 100, 100), base);
});

check("panBy translates tx/ty and preserves k", () => {
  const t: ViewTransform = { k: 120, tx: 50, ty: -30 };
  const p = panBy(t, -25.5, 42.25);
  assert.deepEqual(p, { k: 120, tx: 24.5, ty: 12.25 });
  assert.deepEqual(t, { k: 120, tx: 50, ty: -30 });
});

check("clampTransform enforces MIN/MAX_ZOOM_FACTOR and keeps region overlapping view", () => {
  const base = fitTo(sampleBounds, sampleSize, DEFAULT_FIT_PADDING);
  const expectedMinK = base.k * MIN_ZOOM_FACTOR;
  const expectedMaxK = base.k * MAX_ZOOM_FACTOR;

  const tooSmall = clampTransform({ k: base.k * 0.01, tx: base.tx, ty: base.ty }, sampleBounds, sampleSize);
  assert.ok(Math.abs(tooSmall.k - expectedMinK) < 1e-9);

  const tooLarge = clampTransform({ k: base.k * 500, tx: base.tx, ty: base.ty }, sampleBounds, sampleSize);
  assert.ok(Math.abs(tooLarge.k - expectedMaxK) < 1e-9);

  const badK = clampTransform({ k: Number.NaN, tx: Number.NaN, ty: Number.NaN }, sampleBounds, sampleSize);
  assert.ok(Math.abs(badK.k - expectedMinK) < 1e-9);
  assert.ok(Number.isFinite(badK.tx) && Number.isFinite(badK.ty));

  const marginX = Math.min(64, sampleSize.width * 0.25);
  const marginY = Math.min(64, sampleSize.height * 0.25);
  const zooms = [expectedMinK, base.k * 4, expectedMaxK];
  const extremeOffsets = [-1e7, 1e7];

  for (const k of zooms) {
    for (const ox of extremeOffsets) {
      for (const oy of extremeOffsets) {
        const clamped = clampTransform({ k, tx: ox, ty: oy }, sampleBounds, sampleSize);
        const [sMinX, sMinY] = planeToScreen(clamped, sampleBounds.minX, sampleBounds.minY);
        const [sMaxX, sMaxY] = planeToScreen(clamped, sampleBounds.maxX, sampleBounds.maxY);

        assert.ok(sMaxX >= marginX - 1e-9, `region panned past left margin: sMaxX=${sMaxX}`);
        assert.ok(sMinX <= sampleSize.width - marginX + 1e-9, `region panned past right margin: sMinX=${sMinX}`);
        assert.ok(sMaxY >= marginY - 1e-9, `region panned past top margin: sMaxY=${sMaxY}`);
        assert.ok(sMinY <= sampleSize.height - marginY + 1e-9, `region panned past bottom margin: sMinY=${sMinY}`);
      }
    }
  }
});

check("screenToPlane and planeToScreen round-trip accurately", () => {
  const transforms: ViewTransform[] = [
    { k: 1, tx: 0, ty: 0 },
    { k: 423.75, tx: -120.5, ty: 318.25 },
    { k: 0.05, tx: 1000, ty: -400 },
  ];
  const coords = [
    [0, 0],
    [400, 300],
    [-123.45, 987.65],
    [0.001, -0.002],
  ] as const;

  for (const t of transforms) {
    for (const [x, y] of coords) {
      const [px, py] = screenToPlane(t, x, y);
      const [sx, sy] = planeToScreen(t, px, py);
      assert.ok(Math.abs(sx - x) < 1e-9 && Math.abs(sy - y) < 1e-9);

      const [sx2, sy2] = planeToScreen(t, x, y);
      const [px2, py2] = screenToPlane(t, sx2, sy2);
      assert.ok(Math.abs(px2 - x) < 1e-9 && Math.abs(py2 - y) < 1e-9);
    }
  }
});

console.log(`viewport.test.ts completed in ${(performance.now() - t0).toFixed(2)} ms`);
