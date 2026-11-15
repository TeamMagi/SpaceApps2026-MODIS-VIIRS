import assert from "node:assert/strict";

// api.ts reads `window` and `fetch` only when it is called, but the store module it imports looks at
// `window` while loading, so the browser globals are installed before the dynamic import below.
const globals = globalThis as unknown as Record<string, unknown>;
globals.window = {
  location: { origin: "http://localhost:5173", hash: "" },
  localStorage: { getItem: () => null, setItem: () => undefined },
};

let nextBody: unknown = null;
const requested: string[] = [];
globals.fetch = async (input: unknown): Promise<Response> => {
  requested.push(String(input));
  return new Response(JSON.stringify(nextBody), { status: 200, headers: { "Content-Type": "application/json" } });
};

const api = await import("../src/services/api.ts");
const { Sensor } = await import("../src/types/index.ts");

function check(name: string, fn: () => Promise<void> | void): Promise<void> {
  return Promise.resolve(fn()).then(() => console.log(`ok - ${name}`));
}

// Wire shapes below follow the serde structs in crates/server/src/main.rs and docs/API.md:
// `Sensor` is serialized by variant name, so platforms read "ModisTerra", not "MODIS_Terra".
const REPORT = { per_source: [["MODIS_NRT", 12]], failures: [], transactions_spent: 3, uncovered: [] };
const FILTERS = { min_confidence: "low", excluded_detections: 0 };
const BBOX = { west: 26, south: 36, east: 45, north: 42 };

const CELL = {
  year: 2021,
  month: 8,
  count: 1200,
  count_normalized: 1100.5,
  effort_omega: 1.09,
  mean_frp: 24.5,
  mean_frp_raw: 22.1,
  mean_frp_lower: 20.1,
  mean_frp_upper: 29.3,
  total_frp: 29400,
  total_frp_lower: 25000,
  total_frp_upper: 34000,
  frp_per_detection_eq: 26.7,
  anomaly: 2.1,
  sfai: 1.4,
  quality_flags: ["modis_orbital_drift"],
  conformal: { lower: 1, upper: 2 },
};

const EFFORT = {
  reliable: true,
  reason: null,
  factors: [
    {
      has_modis: true,
      viirs_platforms: ["ViirsSNPP", "ViirsNOAA20"],
      platforms: ["ModisTerra", "ModisAqua", "ViirsSNPP", "ViirsNOAA20"],
      months: 12,
      total: 5000,
      modis_observable: 4000,
      omega: 1.25,
      total_high_intensity: 300,
      modis_observable_high_intensity: 250,
      omega_high_intensity: 1.2,
      snpp_aqua_high_intensity_ratio: 1.18,
    },
  ],
  tropical_swath_gap: null,
};

await check("stats: every platform keeps its own identity", async () => {
  nextBody = {
    totals: { raw_detections: 10, harmonized_records: 8, static_masked: 0, sama_removed: 0, calendar_months: 1, distinct_platforms: 5 },
    sensor_breakdown: ["ModisTerra", "ModisAqua", "ViirsSNPP", "ViirsNOAA20", "ViirsNOAA21"].map((sensor) => ({
      sensor,
      firms_source: "X",
      raw_detections: 2,
      share_pct: 20,
    })),
    confidence_breakdown: { low: 1, nominal: 5, high: 2 },
    daynight_breakdown: { day: 6, night: 2 },
    frp: { harmonized_sum: 80, harmonized_mean: 10, harmonized_max: 30, harmonized_median: 9 },
    temporal: { first_detection: "2026-11-14T10:00:00Z", last_detection: "2026-11-15T11:00:00Z", months_covered: 1 },
    pipeline: { transactions_spent: 3, sources_queried: 5, sources_failed: 0 },
    filters: FILTERS,
    warnings: [],
    report: REPORT,
  };
  const stats = await api.fetchStats({ ...BBOX, days: 2 });
  assert.deepEqual(
    stats.sensorBreakdown.map((entry) => entry.sensor),
    [Sensor.ModisTerra, Sensor.ModisAqua, Sensor.ViirsSNPP, Sensor.ViirsNOAA20, Sensor.ViirsNOAA21],
  );
  assert.equal(stats.pipeline.transactionsSpent, 3);
  assert.match(requested.at(-1) ?? "", /\/api\/stats\?west=26&south=36&east=45&north=42&days=2$/);
});

