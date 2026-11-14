import assert from "node:assert/strict";
import fs from "node:fs";
import { performance } from "node:perf_hooks";
import type { GeoDataset } from "../src/lib/geo/types.ts";
import { firmsCodesFor, matchRegionsToFeatures } from "../src/lib/geo/regionMatch.ts";
import {
  createProjection,
  padBBox,
  projectRing,
  regionExtent,
} from "../src/lib/geo/projection.ts";
import { featuresForRegion } from "./regionFeatures.ts";
import { buildSceneGeometry } from "../src/components/Map/engine/renderer.ts";
import { pickLodLevel, simplifyRing } from "../src/components/Map/engine/lod.ts";
import { hitTestFeatures, pointInRings } from "../src/components/Map/engine/featureHit.ts";
import {
  clampTransform,
  EXTENT_MIN_ZOOM_FACTOR,
  fitTo,
  screenToPlane,
} from "../src/components/Map/engine/viewport.ts";

const t0 = performance.now();

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const dataset = JSON.parse(fs.readFileSync("public/geo/regions.json", "utf8")) as GeoDataset;
// Every country id in FIRMS' own index (data/country/yearly_summary_files.txt), one per line.
const firmsIds = fs
  .readFileSync("tests/fixtures/firms-countries.txt", "utf8")
  .split(/\r?\n/)
  .map((line) => line.trim())
  .filter((line) => line.length > 0);
const asRegion = (id: string) => ({ id, name: id.replace(/_/g, " ") });

// FIRMS publishes these, but Natural Earth 1:50m has no polygon for them (Antarctica is dropped on purpose).
const WITHOUT_GEOMETRY = new Set(["Antarctica", "United_States_Minor_Outlying_Islands"]);

check("regions.json covers the whole world within the size budget", () => {
  assert.ok(dataset.features.length >= 240, `only ${dataset.features.length} features`);
  const bytes = fs.statSync("public/geo/regions.json").size;
  assert.ok(bytes <= 700 * 1024, `regions.json is ${bytes} bytes`);
  const codes = new Set(dataset.features.map((f) => f.code));
  assert.equal(codes.size, dataset.features.length, "feature codes are unique");
  for (const code of ["USA", "BRA", "AUS", "CHN", "IND", "ZAF", "CAN", "NZL", "FJI", "FRA", "NOR", "KOS"]) {
    assert.ok(codes.has(code), `${code} missing`);
  }
  for (const f of dataset.features) assert.match(f.code, /^[A-Z]{3}$/, `${f.name} code ${f.code}`);
});

check("every FIRMS country id has a table entry", () => {
  assert.equal(firmsIds.length, 238);
  for (const id of firmsIds) {
    assert.ok(firmsCodesFor(id), `${id} missing from firmsCountries.json`);
  }
});

check("real FIRMS country list matches geometry, with and without build-time links", () => {
  const unlinked = dataset.features.map((f) => ({ ...f, regionId: null }));
  for (const features of [dataset.features, unlinked]) {
    const match = matchRegionsToFeatures(features, firmsIds.map(asRegion));
    const matched = firmsIds.filter((id) => (match.featuresByRegion.get(id)?.length ?? 0) > 0);
    const missing = firmsIds.filter((id) => !match.featuresByRegion.has(id));
    assert.deepEqual(missing.sort(), [...WITHOUT_GEOMETRY].sort());
    assert.equal(matched.length, firmsIds.length - WITHOUT_GEOMETRY.size);

    const codesOf = (id: string) =>
      (match.featuresByRegion.get(id) ?? []).map((i) => features[i].code).sort();
    const expected: Record<string, string[]> = {
      United_States: ["USA"],
      Czech_Republic: ["CZE"],
      Macedonia_Former_Yugoslav_Republic_of: ["MKD"],
      Republic_of_Congo: ["COG"],
      Democratic_Republic_of_the_Congo: ["COD"],
      Cote_d_Ivoire: ["CIV"],
      Russian_Federation: ["RUS"],
      Turkey: ["TUR"],
      Cyprus: ["CYN", "CYP"],
      Somalia: ["SOL", "SOM"],
      Dem_Rep_Korea: ["PRK"],
      Republic_of_Korea: ["KOR"],
      Lao_PDR: ["LAO"],
      Swaziland: ["SWZ"],
      The_Gambia: ["GMB"],
      Kosovo: ["KOS"],
      French_Guiana: ["GUF"],
      Reunion: ["REU"],
      Svalbard_and_Jan_Mayen: ["SJM"],
      France: ["FRA"],
      Norway: ["NOR"],
    };
    for (const [id, codes] of Object.entries(expected)) {
      assert.deepEqual(codesOf(id), codes, id);
    }
  }
});

