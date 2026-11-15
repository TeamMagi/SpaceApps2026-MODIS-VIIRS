import type { CopyKey } from "../content/copy";
import {
  HotspotSource,
  type CalendarCell,
  type ConformalBand,
  type HarmonizedPoint,
  type MonthCell,
  type PointStages,
} from "../types";

/** Every value of `QualityFlag` (crates/core/src/quality.rs) the backend can put in `quality_flags`. */
export const QUALITY_FLAG_COPY = {
  terra_early_record: "cellFlagTerraEarlyRecord",
  terra_outage: "cellFlagTerraOutage",
  snpp_early_calibration: "cellFlagSnppEarlyCalibration",
  snpp_spurious_scan_lines: "cellFlagSnppSpuriousScanLines",
  modis_orbital_drift: "cellFlagModisOrbitalDrift",
} as const satisfies Record<string, CopyKey>;

export type KnownQualityFlag = keyof typeof QUALITY_FLAG_COPY;

export function qualityFlagCopyKey(flag: string): (typeof QUALITY_FLAG_COPY)[KnownQualityFlag] | null {
  return Object.prototype.hasOwnProperty.call(QUALITY_FLAG_COPY, flag)
    ? QUALITY_FLAG_COPY[flag as KnownQualityFlag]
    : null;
}

/** First month with Aqua in the archive; earlier months are Terra-only (effort.rs `FIRST_TERRA_AQUA_MONTH`). */
const FIRST_TERRA_AQUA = { year: 2002, month: 7 } as const;

export function isTerraOnlyMonth(year: number, month: number): boolean {
  return year * 12 + month < FIRST_TERRA_AQUA.year * 12 + FIRST_TERRA_AQUA.month;
}

export interface SourceSplit {
  matched: number;
  modisOnly: number;
  viirsOnly: number;
}

/**
 * MODIS-only / VIIRS-only split of a month's harmonized records, or null when the point set was
 * sampled or does not add up to the stage total (then the split would be wrong).
 */
export function sourceSplit(
  points: readonly HarmonizedPoint[],
  stages: PointStages,
  truncated: boolean,
): SourceSplit | null {
  if (truncated || points.length !== stages.harmonized) return null;
  let matched = 0;
  let modisOnly = 0;
  let viirsOnly = 0;
  for (const p of points) {
    if (p.source === HotspotSource.Matched) matched += 1;
    else if (p.source === HotspotSource.ModisOnly) modisOnly += 1;
    else viirsOnly += 1;
  }
  return { matched, modisOnly, viirsOnly };
}

export interface Interval {
  value: number;
  lower: number | null;
  upper: number | null;
}

export interface RawDiagnostic {
  modis: number;
  viirs: number;
  total: number;
}

export interface HarmonizedDiagnostic {
  count: number;
  /** Matched records; known from the stage counts even when the point set is sampled. */
  matched: number | null;
  modisOnly: number | null;
  viirsOnly: number | null;
  terraOnly: boolean;
  omega: number | null;
  countNormalized: number | null;
  recovered: Interval | null;
  /** Whether the pre-VIIRS completeness factor k_c scales this month's total FRP; the factor itself is not exposed. */
  completenessApplied: boolean;
  totalFrp: Interval | null;
  frpPerDetectionEq: number | null;
  conformal: ConformalBand | null;
  anomaly: number | null;
}

export interface CellDiagnostic {
  year: number;
  month: number;
  hasData: boolean;
  raw: RawDiagnostic;
  flags: readonly string[];
  /** null in Raw mode: everything harmonization derives stays hidden until the answer is revealed. */
  harmonized: HarmonizedDiagnostic | null;
}

export interface CellDiagnosticInput {
  cell: MonthCell;
  /** The month's `/api/calendar` cell; absent in demo mode and while the calendar loads. */
  calendar?: CalendarCell;
  stages?: PointStages | null;
  sources?: SourceSplit | null;
}

export function buildCellDiagnostic(input: CellDiagnosticInput, revealAnswer: boolean): CellDiagnostic {
  const { cell, calendar, stages, sources } = input;
  const base = {
    year: cell.year,
    month: cell.month,
    hasData: cell.hasData,
    raw: { modis: cell.rawModis, viirs: cell.rawViirs, total: cell.rawCount },
    flags: calendar?.qualityFlags ?? [],
  };
  if (!revealAnswer) return { ...base, harmonized: null };

  const recovered: Interval | null =
    calendar?.countRecovered !== undefined
      ? {
          value: calendar.countRecovered,
          lower: calendar.countRecoveredLower ?? null,
          upper: calendar.countRecoveredUpper ?? null,
        }
      : null;
  const totalFrp: Interval | null = calendar
    ? { value: calendar.totalFrp, lower: calendar.totalFrpLower, upper: calendar.totalFrpUpper }
    : null;

  return {
    ...base,
    harmonized: {
      count: cell.harmonizedCount,
      matched: sources?.matched ?? stages?.matchedPairs ?? null,
      modisOnly: sources?.modisOnly ?? null,
      viirsOnly: sources?.viirsOnly ?? null,
      terraOnly: isTerraOnlyMonth(cell.year, cell.month),
      omega: calendar?.effortOmega ?? null,
      countNormalized: calendar?.countNormalized ?? null,
      recovered,
      completenessApplied: cell.hasData && cell.rawViirs === 0,
      totalFrp,
      frpPerDetectionEq: calendar?.frpPerDetectionEq ?? null,
      conformal: calendar?.conformal ?? null,
      anomaly: calendar ? calendar.anomaly : cell.anomalyHarmonized,
    },
  };
}
