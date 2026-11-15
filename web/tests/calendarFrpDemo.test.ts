import assert from "node:assert/strict";
import { selectCalendarFrp, type CalendarFrpMap } from "../src/hooks/useCalendarFrp.ts";
import type { CalendarCell } from "../src/types/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const cell: CalendarCell = {
  year: 2021,
  month: 8,
  count: 1200,
  countNormalized: 1100,
  effortOmega: 1.1,
  meanFrp: 24.5,
  meanFrpRaw: 22.1,
  meanFrpLower: 20,
  meanFrpUpper: 29,
  totalFrp: 29400,
  totalFrpLower: 25000,
  totalFrpUpper: 34000,
  frpPerDetectionEq: 26.7,
  anomaly: 2.1,
  qualityFlags: [],
};
const live: CalendarFrpMap = new Map([["2021-8", cell]]);

check("demo mode never shows archive FRP, even when the live cache already holds the region", () => {
  assert.equal(selectCalendarFrp(true, "Turkey", live, null).size, 0);
  assert.equal(selectCalendarFrp(true, "Turkey", undefined, { region: "Turkey", cells: live }).size, 0);
});

check("live mode serves the cache first, then the cells loaded for this region", () => {
  assert.equal(selectCalendarFrp(false, "Turkey", live, null), live);
  assert.equal(selectCalendarFrp(false, "Turkey", undefined, { region: "Turkey", cells: live }), live);
});

check("live mode never shows another region's cells while this one loads", () => {
  assert.equal(selectCalendarFrp(false, "Greece", undefined, { region: "Turkey", cells: live }).size, 0);
  assert.equal(selectCalendarFrp(false, "Greece", undefined, null).size, 0);
});
