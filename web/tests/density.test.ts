import assert from "node:assert/strict";
import {
  busiestMonth, cellAreaKm2, cellBounds, cellCol, cellRow, cellValue, densityScale, gridFromPoints, neighbourCell, parseDensity,
  type DensityYear,
} from "../src/lib/density.ts";
import { mockPoints } from "../src/mocks/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const wire = () => ({
  region: "Turkey", year: 2021, cell_deg: 0.25, grid: "global_latlon",
  months: Array.from({ length: 12 }, (_, i) => ({
    month: i + 1, raw_total: i === 6 ? 3 : 0, harmonized_total: i === 6 ? 2 : 0,
    cells: i === 6 ? [[505, 840, 2, 30.5, 1, 40], [506, 841, 1, 5, 1, 7]] : [],
  })),
  max: { raw_count: 2, raw_frp: 30.5, harm_count: 1, harm_frp: 40 },
  bbox: { west: 29, south: 36.5, east: 31, north: 37.1 },
  covered_months: [7],
});

check("parseDensity accepts a valid response", () => {
  const d = parseDensity(wire());
  assert.equal(d.months.length, 12);
  assert.deepEqual(d.months[6].cells[0], { row: 505, col: 840, rawCount: 2, rawFrp: 30.5, harmCount: 1, harmFrp: 40 });
  assert.deepEqual(d.coveredMonths, [7]);
  assert.equal(parseDensity({ ...wire(), bbox: null }).bbox, null);
});

check("parseDensity rejects malformed input with clear messages", () => {
  const bad = (mutate: (w: ReturnType<typeof wire>) => unknown, pattern: RegExp) =>
    assert.throws(() => parseDensity(mutate(wire())), pattern);
  assert.throws(() => parseDensity(null), /response must be an object/);
  assert.throws(() => parseDensity([]), /response must be an object/);
  bad(w => ({ ...w, region: 5 }), /region/);
  bad(w => ({ ...w, cell_deg: 0 }), /cell_deg/);
  bad(w => ({ ...w, year: 2021.5 }), /year/);
  bad(w => ({ ...w, months: "x" }), /months must be an array/);
  bad(w => { w.months[0].month = 13; return w; }, /month/);
  bad(w => { w.months[1].month = 1; return w; }, /twice/);
  bad(w => { w.months[6].cells[0] = [1, 2, 3]; return w; }, /6 elements/);
  bad(w => { w.months[6].cells[0][2] = -1; return w; }, /negative/);
  bad(w => { w.months[6].cells[0][0] = 1.5; return w; }, /row/);
  bad(w => ({ ...w, max: { raw_count: "a" } }), /max/);
  bad(w => ({ ...w, bbox: { west: 1 } }), /bbox/);
  bad(w => ({ ...w, covered_months: [0] }), /covered_months/);
});

check("cellBounds uses the south-west corner formula", () => {
  assert.deepEqual(cellBounds(0, 0, 0.25), { west: -180, south: -90, east: -179.75, north: -89.75 });
  assert.deepEqual(cellBounds(505, 840, 0.25), { west: 30, south: 36.25, east: 30.25, north: 36.5 });
});

check("cellAreaKm2: equator vs 60 degrees is about half, and a 1 degree equator cell is ~12364 km2", () => {
  const equator = cellAreaKm2(360, 0.25);
  const sixty = cellAreaKm2(Math.round((60 + 90) / 0.25), 0.25);
  assert.ok(Math.abs(sixty / equator - 0.5) < 0.01, `${sixty / equator}`);
  assert.ok(Math.abs(cellAreaKm2(90, 1) - 12364) < 15, `${cellAreaKm2(90, 1)}`);
});

