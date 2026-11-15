import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createTranslator } from "../src/content/copy.ts";
import {
  buildCellDiagnostic,
  isTerraOnlyMonth,
  QUALITY_FLAG_COPY,
  qualityFlagCopyKey,
  sourceSplit,
} from "../src/lib/cellDiagnostic.ts";
import { adaptConformalBand } from "../src/services/api.ts";
import {
  DayNight,
  HotspotSource,
  type CalendarCell,
  type HarmonizedPoint,
  type MonthCell,
  type PointStages,
} from "../src/types/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const CELL: MonthCell = {
  year: 2021,
  month: 8,
  hasData: true,
  rawModis: 4321,
  rawViirs: 8765,
  rawCount: 13086,
  harmonizedCount: 9876,
  anomalyRaw: 1.5,
  anomalyHarmonized: 2.3456,
  meanFrp: 31.7,
  meanFrpLower: 20,
  meanFrpUpper: 40,
};

const CALENDAR: CalendarCell = {
  year: 2021,
  month: 8,
  count: 9876,
  countNormalized: 9123.4,
  effortOmega: 1.0827,
  meanFrp: 31.7,
  meanFrpRaw: 30,
  meanFrpLower: 20,
  meanFrpUpper: 40,
  totalFrp: 123456.7,
  totalFrpLower: 100000.1,
  totalFrpUpper: 150000.9,
  frpPerDetectionEq: 13.5,
  anomaly: 2.3456,
  qualityFlags: ["modis_orbital_drift"],
  conformal: {
    p68Lower: 111111.1,
    p68Upper: 133333.3,
    p90Lower: 99999.9,
    p90Upper: 144444.4,
    era: "viirs",
    bin: 3,
    nCal: 77,
    extrapolated: false,
  },
};

const STAGES: PointStages = {
  rawModis: 4321,
  rawViirs: 8765,
  staticMasked: 0,
  samaRemoved: 0,
  interscanRemoved: 11,
  fleetMerged: 22,
  matchedPairs: 3210,
  viirsAggregated: 5,
  harmonized: 9876,
};

check("Raw mode keeps only raw sensor counts and flags: no harmonized-derived value leaks", () => {
  const diag = buildCellDiagnostic({ cell: CELL, calendar: CALENDAR, stages: STAGES, sources: null }, false);
  assert.equal(diag.harmonized, null);
  assert.deepEqual(diag.raw, { modis: 4321, viirs: 8765, total: 13086 });
  assert.deepEqual(diag.flags, ["modis_orbital_drift"]);
  const json = JSON.stringify(diag);
  for (const secret of [
    "9876",
    "9123",
    "1.0827",
    "123456",
    "100000",
    "150000",
    "2.3456",
    "13.5",
    "111111",
    "3210",
    "conformal",
  ]) {
    assert.ok(!json.includes(secret), `Raw diagnostic must not contain ${secret}`);
  }
});

check("Harmonized mode shapes every section and shows missing values as null", () => {
  const diag = buildCellDiagnostic({ cell: CELL, calendar: CALENDAR, stages: STAGES, sources: null }, true);
  const h = diag.harmonized;
  assert.ok(h);
  assert.equal(h.count, 9876);
  assert.equal(h.matched, 3210);
  assert.equal(h.modisOnly, null);
  assert.equal(h.omega, 1.0827);
  assert.equal(h.countNormalized, 9123.4);
  assert.deepEqual(h.totalFrp, { value: 123456.7, lower: 100000.1, upper: 150000.9 });
  assert.equal(h.conformal?.p68Upper, 133333.3);
  assert.equal(h.anomaly, 2.3456);
  assert.equal(h.terraOnly, false);

  const demo = buildCellDiagnostic({ cell: CELL }, true).harmonized;
  assert.ok(demo);
  assert.equal(demo.omega, null);
  assert.equal(demo.totalFrp, null);
  assert.equal(demo.conformal, null);
  assert.equal(demo.matched, null);
  assert.equal(demo.anomaly, 2.3456);
});

check("completeness k_c applies only to months without VIIRS", () => {
  const pre = { ...CELL, year: 2008, rawViirs: 0 };
  assert.equal(buildCellDiagnostic({ cell: pre }, true).harmonized?.completenessApplied, true);
  assert.equal(buildCellDiagnostic({ cell: CELL }, true).harmonized?.completenessApplied, false);
});

