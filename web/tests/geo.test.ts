import assert from "node:assert/strict";
import fs from "node:fs";
import { performance } from "node:perf_hooks";
import type { GeoDataset } from "../src/lib/geo/types.ts";
import {
  createProjection,
  fitTransform,
  padBBox,
  planeBounds,
  regionExtent,
} from "../src/lib/geo/projection.ts";
import { featuresForRegion } from "./regionFeatures.ts";
import {
  GeoLoadError,
  loadGeoDataset,
} from "../src/lib/geo/loadGeo.ts";

const REGIONS_PATH =
  "public/geo/regions.json";

const NASA_REGION_IDS = [
  "Armenia",
  "Azerbaijan",
  "Bulgaria",
  "Cyprus",
  "Georgia",
  "Greece",
  "Iran",
  "Iraq",
  "Russian_Federation",
  "Syria",
  "Turkey",
] as const;

const t0 = performance.now();

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

function haversineKm(lat1: number, lon1: number, lat2: number, lon2: number): number {
  const R = 6371;
  const toRad = Math.PI / 180;
  const dLat = (lat2 - lat1) * toRad;
  const dLon = (lon2 - lon1) * toRad;
  const a =
    Math.sin(dLat / 2) ** 2 +
    Math.cos(lat1 * toRad) * Math.cos(lat2 * toRad) * Math.sin(dLon / 2) ** 2;
  return 2 * R * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
}

const raw = fs.readFileSync(REGIONS_PATH, "utf8");
const dataset = JSON.parse(raw) as GeoDataset;

check("regions.json meta fields present and non-empty", () => {
  assert.ok(dataset && typeof dataset === "object");
  assert.ok(dataset.meta && typeof dataset.meta === "object");
  const requiredMetaKeys = [
    "package",
    "packageVersion",
    "source",
    "sourceVersion",
    "license",
    "resolution",
    "simplification",
  ] as const;
  for (const key of requiredMetaKeys) {
    assert.equal(typeof dataset.meta[key], "string", `meta.${key} must be string`);
    assert.ok(dataset.meta[key].trim().length > 0, `meta.${key} must be non-empty`);
  }
  assert.ok(Array.isArray(dataset.features) && dataset.features.length > 0);
});

check("all 11 NASA region ids mapped with >= 1 feature, Cyprus has CYP+CYN, Nowhere unmapped", () => {
  for (const regionId of NASA_REGION_IDS) {
    const feats = featuresForRegion(dataset, regionId);
    assert.ok(feats.length >= 1, `${regionId} should have >= 1 feature`);
  }
  const cyprusFeats = featuresForRegion(dataset, "Cyprus");
  assert.equal(cyprusFeats.length, 2, "Cyprus must have exactly 2 features");
  assert.deepEqual(
    cyprusFeats.map((f) => f.code).sort(),
    ["CYN", "CYP"],
  );
  assert.deepEqual(featuresForRegion(dataset, "Nowhere"), []);
});

check("all feature bboxes valid and all polygon rings closed with finite coordinates", () => {
  for (const f of dataset.features) {
    assert.equal(typeof f.code, "string");
    assert.ok(f.code.length > 0);
    assert.equal(f.bbox.length, 4);
    for (const c of f.bbox) {
      assert.ok(Number.isFinite(c), `${f.code} bbox coord not finite: ${c}`);
    }
    assert.ok(f.bbox[0] < f.bbox[2], `${f.code} bbox west < east`);
    assert.ok(f.bbox[1] < f.bbox[3], `${f.code} bbox south < north`);
    assert.ok(f.polygons.length > 0, `${f.code} must have polygons`);

    for (const poly of f.polygons) {
      assert.ok(poly.length > 0, `${f.code} polygon must have >= 1 ring`);
      for (const ring of poly) {
        assert.ok(ring.length >= 4, `${f.code} ring must have >= 4 positions`);
        for (const pos of ring) {
          assert.ok(
            Number.isFinite(pos[0]) && Number.isFinite(pos[1]),
            `${f.code} non-finite coordinate`,
          );
        }
        const first = ring[0];
        const last = ring[ring.length - 1];
        assert.equal(first[0], last[0], `${f.code} ring not closed on lon`);
        assert.equal(first[1], last[1], `${f.code} ring not closed on lat`);
      }
    }
  }
});

check("createProjection(padBBox(regionExtent, 0.08)) + planeBounds finite and non-degenerate for all regions", () => {
  for (const regionId of NASA_REGION_IDS) {
    const feats = featuresForRegion(dataset, regionId);
    const bbox = padBBox(regionExtent(feats), 0.08);
    const proj = createProjection(bbox);
    const pb = planeBounds(proj, bbox);
    assert.ok(
      Number.isFinite(pb.minX) &&
        Number.isFinite(pb.minY) &&
        Number.isFinite(pb.maxX) &&
        Number.isFinite(pb.maxY),
      `${regionId} planeBounds not finite`,
    );
    assert.ok(pb.maxX > pb.minX, `${regionId} maxX > minX`);
    assert.ok(pb.maxY > pb.minY, `${regionId} maxY > minY`);
  }
  assert.throws(() => regionExtent([]));
});

