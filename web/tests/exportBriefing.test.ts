import assert from "node:assert/strict";
import {
  historicalDeviation,
  lastSevenDays,
  mostActiveSensor,
  platformLabel,
  sensorCounts,
  yearlyTrend,
} from "../src/lib/briefingMetrics.ts";
import { parseCaseStudy } from "../src/lib/caseStudy.ts";
import { parseContentDispositionFileName } from "../src/lib/export.ts";
import { exportFileName, formatDayRange } from "../src/lib/format.ts";
import { mapPointsToGeoJson } from "../src/lib/geojson.ts";
import type { MapPointSet, MonthCell, YearlyPoint } from "../src/types/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const raw = (id: string, acquired: string) => ({
  id, lat: 37.5, lon: 30.25, sensor: "VIIRS", frp: 5, confidence: "nominal", acquired, daynight: "D", harmonizedId: null,
});
const harm = (id: string, acquired: string) => ({
  id, lat: 36.5, lon: 29.5, frp: 4, frpCorrected: 4.5, source: "viirs", confidence: "nominal", acquired,
  daynight: "N", vzaFactor: null, nObs: 1, platforms: [], frpLower: null, frpUpper: null,
});
const pointSet = (extra: Record<string, unknown> = {}) =>
  ({
    period: { year: 2021, month: 8 },
    raw: [raw("r1", "2021-08-24T23:59:00Z"), raw("r2", "2021-08-25T00:00:00Z"), raw("r3", "2021-08-31T23:59:59Z"), raw("r4", "2021-09-01T00:00:00Z")],
    harmonized: [harm("h1", "2021-08-28T10:00:00Z")],
    truncated: false,
    ...extra,
  }) as unknown as MapPointSet;

check("lastSevenDays window covers the last 7 UTC days of the month", () => {
  const raws = lastSevenDays(pointSet(), false);
  assert.deepEqual(raws, { start: "2021-08-25", end: "2021-08-31", count: 2 });
  assert.equal(lastSevenDays(pointSet(), true)?.count, 1);
});

check("lastSevenDays uses 31 Dec for whole-year periods and skips truncated or unknown periods", () => {
  const yearly = lastSevenDays(pointSet({ period: { year: 2021, month: null } }), true);
  assert.equal(yearly?.end, "2021-12-31");
  assert.equal(lastSevenDays(pointSet({ truncated: true }), true), null);
  assert.equal(lastSevenDays(pointSet({ period: { year: null, month: null } }), true), null);
});

const cell = (year: number, month: number, harmonizedCount: number, hasData = true) =>
  ({ year, month, hasData, harmonizedCount }) as MonthCell;

check("historicalDeviation compares a month with the same month in other years", () => {
  const months = [cell(2018, 8, 90), cell(2019, 8, 110), cell(2020, 8, 100), cell(2021, 8, 150), cell(2021, 7, 5)];
  const d = historicalDeviation({ year: 2021, month: 8 }, months, []);
  assert.equal(d?.mean, 100);
  assert.ok(Math.abs((d?.ratio ?? 0) - 0.5) < 1e-9);
});

check("historicalDeviation returns null without enough baseline or a zero mean", () => {
  assert.equal(historicalDeviation({ year: 2021, month: 8 }, [cell(2020, 8, 100), cell(2021, 8, 150)], []), null);
  const zeros = [cell(2018, 8, 0), cell(2019, 8, 0), cell(2020, 8, 0), cell(2021, 8, 5)];
  assert.equal(historicalDeviation({ year: 2021, month: 8 }, zeros, []), null);
  assert.equal(historicalDeviation({ year: 2021, month: 8 }, [cell(2018, 8, 1, false)], []), null);
});

const yr = (year: number, harmonized: number, partial = false) => ({ year, harmonized, partial }) as YearlyPoint;

check("historicalDeviation for a whole year skips partial years", () => {
  const yearly = [yr(2017, 100), yr(2018, 100), yr(2019, 100), yr(2020, 9999, true), yr(2021, 200)];
  const d = historicalDeviation({ year: 2021, month: null }, [], yearly);
  assert.equal(d?.mean, 100);
  assert.equal(d?.ratio, 1);
});

check("mostActiveSensor picks the larger raw count", () => {
  assert.deepEqual(mostActiveSensor({ modis: 25, viirs: 75 }), { sensor: "VIIRS", share: 0.75 });
  assert.equal(mostActiveSensor({ modis: 3, viirs: 1 })?.sensor, "MODIS");
  assert.equal(mostActiveSensor({ modis: 0, viirs: 0 }), null);
  assert.equal(mostActiveSensor({ modis: 4, viirs: 4 }), null);
});

check("platformLabel maps enum names to human labels", () => {
  assert.equal(platformLabel("ModisTerra"), "Terra");
  assert.equal(platformLabel("ModisAqua"), "Aqua");
  assert.equal(platformLabel("ViirsSNPP"), "S-NPP");
  assert.equal(platformLabel("ViirsNOAA20"), "NOAA-20");
  assert.equal(platformLabel("ViirsNOAA21"), "NOAA-21");
  assert.equal(platformLabel("Other"), "Other");
});

