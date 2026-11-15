import assert from "node:assert/strict";
import { annualMeanFrp } from "../src/lib/annualFrp.ts";
import type { MonthCell } from "../src/types/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

function makeCell(overrides: Partial<MonthCell> & Pick<MonthCell, "year" | "month">): MonthCell {
  return {
    hasData: true,
    rawModis: 0,
    rawViirs: 0,
    rawCount: 0,
    harmonizedCount: 0,
    anomalyRaw: null,
    anomalyHarmonized: null,
    meanFrp: null,
    meanFrpLower: null,
    meanFrpUpper: null,
    ...overrides,
  };
}

check("annualMeanFrp computes count-weighted mean and bounds across months", () => {
  const result = annualMeanFrp([
    makeCell({
      year: 2021,
      month: 7,
      harmonizedCount: 10,
      meanFrp: 20,
      meanFrpLower: 15,
      meanFrpUpper: 25,
    }),
    makeCell({
      year: 2021,
      month: 8,
      harmonizedCount: 30,
      meanFrp: 60,
      meanFrpLower: 51,
      meanFrpUpper: 69,
    }),
  ]);

  assert.equal(result.length, 1);
  assert.equal(result[0].year, 2021);
  assert.equal(result[0].months, 2);
  assert.ok(result[0].mean !== null && Math.abs(result[0].mean - 50) < 1e-12);
  assert.ok(result[0].lower !== null && Math.abs(result[0].lower - 42) < 1e-12);
  assert.ok(result[0].upper !== null && Math.abs(result[0].upper - 58) < 1e-12);
});

check("annualMeanFrp drops year bounds to null when any contributing month has null bounds, keeping mean", () => {
  const result = annualMeanFrp([
    makeCell({
      year: 2020,
      month: 6,
      harmonizedCount: 10,
      meanFrp: 30,
      meanFrpLower: 24,
      meanFrpUpper: 36,
    }),
    makeCell({
      year: 2020,
      month: 7,
      harmonizedCount: 10,
      meanFrp: 50,
      meanFrpLower: null,
      meanFrpUpper: 60,
    }),
  ]);

  assert.equal(result.length, 1);
  assert.equal(result[0].year, 2020);
  assert.equal(result[0].months, 2);
  assert.ok(result[0].mean !== null && Math.abs(result[0].mean - 40) < 1e-12);
  assert.equal(result[0].lower, null);
  assert.equal(result[0].upper, null);
});

check("annualMeanFrp ignores months with hasData false, harmonizedCount 0, or non-finite meanFrp", () => {
  const result = annualMeanFrp([
    makeCell({
      year: 2019,
      month: 1,
      hasData: false,
      harmonizedCount: 100,
      meanFrp: 200,
      meanFrpLower: null,
      meanFrpUpper: null,
    }),
    makeCell({
      year: 2019,
      month: 2,
      hasData: true,
      harmonizedCount: 0,
      meanFrp: 500,
      meanFrpLower: null,
      meanFrpUpper: null,
    }),
    makeCell({
      year: 2019,
      month: 3,
      hasData: true,
      harmonizedCount: 5,
      meanFrp: null,
      meanFrpLower: null,
      meanFrpUpper: null,
    }),
    makeCell({
      year: 2019,
      month: 7,
      hasData: true,
      harmonizedCount: 20,
      meanFrp: 42.5,
      meanFrpLower: 35,
      meanFrpUpper: 50,
    }),
  ]);

  assert.equal(result.length, 1);
  assert.equal(result[0].year, 2019);
  assert.equal(result[0].months, 1);
  assert.equal(result[0].mean, 42.5);
  assert.equal(result[0].lower, 35);
  assert.equal(result[0].upper, 50);
});

check("annualMeanFrp returns null metrics and months 0 for years with no contributing month, preserving input year order", () => {
  const result = annualMeanFrp([
    makeCell({ year: 2005, month: 1, hasData: false }),
    makeCell({ year: 2005, month: 2, hasData: true, harmonizedCount: 0 }),
    makeCell({
      year: 2003,
      month: 8,
      hasData: true,
      harmonizedCount: 4,
      meanFrp: 30,
      meanFrpLower: 25,
      meanFrpUpper: 35,
    }),
  ]);

  assert.deepEqual(result, [
    { year: 2005, mean: null, lower: null, upper: null, months: 0 },
    { year: 2003, mean: 30, lower: 25, upper: 35, months: 1 },
  ]);
});
