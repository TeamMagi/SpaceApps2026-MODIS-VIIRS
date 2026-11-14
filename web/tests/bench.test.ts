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

class BenchPath2D {
  opCount = 0;
  moveTo(): void {
    this.opCount++;
  }
  lineTo(): void {
    this.opCount++;
  }
  closePath(): void {
    this.opCount++;
  }
  arc(): void {
    this.opCount++;
  }
  rect(): void {
    this.opCount++;
  }
}

(globalThis as unknown as { Path2D: typeof BenchPath2D }).Path2D = BenchPath2D;

const { createProjection, padBBox, regionExtent } = await import(
  "../src/lib/geo/projection.ts"
);
const { featuresForRegion } = await import("./regionFeatures.ts");
const { DEFAULT_MAX_DRAWN, buildScene, drawMap } = await import(
  "../src/components/Map/engine/renderer.ts"
);
const { fitTo, zoomAt } = await import(
  "../src/components/Map/engine/viewport.ts"
);

function createBenchCtx(): { ctx: CanvasRenderingContext2D; counts: Record<string, number> } {
  const counts: Record<string, number> = {};
  const target: Record<string | symbol, unknown> = {
    canvas: {
      width: 0,
      height: 0,
      clientWidth: 1200,
      clientHeight: 800,
      dataset: {},
      className: "",
      isConnected: false,
      getAttribute: () => null,
    },
  };
  const fnCache = new Map<string, () => void>();

  const ctx = new Proxy(target, {
    get(obj, prop) {
      if (prop in obj) return obj[prop];
      if (typeof prop === "symbol") return undefined;
      let fn = fnCache.get(prop);
      if (!fn) {
        fn = () => {
          counts[prop] = (counts[prop] ?? 0) + 1;
        };
        fnCache.set(prop, fn);
      }
      return fn;
    },
    set(obj, prop, value) {
      obj[prop] = value;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;

  return { ctx, counts };
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

const REGIONS_PATH =
  "public/geo/regions.json";
const dataset = JSON.parse(fs.readFileSync(REGIONS_PATH, "utf8")) as GeoDataset;
const azeFeats = featuresForRegion(dataset, "Azerbaijan");
const azeBBox = regionExtent(azeFeats);
const azeProj = createProjection(padBBox(azeBBox, 0.08));

const POINT_COUNT = 100_000;
const rng = mulberry32(0xbe4c0100);
const [west, south, east, north] = azeBBox;
const raw: RawPoint[] = new Array(POINT_COUNT);
const harmonized: HarmonizedPoint[] = new Array(POINT_COUNT);

for (let i = 0; i < POINT_COUNT; i++) {
  const lon = west + rng() * (east - west);
  const lat = south + rng() * (north - south);
  const frp = 2 + rng() * 350;
  const id = `p-${i}`;
  raw[i] = {
    id,
    lat,
    lon,
    sensor: i % 4 === 0 ? "MODIS" : "VIIRS",
    frp,
    confidence: Confidence.Nominal,
    acquired: "2024-08-01T12:00:00Z",
    daynight: DayNight.Day,
    harmonizedId: id,
  };
  harmonized[i] = {
    id,
    lat,
    lon,
    frp,
    frpCorrected: frp * 0.92,
    source: i % 5 === 0 ? HotspotSource.Matched : HotspotSource.ViirsOnly,
    confidence: Confidence.Nominal,
    acquired: "2024-08-01T12:00:00Z",
    daynight: DayNight.Day,
  };
}

const points: ScenePointInput = { raw, harmonized };

const tBuild0 = performance.now();
const scene = buildScene(dataset, "Azerbaijan", points, azeProj);
const buildMs = performance.now() - tBuild0;
console.log(`timing - buildScene (100k raw + 100k harm + indices): ${buildMs.toFixed(2)} ms`);
assert.ok(buildMs < 2000, `buildScene took ${buildMs.toFixed(2)} ms (>= 2000 ms)`);
assert.equal(scene.raw.count, POINT_COUNT);
assert.equal(scene.harmonized.count, POINT_COUNT);
console.log("ok - buildScene 100k points within 2000 ms ceiling");

const size = { width: 1200, height: 800 };
const fullTransform = fitTo(scene.bounds, size, 20);
const zoomedTransform = zoomAt(fullTransform, 6, 600, 400);

const fullBench = createBenchCtx();
const tFull0 = performance.now();
const fullStats = drawMap(fullBench.ctx, scene, {
  transform: fullTransform,
  width: size.width,
  height: size.height,
  dpr: 2,
  harmonized: false,
});
const fullMs = performance.now() - tFull0;
console.log(
  `timing - drawMap full view: ${fullMs.toFixed(2)} ms (drawn=${fullStats.drawn}, inView=${fullStats.inView}, total=${fullStats.total})`,
);
assert.ok(fullMs < 500, `drawMap full view took ${fullMs.toFixed(2)} ms (>= 500 ms)`);
assert.ok(
  fullStats.drawn <= DEFAULT_MAX_DRAWN,
  `drawn ${fullStats.drawn} exceeded DEFAULT_MAX_DRAWN ${DEFAULT_MAX_DRAWN}`,
);
assert.equal(fullStats.drawn, DEFAULT_MAX_DRAWN);
console.log("ok - drawMap full view <= 30000 default cap and < 500 ms");

const zoomBench = createBenchCtx();
const tZoom0 = performance.now();
const zoomStats = drawMap(zoomBench.ctx, scene, {
  transform: zoomedTransform,
  width: size.width,
  height: size.height,
  dpr: 2,
  harmonized: true,
});
const zoomMs = performance.now() - tZoom0;
console.log(
  `timing - drawMap zoomed in (harmonized): ${zoomMs.toFixed(2)} ms (drawn=${zoomStats.drawn}, inView=${zoomStats.inView}, total=${zoomStats.total})`,
);
assert.ok(zoomMs < 500, `drawMap zoomed in took ${zoomMs.toFixed(2)} ms (>= 500 ms)`);
assert.ok(zoomStats.inView < fullStats.inView, "zoomed inView should be smaller than full view");
assert.ok(zoomStats.drawn <= DEFAULT_MAX_DRAWN);
console.log("ok - drawMap zoomed in < 500 ms and culled offscreen points");
