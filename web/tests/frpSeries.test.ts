import assert from "node:assert/strict";
import { clampRange, clampTooltipLeft, densify, isNotComparable, fromLiveCalendar, fullRange, pathSegments, seriesValues, type FrpSeriesPoint } from "../src/lib/frpSeries.ts";
import { fromMock, mockFrpMonthly } from "../src/mocks/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const near = (actual: number | null, expected: number, tolerance = 1e-9): void => {
  assert.ok(actual !== null && Math.abs(actual - expected) <= tolerance, `${actual} != ${expected}`);
};
const point = (over: Partial<FrpSeriesPoint> = {}): FrpSeriesPoint => ({
  year: 2020, month: 7,
  modis: { count: 10, frpSum: 500 },
  viirs: { count: 40, frpSum: 800 },
  harmonized: { count: 30, countNormalized: 25, totalFrp: 2500, totalFrpLower: 2000, totalFrpUpper: 3000 },
  ...over,
});

check("seriesValues perDetection: means, harmonized per normalized detection, band", () => {
  const v = seriesValues(point(), "perDetection");
  near(v.modis, 50);
  near(v.viirs, 20);
  near(v.harmonized, 100);
  near(v.harmonizedLower, 80);
  near(v.harmonizedUpper, 120);
});

check("seriesValues perDetection: zero counts, not observable and null countNormalized give null", () => {
  const v = seriesValues(point({ modis: { count: 0, frpSum: 0 }, viirs: null }), "perDetection");
  assert.equal(v.modis, null);
  assert.equal(v.viirs, null);
  const h = (countNormalized: number | null) =>
    seriesValues(point({ harmonized: { count: 3, countNormalized, totalFrp: 10, totalFrpLower: 5, totalFrpUpper: 15 } }), "perDetection");
  for (const n of [null, 0]) {
    const x = h(n);
    assert.deepEqual([x.harmonized, x.harmonizedLower, x.harmonizedUpper], [null, null, null]);
  }
  const noBounds = seriesValues(point({ harmonized: { count: 3, countNormalized: 2, totalFrp: 10, totalFrpLower: null, totalFrpUpper: null } }), "perDetection");
  near(noBounds.harmonized, 5);
  assert.deepEqual([noBounds.harmonizedLower, noBounds.harmonizedUpper], [null, null]);
  assert.equal(seriesValues(point({ harmonized: null }), "perDetection").harmonized, null);
});

check("seriesValues total: sums, a real zero stays zero, not observable is null", () => {
  const v = seriesValues(point(), "total");
  assert.deepEqual([v.modis, v.viirs, v.harmonized, v.harmonizedLower, v.harmonizedUpper], [500, 800, 2500, 2000, 3000]);
  const z = seriesValues(point({ modis: { count: 0, frpSum: 0 }, viirs: null }), "total");
  assert.equal(z.modis, 0);
  assert.equal(z.viirs, null);
});

check("pathSegments breaks at null and never draws null as zero", () => {
  const segments = pathSegments([{ x: 0, y: 1 }, { x: 1, y: 2 }, { x: 2, y: null }, { x: 3, y: 4 }, { x: 4, y: null }, { x: 5, y: null }, { x: 6, y: 0 }]);
  assert.deepEqual(segments, [[{ x: 0, y: 1 }, { x: 1, y: 2 }], [{ x: 3, y: 4 }], [{ x: 6, y: 0 }]]);
  assert.deepEqual(pathSegments([]), []);
  assert.deepEqual(pathSegments([{ x: 0, y: null }]), []);
});

check("zoom: clamp to data, minimum span 12 months, reset", () => {
  assert.deepEqual(clampRange({ start: -5, end: 400 }, 100), { start: 0, end: 99 });
  assert.deepEqual(clampRange({ start: 10, end: 12 }, 100), { start: 10, end: 21 });
  assert.deepEqual(clampRange({ start: 95, end: 99 }, 100), { start: 88, end: 99 });
  assert.deepEqual(clampRange({ start: 40, end: 20 }, 100), { start: 20, end: 40 });
  assert.deepEqual(clampRange({ start: 3, end: 4 }, 5), { start: 0, end: 4 });
  assert.deepEqual(clampRange({ start: 0, end: 0 }, 0), { start: 0, end: 0 });
  assert.deepEqual(fullRange(100), { start: 0, end: 99 });
});

const liveFixture = () => ({
  calendar: [
    { year: 2020, month: 7, count: 30, count_normalized: 25.5, total_frp: 2500, total_frp_lower: 2000, total_frp_upper: 3000 },
    { year: 2020, month: 8, count: 4, count_normalized: null, total_frp: 100, total_frp_lower: null, total_frp_upper: null },
  ],
  raw_monthly: [
    { year: 2020, month: 7, modis_covered: true, modis_count: 10, modis_frp_sum: 500, viirs_covered: true, viirs_count: 40, viirs_frp_sum: 800 },
    { year: 2020, month: 9, modis_covered: true, modis_count: 0, modis_frp_sum: 0, viirs_covered: true, viirs_count: 0, viirs_frp_sum: 0 },
    { year: 2000, month: 12, modis_covered: true, modis_count: 3, modis_frp_sum: 90, viirs_covered: false, viirs_count: 0, viirs_frp_sum: 0 },
    { year: 1999, month: 1, modis_covered: false, modis_count: 0, modis_frp_sum: 0, viirs_covered: false, viirs_count: 0, viirs_frp_sum: 0 },
  ],
});

