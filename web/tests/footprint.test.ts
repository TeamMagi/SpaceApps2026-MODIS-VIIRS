import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  MODIS_EDGE_SCAN_ANGLE_DEG,
  THRESHOLD_SCALE_MAX_MW,
  computeModisFootprint,
  computeViirsTiling,
  detectionThresholds,
  findModisMember,
  findViirsInFootprint,
  modisFootprintAtScanAngle,
  scanAngleFromScanKm,
  scanAngleToVzaDeg,
  thresholdFraction,
  vzaDegFromFactor,
  vzaToScanAngleDeg,
} from "../src/lib/footprint";
import { Confidence, DayNight, type RawPoint } from "../src/types";

function near(actual: number, expected: number, tol: number, label: string): void {
  assert.ok(Math.abs(actual - expected) <= tol, `${label}: expected ${expected} ± ${tol}, got ${actual}`);
}

function viirs(id: string, lat: number, lon: number, acquired: string, harmonizedId: string | null): RawPoint {
  return {
    id,
    lat,
    lon,
    sensor: "VIIRS",
    frp: 5,
    confidence: Confidence.Nominal,
    acquired,
    daynight: DayNight.Day,
    harmonizedId,
  };
}

describe("MODIS footprint geometry", () => {
  it("is 1 km x 1 km at nadir", () => {
    const nadir = modisFootprintAtScanAngle(0);
    near(nadir.scanKm, 1, 1e-9, "scan");
    near(nadir.trackKm, 1, 1e-9, "track");
    near(nadir.areaKm2, 1, 1e-9, "area");
    assert.equal(nadir.vzaDeg, 0);
    assert.equal(nadir.source, "model");
  });

  it("grows to ~4.8 x 2.0 km (9.66-9.70 km2) at the 55 deg scan edge, VZA ~65.5 deg", () => {
    const edge = modisFootprintAtScanAngle(MODIS_EDGE_SCAN_ANGLE_DEG);
    near(edge.vzaDeg, 65.47, 0.05, "vza");
    near(edge.scanKm, 4.83, 0.03, "scan");
    near(edge.trackKm, 2.0, 0.02, "track");
    assert.ok(edge.areaKm2 >= 9.66 && edge.areaKm2 <= 9.71, `edge area ${edge.areaKm2}`);
  });

  it("grows monotonically across the swath", () => {
    let previous = 0;
    for (let beta = 0; beta <= 55; beta += 5) {
      const area = modisFootprintAtScanAngle(beta).areaKm2;
      assert.ok(area > previous, `area at ${beta} deg`);
      previous = area;
    }
  });

  it("round-trips scan angle <-> VZA and scan km -> scan angle", () => {
    for (const beta of [0, 10, 35, 50, 55]) {
      near(vzaToScanAngleDeg(scanAngleToVzaDeg(beta)), beta, 1e-6, `beta ${beta}`);
      near(scanAngleFromScanKm(modisFootprintAtScanAngle(beta).scanKm), beta, 1e-4, `scan km at ${beta}`);
    }
    assert.equal(scanAngleFromScanKm(0.9), 0);
    assert.equal(scanAngleFromScanKm(9), MODIS_EDGE_SCAN_ANGLE_DEG);
  });

  it("uses FIRMS scan x track when present", () => {
    const measured = computeModisFootprint({ scanKm: 4.8, trackKm: 2.0, vzaFactor: 1.1 });
    assert.equal(measured.source, "measured");
    near(measured.areaKm2, 9.6, 1e-9, "area");
    assert.ok(measured.vzaDeg > 60 && measured.vzaDeg < 66);
  });

  it("derives the footprint from the harmonized VZA factor, else falls back to the swath model", () => {
    const theta = scanAngleToVzaDeg(55) * (Math.PI / 180);
    const factor = 0.2017 * theta * theta + 0.0168 * theta + 1;
    near(vzaDegFromFactor(factor), scanAngleToVzaDeg(55), 1e-6, "vza from factor");
    assert.equal(vzaDegFromFactor(1), 0);
    assert.equal(vzaDegFromFactor(0.9), 0);

    const fromVza = computeModisFootprint({ vzaFactor: factor });
    assert.equal(fromVza.source, "vza");
    near(fromVza.areaKm2, modisFootprintAtScanAngle(55).areaKm2, 1e-3, "area from vza");

    const fallback = computeModisFootprint({ fallbackScanAngleDeg: 0 });
    assert.equal(fallback.source, "model");
    near(fallback.areaKm2, 1, 1e-9, "fallback area");
    assert.equal(computeModisFootprint({ scanKm: 1, trackKm: null }).source, "model");
  });
});

