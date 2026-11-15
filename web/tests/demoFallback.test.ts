import assert from "node:assert/strict";
import { decideNextDataMode } from "../src/lib/dataMode.ts";
import {
  buildDemoPoints,
  buildDemoRegions,
  buildDemoSeries,
  demoCoveredMonth,
  DEMO_SOURCE_LABEL,
  isDemoPeriodCovered,
} from "../src/lib/demoData.ts";
import { parseHash, serializeHash, type UrlState } from "../src/lib/urlState.ts";
import { mockPoints, mockSeries } from "../src/mocks/index.ts";

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

check("buildDemoRegions returns one region consistent with mockSeries and synthetic source", () => {
  const regionsResp = buildDemoRegions();
  assert.equal(regionsResp.regions.length, 1);
  const only = regionsResp.regions[0];
  assert.equal(only.id, mockSeries.region);
  assert.equal(only.name, mockSeries.region);
  assert.deepEqual(only.modisYears, [2000, 2025]);
  assert.deepEqual(only.viirsYears, [2012, 2025]);
  assert.equal(only.tooLarge, false);

  assert.equal(regionsResp.source, DEMO_SOURCE_LABEL);
  assert.ok(regionsResp.source.toLowerCase().includes("synthetic"));
  assert.ok(regionsResp.source.toLowerCase().includes("demo"));
  assert.ok(!regionsResp.source.toLowerCase().startsWith("nasa firms"));
});

check("buildDemoSeries returns mockSeries for fixture region and empty series otherwise", () => {
  const series = buildDemoSeries(mockSeries.region);
  assert.equal(series.region, mockSeries.region);
  assert.equal(series.months.length, mockSeries.months.length);
  assert.equal(series.years.length, mockSeries.years.length);

  const other = buildDemoSeries("UnknownRegion");
  assert.equal(other.region, "UnknownRegion");
  assert.deepEqual(other.years, []);
  assert.deepEqual(other.months, []);
  assert.equal(other.cliff, null);
  assert.equal(other.coverage, null);
});

check("buildDemoPoints covers fixture year/month and returns well-formed empty response for uncovered periods", () => {
  const fullYear = buildDemoPoints({ region: mockPoints.region, year: mockPoints.year });
  assert.equal(fullYear.raw.length, mockPoints.raw.length);
  assert.equal(fullYear.harmonized.length, mockPoints.harmonized.length);

  const aug = buildDemoPoints({ region: mockPoints.region, year: 2021, month: 8 });
  assert.equal(aug.region, mockPoints.region);
  assert.equal(aug.year, 2021);
  assert.equal(aug.month, 8);
  assert.ok(aug.raw.length > 0);
  assert.ok(aug.harmonized.length > 0);
  assert.equal(aug.stages.rawModis + aug.stages.rawViirs, aug.raw.length);
  assert.equal(
    aug.raw.length - aug.stages.interscanRemoved - aug.stages.fleetMerged - aug.stages.matchedPairs,
    aug.harmonized.length,
  );

  const uncoveredYear = buildDemoPoints({ region: mockPoints.region, year: 2019, month: 8 });
  assert.equal(uncoveredYear.region, mockPoints.region);
  assert.equal(uncoveredYear.year, 2019);
  assert.equal(uncoveredYear.month, 8);
  assert.equal(uncoveredYear.truncated, false);
  assert.deepEqual(uncoveredYear.raw, []);
  assert.deepEqual(uncoveredYear.harmonized, []);
  assert.equal(uncoveredYear.bbox, null);
  assert.deepEqual(uncoveredYear.stages, {
    rawModis: 0,
    rawViirs: 0,
    staticMasked: 0,
    samaRemoved: 0,
    interscanRemoved: 0,
    fleetMerged: 0,
    matchedPairs: 0,
    harmonized: 0,
  });

  const uncoveredRegion = buildDemoPoints({ region: "Greece", year: 2021, month: 8 });
  assert.equal(uncoveredRegion.region, "Greece");
  assert.deepEqual(uncoveredRegion.raw, []);
  assert.deepEqual(uncoveredRegion.harmonized, []);
  assert.equal(uncoveredRegion.bbox, null);
});