check("fromLiveCalendar: valid fixture, sorted, cell merged with raw", () => {
  const points = fromLiveCalendar(liveFixture());
  assert.deepEqual(points.map(p => [p.year, p.month]), [[1999, 1], [2000, 12], [2020, 7], [2020, 8], [2020, 9]]);
  const july = points[2];
  assert.deepEqual(july.modis, { count: 10, frpSum: 500 });
  assert.deepEqual(july.viirs, { count: 40, frpSum: 800 });
  assert.deepEqual(july.harmonized, { count: 30, countNormalized: 25.5, totalFrp: 2500, totalFrpLower: 2000, totalFrpUpper: 3000 });
  assert.equal(points[3].modis, null);
  assert.equal(points[3].harmonized?.countNormalized, null);
});

check("fromLiveCalendar: missing raw_monthly is a clear error", () => {
  const { raw_monthly: _unused, ...old } = liveFixture();
  assert.throws(() => fromLiveCalendar(old), /raw_monthly.*too old/);
  assert.throws(() => fromLiveCalendar(null), /Invalid/);
  assert.throws(() => fromLiveCalendar({ calendar: [{ year: "2020" }], raw_monthly: [] }), /calendar\[0\]\.year/);
});

check("fromLiveCalendar: zero-detection covered month and uncovered sensor", () => {
  const points = fromLiveCalendar(liveFixture());
  const sept = points[4];
  assert.deepEqual(sept.harmonized, { count: 0, countNormalized: null, totalFrp: 0, totalFrpLower: null, totalFrpUpper: null });
  assert.deepEqual(sept.modis, { count: 0, frpSum: 0 });
  const dec2000 = points[1];
  assert.equal(dec2000.viirs, null);
  assert.deepEqual(dec2000.harmonized?.count, 0);
  assert.equal(points[0].harmonized, null);
  assert.equal(seriesValues(sept, "perDetection").harmonized, null);
  assert.equal(seriesValues(sept, "total").harmonized, 0);
});

check("densify fills missing months with all-null points", () => {
  const dense = densify([point({ year: 2020, month: 11 }), point({ year: 2021, month: 2 })]);
  assert.deepEqual(dense.map(p => [p.year, p.month]), [[2020, 11], [2020, 12], [2021, 1], [2021, 2]]);
  assert.deepEqual([dense[1].modis, dense[1].viirs, dense[1].harmonized], [null, null, null]);
});

const mock = fromMock(mockFrpMonthly);

check("fromMock: VIIRS null before 2012, MODIS present, harmonized without bounds", () => {
  assert.equal(mock.length, mockFrpMonthly.length);
  for (const p of mock) {
    assert.equal(p.viirs === null, p.year < 2012);
    assert.notEqual(p.modis, null);
    assert.equal(p.harmonized?.totalFrpLower, null);
  }
});

function yearPerDetection(year: number, kind: "raw" | "harmonized"): number {
  const rows = mock.filter(p => p.year === year);
  const sum = (f: (p: FrpSeriesPoint) => number) => rows.reduce((a, p) => a + f(p), 0);
  if (kind === "raw") {
    return sum(p => (p.modis?.frpSum ?? 0) + (p.viirs?.frpSum ?? 0)) / sum(p => (p.modis?.count ?? 0) + (p.viirs?.count ?? 0));
  }
  return sum(p => p.harmonized?.totalFrp ?? 0) / sum(p => p.harmonized?.countNormalized ?? 0);
}

check("mock: raw per-detection cliff at 2012, harmonized per-detection continuous", () => {
  assert.ok(yearPerDetection(2012, "raw") < 0.75 * yearPerDetection(2011, "raw"));
  const change = Math.abs(yearPerDetection(2012, "harmonized") - yearPerDetection(2011, "harmonized")) / yearPerDetection(2011, "harmonized");
  assert.ok(change < 0.2, `harmonized change ${change}`);
  const v = seriesValues(mock.find(p => p.year === 2012 && p.month === 7) as FrpSeriesPoint, "perDetection");
  assert.ok(v.harmonized !== null && v.viirs !== null && v.viirs < v.harmonized);
});

check("isNotComparable: only perDetection, records present, countNormalized missing", () => {
  const h = (count: number, countNormalized: number | null) => point({ harmonized: { count, countNormalized, totalFrp: 10, totalFrpLower: null, totalFrpUpper: null } });
  assert.equal(isNotComparable(h(5, null), "perDetection"), true);
  assert.equal(isNotComparable(h(5, null), "total"), false);
  assert.equal(isNotComparable(h(0, null), "perDetection"), false);
  assert.equal(isNotComparable(h(5, 4), "perDetection"), false);
  assert.equal(isNotComparable(point({ harmonized: null }), "perDetection"), false);
});

check("clampTooltipLeft keeps the tooltip inside the container", () => {
  assert.equal(clampTooltipLeft(300, 100, 600), 250);
  assert.equal(clampTooltipLeft(10, 100, 600), 0);
  assert.equal(clampTooltipLeft(595, 100, 600), 500);
  assert.equal(clampTooltipLeft(50, 400, 300), 0);
});
