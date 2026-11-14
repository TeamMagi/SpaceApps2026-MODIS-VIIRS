import assert from "node:assert/strict";
import fs from "node:fs";
import { performance } from "node:perf_hooks";
import type { GeoDataset, GeoFeature } from "../src/lib/geo/types.ts";
import { matchRegionsToFeatures, tokenizeGeoName } from "../src/lib/geo/regionMatch.ts";
import { createProjection, padBBox, regionExtent } from "../src/lib/geo/projection.ts";
import {
  buildScene,
  buildSceneGeometry,
  buildScenePoints,
  composeScene,
} from "../src/components/Map/engine/renderer.ts";
import { hitTestFeatures, pointInRings } from "../src/components/Map/engine/featureHit.ts";

const t0 = performance.now();

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const dataset = JSON.parse(fs.readFileSync("public/geo/regions.json", "utf8")) as GeoDataset;
const archiveSet = JSON.parse(fs.readFileSync("../data/archive-set.json", "utf8")) as {
  countries: string[];
};
const archiveRegions = archiveSet.countries.map((id) => ({ id, name: id.replace(/_/g, " ") }));

function square(x0: number, y0: number, x1: number, y1: number): [number, number][] {
  return [
    [x0, y0],
    [x1, y0],
    [x1, y1],
    [x0, y1],
    [x0, y0],
  ];
}

function feature(code: string, name: string, regionId: string | null = null): GeoFeature {
  return { code, name, regionId, bbox: [0, 0, 1, 1], polygons: [[square(0, 0, 1, 1)]] };
}

check("tokenizer folds case, diacritics, underscores, filler words and marks abbreviations", () => {
  assert.equal(tokenizeGeoName("Russian_Federation").key, "russian federation");
  assert.equal(tokenizeGeoName("Côte d'Ivoire").key, tokenizeGeoName("Cote_d_Ivoire").key);
  const bosnia = tokenizeGeoName("Bosnia and Herz.");
  assert.deepEqual(bosnia.words, ["bosnia", "herz"]);
  assert.deepEqual(bosnia.abbreviated, [false, true]);
});

check("every archive region in data/archive-set.json matches regions.json geometry", () => {
  const match = matchRegionsToFeatures(dataset.features, archiveRegions);
  for (const { id } of archiveRegions) {
    assert.ok((match.featuresByRegion.get(id)?.length ?? 0) > 0, `${id} has no feature`);
  }
  const codesOf = (id: string) =>
    (match.featuresByRegion.get(id) ?? []).map((i) => dataset.features[i].code).sort();
  assert.deepEqual(codesOf("Cyprus"), ["CYN", "CYP"]);
  assert.deepEqual(codesOf("Russian_Federation"), ["RUS"]);
  assert.equal(match.regionByFeature.filter((r) => r !== null).length, 12);
});

check("future regions match by ISO code or normalized name without a lookup table", () => {
  const match = matchRegionsToFeatures(dataset.features, [
    { id: "Kazakhstan", name: "Kazakhstan" },
    { id: "UKR", name: "Ukraine" },
    { id: "Saudi_Arabia", name: "Saudi Arabia" },
    { id: "Bosnia_and_Herzegovina", name: "Bosnia and Herzegovina" },
    { id: "Atlantis", name: "Atlantis" },
  ]);
  const codeOf = (id: string) =>
    (match.featuresByRegion.get(id) ?? []).map((i) => dataset.features[i].code);
  assert.deepEqual(codeOf("Kazakhstan"), ["KAZ"]);
  assert.deepEqual(codeOf("UKR"), ["UKR"]);
  assert.deepEqual(codeOf("Saudi_Arabia"), ["SAU"]);
  assert.deepEqual(codeOf("Bosnia_and_Herzegovina"), ["BIH"]);
  assert.equal(match.featuresByRegion.has("Atlantis"), false);
});

check("abbreviation matching needs a trailing dot and a unique candidate", () => {
  const features = [
    feature("COD", "Dem. Rep. Congo"),
    feature("COG", "Congo"),
    feature("NER", "Niger"),
    feature("SSD", "S. Sudan"),
    feature("SDN", "Sudan"),
  ];
  const match = matchRegionsToFeatures(features, [
    { id: "Democratic_Republic_of_the_Congo", name: "Democratic Republic of the Congo" },
    { id: "Congo", name: "Congo" },
    { id: "Nigeria", name: "Nigeria" },
    { id: "South_Sudan", name: "South Sudan" },
    { id: "Sudan", name: "Sudan" },
  ]);
  assert.deepEqual(match.regionByFeature, [
    "Democratic_Republic_of_the_Congo",
    "Congo",
    null,
    "South_Sudan",
    "Sudan",
  ]);
});

