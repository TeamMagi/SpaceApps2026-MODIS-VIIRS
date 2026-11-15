import assert from "node:assert/strict";
import { DEFAULT_PIPELINE_CONFIG, parseConfig } from "../src/services/pipelineConfig.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

/** `GET /api/config` for a server running the code defaults of crates/core (config.rs, uncertainty.rs, calibration.rs). */
const SERVER_DEFAULTS = {
  dedup: { max_time_diff_secs: 8, min_overlap_ratio: 0.3 },
  fleet: { viirs_window_secs: 3600, modis_window_secs: 10800, radius_km: 2 },
  matching: { radius_km: 2, window_secs: 10800, simultaneous_window_secs: 1200 },
  vza: { a: 0.2017, b: 0.0168, c: 1 },
  calibration: {
    enabled: true,
    bands: [
      { min_abs_lat: 0, max_abs_lat: 30, ratio: 1.11, ratio_sigma: 0.08 },
      { min_abs_lat: 30, max_abs_lat: 50, ratio: 1.18, ratio_sigma: 0.15 },
      { min_abs_lat: 55, max_abs_lat: 70, ratio: 1.28, ratio_sigma: 0.11 },
    ],
  },
  uncertainty: {
    modis_sensor_sigma: 0.266,
    viirs_sensor_sigma: 0.08,
    vza_error_coeff: 0.05,
    modis_scale_sigma: 0.1,
    z: 1,
  },
  static_mask: { enabled: true, min_active_months: 10 },
  sama: { enabled: false, west: -110, east: 11, south: -55, north: 7 },
};

check("offline fallback thresholds equal what a default-configured server reports", () => {
  assert.deepEqual(parseConfig(SERVER_DEFAULTS), DEFAULT_PIPELINE_CONFIG);
});

check("the MODIS single-pixel sigma fallback is the published 0.266 (Freeborn et al. 2014a), not a guess", () => {
  assert.equal(DEFAULT_PIPELINE_CONFIG.uncertainty.modisSensorSigma, 0.266);
  assert.equal(DEFAULT_PIPELINE_CONFIG.uncertainty.viirsSensorSigma, 0.08);
});

check("a server that configured other thresholds is shown as configured", () => {
  const tuned = parseConfig({
    ...SERVER_DEFAULTS,
    matching: { ...SERVER_DEFAULTS.matching, radius_km: 4.5 },
    uncertainty: { ...SERVER_DEFAULTS.uncertainty, modis_sensor_sigma: 0.2 },
  });
  assert.equal(tuned.matching.radiusKm, 4.5);
  assert.equal(tuned.uncertainty.modisSensorSigma, 0.2);
});

check("a payload missing a section or holding a non-finite number is rejected, not half-read", () => {
  const { uncertainty: _omitted, ...withoutUncertainty } = SERVER_DEFAULTS;
  assert.throws(() => parseConfig(withoutUncertainty));
  assert.throws(() => parseConfig({ ...SERVER_DEFAULTS, vza: { a: null, b: 0.0168, c: 1 } }));
  assert.throws(() => parseConfig(null));
});