check("aliases and alternative spellings resolve to the same countries", () => {
  const aliases: Record<string, string> = {
    United_States: "USA",
    Czech_Republic: "CZE",
    Czechia: "CZE",
    North_Macedonia: "MKD",
    Republic_of_the_Congo: "COG",
    Democratic_Republic_of_the_Congo: "COD",
    Ivory_Coast: "CIV",
    "Côte d'Ivoire": "CIV",
    Russian_Federation: "RUS",
    Russia: "RUS",
    Turkey: "TUR",
    Türkiye: "TUR",
    Eswatini: "SWZ",
  };
  for (const [id, code] of Object.entries(aliases)) {
    const match = matchRegionsToFeatures(dataset.features, [{ id, name: id }]);
    const codes = (match.featuresByRegion.get(id) ?? []).map((i) => dataset.features[i].code);
    assert.ok(codes.includes(code), `${id} -> ${codes.join(",")}, expected ${code}`);
  }
});

check("antimeridian countries get a short longitude extent so their projection is centred on them", () => {
  for (const id of ["United_States", "Fiji", "Russian_Federation", "New_Zealand", "Kiribati"]) {
    const [west, , east] = regionExtent(featuresForRegion(dataset, id));
    assert.ok(east - west < 180, `${id} spans ${west}..${east}`);
  }
});

check("rings straddling the projection seam stay continuous (no map-wide streaks)", () => {
  const turkey = featuresForRegion(dataset, "Turkey");
  const projection = createProjection(padBBox(regionExtent(turkey), 0.08));
  const geometry = buildSceneGeometry(dataset, "Turkey", projection, new Set());
  const all = geometry.allBounds;
  assert.ok(all);
  const planeW = all.maxX - all.minX;
  for (const f of geometry.features) {
    for (const ring of f.rings) {
      for (let i = 2; i < ring.length; i += 2) {
        const jump = Math.hypot(ring[i] - ring[i - 2], ring[i + 1] - ring[i - 1]);
        assert.ok(jump < planeW * 0.2, `jump ${jump.toFixed(3)} of plane width ${planeW.toFixed(3)}`);
      }
    }
  }
  // Alaska sits across the seam (35°E + 180°) from Türkiye; its rings must not span the map.
  const usa = dataset.features.findIndex((f) => f.code === "USA");
  for (const ring of geometry.features[usa].rings) {
    let minX = Infinity;
    let maxX = -Infinity;
    for (let i = 0; i < ring.length; i += 2) {
      minX = Math.min(minX, ring[i]);
      maxX = Math.max(maxX, ring[i]);
    }
    assert.ok(maxX - minX < planeW * 0.5);
  }
});

check("projectRing equals per-vertex projection for rings away from the seam", () => {
  const projection = createProjection([25, 35, 45, 42]);
  const ring = featuresForRegion(dataset, "Turkey")[0].polygons[0][0];
  const coords = projectRing(projection, ring);
  for (let i = 0; i < ring.length; i++) {
    const [x, y] = projection.project(ring[i][0], ring[i][1]);
    assert.ok(Math.abs(coords[i * 2] - x) < 1e-6 && Math.abs(coords[i * 2 + 1] - y) < 1e-6);
  }
});

