import assert from "node:assert/strict";
import fs from "node:fs";
import { performance } from "node:perf_hooks";
import type { GeoDataset } from "../src/lib/geo/types.ts";
import type { ScenePointInput } from "../src/components/Map/engine/types.ts";
import {
  Confidence,
  DayNight,
  HotspotSource,
  type HarmonizedPoint,
  type RawPoint,
} from "../src/types/index.ts";

class StubPath2D {
  ops: string[] = [];
  moveTo(x: number, y: number): void {
    this.ops.push(`M:${x.toFixed(3)},${y.toFixed(3)}`);
  }
  lineTo(x: number, y: number): void {
    this.ops.push(`L:${x.toFixed(3)},${y.toFixed(3)}`);
  }
  closePath(): void {
    this.ops.push("Z");
  }
  arc(x: number, y: number, r: number, a0: number, a1: number): void {
    this.ops.push(`A:${x.toFixed(3)},${y.toFixed(3)},${r.toFixed(3)},${a0.toFixed(3)},${a1.toFixed(3)}`);
  }
  rect(x: number, y: number, w: number, h: number): void {
    this.ops.push(`R:${x.toFixed(3)},${y.toFixed(3)},${w.toFixed(3)},${h.toFixed(3)}`);
  }
}

(globalThis as unknown as { Path2D: typeof StubPath2D }).Path2D = StubPath2D;

const { createProjection, padBBox, regionExtent } = await import(
  "../src/lib/geo/projection.ts"
);
const { featuresForRegion } = await import(
  "../src/lib/geo/regionCodes.ts"
);
const { buildScene, drawMap, readPalette } = await import(
  "../src/components/Map/engine/renderer.ts"
);
const { fitTo } = await import(
  "../src/components/Map/engine/viewport.ts"
);
const { PointFlag, PointShape } = await import(
  "../src/components/Map/engine/types.ts"
);

const t0 = performance.now();

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

interface FakeCtxRecorder {
  ctx: CanvasRenderingContext2D;
  calls: string[];
  counts: Record<string, number>;
}

