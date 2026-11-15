import assert from "node:assert/strict";
import {
  buildPairScatter,
  buildSourceDonut,
  extractMatchedPairs,
  medianRatio,
  monthlyBarsForYear,
} from "../src/lib/sensorCharts.ts";
import { HotspotSource, type HarmonizedPoint, type MonthCell, type PointStages, type RawPoint } from "../src/types/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

function cell(year: number, month: number, rawModis: number, rawViirs: number, hasData = true): MonthCell {
  return {
    year, month, hasData, rawModis, rawViirs, rawCount: rawModis + rawViirs, harmonizedCount: 1,
    anomalyRaw: null, anomalyHarmonized: null, meanFrp: null, meanFrpLower: null, meanFrpUpper: null,
  };
}

function raw(id: string, sensor: "MODIS" | "VIIRS", frp: number, harmonizedId: string | null): RawPoint {
  return { id, lat: 0, lon: 0, sensor, frp, confidence: "nominal", acquired: "2021-08-01T00:00:00Z", daynight: "day", harmonizedId } as unknown as RawPoint;
}

function rec(id: string, source: HotspotSource, frp: number): HarmonizedPoint {
  return {
    id, lat: 0, lon: 0, frp, frpCorrected: frp, source, confidence: "nominal", acquired: "2021-08-01T00:00:00Z",
    daynight: "day", vzaFactor: null, nObs: 1, platforms: [], frpLower: null, frpUpper: null,
  } as unknown as HarmonizedPoint;
}

const stages = { rawModis: 70, rawViirs: 30 } as PointStages;

check("monthly bars: 12 entries, other years ignored, missing months empty", () => {
  const bars = monthlyBarsForYear([cell(2021, 1, 5, 7), cell(2021, 3, 0, 0, false), cell(2020, 2, 99, 99)], 2021);
  assert.equal(bars.length, 12);
  assert.deepEqual(bars[0], { month: 1, modis: 5, viirs: 7, hasData: true });
  assert.equal(bars[1].hasData, false);
  assert.equal(bars[2].hasData, false);
  assert.equal(bars[1].modis, 0);
});

check("pairs: mean of MODIS members vs summed VIIRS record FRP", () => {
  const pairs = extractMatchedPairs(
    [rec("h1", HotspotSource.Matched, 30)],
    [raw("a", "MODIS", 10, "h1"), raw("b", "MODIS", 20, "h1"), raw("c", "VIIRS", 12, "h1"), raw("d", "VIIRS", 18, "h1")],
  );
  assert.equal(pairs.length, 1);
  assert.equal(pairs[0].modisFrp, 15);
  assert.equal(pairs[0].viirsFrp, 30);
  assert.equal(pairs[0].ratio, 2);
});

check("pairs: skips unlinked, non-finite, non-positive and non-matched records", () => {
  const pairs = extractMatchedPairs(
    [
      rec("h1", HotspotSource.Matched, 5),
      rec("h2", HotspotSource.Matched, Number.NaN),
      rec("h3", HotspotSource.ViirsOnly, 5),
      rec("h4", HotspotSource.Matched, 5),
      rec("h5", HotspotSource.Matched, 0),
    ],
    [
      raw("a", "MODIS", Number.NaN, "h1"),
      raw("b", "MODIS", 4, "h2"),
      raw("c", "MODIS", 4, "h3"),
      raw("d", "VIIRS", 4, "h4"),
      raw("e", "MODIS", 4, null),
      raw("f", "MODIS", 4, "h5"),
    ],
  );
  assert.equal(pairs.length, 0);
});

check("pairs: a NaN MODIS member is dropped, the finite one still pairs", () => {
  const pairs = extractMatchedPairs(
    [rec("h1", HotspotSource.Matched, 8)],
    [raw("a", "MODIS", Number.NaN, "h1"), raw("b", "MODIS", 4, "h1")],
  );
  assert.equal(pairs[0].modisFrp, 4);
});

check("median ratio: odd, even, empty", () => {
  const p = (ratio: number) => ({ id: "x", modisFrp: 1, viirsFrp: ratio, ratio });
  assert.equal(medianRatio([]), null);
  assert.equal(medianRatio([p(3), p(1), p(2)]), 2);
  assert.equal(medianRatio([p(4), p(1), p(2), p(3)]), 2.5);
});

check("scatter: null in Raw mode, populated when revealed", () => {
  const h = [rec("h1", HotspotSource.Matched, 6)];
  const r = [raw("a", "MODIS", 3, "h1")];
  assert.equal(buildPairScatter(false, h, r), null);
  const s = buildPairScatter(true, h, r);
  assert.equal(s?.pairs.length, 1);
  assert.equal(s?.medianRatio, 2);
});

check("donut: harmonized shares sum to 1", () => {
  const d = buildSourceDonut(
    true,
    [rec("1", HotspotSource.ModisOnly, 1), rec("2", HotspotSource.ViirsOnly, 1), rec("3", HotspotSource.ViirsOnly, 1), rec("4", HotspotSource.Matched, 1)],
    stages,
  );
  assert.equal(d.kind, "harmonized");
  assert.equal(d.total, 4);
  assert.ok(Math.abs(d.segments.reduce((s, x) => s + x.share, 0) - 1) < 1e-12);
});

check("donut: empty harmonized has zero shares", () => {
  const d = buildSourceDonut(true, [], stages);
  assert.equal(d.total, 0);
  assert.ok(d.segments.every((s) => s.share === 0));
});

check("donut: Raw mode uses stage raw counts and ignores harmonized records", () => {
  const d = buildSourceDonut(false, [rec("1", HotspotSource.Matched, 1)], stages);
  assert.equal(d.kind, "raw");
  assert.deepEqual(d.segments.map((s) => [s.key, s.count]), [["modis", 70], ["viirs", 30]]);
  assert.equal(d.total, 100);
  assert.ok(d.segments.every((s) => s.key !== "matched"));
});