check("sensorCounts reports its period, or null for the whole record", () => {
  const stages = { rawModis: 3, rawViirs: 4 };
  const period = { year: 2021, month: 8 };
  assert.deepEqual(sensorCounts({ hasData: true, stages, period } as never, { modis: 10, viirs: 20 }), { modis: 3, viirs: 4, period });
  assert.deepEqual(sensorCounts({ hasData: false, stages, period } as never, { modis: 10, viirs: 20 }), { modis: 10, viirs: 20, period: null });
});

check("yearlyTrend: rising, falling, flat threshold and missing data", () => {
  const rising = yearlyTrend([yr(2017, 100), yr(2018, 110), yr(2019, 120), yr(2020, 130), yr(2021, 140)]);
  assert.equal(rising?.direction, "rising");
  assert.ok((rising?.perYear ?? 0) > 0.07 && (rising?.perYear ?? 0) < 0.09);
  assert.equal(rising?.normalized, false);
  assert.equal(yearlyTrend([yr(2017, 140), yr(2018, 130), yr(2019, 120), yr(2020, 110), yr(2021, 100)])?.direction, "falling");
  const flat = yearlyTrend([yr(2017, 100), yr(2018, 101), yr(2019, 100), yr(2020, 101), yr(2021, 100)]);
  assert.equal(flat?.direction, "flat");
  assert.equal(yearlyTrend([yr(2018, 1), yr(2019, 2), yr(2020, 3)]), null);
  assert.equal(yearlyTrend([yr(2017, 0), yr(2018, 0), yr(2019, 0), yr(2020, 0)]), null);
});

check("yearlyTrend ignores partial years and uses the last five complete ones", () => {
  const t = yearlyTrend([yr(2015, 1), yr(2016, 1), yr(2017, 100), yr(2018, 100), yr(2019, 100), yr(2020, 100), yr(2021, 100), yr(2022, 5, true)]);
  assert.equal(t?.fromYear, 2017);
  assert.equal(t?.toYear, 2021);
  assert.equal(t?.direction, "flat");
});

check("mapPointsToGeoJson writes [lon, lat] features and the demo flag", () => {
  const live = mapPointsToGeoJson(pointSet(), true);
  assert.equal(live.type, "FeatureCollection");
  assert.equal(live.features.length, 1);
  assert.deepEqual(live.features[0].geometry, { type: "Point", coordinates: [29.5, 36.5] });
  assert.equal(live.features[0].properties.frp_corrected_mw, 4.5);
  assert.equal(live.synthetic, undefined);
  const demoRaw = mapPointsToGeoJson(pointSet(), false, true);
  assert.equal(demoRaw.synthetic, true);
  assert.equal(demoRaw.view, "raw");
  assert.deepEqual(demoRaw.features[0].geometry.coordinates, [30.25, 37.5]);
  assert.equal(demoRaw.features[0].properties.sensor, "VIIRS");
  assert.equal("harmonizedId" in demoRaw.features[0].properties, false);
});

check("exportFileName covers extensions, demo tag and _raw", () => {
  const month = { year: 2021, month: 8 };
  assert.equal(exportFileName("turkey", month, "csv"), "pyrosync_turkey_2021-08.csv");
  assert.equal(exportFileName("turkey", month, "geojson"), "pyrosync_turkey_2021-08.geojson");
  assert.equal(exportFileName("turkey", month, "geojson", false, true), "pyrosync_turkey_2021-08_raw.geojson");
  assert.equal(exportFileName("turkey", { year: 2021, month: null }, "csv", true), "pyrosync_turkey_demo_2021.csv");
});

check("parseContentDispositionFileName", () => {
  assert.equal(parseContentDispositionFileName('attachment; filename="pyrosync_turkey_2021-08.csv"'), "pyrosync_turkey_2021-08.csv");
  assert.equal(parseContentDispositionFileName("attachment; filename=a_raw.geojson"), "a_raw.geojson");
  assert.equal(parseContentDispositionFileName("attachment; filename*=UTF-8''m%C4%B1r.csv"), "mır.csv");
  assert.equal(parseContentDispositionFileName('attachment; filename="../x.csv"'), ".._x.csv");
  assert.equal(parseContentDispositionFileName("attachment"), null);
  assert.equal(parseContentDispositionFileName(null), null);
});

check("formatDayRange", () => {
  assert.equal(formatDayRange("en", "2021-08-25", "2021-08-31"), "25–31 Aug 2021");
  assert.equal(formatDayRange("en", "2021-08-28", "2021-09-03"), "28 Aug – 3 Sep 2021");
  assert.equal(formatDayRange("en", "2021-12-28", "2022-01-03"), "28 Dec 2021 – 3 Jan 2022");
});

check("parseCaseStudy reads the figures and rejects incomplete files", () => {
  const month = (count: number, z: number) => ({
    anomaly_2021: { august: { count, anomaly_z: z, rank_all_months: 2, of_all_months: 152 }, july: { anomaly_z: z } },
  });
  const json = {
    meta: { years: [2012, 2025] },
    regions: { antalya: month(1759, 3.6), mugla: month(3129, 3.4), mediterranean: month(1, 3.5) },
  };
  const parsed = parseCaseStudy(json);
  assert.equal(parsed?.antalyaAugust.count, 1759);
  assert.equal(parsed?.muglaAugust.ofAllMonths, 152);
  assert.equal(parsed?.mediterraneanJulyZ, 3.5);
  assert.deepEqual(parsed?.years, [2012, 2025]);
  assert.equal(parseCaseStudy({ regions: {} }), null);
  assert.equal(parseCaseStudy(null), null);
});
