/** Statistics card model (Y4): one selected month, derived per mode from live or fixture data. */

export type StatsMode = "raw" | "harmonized";
export type CardId = "detections" | "removed" | "platforms" | "meanFrp";
export const CARD_IDS: readonly CardId[] = ["detections", "removed", "platforms", "meanFrp"];

export interface YearMonth {
  year: number;
  month: number;
}

export interface StatsMonth extends YearMonth {
  modisCovered: boolean;
  viirsCovered: boolean;
  rawCount: number;
  rawFrpSum: number;
  /** Calendar-cell record count; 0 for a covered month without a cell. */
  harmonizedCount: number;
  /** MODIS-equivalent FRP per normalized detection; null when the cell has none (e.g. Terra-only months). */
  harmonizedMeanFrp: number | null;
  /** Platforms with count > 0; null when the backend does not report platform counts. */
  platforms: readonly string[] | null;
}

export interface StatsDataset {
  source: "live" | "fixture";
  months: readonly StatsMonth[];
  /** Last month with archive coverage (month precision: /api/calendar carries no coverage dates). */
  coverageThrough: YearMonth | null;
  /** Fixture generation time; set for fixtures only. */
  generatedAt: string | null;
}

export type TrendReason = "noPrevious" | "notCovered" | "differentCoverage" | "missingValue" | "zeroBase";

export type Trend =
  | { kind: "change"; direction: "up" | "down" | "flat"; unit: "relative" | "points"; ratio: number; comparedTo: YearMonth }
  | { kind: "none"; reason: TrendReason; comparedTo: YearMonth };

export interface StatsCard {
  id: CardId;
  value: number | null;
  /** Raw view of a metric that needs the harmonized answer. */
  hidden: boolean;
  platforms: readonly string[] | null;
  trend: Trend;
  /** Same metric for the 12 months ending at `selected`; null = no value. */
  spark: readonly (number | null)[];
  sparkStart: YearMonth;
}

export interface StatsCardModel {
  selected: YearMonth;
  cards: readonly StatsCard[];
}

export const SPARK_MONTHS = 12;

const PLATFORM_ORDER = ["ModisTerra", "ModisAqua", "ViirsSNPP", "ViirsNOAA20", "ViirsNOAA21"] as const;
const PLATFORM_LABEL: Record<string, string> = {
  ModisTerra: "Terra", ModisAqua: "Aqua", ViirsSNPP: "S-NPP", ViirsNOAA20: "NOAA-20", ViirsNOAA21: "NOAA-21",
};

export function platformLabel(key: string): string {
  return PLATFORM_LABEL[key] ?? key;
}

function platformRank(key: string): number {
  const i = (PLATFORM_ORDER as readonly string[]).indexOf(key);
  return i < 0 ? PLATFORM_ORDER.length : i;
}

const ordinal = (p: YearMonth): number => p.year * 12 + p.month - 1;
const fromOrdinal = (o: number): YearMonth => ({ year: Math.floor(o / 12), month: (o % 12) + 1 });

export function isCovered(m: StatsMonth): boolean {
  return m.modisCovered || m.viirsCovered;
}

export function removalRatio(raw: number, harmonized: number): number | null {
  return raw > 0 ? (raw - harmonized) / raw : null;
}

export function rawMeanFrp(m: Pick<StatsMonth, "rawCount" | "rawFrpSum">): number | null {
  return m.rawCount > 0 ? m.rawFrpSum / m.rawCount : null;
}

/** Metric value of one month; null for an uncovered month, a zero denominator, missing data or a hidden answer. */
export function metricValue(m: StatsMonth | undefined, id: CardId, mode: StatsMode): number | null {
  if (m === undefined || !isCovered(m)) return null;
  switch (id) {
    case "detections":
      return mode === "raw" ? m.rawCount : m.harmonizedCount;
    case "removed":
      return mode === "raw" ? null : removalRatio(m.rawCount, m.harmonizedCount);
    case "platforms":
      return m.platforms === null ? null : m.platforms.length;
    case "meanFrp":
      return mode === "raw" ? rawMeanFrp(m) : m.harmonizedMeanFrp;
  }
}

