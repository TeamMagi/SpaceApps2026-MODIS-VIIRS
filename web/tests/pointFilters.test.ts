import assert from "node:assert/strict";
import {
  DEFAULT_POINT_FILTERS,
  activeFilterCount,
  filterPoints,
  getPointFilters,
  resetPointFilters,
  setPointFilters,
  subscribePointFilters,
} from "../src/lib/pointFilters.ts";
import { DayNight } from "../src/types/index.ts";

function check(name: string, fn: () => void): void {
  resetPointFilters();
  fn();
  resetPointFilters();
  console.log(`ok - ${name}`);
}

const pts = [
  { id: "a", frp: 2, daynight: DayNight.Day },
  { id: "b", frp: 10, daynight: DayNight.Night },
  { id: "c", frp: 30, daynight: DayNight.Day },
  { id: "d", frp: 10, daynight: DayNight.Day },
];
const ids = (list: readonly { id: string }[]) => list.map((p) => p.id);

check("no active filter returns the same array", () => {
  assert.equal(filterPoints(pts, DEFAULT_POINT_FILTERS), pts);
});

check("minFrp keeps points at the threshold", () => {
  assert.deepEqual(ids(filterPoints(pts, { minFrp: 10, dayNight: "all" })), ["b", "c", "d"]);
});

check("day and night filters", () => {
  assert.deepEqual(ids(filterPoints(pts, { minFrp: 0, dayNight: "day" })), ["a", "c", "d"]);
  assert.deepEqual(ids(filterPoints(pts, { minFrp: 0, dayNight: "night" })), ["b"]);
});

check("filters combine", () => {
  assert.deepEqual(ids(filterPoints(pts, { minFrp: 10, dayNight: "day" })), ["c", "d"]);
});

check("activeFilterCount", () => {
  assert.equal(activeFilterCount(DEFAULT_POINT_FILTERS), 0);
  assert.equal(activeFilterCount({ minFrp: 5, dayNight: "all" }), 1);
  assert.equal(activeFilterCount({ minFrp: 0, dayNight: "night" }), 1);
  assert.equal(activeFilterCount({ minFrp: 5, dayNight: "night" }), 2);
});

check("setPointFilters patches partially and notifies", () => {
  let calls = 0;
  const off = subscribePointFilters(() => calls++);
  setPointFilters({ minFrp: 25 });
  assert.deepEqual(getPointFilters(), { minFrp: 25, dayNight: "all" });
  setPointFilters({ dayNight: "day" });
  assert.deepEqual(getPointFilters(), { minFrp: 25, dayNight: "day" });
  assert.ok(calls >= 2);
  off();
  const before = calls;
  setPointFilters({ minFrp: 50 });
  assert.equal(calls, before);
});

check("resetPointFilters restores defaults and notifies", () => {
  setPointFilters({ minFrp: 50, dayNight: "night" });
  let calls = 0;
  const off = subscribePointFilters(() => calls++);
  resetPointFilters();
  assert.deepEqual(getPointFilters(), DEFAULT_POINT_FILTERS);
  assert.ok(calls >= 1);
  off();
});
