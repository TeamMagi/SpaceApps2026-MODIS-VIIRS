
export interface CalibrationBand {
  minAbsLat: number;
  maxAbsLat: number;
  ratio: number;
  ratioSigma: number;
}

/** The subset of `GET /api/config` the methods page shows (camelCase mirror of the wire shape). */
export interface PipelineConfig {
  dedup: { maxTimeDiffSecs: number; minOverlapRatio: number };
  fleet: { viirsWindowSecs: number; modisWindowSecs: number; radiusKm: number };
  matching: { radiusKm: number; windowSecs: number };
  vza: { a: number; b: number; c: number };
  calibration: { enabled: boolean; bands: readonly CalibrationBand[] };
  uncertainty: {
    modisSensorSigma: number;
    viirsSensorSigma: number;
    vzaErrorCoeff: number;
    modisScaleSigma: number;
    z: number;
  };
}

/** Code defaults from `crates/core/src/config.rs`; shown whenever the server is unreachable. */
export const DEFAULT_PIPELINE_CONFIG: PipelineConfig = {
  dedup: { maxTimeDiffSecs: 8, minOverlapRatio: 0.3 },
  fleet: { viirsWindowSecs: 3600, modisWindowSecs: 10800, radiusKm: 2 },
  matching: { radiusKm: 2, windowSecs: 10800 },
  vza: { a: 0.2017, b: 0.0168, c: 1 },
  calibration: {
    enabled: true,
    bands: [
      { minAbsLat: 0, maxAbsLat: 30, ratio: 1.11, ratioSigma: 0.08 },
      { minAbsLat: 30, maxAbsLat: 50, ratio: 1.18, ratioSigma: 0.15 },
      { minAbsLat: 55, maxAbsLat: 70, ratio: 1.28, ratioSigma: 0.11 },
    ],
  },
  uncertainty: { modisSensorSigma: 0.12, viirsSensorSigma: 0.08, vzaErrorCoeff: 0.05, modisScaleSigma: 0.1, z: 1 },
};

type Json = Record<string, unknown>;

function record(value: unknown): Json | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Json) : null;
}

function num(source: Json, key: string): number {
  const value = source[key];
  if (typeof value !== "number" || !Number.isFinite(value)) throw new Error(`invalid ${key}`);
  return value;
}

function section(source: Json, key: string): Json {
  const value = record(source[key]);
  if (!value) throw new Error(`missing ${key}`);
  return value;
}

function parseConfig(wire: unknown): PipelineConfig {
  const root = record(wire);
  if (!root) throw new Error("invalid config");
  const dedup = section(root, "dedup");
  const fleet = section(root, "fleet");
  const matching = section(root, "matching");
  const vza = section(root, "vza");
  const calibration = section(root, "calibration");
  const uncertainty = section(root, "uncertainty");
  const bands = calibration.bands;
  if (typeof calibration.enabled !== "boolean" || !Array.isArray(bands)) throw new Error("invalid calibration");
  return {
    dedup: { maxTimeDiffSecs: num(dedup, "max_time_diff_secs"), minOverlapRatio: num(dedup, "min_overlap_ratio") },
    fleet: {
      viirsWindowSecs: num(fleet, "viirs_window_secs"),
      modisWindowSecs: num(fleet, "modis_window_secs"),
      radiusKm: num(fleet, "radius_km"),
    },
    matching: { radiusKm: num(matching, "radius_km"), windowSecs: num(matching, "window_secs") },
    vza: { a: num(vza, "a"), b: num(vza, "b"), c: num(vza, "c") },
    calibration: {
      enabled: calibration.enabled,
      bands: bands.map((band) => {
        const b = record(band);
        if (!b) throw new Error("invalid band");
        return {
          minAbsLat: num(b, "min_abs_lat"),
          maxAbsLat: num(b, "max_abs_lat"),
          ratio: num(b, "ratio"),
          ratioSigma: num(b, "ratio_sigma"),
        };
      }),
    },
    uncertainty: {
      modisSensorSigma: num(uncertainty, "modis_sensor_sigma"),
      viirsSensorSigma: num(uncertainty, "viirs_sensor_sigma"),
      vzaErrorCoeff: num(uncertainty, "vza_error_coeff"),
      modisScaleSigma: num(uncertainty, "modis_scale_sigma"),
      z: num(uncertainty, "z"),
    },
  };
}

export async function fetchPipelineConfig(signal?: AbortSignal): Promise<PipelineConfig> {
  const response = await fetch("/api/config", { signal });
  if (!response.ok) throw new Error(`config request failed (${response.status})`);
  return parseConfig(await response.json());
}