await check("harmonize: platforms, quality flags and the extra server fields survive", async () => {
  nextBody = {
    raw: { modis_count: 6, viirs_count: 4, total_raw: 10 },
    harmonized: {
      count: 1,
      static_masked: 0,
      sama_removed: 0,
      hotspots: [
        {
          lat: 39,
          lon: 32,
          frp: 30,
          frp_corrected: 31.2,
          frp_lower: 25,
          frp_upper: 37,
          vza_factor: 1.04,
          n_obs: 2,
          platforms: ["ModisTerra", "ViirsSNPP"],
          confidence: "High",
          source: "Matched",
          acquired: "2026-11-15T11:00:00Z",
          daynight: "Day",
        },
      ],
    },
    calendar: [CELL],
    effort: EFFORT,
    uncertainty: { confidence: 0.6827, note: "n", atmospheric_bias: null },
    filters: FILTERS,
    warnings: [],
    report: REPORT,
  };
  const res = await api.fetchHarmonize(BBOX);
  assert.deepEqual(res.harmonized.hotspots[0].platforms, [Sensor.ModisTerra, Sensor.ViirsSNPP]);
  assert.deepEqual(res.effort.factors[0].viirsPlatforms, [Sensor.ViirsSNPP, Sensor.ViirsNOAA20]);
  assert.deepEqual(res.calendar[0].qualityFlags, ["modis_orbital_drift"]);
  assert.equal(res.calendar[0].countNormalized, 1100.5);
});

await check("compare: the VZA adjustment is the new positive field, not the legacy negative one", async () => {
  nextBody = {
    counts: {
      raw_total: 10,
      raw_modis: 6,
      raw_viirs: 4,
      harmonized_total: 8,
      removed_total: 2,
      removed_pct: 20,
      static_masked: 0,
      sama_removed: 0,
      by_source: { modis_only: 3, viirs_only: 3, matched: 2 },
    },
    frp: { raw_sum: 100, harmonized_raw_sum: 90, harmonized_corrected_sum: 94, reduction_pct: 6 },
    merging: {
      records_with_multiple_observations: 2,
      observations_merged: 2,
      max_observations_in_one_record: 2,
      platform_combinations: [{ platforms: ["ModisAqua", "ViirsNOAA20"], records: 2 }],
    },
    vza: {
      records_with_correction: 4,
      mean_factor: 1.05,
      max_factor: 1.3,
      mean_frp_reduction_pct: -5.2,
      mean_frp_adjustment_pct: 5.2,
    },
    filters: { min_confidence: "nominal", excluded_detections: 7 },
    warnings: [],
    report: REPORT,
  };
  const res = await api.fetchCompare({ ...BBOX, minConfidence: "nominal" });
  assert.equal(res.vza.meanFrpAdjustmentPct, 5.2);
  assert.deepEqual(res.merging.platformCombinations[0].platforms, [Sensor.ModisAqua, Sensor.ViirsNOAA20]);
  assert.deepEqual(res.filters, { minConfidence: "nominal", excludedDetections: 7 });
  assert.match(requested.at(-1) ?? "", /min_confidence=nominal/);
});

await check("calendar: archive coverage names the platforms the archive really has", async () => {
  nextBody = {
    country: "Turkey",
    years: { requested_from: 2000, requested_to: 2025, effective_from: 2000, effective_to: 2025 },
    bbox: null,
    coverage: {
      sensors_available: ["ModisTerra", "ModisAqua", "ViirsSNPP", "ViirsNOAA20"],
      sensors_unavailable: ["ViirsNOAA21"],
      note: "n",
    },
    counts: { raw_detections: 10, harmonized_records: 8 },
    calendar: [CELL],
    seasonality: { metric: "count", calendar_year: [], fire_year: [] },
    raw_monthly: [],
    effort: EFFORT,
    uncertainty: { confidence: 0.6827, note: "n", atmospheric_bias: null },
    filters: FILTERS,
    fetch: {
      per_sensor_year: [{ sensor: "ModisTerra", year: 2021, detections: 5 }],
      failures: [{ sensor: "ViirsSNPP", year: 2012, error: "missing" }],
    },
    warnings: [],
  };
  const res = await api.fetchCalendar({ country: "Turkey" });
  assert.deepEqual(res.coverage.sensorsAvailable, [Sensor.ModisTerra, Sensor.ModisAqua, Sensor.ViirsSNPP, Sensor.ViirsNOAA20]);
  assert.deepEqual(res.coverage.sensorsUnavailable, [Sensor.ViirsNOAA21]);
  assert.equal(res.fetch.perSensorYear[0].sensor, Sensor.ModisTerra);
  assert.equal(res.fetch.failures[0].sensor, Sensor.ViirsSNPP);
});

await check("adaptSensor rejects names it does not know instead of guessing a platform", () => {
  for (const known of Object.values(Sensor)) assert.equal(api.adaptSensor(known), known);
  for (const unknown of ["MODIS_Terra", "VIIRS_SNPP", "ViirsNOAA22", "", "constructor"]) {
    assert.throws(
      () => api.adaptSensor(unknown),
      (error: unknown) => error instanceof api.ApiError && error.code === "invalid_response",
      unknown,
    );
  }
});
