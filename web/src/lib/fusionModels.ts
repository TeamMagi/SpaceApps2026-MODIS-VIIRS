import type { FusionModel } from "../types";

export type FusionLabelKey = "fusionViirsPreferred" | "fusionEqualWeight" | "fusionModisCalibrated";

export const FUSION_LABEL_KEY: Readonly<Record<FusionModel, FusionLabelKey>> = {
  viirs_preferred: "fusionViirsPreferred",
  equal_weight: "fusionEqualWeight",
  modis_calibrated: "fusionModisCalibrated",
};
