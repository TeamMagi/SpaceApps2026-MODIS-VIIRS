/**
 * Pure data shaping for the "Sensors" chart view: monthly bars, matched-pair FRP scatter, source donut.
 * Functions that depend on harmonization output take `revealAnswer` and return no harmonized-derived
 * numbers while it is false.
 */

import { HotspotSource, type HarmonizedPoint, type MonthCell, type PointStages, type RawPoint } from "../types";

export interface SensorMonthBar {
  month: number;
  modis: number;
  viirs: number;
  hasData: boolean;
}

/** Twelve entries for `year`; months the series does not cover are zero with `hasData: false`. */
export function monthlyBarsForYear(months: readonly MonthCell[], year: number): SensorMonthBar[] {
  const byMonth = new Map<number, MonthCell>();
  for (const cell of months) {
    if (cell.year === year) byMonth.set(cell.month, cell);
  }
  return Array.from({ length: 12 }, (_, i) => {
    const cell = byMonth.get(i + 1);
    if (!cell || !cell.hasData) return { month: i + 1, modis: 0, viirs: 0, hasData: false };
    return { month: i + 1, modis: cell.rawModis, viirs: cell.rawViirs, hasData: true };
  });
}

export interface MatchedPair {
  id: string;
  /** Mean FRP of the linked raw MODIS members, MW. */
  modisFrp: number;
  /** FRP of the Matched record (sum over the VIIRS pixels of the seed pass), MW. */
  viirsFrp: number;
  ratio: number;
}

function isPositive(value: number): boolean {
  return Number.isFinite(value) && value > 0;
}

/**
 * One Matched record is one MODIS footprint (fleet members of the same pixel averaged) against the
 * sum of the N VIIRS pixels inside it (R10), so the comparable quantities are the MODIS pixel FRP
 * (mean of its members) and the record's summed VIIRS FRP. Log axes need positive finite values.
 */
export function extractMatchedPairs(
  harmonized: readonly HarmonizedPoint[],
  raw: readonly RawPoint[],
): MatchedPair[] {
  const modisByRecord = new Map<string, number[]>();
  for (const point of raw) {
    if (point.sensor !== "MODIS" || point.harmonizedId === null || !isPositive(point.frp)) continue;
    const list = modisByRecord.get(point.harmonizedId);
    if (list) list.push(point.frp);
    else modisByRecord.set(point.harmonizedId, [point.frp]);
  }
  const pairs: MatchedPair[] = [];
  for (const record of harmonized) {
    if (record.source !== HotspotSource.Matched || !isPositive(record.frp)) continue;
    const members = modisByRecord.get(record.id);
    if (!members || members.length === 0) continue;
    const modisFrp = members.reduce((sum, v) => sum + v, 0) / members.length;
    pairs.push({ id: record.id, modisFrp, viirsFrp: record.frp, ratio: record.frp / modisFrp });
  }
  return pairs;
}

export function medianRatio(pairs: readonly MatchedPair[]): number | null {
  if (pairs.length === 0) return null;
  const sorted = pairs.map((p) => p.ratio).sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 === 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

export interface PairScatter {
  pairs: MatchedPair[];
  medianRatio: number | null;
}

/** Null in Raw mode: matching is harmonization output. */
export function buildPairScatter(
  revealAnswer: boolean,
  harmonized: readonly HarmonizedPoint[],
  raw: readonly RawPoint[],
): PairScatter | null {
  if (!revealAnswer) return null;
  const pairs = extractMatchedPairs(harmonized, raw);
  return { pairs, medianRatio: medianRatio(pairs) };
}

export type DonutKey = "modisOnly" | "viirsOnly" | "matched" | "modis" | "viirs";

export interface DonutSegment {
  key: DonutKey;
  count: number;
  share: number;
}

export interface SourceDonut {
  kind: "harmonized" | "raw";
  total: number;
  segments: DonutSegment[];
}

function toSegments(entries: readonly (readonly [DonutKey, number])[]): { total: number; segments: DonutSegment[] } {
  const total = entries.reduce((sum, [, count]) => sum + count, 0);
  return {
    total,
    segments: entries.map(([key, count]) => ({ key, count, share: total > 0 ? count / total : 0 })),
  };
}

/**
 * Harmonized: record sources of the loaded points. Raw: the period's raw MODIS/VIIRS split from the
 * stage counts (full counts even when the loaded points are a downsample); no harmonized number is read.
 */
export function buildSourceDonut(
  revealAnswer: boolean,
  harmonized: readonly HarmonizedPoint[],
  stages: PointStages,
): SourceDonut {
  if (!revealAnswer) {
    return { kind: "raw", ...toSegments([["modis", stages.rawModis], ["viirs", stages.rawViirs]]) };
  }
  let modisOnly = 0;
  let viirsOnly = 0;
  let matched = 0;
  for (const point of harmonized) {
    if (point.source === HotspotSource.Matched) matched += 1;
    else if (point.source === HotspotSource.ModisOnly) modisOnly += 1;
    else viirsOnly += 1;
  }
  return {
    kind: "harmonized",
    ...toSegments([["modisOnly", modisOnly], ["viirsOnly", viirsOnly], ["matched", matched]]),
  };
}
