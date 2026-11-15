/**
 * Türkiye 2021 Mediterranean fires (S7): the figures in data/case-study/turkey/stats.json.
 * The JSON sits outside web/ and is loaded with a dynamic import, so it is its own chunk and is
 * fetched only when the briefing for Türkiye is open in Harmonized mode.
 */

export interface CaseStudyMonth {
  count: number;
  anomalyZ: number;
  rankAllMonths: number;
  ofAllMonths: number;
}

export interface CaseStudySummary {
  /** First and last archive year, e.g. [2012, 2025]. */
  years: readonly [number, number] | null;
  antalyaAugust: CaseStudyMonth;
  muglaAugust: CaseStudyMonth;
  mediterraneanJulyZ: number;
  mediterraneanAugustZ: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function num(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function readMonth(region: unknown, month: "july" | "august"): Record<string, unknown> | null {
  if (!isRecord(region) || !isRecord(region.anomaly_2021)) return null;
  const entry = region.anomaly_2021[month];
  return isRecord(entry) ? entry : null;
}

function readCaseMonth(region: unknown): CaseStudyMonth | null {
  const m = readMonth(region, "august");
  if (!m) return null;
  const count = num(m.count);
  const anomalyZ = num(m.anomaly_z);
  const rankAllMonths = num(m.rank_all_months);
  const ofAllMonths = num(m.of_all_months);
  if (count === null || anomalyZ === null || rankAllMonths === null || ofAllMonths === null) return null;
  return { count, anomalyZ, rankAllMonths, ofAllMonths };
}

/** Runtime guard for the S7 stats file: null when any figure the card shows is missing. */
export function parseCaseStudy(json: unknown): CaseStudySummary | null {
  if (!isRecord(json) || !isRecord(json.regions)) return null;
  const { antalya, mugla, mediterranean } = json.regions;
  const antalyaAugust = readCaseMonth(antalya);
  const muglaAugust = readCaseMonth(mugla);
  const julyZ = num(readMonth(mediterranean, "july")?.anomaly_z);
  const augustZ = num(readMonth(mediterranean, "august")?.anomaly_z);
  if (!antalyaAugust || !muglaAugust || julyZ === null || augustZ === null) return null;

  let years: CaseStudySummary["years"] = null;
  const metaYears = isRecord(json.meta) ? json.meta.years : null;
  if (Array.isArray(metaYears) && metaYears.length === 2) {
    const [first, last] = metaYears.map(num);
    if (first !== null && last !== null) years = [first, last];
  }
  return { years, antalyaAugust, muglaAugust, mediterraneanJulyZ: julyZ, mediterraneanAugustZ: augustZ };
}

let pending: Promise<CaseStudySummary | null> | null = null;

export function loadCaseStudy(): Promise<CaseStudySummary | null> {
  pending ??= import("../../../data/case-study/turkey/stats.json").then(
    (module) => parseCaseStudy(module.default),
    () => {
      pending = null;
      return null;
    },
  );
  return pending;
}