check("urlState omits default data=live and round-trips data=demo", () => {
  const hashDefault = serializeHash(BASE_STATE);
  const hashLive = serializeHash({ ...BASE_STATE, data: "live" });
  assert.equal(hashDefault.includes("data="), false);
  assert.equal(hashLive.includes("data="), false);
  assert.equal(parseHash(hashDefault).data, undefined);

  const demoState: UrlState = { ...BASE_STATE, data: "demo" };
  const demoHash = serializeHash(demoState);
  assert.equal(demoHash.includes("data=demo"), true);
  const parsed = parseHash(demoHash);
  assert.equal(parsed.data, "demo");
  assert.equal(parsed.region, demoState.region);
  assert.deepEqual(parsed.month, demoState.month);
});

check("urlState accepts data=live and ignores invalid data values", () => {
  assert.equal(parseHash("#region=Turkey&data=live").data, "live");
  for (const bogus of ["synthetic", "DEMO", "offline", "1", ""]) {
    const parsed = parseHash(`#region=Turkey&mode=raw&data=${bogus}`);
    assert.equal(parsed.region, "Turkey");
    assert.equal(parsed.mode, "raw");
    assert.equal(parsed.data, undefined);
  }
});

check("decideNextDataMode maps fetch and probe outcomes accurately", () => {
  assert.deepEqual(
    decideNextDataMode(
      { dataMode: "live", dataModeSource: "auto" },
      { source: "fetch", ok: false, kind: "unreachable", status: null },
    ),
    { dataMode: "demo", dataModeSource: "auto" },
  );

  assert.deepEqual(
    decideNextDataMode(
      { dataMode: "live", dataModeSource: "auto" },
      { source: "fetch", ok: false, kind: "server", status: 404 },
    ),
    { dataMode: "live", dataModeSource: "auto" },
  );

  assert.deepEqual(
    decideNextDataMode(
      { dataMode: "live", dataModeSource: "auto" },
      { source: "fetch", ok: false, kind: "server", status: 400 },
    ),
    { dataMode: "live", dataModeSource: "auto" },
  );

  assert.deepEqual(
    decideNextDataMode(
      { dataMode: "live", dataModeSource: "auto" },
      { source: "probe", ok: false, status: 503 },
    ),
    { dataMode: "demo", dataModeSource: "auto" },
  );

  assert.deepEqual(
    decideNextDataMode(
      { dataMode: "live", dataModeSource: "auto" },
      { source: "probe", ok: false, status: null },
    ),
    { dataMode: "demo", dataModeSource: "auto" },
  );

  assert.deepEqual(
    decideNextDataMode(
      { dataMode: "demo", dataModeSource: "auto" },
      { source: "probe", ok: true },
    ),
    { dataMode: "live", dataModeSource: "auto" },
  );

  assert.deepEqual(
    decideNextDataMode(
      { dataMode: "demo", dataModeSource: "manual" },
      { source: "probe", ok: true },
    ),
    { dataMode: "demo", dataModeSource: "manual" },
  );

  assert.deepEqual(
    decideNextDataMode(
      { dataMode: "demo", dataModeSource: "manual" },
      { source: "probe", ok: true, forceLive: true },
    ),
    { dataMode: "live", dataModeSource: "auto" },
  );
});

check("demoCoveredMonth returns the busiest covered year/month for buildDemoPoints", () => {
  const covered = demoCoveredMonth();
  const points = buildDemoPoints({
    region: mockPoints.region,
    year: covered.year,
    month: covered.month,
  });
  assert.ok(points.harmonized.length > 0);

  for (let month = 1; month <= 12; month += 1) {
    const candidate = buildDemoPoints({
      region: mockPoints.region,
      year: mockPoints.year,
      month,
    });
    assert.ok(
      points.harmonized.length >= candidate.harmonized.length,
      `expected covered month ${covered.year}-${covered.month} (${points.harmonized.length}) >= month ${month} (${candidate.harmonized.length})`,
    );
  }

  assert.equal(isDemoPeriodCovered(mockPoints.region, covered.year, covered), true);
  assert.equal(isDemoPeriodCovered(mockPoints.region, null, covered), true);
  assert.equal(isDemoPeriodCovered(mockPoints.region, 2025, null), false);
  assert.equal(isDemoPeriodCovered(mockPoints.region, 2025, { year: 2025, month: 8 }), false);
  assert.equal(isDemoPeriodCovered(mockPoints.region, 2025, covered), false);
  assert.equal(isDemoPeriodCovered("UnknownRegion", covered.year, covered), false);
});