check("gridFromPoints: totals equal the mock and indexing matches the formula", () => {
  const d = gridFromPoints(mockPoints, 0.25);
  assert.equal(d.months.reduce((n, m) => n + m.rawTotal, 0), mockPoints.raw.length);
  assert.equal(d.months.reduce((n, m) => n + m.harmonizedTotal, 0), mockPoints.harmonized.length);
  const cells = d.months.flatMap(m => m.cells);
  assert.equal(cells.reduce((n, c) => n + c.rawCount, 0), mockPoints.raw.length);
  assert.equal(cells.reduce((n, c) => n + c.harmCount, 0), mockPoints.harmonized.length);
  const frp = (xs: readonly number[]) => xs.reduce((a, b) => a + b, 0);
  assert.ok(Math.abs(frp(cells.map(c => c.rawFrp)) - frp(mockPoints.raw.map(p => p.frp))) < 1e-6);
  assert.ok(Math.abs(frp(cells.map(c => c.harmFrp)) - frp(mockPoints.harmonized.map(p => p.frpCorrected))) < 1e-6);
  const first = mockPoints.raw[0];
  const month = new Date(first.acquired).getUTCMonth();
  assert.ok(d.months[month].cells.some(c => c.row === Math.floor((first.lat + 90) / 0.25) && c.col === Math.floor((first.lon + 180) / 0.25) && c.rawCount > 0));
  assert.deepEqual(d.months.map(m => m.month), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
});

check("grid indexing on cell boundaries", () => {
  assert.equal(cellRow(-90, 0.25), 0);
  assert.equal(cellRow(0, 0.25), 360);
  assert.equal(cellRow(0.2499, 0.25), 360);
  assert.equal(cellRow(0.25, 0.25), 361);
  assert.equal(cellCol(-180, 0.25), 0);
  assert.equal(cellCol(30, 0.25), 840);
  assert.equal(cellCol(29.999, 0.25), 839);
  const d = gridFromPoints({
    region: "X", year: 2021, bbox: null,
    raw: [{ lat: 0, lon: 30, frp: 1, acquired: "2021-02-10T00:00:00Z" }, { lat: 0.2499, lon: 30.25, frp: 2, acquired: "2021-02-11T00:00:00Z" }],
    harmonized: [],
  }, 0.25);
  assert.deepEqual(d.months[1].cells.map(c => [c.row, c.col]), [[360, 840], [360, 841]]);
});

const rich = (): DensityYear => {
  const cells = (month: number) => Array.from({ length: 40 }, (_, i) => ({
    row: i, col: month, rawCount: 1 + ((i * 7 + month) % 50), rawFrp: 3 + i * month * 1.3, harmCount: 1 + ((i * 5) % 40), harmFrp: 2 + i * 2.1 + month,
  }));
  return {
    region: "X", year: 2021, cellDeg: 0.25, bbox: null, coveredMonths: [1, 2, 3], max: { rawCount: 0, rawFrp: 0, harmCount: 0, harmFrp: 0 },
    months: Array.from({ length: 12 }, (_, i) => ({ month: i + 1, rawTotal: i, harmonizedTotal: 0, cells: i < 3 ? cells(i + 1) : [] })),
  };
};

check("class breaks are one fixed set per metric, independent of month and mode", () => {
  const data = rich();
  const a = densityScale(data, "count");
  assert.equal(a.method, "quantile");
  assert.equal(a.breaks.length, 7);
  const reordered: DensityYear = { ...data, months: [...data.months].reverse() };
  assert.deepEqual(densityScale(reordered, "count").breaks, a.breaks);
  assert.deepEqual(densityScale(data, "count").breaks, a.breaks);
  const cell = data.months[0].cells[3];
  assert.equal(a.classOf(cellValue(cell, "count", "raw")), a.classOf(cellValue({ ...cell }, "count", "raw")));
  const f = densityScale(data, "frp");
  assert.notDeepEqual(f.breaks, a.breaks);
  assert.ok(f.breaks.every((b, i) => i === 0 || b > f.breaks[i - 1]));
});

check("classOf: empty cells are class 0, extremes hit class 1 and 8", () => {
  const s = densityScale(rich(), "frp");
  assert.equal(s.classOf(0), 0);
  assert.equal(s.classOf(-3), 0);
  assert.equal(s.classOf(s.min), 1);
  assert.equal(s.classOf(s.max), 8);
  assert.equal(s.classOf(s.breaks[3]), 5);
});

check("fixed log breaks are used for few cells or tied quantiles", () => {
  const few = densityScale(gridFromPoints(mockPoints, 0.25), "frp");
  const sparse: DensityYear = { ...rich(), months: rich().months.map(m => ({ ...m, cells: m.cells.slice(0, 1) })) };
  assert.equal(densityScale(sparse, "frp").method, "fixed");
  assert.deepEqual(densityScale(sparse, "frp").breaks, [5, 10, 20, 40, 80, 160, 320]);
  const tied: DensityYear = { ...rich(), months: rich().months.map(m => ({ ...m, cells: m.cells.map(c => ({ ...c, rawCount: 1, harmCount: 1 })) })) };
  assert.equal(densityScale(tied, "count").method, "fixed");
  assert.ok(few.breaks.length === 7);
});

check("zero cells are excluded from the scale", () => {
  const data = rich();
  const withZeros: DensityYear = { ...data, months: data.months.map(m => ({ ...m, cells: [...m.cells, { row: 99, col: 99, rawCount: 0, rawFrp: 0, harmCount: 0, harmFrp: 0 }] })) };
  assert.deepEqual(densityScale(withZeros, "count").breaks, densityScale(data, "count").breaks);
  assert.ok(densityScale(withZeros, "count").min > 0);
});

check("busiestMonth and neighbourCell", () => {
  const data = rich();
  assert.equal(busiestMonth(data), 12);
  const c = (row: number, col: number) => ({ row, col, rawCount: 1, rawFrp: 1, harmCount: 1, harmFrp: 1 });
  const cells = [c(0, 0), c(0, 3), c(5, 1), c(1, 1)];
  assert.deepEqual(neighbourCell(cells, { row: 0, col: 0 }, "right"), cells[1]);
  assert.deepEqual(neighbourCell(cells, { row: 0, col: 0 }, "up"), cells[3]);
  assert.equal(neighbourCell(cells, { row: 0, col: 0 }, "left"), null);
});
