import assert from "node:assert/strict";
import { performance } from "node:perf_hooks";
import { createTranslator } from "../src/content/copy.ts";
import {
  aggregateRegionTotals,
  computeDoubleCounts,
  summarizePointStages,
} from "../src/lib/receiptCounts.ts";
import type { PointStages, YearlyPoint } from "../src/types/index.ts";

const t0 = performance.now();

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

function makeYear(overrides: Partial<YearlyPoint> & { year: number }): YearlyPoint {
  return {
    raw: 0,
    rawModis: 0,
    rawViirs: 0,
    harmonized: 0,
    matched: 0,
    modisOnly: 0,
    viirsOnly: 0,
    staticMasked: 0,
    samaRemoved: 0,
    interscanRemoved: 0,
    fleetMerged: 0,
    firstDate: null,
    lastDate: null,
    partial: false,
    modisFirstDate: null,
    modisLastDate: null,
    viirsFirstDate: null,
    viirsLastDate: null,
    ...overrides,
  };
}

check("computeDoubleCounts excludes staticMasked and samaRemoved from duplicate removals", () => {
  assert.equal(
    computeDoubleCounts({ raw: 1000, staticMasked: 250, samaRemoved: 50, harmonized: 520 }),
    180,
  );
  assert.equal(
    computeDoubleCounts({ raw: 100, staticMasked: 60, samaRemoved: 50, harmonized: 0 }),
    0,
  );
});

check("aggregateRegionTotals keeps raw - staticMasked - samaRemoved - interscan - fleet - matched = harmonized", () => {
  const years: YearlyPoint[] = [
    makeYear({
      year: 2020,
      rawModis: 300,
      rawViirs: 700,
      raw: 1000,
      staticMasked: 120,
      samaRemoved: 30,
      interscanRemoved: 50,
      fleetMerged: 40,
      matched: 60,
      modisOnly: 180,
      viirsOnly: 460,
      harmonized: 700,
    }),
    makeYear({
      year: 2021,
      rawModis: 200,
      rawViirs: 400,
      raw: 600,
      staticMasked: 80,
      samaRemoved: 20,
      interscanRemoved: 25,
      fleetMerged: 15,
      matched: 35,
      modisOnly: 100,
      viirsOnly: 290,
      harmonized: 425,
    }),
  ];

  const totals = aggregateRegionTotals(years);
  assert.equal(totals.raw, 1600);
  assert.equal(totals.staticMasked, 200);
  assert.equal(totals.samaRemoved, 50);
  assert.equal(totals.interscanRemoved, 75);
  assert.equal(totals.fleetMerged, 55);
  assert.equal(totals.matched, 95);
  assert.equal(totals.harmonized, 1125);
  assert.equal(totals.doubleCounts, 75 + 55 + 95);
  assert.equal(totals.doubleCountShare, 225 / 1600);
  assert.equal(
    totals.raw -
      totals.staticMasked -
      totals.samaRemoved -
      totals.interscanRemoved -
      totals.fleetMerged -
      totals.matched,
    totals.harmonized,
  );
});

check("summarizePointStages matches archive aggregation for identical counts across period and archive modes", () => {
  const year = makeYear({
    year: 2023,
    rawModis: 450,
    rawViirs: 1550,
    raw: 2000,
    staticMasked: 300,
    samaRemoved: 45,
    interscanRemoved: 110,
    fleetMerged: 90,
    matched: 155,
    harmonized: 1300,
  });
  const stages: PointStages = {
    rawModis: year.rawModis,
    rawViirs: year.rawViirs,
    staticMasked: year.staticMasked,
    samaRemoved: year.samaRemoved,
    interscanRemoved: year.interscanRemoved,
    fleetMerged: year.fleetMerged,
    matchedPairs: year.matched,
    harmonized: year.harmonized,
  };

  const archiveTotals = aggregateRegionTotals([year]);
  const periodCounts = summarizePointStages(stages);

  assert.equal(periodCounts.raw, archiveTotals.raw);
  assert.equal(periodCounts.staticMasked, archiveTotals.staticMasked);
  assert.equal(periodCounts.samaRemoved, archiveTotals.samaRemoved);
  assert.equal(periodCounts.doubleCounts, archiveTotals.doubleCounts);
  assert.equal(
    periodCounts.raw -
      periodCounts.staticMasked -
      periodCounts.samaRemoved -
      periodCounts.interscanRemoved -
      periodCounts.fleetMerged -
      periodCounts.matchedPairs,
    periodCounts.harmonized,
  );
});

check("legacy zero-default mask/SAMA counts preserve doubleCounts and empty input stays zero", () => {
  const empty = aggregateRegionTotals([]);
  assert.equal(empty.raw, 0);
  assert.equal(empty.doubleCounts, 0);
  assert.equal(empty.doubleCountShare, 0);

  const legacy = aggregateRegionTotals([
    makeYear({
      year: 2015,
      rawModis: 100,
      rawViirs: 200,
      raw: 300,
      interscanRemoved: 10,
      matched: 20,
      harmonized: 250,
    }),
  ]);
  assert.equal(legacy.staticMasked, 0);
  assert.equal(legacy.samaRemoved, 0);
  assert.equal(legacy.doubleCounts, 50);
});

check("copy dictionary defines EN and TR receipt labels for static mask and SAMA", () => {
  const en = createTranslator("en");
  const tr = createTranslator("tr");
  assert.ok(en("receiptStaticMasked").length > 0);
  assert.ok(en("receiptSamaRemoved").length > 0);
  assert.ok(tr("receiptStaticMasked").length > 0);
  assert.ok(tr("receiptSamaRemoved").length > 0);
});

console.log(`receiptCounts.test.ts completed in ${(performance.now() - t0).toFixed(2)} ms`);
