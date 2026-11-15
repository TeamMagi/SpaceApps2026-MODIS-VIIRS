import type { CalendarCell, OverpassMedian } from "../types";
import { DayNight } from "../types";

export type ModisPlatform = "Terra" | "Aqua";

/** Median passes from `/api/points` that differ from nominal by at least this many minutes count as deviating. */
export const DRIFT_DEVIATION_MIN = 20;

/** First year the backend flags `modis_orbital_drift`. */
export const DRIFT_FLAG_START_YEAR = 2021;

export const DRIFT_FLAG = "modis_orbital_drift";

const DAY_MINUTES = 1440;

/**
 * Standard mission design equator-crossing times in minutes after local midnight: Terra 10:30
 * descending (day) and 22:30 (night), Aqua 13:30 ascending (day) and 01:30 (night).
 */
const NOMINAL_MINUTES: Record<ModisPlatform, Record<DayNight, number>> = {
  Terra: { [DayNight.Day]: 10 * 60 + 30, [DayNight.Night]: 22 * 60 + 30 },
  Aqua: { [DayNight.Day]: 13 * 60 + 30, [DayNight.Night]: 1 * 60 + 30 },
};

export interface DriftRow {
  platform: ModisPlatform;
  daynight: DayNight;
  /** "HH:MM" of the period's median. */
  medianTime: string;
  /** "HH:MM" of the nominal crossing time. */
  nominalTime: string;
  /** Median minus nominal in whole minutes, shortest way round the clock, in [-720, 720]. */
  diffMinutes: number;
  n: number;
}

/** "HH:MM" for minutes after midnight, wrapped to a day and rounded to the nearest minute. */
export function formatClock(minutes: number): string {
  const total = Math.round(((minutes % DAY_MINUTES) + DAY_MINUTES) % DAY_MINUTES) % DAY_MINUTES;
  const hh = String(Math.floor(total / 60)).padStart(2, "0");
  const mm = String(total % 60).padStart(2, "0");
  return `${hh}:${mm}`;
}

/** `actual - nominal` as the shortest signed distance on the 24 h clock: 00:10 vs 23:50 is +20, not −1420. */
export function signedMinuteDiff(actual: number, nominal: number): number {
  const raw = (((actual - nominal) % DAY_MINUTES) + DAY_MINUTES) % DAY_MINUTES;
  return raw > DAY_MINUTES / 2 ? raw - DAY_MINUTES : raw;
}

function isModisPlatform(platform: string): platform is ModisPlatform {
  return platform === "Terra" || platform === "Aqua";
}

/** Terra and Aqua rows of the overpass medians, Terra first and Day before Night. */
export function buildDriftRows(overpass: readonly OverpassMedian[] | null | undefined): DriftRow[] {
  if (!overpass) return [];
  const rows: DriftRow[] = [];
  for (const item of overpass) {
    if (!isModisPlatform(item.platform)) continue;
    const nominal = NOMINAL_MINUTES[item.platform][item.daynight];
    rows.push({
      platform: item.platform,
      daynight: item.daynight,
      medianTime: formatClock(item.medianMinutes),
      nominalTime: formatClock(nominal),
      diffMinutes: Math.round(signedMinuteDiff(item.medianMinutes, nominal)),
      n: item.n,
    });
  }
  const platformOrder = (p: ModisPlatform) => (p === "Terra" ? 0 : 1);
  const passOrder = (d: DayNight) => (d === DayNight.Day ? 0 : 1);
  return rows.sort(
    (a, b) => platformOrder(a.platform) - platformOrder(b.platform) || passOrder(a.daynight) - passOrder(b.daynight),
  );
}

export function driftRowsDeviate(rows: readonly DriftRow[], threshold = DRIFT_DEVIATION_MIN): boolean {
  return rows.some((row) => Math.abs(row.diffMinutes) >= threshold);
}

export interface DriftPeriod {
  year: number;
  /** null = the whole year. */
  month: number | null;
}

/** True when a calendar cell of the period carries the `modis_orbital_drift` flag. */
export function periodHasDriftFlag(cells: Iterable<CalendarCell>, period: DriftPeriod): boolean {
  for (const cell of cells) {
    if (cell.year !== period.year) continue;
    if (period.month !== null && cell.month !== period.month) continue;
    if (cell.qualityFlags.includes(DRIFT_FLAG)) return true;
  }
  return false;
}

export interface DriftStripInput {
  /** Flag seen on the period's calendar cells (live) or implied by the period's year (demo). */
  flagged: boolean;
  rows: readonly DriftRow[];
  dismissed: boolean;
}

export interface DriftStripState {
  visible: boolean;
  /** Shown because the flag or the data says so; the flag takes precedence. */
  reason: "flag" | "deviation" | null;
  showNumbers: boolean;
}

export function driftStripState({ flagged, rows, dismissed }: DriftStripInput): DriftStripState {
  const deviates = driftRowsDeviate(rows);
  const reason = flagged ? "flag" : deviates ? "deviation" : null;
  return { visible: !dismissed && reason !== null, reason, showNumbers: rows.length > 0 };
}

/** The flag is date-based, so demo mode (no calendar cells) infers it from the period's year. */
export function demoPeriodIsFlagged(period: DriftPeriod | null): boolean {
  return period !== null && period.year >= DRIFT_FLAG_START_YEAR;
}