check("Terra-only months before 2002-07 are flagged and carry the recovered estimate", () => {
  assert.equal(isTerraOnlyMonth(2002, 6), true);
  assert.equal(isTerraOnlyMonth(2000, 11), true);
  assert.equal(isTerraOnlyMonth(2002, 7), false);
  assert.equal(isTerraOnlyMonth(2021, 8), false);
  const cell = { ...CELL, year: 2001, month: 5, rawViirs: 0 };
  const calendar: CalendarCell = {
    ...CALENDAR,
    year: 2001,
    month: 5,
    countNormalized: null,
    effortOmega: null,
    anomaly: null,
    conformal: undefined,
    countRecovered: 400,
    countRecoveredLower: 350,
    countRecoveredUpper: 470,
  };
  const h = buildCellDiagnostic({ cell, calendar }, true).harmonized;
  assert.ok(h);
  assert.equal(h.terraOnly, true);
  assert.equal(h.omega, null);
  assert.deepEqual(h.recovered, { value: 400, lower: 350, upper: 470 });
  for (const lang of ["en", "tr"] as const) {
    const t = createTranslator(lang);
    assert.ok(t("cellPreAquaTitle").length > 0);
    assert.ok(t("cellPreAquaBody").includes("2002-07"));
    assert.ok(t("cellRecoveredNote").length > 0);
  }
});

check("every flag the backend can send has an EN and TR label", () => {
  const source = readFileSync("../crates/core/src/quality.rs", "utf8");
  const enumBody = /pub enum QualityFlag \{([\s\S]*?)\n\}/.exec(source)?.[1] ?? "";
  const variants = [...enumBody.matchAll(/^\s{4}([A-Z][A-Za-z0-9]*),\s*$/gm)].map((m) => m[1]);
  assert.ok(variants.length >= 5, "QualityFlag variants were not found");
  const snake = variants.map((v) => v.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase());
  assert.deepEqual([...snake].sort(), Object.keys(QUALITY_FLAG_COPY).sort());
  for (const lang of ["en", "tr"] as const) {
    const t = createTranslator(lang);
    for (const flag of snake) {
      const key = qualityFlagCopyKey(flag);
      assert.ok(key, `no copy for ${flag}`);
      assert.ok(t(key).length > 0);
    }
    assert.ok(t("cellFlagUnknown", { flag: "x" }).includes("x"));
  }
  assert.equal(qualityFlagCopyKey("not_a_flag"), null);
  assert.equal(qualityFlagCopyKey("toString"), null);
});

function point(source: HotspotSource): HarmonizedPoint {
  return {
    id: "h",
    lat: 0,
    lon: 0,
    frp: 1,
    frpCorrected: 1,
    source,
    confidence: "Nominal",
    acquired: "2021-08-01T10:00:00Z",
    daynight: DayNight.Day,
    vzaFactor: null,
    nObs: 1,
    platforms: [],
    frpLower: null,
    frpUpper: null,
  } as unknown as HarmonizedPoint;
}

check("the MODIS-only / VIIRS-only split needs the whole point set", () => {
  const points = [
    point(HotspotSource.Matched),
    point(HotspotSource.ModisOnly),
    point(HotspotSource.ModisOnly),
    point(HotspotSource.ViirsOnly),
  ];
  const stages = { ...STAGES, harmonized: 4 };
  assert.deepEqual(sourceSplit(points, stages, false), { matched: 1, modisOnly: 2, viirsOnly: 1 });
  assert.equal(sourceSplit(points, stages, true), null);
  assert.equal(sourceSplit(points, { ...stages, harmonized: 40 }, false), null);
});

check("a conformal band is adapted from the wire shape; a malformed or absent one is dropped", () => {
  const wire = {
    p68_lower: 1,
    p68_upper: 2,
    p90_lower: 0.5,
    p90_upper: 3,
    era: "pre_viirs",
    bin: 2,
    n_cal: 40,
    extrapolated: true,
  };
  assert.deepEqual(adaptConformalBand(wire), {
    p68Lower: 1,
    p68Upper: 2,
    p90Lower: 0.5,
    p90Upper: 3,
    era: "pre_viirs",
    bin: 2,
    nCal: 40,
    extrapolated: true,
  });
  assert.equal(adaptConformalBand(undefined), null);
  assert.equal(adaptConformalBand({ lower: 1, upper: 2 }), null);
  assert.equal(adaptConformalBand({ ...wire, era: "other" }), null);
});
