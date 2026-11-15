import assert from "node:assert/strict";
import {
  DEFAULT_PIPELINE_CONFIG,
  makePointsKey,
  overridesToQueryParams,
  uiOverridesToQueryParams,
} from "../src/lib/pipelineOverrides.ts";
import type { PipelineConfig } from "../src/types/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

check("overridesToQueryParams omits values equal to server defaults", () => {
  const query = overridesToQueryParams({
    dedupTimeSecs: 8,
    dedupOverlap: 0.3,
    matchRadiusKm: 2.0,
    matchWindowSecs: 10800,
    fleetRadiusKm: 2.0,
  });
  assert.deepEqual(query, {});
});

check("overridesToQueryParams emits only non-default overrides", () => {
  const query = overridesToQueryParams({
    dedupTimeSecs: 12,
    dedupOverlap: 0.3, // equals default
    matchRadiusKm: 5.5,
  });
  assert.deepEqual(query, {
    dedup_time_secs: "12",
    match_radius_km: "5.5",
  });
});

check("uiOverridesToQueryParams converts percent to ratio and minutes to seconds while omitting defaults", () => {
  const allDefaults = uiOverridesToQueryParams({
    dedupTimeSecs: 8,
    dedupOverlapPct: 30,
    matchRadiusKm: 2,
    matchWindowMins: 180,
    fleetRadiusKm: 2,
  });
  assert.deepEqual(allDefaults, {});

  const mixed = uiOverridesToQueryParams({
    dedupTimeSecs: 10,
    dedupOverlapPct: 45,
    matchRadiusKm: 2, // default
    matchWindowMins: 90,
    fleetRadiusKm: 3.5,
  });
  assert.deepEqual(mixed, {
    dedup_time_secs: "10",
    dedup_overlap: "0.45",
    match_window_secs: "5400",
    fleet_radius_km: "3.5",
  });
});

check("uiOverridesToQueryParams compares against custom server defaults", () => {
  const serverConfig: PipelineConfig = {
    ...DEFAULT_PIPELINE_CONFIG,
    dedup: { maxTimeDiffSecs: 10, minOverlapRatio: 0.5 },
    matching: { radiusKm: 3.0, windowSecs: 7200 },
  };
  const query = uiOverridesToQueryParams(
    {
      dedupTimeSecs: 10,
      dedupOverlapPct: 50,
      matchRadiusKm: 3,
      matchWindowMins: 120,
      fleetRadiusKm: 1.5,
    },
    serverConfig,
  );
  assert.deepEqual(query, {
    fleet_radius_km: "1.5",
  });
});

check("makePointsKey is stable for equal overrides and distinguishes differing sets", () => {
  const keyEmpty = makePointsKey("Turkey", 2021, 8, undefined);
  const keyExplicitDefaults = makePointsKey("Turkey", 2021, 8, {
    dedupTimeSecs: 8,
    dedupOverlap: 0.3,
    matchRadiusKm: 2,
    matchWindowSecs: 10800,
    fleetRadiusKm: 2,
  });
  assert.equal(keyEmpty, keyExplicitDefaults);

  const keyA = makePointsKey("Turkey", 2021, 8, {
    dedupTimeSecs: 15,
    fleetRadiusKm: 3.5,
  });
  const keyB = makePointsKey("Turkey", 2021, 8, {
    fleetRadiusKm: 3.5,
    dedupTimeSecs: 15,
    matchRadiusKm: 2.0, // redundant default
  });
  assert.equal(keyA, keyB);
  assert.notEqual(keyA, keyEmpty);

  const keyDifferentOverride = makePointsKey("Turkey", 2021, 8, {
    dedupTimeSecs: 16,
    fleetRadiusKm: 3.5,
  });
  assert.notEqual(keyA, keyDifferentOverride);

  const keyDifferentMonth = makePointsKey("Turkey", 2021, 7, {
    dedupTimeSecs: 15,
    fleetRadiusKm: 3.5,
  });
  assert.notEqual(keyA, keyDifferentMonth);
});
