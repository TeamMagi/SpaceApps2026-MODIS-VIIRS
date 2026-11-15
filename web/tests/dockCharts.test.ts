import assert from "node:assert/strict";
import { anomalyTone, bandPath, linePath, monthAverages, niceTicks, scaleLinear } from "../src/lib/dockCharts.ts";
import type { MonthCell } from "../src/types/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

function cell(year: number, month: number, harmonizedCount: number, rawCount: number, hasData = true): MonthCell {
  return {
    year,
    month,
    hasData,
    rawModis: 0,
    rawViirs: 0,
    rawCount,
    harmonizedCount,
    anomalyRaw: null,
    anomalyHarmonized: null,
    meanFrp: null,
    meanFrpLower: null,
    meanFrpUpper: null,
  };
}

check("scaleLinear maps domain to range and tolerates a flat domain", () => {
  const s = scaleLinear(0, 10, 100, 200);
  assert.equal(s(5), 150);
  assert.equal(scaleLinear(3, 3, 7, 9)(3), 7);
});

check("linePath breaks at nulls", () => {
  const d = linePath([
    { x: 0, y: 1 },
    { x: 10, y: 2 },
    { x: 20, y: null },
    { x: 30, y: 3 },
  ]);
  assert.equal(d, "M0.0 1.0L10.0 2.0M30.0 3.0");
  assert.equal(linePath([{ x: 0, y: null }]), "");
});

check("bandPath emits one closed polygon per run", () => {
  const d = bandPath([
    { x: 0, lo: 1, hi: 3 },
    { x: 10, lo: 2, hi: 4 },
    { x: 20, lo: null, hi: null },
    { x: 30, lo: 5, hi: 6 },
  ]);
  assert.equal(d, "M0.0 3.0L10.0 4.0L10.0 2.0L0.0 1.0ZM30.0 6.0L30.0 5.0Z");
});

check("niceTicks gives round 0/mid/top covering the max", () => {
  assert.deepEqual(niceTicks(86054).ticks, [0, 45000, 90000]);
  assert.deepEqual(niceTicks(33).ticks, [0, 20, 40]);
  assert.ok(niceTicks(0.7).top >= 0.7);
  assert.deepEqual(niceTicks(0).ticks, [0, 0.5, 1]);
  for (const m of [1, 7, 99, 1234, 5960, 123456]) assert.ok(niceTicks(m).top >= m);
});

check("monthAverages excludes the selected year and missing data", () => {
  const months = [cell(2020, 1, 10, 20), cell(2021, 1, 30, 40), cell(2022, 1, 999, 999), cell(2020, 2, 0, 0, false)];
  const harm = monthAverages(months, 2022, true);
  assert.equal(harm[0], 20);
  assert.equal(harm[1], null);
  assert.equal(monthAverages(months, 2022, false)[0], 30);
});

check("anomalyTone flags z >= 1 only", () => {
  assert.equal(anomalyTone(1), "hot");
  assert.equal(anomalyTone(0.99), "normal");
  assert.equal(anomalyTone(null), "normal");
});