describe("375 m VIIRS tiling", () => {
  it("fits ~7 VIIRS pixels in a nadir MODIS pixel and ~69 at the edge", () => {
    const nadir = computeViirsTiling(modisFootprintAtScanAngle(0));
    near(nadir.areaRatio, 1 / 0.140625, 1e-9, "nadir ratio");
    assert.equal(nadir.pixelCount, 7);
    assert.equal(nadir.cols, 3);
    assert.equal(nadir.rows, 3);

    const edge = computeViirsTiling(modisFootprintAtScanAngle(55));
    assert.ok(edge.areaRatio > 68 && edge.areaRatio < 70, `edge ratio ${edge.areaRatio}`);
    assert.equal(edge.cols, 13);
    assert.equal(edge.rows, 6);
    near(edge.areaRatioVsViirsEdge, edge.areaRatio * (0.140625 / 0.62), 1e-9, "vs viirs edge");
  });

  it("gives the flare-chronos order of magnitude (~74x) for a measured 5.2 x 2.0 km FIRMS pixel", () => {
    const tiling = computeViirsTiling(computeModisFootprint({ scanKm: 5.2, trackKm: 2.0 }));
    assert.equal(tiling.pixelCount, 74);
  });
});

describe("detection thresholds", () => {
  it("runs MODIS 5.7 -> 40 MW by day and keeps VIIRS at 0.7-3.7 MW", () => {
    const nadir = detectionThresholds(1, DayNight.Day);
    near(nadir.modisMw, 5.7, 1e-9, "day nadir");
    assert.equal(nadir.viirsMw, 3.7);
    assert.equal(nadir.viirsMinMw, 0.7);
    assert.equal(nadir.viirsMaxMw, 3.7);

    const edge = detectionThresholds(modisFootprintAtScanAngle(55).areaKm2, DayNight.Day);
    near(edge.modisMw, 40, 1e-9, "day edge");
    near(edge.ratio, 40 / 3.7, 1e-9, "ratio");

    const mid = detectionThresholds(5, DayNight.Day);
    assert.ok(mid.modisMw > 5.7 && mid.modisMw < 40);
  });

  it("uses the night rows (MODIS 3.7 -> 39 MW, VIIRS 0.7 MW) and clamps outside the swath", () => {
    const night = detectionThresholds(1, DayNight.Night);
    near(night.modisMw, 3.7, 1e-9, "night nadir");
    assert.equal(night.viirsMw, 0.7);
    near(detectionThresholds(50, DayNight.Night).modisMw, 39, 1e-9, "clamped edge");
    near(detectionThresholds(0.2, DayNight.Day).modisMw, 5.7, 1e-9, "clamped nadir");
  });

  it("maps thresholds onto the shared bar", () => {
    assert.equal(thresholdFraction(0), 0);
    assert.equal(thresholdFraction(THRESHOLD_SCALE_MAX_MW * 2), 1);
    near(thresholdFraction(21), 0.5, 1e-9, "half");
  });
});

describe("real VIIRS pixels inside a MODIS footprint", () => {
  const center = { lat: 37, lon: 28, acquired: "2023-07-15T11:00:00Z" };
  const footprint = { scanKm: 4.8, trackKm: 2.0 };
  const kmPerDegLon = 111.32 * Math.cos((37 * Math.PI) / 180);
  const points: RawPoint[] = [
    viirs("inside", 37 + 0.5 / 111.32, 28 + 1 / kmPerDegLon, "2023-07-15T11:15:00Z", "h42"),
    viirs("eastOfBox", 37, 28 + 4 / kmPerDegLon, "2023-07-15T11:15:00Z", "h42"),
    viirs("northOfBox", 37 + 1.5 / 111.32, 28, "2023-07-15T11:05:00Z", null),
    viirs("late", 37, 28, "2023-07-15T17:30:00Z", null),
    { ...viirs("modis", 37, 28, "2023-07-15T11:00:00Z", "h42"), sensor: "MODIS" },
  ];

  it("keeps only VIIRS inside the scan x track box and the 180 min window", () => {
    const found = findViirsInFootprint(center, footprint, points);
    assert.deepEqual(found.map((f) => f.point.id), ["inside"]);
    const first = found[0]!;
    assert.equal(first.inside, true);
    assert.equal(first.linked, false);
    near(first.dxKm, 1, 1e-9, "dx");
    near(first.dyKm, 0.5, 1e-9, "dy");
  });

  it("never links by harmonized id unless the caller passes one (Raw mode)", () => {
    const raw = findViirsInFootprint(center, footprint, points, { linkedHarmonizedId: null });
    assert.ok(raw.every((f) => !f.linked));
    assert.equal(raw.length, 1);

    const harmonized = findViirsInFootprint(center, footprint, points, { linkedHarmonizedId: "h42" });
    assert.deepEqual(harmonized.map((f) => f.point.id), ["inside", "eastOfBox"]);
    assert.equal(harmonized[1]!.inside, false);
    assert.equal(harmonized[1]!.linked, true);
  });

  it("finds the MODIS member of a harmonized point", () => {
    assert.equal(findModisMember("h42", points)?.id, "modis");
    assert.equal(findModisMember("h7", points), null);
  });
});