function createFakeCtx(clientWidth = 800, clientHeight = 600): FakeCtxRecorder {
  const calls: string[] = [];
  const counts: Record<string, number> = {};
  const target: Record<string | symbol, unknown> = {
    canvas: {
      width: 0,
      height: 0,
      clientWidth,
      clientHeight,
      dataset: {},
      className: "",
      isConnected: false,
      getAttribute: () => null,
    },
  };
  const fnCache = new Map<string, (...args: unknown[]) => void>();

  const formatArg = (arg: unknown): string => {
    if (arg instanceof StubPath2D) return `Path2D(${arg.ops.length}:${arg.ops.join(";")})`;
    if (Array.isArray(arg)) return `[${arg.join(",")}]`;
    if (typeof arg === "number") return arg.toFixed(4);
    return String(arg);
  };

  const ctx = new Proxy(target, {
    get(obj, prop) {
      if (prop in obj) return obj[prop];
      if (typeof prop === "symbol") return undefined;
      let fn = fnCache.get(prop);
      if (!fn) {
        fn = (...args: unknown[]) => {
          counts[prop] = (counts[prop] ?? 0) + 1;
          calls.push(`${prop}(${args.map(formatArg).join(",")})`);
        };
        fnCache.set(prop, fn);
      }
      return fn;
    },
    set(obj, prop, value) {
      obj[prop] = value;
      if (typeof prop === "string") {
        calls.push(`set:${prop}=${String(value)}`);
      }
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;

  return { ctx, calls, counts };
}

const REGIONS_PATH =
  "public/geo/regions.json";
const dataset = JSON.parse(fs.readFileSync(REGIONS_PATH, "utf8")) as GeoDataset;
const azeFeats = featuresForRegion(dataset, "Azerbaijan");
const azeBBox = regionExtent(azeFeats);
const azeProj = createProjection(padBBox(azeBBox, 0.08));

check("buildScene edge cases: null dataset, empty features, empty points, unmapped region, null projection", () => {
  const sNull = buildScene(null, "Azerbaijan", null, azeProj);
  assert.equal(sNull.contextRings.length, 0);
  assert.equal(sNull.selectedRings.length, 0);
  assert.equal(sNull.contextPath, null);
  assert.equal(sNull.selectedPath, null);
  assert.equal(sNull.raw.count, 0);
  assert.equal(sNull.harmonized.count, 0);

  const sEmptyFeats = buildScene([], "Azerbaijan", { raw: [], harmonized: [] }, azeProj);
  assert.equal(sEmptyFeats.selectedRings.length, 0);
  assert.equal(sEmptyFeats.raw.count, 0);

  const sEmptyPts = buildScene(dataset, "Azerbaijan", { raw: [], harmonized: [] }, azeProj);
  assert.ok(sEmptyPts.selectedRings.length > 0);
  assert.ok(sEmptyPts.contextRings.length > 0);
  assert.ok(sEmptyPts.selectedPath instanceof StubPath2D);
  assert.ok(sEmptyPts.contextPath instanceof StubPath2D);
  assert.equal(sEmptyPts.raw.count, 0);
  assert.equal(sEmptyPts.harmonized.count, 0);

  const sNoRegion = buildScene(dataset, "Nowhere", { raw: [], harmonized: [] }, azeProj);
  assert.equal(sNoRegion.selectedRings.length, 0);
  assert.ok(sNoRegion.contextRings.length > 0);

  const sNullProj = buildScene(dataset, "Azerbaijan", { raw: [], harmonized: [] }, null);
  assert.deepEqual(sNullProj.bounds, { minX: -1, minY: -1, maxX: 1, maxY: 1 });
});

// Build 2,000 synthetic raw points inside Azerbaijan's bbox + paired harmonized points.
const rng = mulberry32(0x42e2000);
const [west, south, east, north] = azeBBox;
const RAW_COUNT = 2000;
const rawPoints: RawPoint[] = new Array(RAW_COUNT);
const harmPoints: HarmonizedPoint[] = [];

for (let i = 0; i < RAW_COUNT; i++) {
  const lon = west + rng() * (east - west);
  const lat = south + rng() * (north - south);
  const sensor = i % 3 === 0 ? "MODIS" : "VIIRS";
  const frp = 1 + rng() * 400;
  let harmonizedId: string | null = `h-${i}`;
  if (i % 10 === 0) {
    harmonizedId = null; // interscan removed duplicate
  } else if (i % 10 === 1 && i > 0) {
    harmonizedId = `h-${i - 2}`; // shares harmonizedId
  }

  rawPoints[i] = {
    id: `r-${i}`,
    lat,
    lon,
    sensor,
    frp,
    confidence: Confidence.Nominal,
    acquired: "2024-07-15T10:00:00Z",
    daynight: DayNight.Day,
    harmonizedId,
  };

  if (harmonizedId !== null && i % 10 !== 1) {
    harmPoints.push({
      id: harmonizedId,
      lat,
      lon,
      frp,
      frpCorrected: frp * 0.95,
      source: i % 5 === 2 ? HotspotSource.Matched : sensor === "MODIS" ? HotspotSource.ModisOnly : HotspotSource.ViirsOnly,
      confidence: Confidence.Nominal,
      acquired: "2024-07-15T10:00:00Z",
      daynight: DayNight.Day,
    });
  }
}

const pointInput: ScenePointInput = { raw: rawPoints, harmonized: harmPoints };
const scene = buildScene(dataset, "Azerbaijan", pointInput, azeProj);

check("buildScene with Azerbaijan + 2,000 synthetic points validates typed arrays, buckets, shapes, flags", () => {
  const validMask = PointFlag.Duplicate | PointFlag.Removed | PointFlag.Matched;

  for (const [layer, expectedCount] of [
    [scene.raw, RAW_COUNT],
    [scene.harmonized, harmPoints.length],
  ] as const) {
    assert.equal(layer.count, expectedCount);
    assert.equal(layer.xs.length, expectedCount);
    assert.equal(layer.ys.length, expectedCount);
    assert.equal(layer.frp.length, expectedCount);
    assert.equal(layer.buckets.length, expectedCount);
    assert.equal(layer.shapes.length, expectedCount);
    assert.equal(layer.flags.length, expectedCount);
    assert.equal(layer.ids.length, expectedCount);
    assert.equal(layer.indexById.size, expectedCount);
    assert.equal(layer.spatialIndex.count, expectedCount);

    for (let i = 0; i < expectedCount; i++) {
      assert.ok(Number.isFinite(layer.xs[i]) && Number.isFinite(layer.ys[i]));
      assert.ok(layer.buckets[i] >= 1 && layer.buckets[i] <= 8, `invalid bucket ${layer.buckets[i]}`);
      assert.equal(layer.flags[i] & ~validMask, 0, `invalid flag bits ${layer.flags[i]}`);
    }
  }

  assert.equal(scene.raw.shapes[0], PointShape.HollowCircle);
  assert.equal(scene.raw.shapes[1], PointShape.Square);
  assert.equal(scene.raw.flags[0], PointFlag.Duplicate | PointFlag.Removed);
  assert.equal(scene.harmonized.shapes[0], PointShape.Circle);
});

check("drawMap handles zero-size view and normal view DrawStats", () => {
  const fit = fitTo(scene.bounds, { width: 800, height: 600 }, 20);

  const zeroRec = createFakeCtx();
  const z1 = drawMap(zeroRec.ctx, scene, { transform: fit, width: 0, height: 600, dpr: 2 });
  assert.deepEqual(z1, { drawn: 0, inView: 0, total: RAW_COUNT });
  assert.equal(zeroRec.calls.length, 0);

  const z2 = drawMap(zeroRec.ctx, scene, { transform: fit, width: 800, height: 0, dpr: 2 });
  assert.deepEqual(z2, { drawn: 0, inView: 0, total: RAW_COUNT });

  const normalRec = createFakeCtx();
  const stats = drawMap(normalRec.ctx, scene, {
    transform: fit,
    width: 800,
    height: 600,
    dpr: 2,
    harmonized: false,
    showDuplicateRings: true,
    hoveredIndex: 0,
    selectedId: "r-5",
  });
  assert.equal(stats.total, RAW_COUNT);
  assert.equal(stats.inView, RAW_COUNT);
  assert.equal(stats.drawn, RAW_COUNT);
  assert.ok(stats.drawn <= stats.inView && stats.inView <= stats.total);
  assert.equal(normalRec.ctx.canvas.width, 1600);
  assert.equal(normalRec.ctx.canvas.height, 1200);
  assert.ok((normalRec.counts["fill"] ?? 0) > 0);
  assert.ok((normalRec.counts["stroke"] ?? 0) > 0);
});

check("drawMap zoomed/panned so roughly half the points are offscreen decreases inView accordingly", () => {
  const fit = fitTo(scene.bounds, { width: 800, height: 600 }, 20);
  // Shift the scene right so the right half of Azerbaijan's bbox is beyond width + VIEWPORT_MARGIN_PX (812).
  const midPlaneX = (scene.bounds.minX + scene.bounds.maxX) / 2;
  const shifted = { k: fit.k, tx: 812 - midPlaneX * fit.k, ty: fit.ty };

  const rec = createFakeCtx();
  const stats = drawMap(rec.ctx, scene, {
    transform: shifted,
    width: 800,
    height: 600,
    dpr: 1,
    harmonized: false,
  });
  assert.equal(stats.total, RAW_COUNT);
  assert.ok(
    stats.inView > RAW_COUNT * 0.3 && stats.inView < RAW_COUNT * 0.7,
    `expected roughly half inView, got ${stats.inView}/${RAW_COUNT}`,
  );
  assert.equal(stats.drawn, stats.inView);
});

check("drawMap maxDrawn cap and deterministic sampling across calls", () => {
  const fit = fitTo(scene.bounds, { width: 800, height: 600 }, 20);
  const view = {
    transform: fit,
    width: 800,
    height: 600,
    dpr: 1,
    harmonized: true,
    showDuplicateRings: true,
    maxDrawn: 500,
  };

  const rec1 = createFakeCtx();
  const s1 = drawMap(rec1.ctx, scene, view);
  assert.equal(s1.inView, harmPoints.length);
  assert.equal(s1.drawn, 500);

  const rec2 = createFakeCtx();
  const s2 = drawMap(rec2.ctx, scene, view);
  assert.deepEqual(s2, s1);
  assert.deepEqual(rec1.calls, rec2.calls, "sampling and draw call sequence must be deterministic");
});

check("readPalette(null, theme) falls back cleanly without getComputedStyle", () => {
  const dark = readPalette(null, "dark");
  const light = readPalette(null, "light");
  assert.equal(dark.frp.length, 8);
  assert.equal(light.frp.length, 8);
  assert.ok(dark.water.length > 0 && light.water.length > 0);
  assert.notEqual(dark.water, light.water);
});

console.log(`scene.test.ts completed in ${(performance.now() - t0).toFixed(2)} ms`);
