import type { CopyKey } from "../../content/copy";
import type { PipelineConfig } from "../../services/pipelineConfig";
import type { PipelineStageKey } from "../../types";

/** The stages shown in the strip. Fleet consolidation is part of stage 2 on this page, not a stage of its own. */
export type FlowStage = Exclude<PipelineStageKey, "fleet">;

export const STAGE_DETAIL = {
  collect: { what: "methodsDetailCollectWhat", why: "methodsDetailCollectWhy", io: "methodsDetailCollectIo" },
  dedup: { what: "methodsDetailDedupWhat", why: "methodsDetailDedupWhy", io: "methodsDetailDedupIo" },
  angle: { what: "methodsDetailAngleWhat", why: "methodsDetailAngleWhy", io: "methodsDetailAngleIo" },
  grid: { what: "methodsDetailGridWhat", why: "methodsDetailGridWhy", io: "methodsDetailGridIo" },
  match: { what: "methodsDetailMatchWhat", why: "methodsDetailMatchWhy", io: "methodsDetailMatchIo" },
  fuse: { what: "methodsDetailFuseWhat", why: "methodsDetailFuseWhy", io: "methodsDetailFuseIo" },
  calendar: { what: "methodsDetailCalendarWhat", why: "methodsDetailCalendarWhy", io: "methodsDetailCalendarIo" },
} as const satisfies Record<FlowStage, { what: CopyKey; why: CopyKey; io: CopyKey }>;

export interface StageParam {
  /** Config key or constant name exactly as it appears in the code. */
  name: string;
  value: string;
}

const bandLabel = (min: number, max: number) => `${min}–${max}°`;

/** Config keys stay untranslated on purpose: they are the names a reader would search the code or TOML for. */
export function stageParams(config: PipelineConfig): Record<FlowStage, readonly StageParam[]> {
  const { dedup, fleet, matching, vza, calibration, uncertainty } = config;
  return {
    collect: [
      { name: "ARCHIVE_MAX_REGION_MB", value: "512" },
      { name: "AREA_CACHE_NRT_TTL_SECS", value: "600" },
    ],
    dedup: [
      { name: "dedup.max_time_diff_secs", value: String(dedup.maxTimeDiffSecs) },
      { name: "dedup.min_overlap_ratio", value: String(dedup.minOverlapRatio) },
      { name: "fleet.viirs_window_secs", value: String(fleet.viirsWindowSecs) },
      { name: "fleet.modis_window_secs", value: String(fleet.modisWindowSecs) },
      { name: "fleet.radius_km", value: String(fleet.radiusKm) },
    ],
    angle: [
      { name: "vza.a", value: String(vza.a) },
      { name: "vza.b", value: String(vza.b) },
      { name: "vza.c", value: String(vza.c) },
    ],
    grid: [
      { name: "INDEX_RESOLUTION", value: "8" },
      { name: "MIN_CELL_WIDTH_KM", value: "0.64" },
      { name: "COVERAGE_MARGIN", value: "1.15" },
      { name: "RING_SLACK", value: "2" },
    ],
    match: [
      { name: "matching.radius_km", value: String(matching.radiusKm) },
      { name: "matching.window_secs", value: String(matching.windowSecs) },
    ],
    fuse: [
      { name: "uncertainty.modis_sensor_sigma", value: String(uncertainty.modisSensorSigma) },
      { name: "uncertainty.viirs_sensor_sigma", value: String(uncertainty.viirsSensorSigma) },
      { name: "uncertainty.vza_error_coeff", value: String(uncertainty.vzaErrorCoeff) },
      { name: "uncertainty.modis_scale_sigma", value: String(uncertainty.modisScaleSigma) },
      { name: "uncertainty.z", value: String(uncertainty.z) },
    ],
    calendar: [
      { name: "calibration.enabled", value: String(calibration.enabled) },
      ...calibration.bands.map((band) => ({
        name: `k_c · |lat| ${bandLabel(band.minAbsLat, band.maxAbsLat)}`,
        value: `${band.ratio} (σ ${band.ratioSigma})`,
      })),
      { name: "MIN_MONTHS_PER_CONFIG", value: "3" },
    ],
  };
}
