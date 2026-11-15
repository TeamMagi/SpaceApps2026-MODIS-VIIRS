import assert from "node:assert/strict";
import { applyDemoFusionModel } from "../src/lib/demoData.ts";
import { makePointsKey } from "../src/lib/pipelineOverrides.ts";
import { parseHash, serializeHash, type UrlState } from "../src/lib/urlState.ts";
import type { PointsResponse } from "../src/services/api.ts";
import { adaptFusionComparison, adaptFusionModel } from "../src/services/api.ts";
import {
  Confidence,
  DayNight,
  HotspotSource,
  type HarmonizedPoint,
  type RawPoint,
} from "../src/types/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const BASE_STATE: UrlState = {
  region: "Turkey",
  mode: "harmonized",
  month: { year: 2021, month: 8 },
  lang: "en",
  minConfidence: "low",
};

check("fm round-trips for all models and is omitted for the default", () => {
  const defaultHash = serializeHash({ ...BASE_STATE, fusionModel: "viirs_preferred" });
  assert.equal(defaultHash.includes("fm="), false);
  assert.equal(defaultHash, serializeHash(BASE_STATE));
  assert.equal(parseHash(defaultHash).fusionModel, "viirs_preferred");

  const eq = serializeHash({ ...BASE_STATE, fusionModel: "equal_weight" });
  assert.ok(eq.includes("fm=eq"));
  assert.equal(parseHash(eq).fusionModel, "equal_weight");

  const mc = serializeHash({ ...BASE_STATE, fusionModel: "modis_calibrated" });
  assert.ok(mc.includes("fm=mc"));
  assert.equal(parseHash(mc).fusionModel, "modis_calibrated");
});

check("unknown fm falls back to the default", () => {
  assert.equal(parseHash("#region=Turkey&fm=bogus").fusionModel, "viirs_preferred");
  assert.equal(parseHash("#region=Turkey&fm=constructor").fusionModel, "viirs_preferred");
});

check("points cache key differs per model but is unchanged for the default", () => {
  const base = makePointsKey("Turkey", 2021, 8);
  assert.equal(makePointsKey("Turkey", 2021, 8, undefined, undefined, "low", "viirs_preferred"), base);
  assert.notEqual(makePointsKey("Turkey", 2021, 8, undefined, undefined, "low", "equal_weight"), base);
});

check("api adapter maps fusion_comparison (present / null / missing)", () => {
  const mapped = adaptFusionComparison({
    model: "modis_calibrated",
    baseline_count: 100,
    baseline_total_frp: 2000.5,
    count: 90,
    total_frp: 1800,
    dropped_below_modis_limit: 12,
  });
  assert.deepEqual(mapped, {
    model: "modis_calibrated",
    baselineCount: 100,
    baselineTotalFrp: 2000.5,
    count: 90,
    totalFrp: 1800,
    droppedBelowModisLimit: 12,
  });
  assert.equal(adaptFusionComparison(null), null);
  assert.equal(adaptFusionComparison(undefined), null);
  assert.equal(adaptFusionModel(undefined), "viirs_preferred");
  assert.equal(adaptFusionModel("equal_weight"), "equal_weight");
});

function harmonized(id: string, source: HotspotSource, frp: number, daynight = DayNight.Day): HarmonizedPoint {
  return {
    id,
    lat: 0,
    lon: 0,
    frp,
    frpCorrected: frp,
    source,
    confidence: Confidence.Nominal,
    acquired: "2021-08-01T10:00:00Z",
    daynight,
    vzaFactor: null,
    nObs: 1,
    platforms: [],
    frpLower: null,
    frpUpper: null,
  };
}

function raw(id: string, sensor: "MODIS" | "VIIRS", frp: number, harmonizedId: string): RawPoint {
  return {
    id,
    lat: 0,
    lon: 0,
    sensor,
    frp,
    confidence: Confidence.Nominal,
    acquired: "2021-08-01T10:00:00Z",
    daynight: DayNight.Day,
    harmonizedId,
  };
}

const FIXTURE: PointsResponse = {
  region: "Turkey",
  year: 2021,
  month: 8,
  truncated: false,
  limit: 4000,
  raw: [raw("r1", "MODIS", 10, "m"), raw("r2", "VIIRS", 30, "m")],
  harmonized: [
    harmonized("m", HotspotSource.Matched, 30),
    harmonized("weak", HotspotSource.ViirsOnly, 2),
    harmonized("strong", HotspotSource.ViirsOnly, 20),
  ],
  stages: {
    rawModis: 1,
    rawViirs: 2,
    staticMasked: 0,
    samaRemoved: 0,
    interscanRemoved: 0,
    fleetMerged: 0,
    matchedPairs: 1,
    harmonized: 3,
  },
  bbox: null,
  filters: { minConfidence: "low", excludedDetections: 0 },
  tropicalSwathGap: null,
};

check("demo modis_calibrated drops weak ViirsOnly, keeps strong, reports the comparison", () => {
  const out = applyDemoFusionModel(FIXTURE, "modis_calibrated");
  const ids = out.harmonized.map((p) => p.id);
  assert.deepEqual(ids, ["m", "strong"]);
  assert.equal(out.harmonized[0].frpCorrected, 10);
  assert.equal(out.stages.harmonized, 2);
  assert.deepEqual(out.fusionComparison, {
    model: "modis_calibrated",
    baselineCount: 3,
    baselineTotalFrp: 52,
    count: 2,
    totalFrp: 30,
    droppedBelowModisLimit: 1,
  });
});

check("demo equal_weight averages member FRPs and the default model is untouched", () => {
  const out = applyDemoFusionModel(FIXTURE, "equal_weight");
  assert.equal(out.harmonized.find((p) => p.id === "m")?.frpCorrected, 20);
  assert.equal(out.harmonized.length, 3);
  assert.equal(out.fusionComparison?.droppedBelowModisLimit, 0);
  assert.equal(applyDemoFusionModel(FIXTURE, "viirs_preferred"), FIXTURE);
});