export function trendVs(
  index: ReadonlyMap<number, StatsMonth>,
  selected: YearMonth,
  id: CardId,
  mode: StatsMode,
): Trend {
  const comparedTo = { year: selected.year - 1, month: selected.month };
  const none = (reason: TrendReason): Trend => ({ kind: "none", reason, comparedTo });
  const current = index.get(ordinal(selected));
  const previous = index.get(ordinal(comparedTo));
  if (previous === undefined) return none("noPrevious");
  if (current === undefined || !isCovered(current) || !isCovered(previous)) return none("notCovered");
  if (current.modisCovered !== previous.modisCovered || current.viirsCovered !== previous.viirsCovered) return none("differentCoverage");
  const now = metricValue(current, id, mode);
  const base = metricValue(previous, id, mode);
  if (now === null || base === null) return none("missingValue");
  // A ratio metric changes in absolute points, so a zero base is still comparable.
  const points = id === "removed";
  if (!points && base <= 0) return none("zeroBase");
  const ratio = points ? now - base : (now - base) / base;
  return { kind: "change", direction: ratio > 0 ? "up" : ratio < 0 ? "down" : "flat", unit: points ? "points" : "relative", ratio, comparedTo };
}

export function sparkWindow(selected: YearMonth): YearMonth[] {
  const end = ordinal(selected);
  return Array.from({ length: SPARK_MONTHS }, (_, i) => fromOrdinal(end - SPARK_MONTHS + 1 + i));
}

export function buildCardModel(dataset: StatsDataset, selected: YearMonth, mode: StatsMode): StatsCardModel {
  const index = new Map(dataset.months.map(m => [ordinal(m), m]));
  const current = index.get(ordinal(selected));
  const window = sparkWindow(selected);
  const cards = CARD_IDS.map((id): StatsCard => ({
    id,
    value: metricValue(current, id, mode),
    hidden: id === "removed" && mode === "raw",
    platforms: id === "platforms" && current !== undefined && isCovered(current) ? current.platforms : null,
    trend: trendVs(index, selected, id, mode),
    spark: window.map(w => metricValue(index.get(ordinal(w)), id, mode)),
    sparkStart: window[0]!,
  }));
  return { selected, cards };
}

type Json = Record<string, unknown>;

function isRecord(value: unknown): value is Json {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function num(row: Json, key: string, where: string): number {
  const value = row[key];
  if (typeof value !== "number" || !Number.isFinite(value)) throw new Error(`Invalid /api/calendar: ${where}.${key} must be a number`);
  return value;
}

function optNum(row: Json, key: string): number | null {
  const value = row[key];
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function platformsOf(row: Json): readonly string[] | null {
  const counts = row.platform_counts;
  if (!isRecord(counts)) return null;
  return Object.entries(counts)
    .filter(([, n]) => typeof n === "number" && n > 0)
    .map(([name]) => name)
    .sort((a, b) => platformRank(a) - platformRank(b) || a.localeCompare(b));
}

function arrayOf(json: Json, key: string): Json[] {
  const value = json[key];
  if (!Array.isArray(value)) throw new Error(`Invalid /api/calendar: ${key} must be an array`);
  return value.map((row, i): Json => {
    if (!isRecord(row)) throw new Error(`Invalid /api/calendar: ${key}[${i}] must be an object`);
    return row;
  });
}

/** Parses /api/calendar into the card dataset. A missing `platform_counts` means unknown platforms, not zero. */
export function fromLiveStats(json: unknown): StatsDataset {
  if (!isRecord(json)) throw new Error("Invalid /api/calendar: response must be an object");
  if (!Array.isArray(json.raw_monthly)) {
    throw new Error("/api/calendar has no raw_monthly: the backend is too old, update and restart the server");
  }
  const cells = new Map<number, { count: number; eq: number | null }>();
  arrayOf(json, "calendar").forEach((row, i) => {
    const where = `calendar[${i}]`;
    cells.set(ordinal({ year: num(row, "year", where), month: num(row, "month", where) }), {
      count: num(row, "count", where),
      eq: optNum(row, "frp_per_detection_eq"),
    });
  });
  const months = arrayOf(json, "raw_monthly").map((row, i): StatsMonth => {
    const where = `raw_monthly[${i}]`;
    const year = num(row, "year", where);
    const month = num(row, "month", where);
    const modisCovered = row.modis_covered === true;
    const viirsCovered = row.viirs_covered === true;
    const cell = cells.get(ordinal({ year, month }));
    return {
      year, month, modisCovered, viirsCovered,
      rawCount: num(row, "modis_count", where) + num(row, "viirs_count", where),
      rawFrpSum: num(row, "modis_frp_sum", where) + num(row, "viirs_frp_sum", where),
      harmonizedCount: cell?.count ?? 0,
      harmonizedMeanFrp: cell?.eq ?? null,
      platforms: platformsOf(row),
    };
  }).sort((a, b) => ordinal(a) - ordinal(b));
  const covered = months.filter(isCovered);
  const last = covered[covered.length - 1];
  return {
    source: "live",
    months,
    coverageThrough: last === undefined ? null : { year: last.year, month: last.month },
    generatedAt: null,
  };
}
