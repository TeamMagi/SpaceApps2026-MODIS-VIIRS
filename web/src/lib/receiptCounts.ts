import type { PointStages, RegionTotals, YearlyPoint } from "../types";

export interface PeriodStageCounts {
  readonly raw: number;
  readonly staticMasked: number;
  readonly samaRemoved: number;
  readonly interscanRemoved: number;
  readonly fleetMerged: number;
  readonly matchedPairs: number;
  readonly viirsAggregated: number;
  readonly doubleCounts: number;
  readonly harmonized: number;
}

/**
 * Computes duplicate-detection removals (`doubleCounts`) excluding static-source masking and SAMA
 * noise filtering so `raw - staticMasked - samaRemoved - doubleCounts === harmonized`.
 */
export function computeDoubleCounts(params: {
  raw: number;
  staticMasked: number;
  samaRemoved: number;
  harmonized: number;
}): number {
  return Math.max(0, params.raw - params.staticMasked - params.samaRemoved - params.harmonized);
}

/** Aggregates yearly archive entries into region-wide totals with mask/SAMA counts excluded from `doubleCounts`. */
export function aggregateRegionTotals(years: readonly YearlyPoint[]): RegionTotals {
  let modis = 0;
  let viirs = 0;
  let raw = 0;
  let harmonized = 0;
  let staticMasked = 0;
  let samaRemoved = 0;
  let interscanRemoved = 0;
  let fleetMerged = 0;
  let matched = 0;
  let viirsAggregated = 0;

  for (const y of years) {
    modis += y.rawModis;
    viirs += y.rawViirs;
    raw += y.raw;
    harmonized += y.harmonized;
    staticMasked += y.staticMasked;
    samaRemoved += y.samaRemoved;
    interscanRemoved += y.interscanRemoved;
    fleetMerged += y.fleetMerged;
    matched += y.matched;
    viirsAggregated += y.viirsAggregated;
  }

  const doubleCounts = computeDoubleCounts({ raw, staticMasked, samaRemoved, harmonized });
  const doubleCountShare = raw > 0 ? doubleCounts / raw : 0;

  return {
    modis,
    viirs,
    raw,
    harmonized,
    staticMasked,
    samaRemoved,
    interscanRemoved,
    fleetMerged,
    matched,
    viirsAggregated,
    doubleCounts,
    doubleCountShare,
  };
}

/** Summarizes `/api/points` stage counts for Receipt and PipelineStrip using the same `doubleCounts` rule. */
export function summarizePointStages(stages: PointStages): PeriodStageCounts {
  const raw = stages.rawModis + stages.rawViirs;
  const doubleCounts = computeDoubleCounts({
    raw,
    staticMasked: stages.staticMasked,
    samaRemoved: stages.samaRemoved,
    harmonized: stages.harmonized,
  });
  return {
    raw,
    staticMasked: stages.staticMasked,
    samaRemoved: stages.samaRemoved,
    interscanRemoved: stages.interscanRemoved,
    fleetMerged: stages.fleetMerged,
    matchedPairs: stages.matchedPairs,
    viirsAggregated: stages.viirsAggregated,
    doubleCounts,
    harmonized: stages.harmonized,
  };
}