check("Russia antimeridian unwrapping: bbox.east > 180, no jump > 25% plane width, plausible aspect (> 1.5)", () => {
  const rusFeats = featuresForRegion(dataset, "Russian_Federation");
  const rusExtent = regionExtent(rusFeats);
  assert.ok(rusExtent[2] > 180, `Expected Russia east > 180, got ${rusExtent[2]}`);
  for (const f of rusFeats) {
    assert.ok(f.bbox[2] > 180, `Expected Russia feature bbox.east > 180, got ${f.bbox[2]}`);
  }

  const padded = padBBox(rusExtent, 0.08);
  const proj = createProjection(padded);
  const pb = planeBounds(proj, rusExtent);
  const planeW = pb.maxX - pb.minX;
  const planeH = pb.maxY - pb.minY;
  const maxAllowedJump = 0.25 * planeW;

  for (const f of rusFeats) {
    for (const poly of f.polygons) {
      for (const ring of poly) {
        let prevX = 0;
        let prevY = 0;
        for (let i = 0; i < ring.length; i++) {
          const [x, y] = proj.project(ring[i][0], ring[i][1]);
          assert.ok(Number.isFinite(x) && Number.isFinite(y));
          if (i > 0) {
            const jump = Math.hypot(x - prevX, y - prevY);
            assert.ok(
              jump <= maxAllowedJump,
              `Russia consecutive vertex jump ${jump} exceeds 25% of plane width ${planeW}`,
            );
          }
          prevX = x;
          prevY = y;
        }
      }
    }
  }

  const aspect = planeW / planeH;
  assert.ok(aspect > 1.5, `Russia plane aspect ${aspect} must be > 1.5`);
});

check("fitTransform produces uniform scale and centres bounds within padding symmetrically", () => {
  const cases = [
    { bounds: { minX: -0.4, minY: -0.25, maxX: 0.6, maxY: 0.15 }, width: 800, height: 600, padding: 24 },
    { bounds: { minX: 10, minY: -5, maxX: 12, maxY: 5 }, width: 320, height: 700, padding: 16 },
  ];
  for (const { bounds, width, height, padding } of cases) {
    const t = fitTransform(bounds, width, height, padding);
    assert.ok(t.k > 0 && Number.isFinite(t.k));

    // Uniform scale: 1 plane unit horizontally and vertically map to equal screen distance.
    const [hx0, hy0] = [0 * t.k + t.tx, 0 * t.k + t.ty];
    const [hx1, hy1] = [1 * t.k + t.tx, 0 * t.k + t.ty];
    const [vx1, vy1] = [0 * t.k + t.tx, 1 * t.k + t.ty];
    const distH = Math.hypot(hx1 - hx0, hy1 - hy0);
    const distV = Math.hypot(vx1 - hx0, vy1 - hy0);
    assert.ok(Math.abs(distH - distV) < 1e-12, `scale not uniform: ${distH} vs ${distV}`);
    assert.ok(Math.abs(distH - t.k) < 1e-12);

    const minSX = bounds.minX * t.k + t.tx;
    const maxSX = bounds.maxX * t.k + t.tx;
    const minSY = bounds.minY * t.k + t.ty;
    const maxSY = bounds.maxY * t.k + t.ty;

    assert.ok(minSX >= padding - 1e-9, `minSX ${minSX} < padding ${padding}`);
    assert.ok(maxSX <= width - padding + 1e-9, `maxSX ${maxSX} > width - padding`);
    assert.ok(minSY >= padding - 1e-9, `minSY ${minSY} < padding ${padding}`);
    assert.ok(maxSY <= height - padding + 1e-9, `maxSY ${maxSY} > height - padding`);

    assert.ok(Math.abs(minSX - (width - maxSX)) <= 1, "horizontal centring off by > 1px");
    assert.ok(Math.abs(minSY - (height - maxSY)) <= 1, "vertical centring off by > 1px");
  }
});

check("Azerbaijan projected bbox aspect within +/-20% of haversine ground aspect", () => {
  const azeFeats = featuresForRegion(dataset, "Azerbaijan");
  const bbox = regionExtent(azeFeats);
  const [west, south, east, north] = bbox;
  const proj = createProjection(padBBox(bbox, 0.08));
  const pb = planeBounds(proj, bbox);
  const projAspect = (pb.maxX - pb.minX) / (pb.maxY - pb.minY);

  const midLat = (south + north) / 2;
  const midLon = (west + east) / 2;
  const groundW = haversineKm(midLat, west, midLat, east);
  const groundH = haversineKm(south, midLon, north, midLon);
  const groundAspect = groundW / groundH;

  const relErr = Math.abs(projAspect - groundAspect) / groundAspect;
  assert.ok(
    relErr <= 0.2,
    `Azerbaijan aspect mismatch: proj=${projAspect.toFixed(4)}, ground=${groundAspect.toFixed(4)}, err=${(relErr * 100).toFixed(2)}%`,
  );
});

await (async () => {
  const err = new GeoLoadError("invalid", "bad schema");
  assert.equal(err.name, "GeoLoadError");
  assert.equal(err.reason, "invalid");
  const ac = new AbortController();
  ac.abort(new Error("aborted-in-test"));
  await assert.rejects(() => loadGeoDataset(ac.signal), /aborted-in-test/);
  console.log("ok - GeoLoadError and aborted loadGeoDataset");
})();

console.log(`geo.test.ts completed in ${(performance.now() - t0).toFixed(2)} ms`);