check("per-ring bboxes never change the hit result and far countries are hittable", () => {
  const turkey = featuresForRegion(dataset, "Turkey");
  const projection = createProjection(padBBox(regionExtent(turkey), 0.08));
  const geometry = buildSceneGeometry(dataset, "Turkey", projection, new Set());

  const at = (lon: number, lat: number) => {
    const [x, y] = projection.project(lon, lat);
    const idx = hitTestFeatures(geometry.features, x, y);
    return idx >= 0 ? dataset.features[idx].code : null;
  };
  assert.equal(at(-47.9, -15.8), "BRA", "Brasília");
  assert.equal(at(149.1, -35.3), "AUS", "Canberra");
  assert.equal(at(-75.7, 45.4), "CAN", "Ottawa");
  assert.equal(at(-99.1, 19.4), "MEX", "Mexico City");
  assert.equal(at(2.35, 48.85), "FRA", "Paris");
  assert.equal(at(-52.3, 4.9), "GUF", "Cayenne");
  assert.equal(at(15.6, 78.2), "SJM", "Longyearbyen");
  assert.equal(at(37.6, 55.75), "RUS", "Moscow");
  assert.equal(at(-30, 30), null, "Atlantic");

  let checked = 0;
  for (const code of ["RUS", "USA", "FRA", "FJI", "CAN", "IDN", "NOR"]) {
    const f = geometry.features[dataset.features.findIndex((d) => d.code === code)];
    for (let i = 0; i < 400; i++) {
      const x = f.minX + ((f.maxX - f.minX) * ((i * 37) % 400)) / 400;
      const y = f.minY + ((f.maxY - f.minY) * ((i * 151) % 400)) / 400;
      assert.equal(pointInRings(f.rings, x, y, f.ringBounds), pointInRings(f.rings, x, y));
      checked++;
    }
  }
  assert.ok(checked > 0);
});

check("world extent: zoom-out floor shows the whole map and pan keeps its centre on the map", () => {
  const size = { width: 1000, height: 640 };
  const region = { minX: -0.1, minY: -0.1, maxX: 0.1, maxY: 0.1 };
  const world = { minX: -3, minY: -2, maxX: 3, maxY: 2.5 };
  const minK = fitTo(world, size, 20).k * EXTENT_MIN_ZOOM_FACTOR;
  const maxK = fitTo(region, size, 20).k * 40;
  const centre = { x: size.width / 2, y: size.height / 2 };

  const zoomedOut = clampTransform({ k: minK / 10, tx: 0, ty: 0 }, world, size, minK, maxK, centre);
  assert.equal(zoomedOut.k, minK);
  assert.ok(world.maxX * zoomedOut.k * 2 < size.width, "whole world fits at the floor");

  const flung = clampTransform({ k: maxK, tx: 1e7, ty: -1e7 }, world, size, minK, maxK, centre);
  const [cx, cy] = screenToPlane(flung, size.width / 2, size.height / 2);
  assert.ok(cx >= world.minX - 1e-9 && cx <= world.maxX + 1e-9);
  assert.ok(cy >= world.minY - 1e-9 && cy <= world.maxY + 1e-9);
});

console.log(`worldGeo tests passed in ${(performance.now() - t0).toFixed(1)} ms`);

check("world-fit zoom picks the coarsest LOD level at 834, 1440 and 2560 px and cuts vertices sharply", () => {
  const turkey = featuresForRegion(dataset, "Turkey");
  const projection = createProjection(padBBox(regionExtent(turkey), 0.08));
  const geometry = buildSceneGeometry(dataset, "Turkey", projection, new Set());
  const all = geometry.allBounds;
  assert.ok(all);
  const span = Math.max(all.maxX - all.minX, all.maxY - all.minY);
  for (const width of [834, 1440, 2560]) {
    assert.equal(pickLodLevel(geometry.lodTolerances, width / span), 0, `width ${width}`);
  }

  const counts = geometry.lodTolerances.map((tol) => {
    let n = 0;
    for (const f of geometry.features) for (const ring of f.rings) n += (simplifyRing(ring, tol)?.length ?? 0) / 2;
    return n;
  });
  let full = 0;
  for (const f of geometry.features) for (const ring of f.rings) full += ring.length / 2;
  console.log(`LOD vertices: full ${full}, levels ${counts.join(", ")}`);
  assert.ok(counts[0] < full * 0.4, `coarse ${counts[0]} vs full ${full}`);
});