check("build-time links win and a feature is never claimed twice", () => {
  const features = [feature("CYP", "Cyprus", "Cyprus"), feature("CYN", "N. Cyprus", "Cyprus")];
  const match = matchRegionsToFeatures(features, [
    { id: "Cyprus", name: "Cyprus" },
    { id: "CYP", name: "Cyprus" },
    { id: "Cyprus", name: "Duplicate entry" },
  ]);
  assert.deepEqual(match.regionByFeature, ["Cyprus", "Cyprus"]);
  assert.equal(match.featuresByRegion.has("CYP"), false);
});

check("pointInRings handles holes and multi-part features with even-odd", () => {
  const ring = (pts: [number, number][]) => Float32Array.from(pts.flat());
  const outer = ring(square(0, 0, 10, 10));
  const hole = ring(square(4, 4, 6, 6));
  const island = ring(square(20, 20, 22, 22));
  const rings = [outer, hole, island];
  assert.equal(pointInRings(rings, 2, 2), true);
  assert.equal(pointInRings(rings, 5, 5), false, "inside the hole");
  assert.equal(pointInRings(rings, 21, 21), true, "second polygon");
  assert.equal(pointInRings(rings, 15, 15), false);
});

check("overlapping features resolve to the smallest one so enclaves stay reachable", () => {
  const big = feature("BIG", "Big");
  const enclave: GeoFeature = {
    code: "ENC",
    name: "Enclave",
    regionId: null,
    bbox: [0.4, 0.4, 0.6, 0.6],
    polygons: [[square(0.4, 0.4, 0.6, 0.6)]],
  };
  const identity = { project: (lon: number, lat: number) => [lon, lat] as const };
  const geometry = buildSceneGeometry([big, enclave], "", identity, new Set());
  assert.equal(hitTestFeatures(geometry.features, 0.5, 0.5), 1);
  assert.equal(hitTestFeatures(geometry.features, 0.1, 0.1), 0);
});

check("geometry is reused when only the points change", () => {
  const projection = createProjection(padBBox(regionExtent(dataset.features), 0.02));
  const geometry = buildSceneGeometry(dataset, "Turkey", projection);
  const a = composeScene("Turkey", geometry, buildScenePoints(null, projection));
  const b = composeScene("Turkey", geometry, buildScenePoints({ raw: [], harmonized: [] }, projection));
  assert.equal(a.features, b.features);
  assert.equal(a.contextPath, b.contextPath);
  assert.deepEqual(a.bounds, geometry.selectedBounds);
});

check("hit test on projected regions.json finds countries and skips filtered features", () => {
  const turkeyIdx = dataset.features.findIndex((f) => f.code === "TUR");
  const projection = createProjection(padBBox(regionExtent([dataset.features[turkeyIdx]]), 0.08));
  const scene = buildScene(dataset, "Turkey", null, projection, new Set([turkeyIdx]));
  assert.equal(scene.features.length, dataset.features.length);

  const at = (lon: number, lat: number, accept?: (i: number) => boolean) => {
    const [x, y] = projection.project(lon, lat);
    const idx = hitTestFeatures(scene.features, x, y, accept);
    return idx >= 0 ? dataset.features[idx].code : null;
  };
  assert.equal(at(32.85, 39.93), "TUR", "Ankara");
  assert.equal(at(23.73, 37.98), "GRC", "Athens");
  assert.equal(at(44.4, 33.3), "IRQ", "Baghdad");
  assert.equal(at(34, 43.2), null, "Black Sea");
  assert.equal(at(32.85, 39.93, (i) => i !== turkeyIdx), null, "selected region filtered out");
});

check("hit test over every feature stays well under a frame budget", () => {
  const projection = createProjection(padBBox(regionExtent(dataset.features), 0.02));
  const scene = buildScene(dataset, "", null, projection, new Set());
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const f of scene.features) {
    minX = Math.min(minX, f.minX);
    minY = Math.min(minY, f.minY);
    maxX = Math.max(maxX, f.maxX);
    maxY = Math.max(maxY, f.maxY);
  }
  const queries = 2000;
  const start = performance.now();
  let hits = 0;
  for (let i = 0; i < queries; i++) {
    const x = minX + ((maxX - minX) * ((i * 7919) % queries)) / queries;
    const y = minY + ((maxY - minY) * ((i * 104729) % queries)) / queries;
    if (hitTestFeatures(scene.features, x, y) >= 0) hits++;
  }
  const perQueryMs = (performance.now() - start) / queries;
  assert.ok(hits > 0);
  assert.ok(perQueryMs < 1, `hit test took ${perQueryMs.toFixed(3)} ms per query`);
});

console.log(`countryPick tests passed in ${(performance.now() - t0).toFixed(1)} ms`);
