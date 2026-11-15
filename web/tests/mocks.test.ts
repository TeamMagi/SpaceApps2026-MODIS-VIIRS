import assert from "node:assert/strict";
import { mockAnnualStats, mockFrpMonthly, mockPoints, mockSeries } from "../src/mocks/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const sum = (values: readonly number[]): number => values.reduce((total, value) => total + value, 0);
const near = (actual: number, expected: number, tolerance: number): void => {
  assert.ok(Math.abs(actual - expected) <= tolerance, `${actual} != ${expected} (+-${tolerance})`);
};
const monthOf = (acquired: string): number => Number(acquired.slice(5, 7));

function yearMean(year: number, pick: "raw" | "harmonized"): number {
  const rows = mockFrpMonthly.filter(row => row.year === year);
  if (pick === "raw") {
    return sum(rows.map(r => r.rawModisFrpSum + r.rawViirsFrpSum)) / sum(rows.map(r => r.rawModisCount + r.rawViirsCount));
  }
  return sum(rows.map(r => r.harmonizedFrpSum)) / sum(rows.map(r => r.harmonizedCount));
}

check("raw points: size, frp range and sensors", () => {
  assert.ok(mockPoints.raw.length >= 500 && mockPoints.raw.length <= 1000);
  for (const point of mockPoints.raw) {
    assert.ok(point.frp >= 1 && point.frp <= 500, `frp ${point.frp}`);
    assert.ok(point.sensor === "MODIS" || point.sensor === "VIIRS");
  }
});

check("raw and harmonized links are consistent", () => {
  const harmonizedIds = new Set(mockPoints.harmonized.map(p => p.id));
  const members = new Map<string, number>();
  for (const point of mockPoints.raw) {
    if (point.harmonizedId === null) continue;
    assert.ok(harmonizedIds.has(point.harmonizedId), `dangling ${point.harmonizedId}`);
    members.set(point.harmonizedId, (members.get(point.harmonizedId) ?? 0) + 1);
  }
  for (const point of mockPoints.harmonized) {
    const count = members.get(point.id) ?? 0;
    assert.ok(count >= 1, `no members for ${point.id}`);
    assert.equal(point.nObs, count);
  }
});

check("pipeline stages add up", () => {
  const { stages, raw, harmonized } = mockPoints;
  assert.equal(stages.rawModis + stages.rawViirs, raw.length);
  assert.equal(raw.length - stages.interscanRemoved - stages.fleetMerged - stages.matchedPairs, harmonized.length);
});

check("series: coverage, order and sensor eras", () => {
  const { months } = mockSeries;
  assert.equal(months.length, 302);
  assert.deepEqual([months[0].year, months[0].month], [2000, 11]);
  assert.deepEqual([months.at(-1)?.year, months.at(-1)?.month], [2025, 12]);
  assert.equal(new Set(months.map(m => `${m.year}-${m.month}`)).size, months.length);
  for (const m of months) {
    assert.equal(m.rawModis + m.rawViirs, m.rawTotal);
    if (m.year < 2012) assert.equal(m.rawViirs, 0);
  }
  assert.ok(months.filter(m => m.year === 2012 && m.rawViirs > 0).length >= 10);
  for (const y of mockSeries.years) assert.equal(y.modisOnly + y.viirsOnly + y.matched, y.harmonized);
});

check("seasonality: summer dominates winter", () => {
  const total = (months: number[]) =>
    sum(mockSeries.months.filter(m => months.includes(m.month)).map(m => m.rawTotal));
  assert.ok(total([7, 8]) > 3 * total([1, 2]));
});

check("sensor cliff: raw count and raw FRP drop, harmonized FRP stays continuous", () => {
  assert.ok(mockSeries.cliff.rawChangeRatio !== null && mockSeries.cliff.rawChangeRatio > 0.5);
  assert.ok(yearMean(2012, "raw") <= 0.75 * yearMean(2011, "raw"));
  const change = Math.abs(yearMean(2012, "harmonized") - yearMean(2011, "harmonized")) / yearMean(2011, "harmonized");
  assert.ok(change < 0.2, `harmonized change ${change}`);
});

check("frpMonths: aligned with months, means equal sums over counts", () => {
  const { months } = mockSeries;
  assert.equal(mockFrpMonthly.length, months.length);
  mockFrpMonthly.forEach((row, i) => {
    const m = months[i];
    assert.deepEqual([row.year, row.month], [m.year, m.month]);
    assert.equal(row.rawModisCount, m.rawModis);
    assert.equal(row.rawViirsCount, m.rawViirs);
    assert.equal(row.harmonizedCount, m.harmonized);
    const rawCount = row.rawModisCount + row.rawViirsCount;
    if (rawCount === 0) assert.equal(row.meanFrpRaw, null);
    else near(row.meanFrpRaw ?? NaN, (row.rawModisFrpSum + row.rawViirsFrpSum) / rawCount, 1e-3);
    if (row.harmonizedCount === 0) assert.equal(row.meanFrpHarmonized, null);
    else near(row.meanFrpHarmonized ?? NaN, row.harmonizedFrpSum / row.harmonizedCount, 1e-3);
  });
});

check("2021: frpMonths and stats match the points", () => {
  for (const row of mockFrpMonthly.filter(r => r.year === 2021)) {
    const raw = mockPoints.raw.filter(p => monthOf(p.acquired) === row.month);
    near(row.rawModisFrpSum, sum(raw.filter(p => p.sensor === "MODIS").map(p => p.frp)), 1e-3);
    near(row.rawViirsFrpSum, sum(raw.filter(p => p.sensor === "VIIRS").map(p => p.frp)), 1e-3);
    const harmonized = mockPoints.harmonized.filter(p => monthOf(p.acquired) === row.month);
    near(row.harmonizedFrpSum, sum(harmonized.map(p => p.frpCorrected)), 1e-3);
  }
  assert.equal(mockAnnualStats.rawCount, 800);
  assert.equal(mockAnnualStats.harmonizedCount, 500);
  near(mockAnnualStats.rawFrpSum, sum(mockPoints.raw.map(p => p.frp)), 1e-3);
});
