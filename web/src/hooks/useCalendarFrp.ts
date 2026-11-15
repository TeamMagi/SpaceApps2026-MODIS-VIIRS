import { useEffect, useState } from "react";
import { fetchCalendar, isAbortError } from "../services/api";
import { useAppStore } from "../store/useAppStore";
import type { CalendarCell } from "../types";

export type CalendarFrpMap = ReadonlyMap<string, CalendarCell>;

const EMPTY: CalendarFrpMap = new Map();

// Archive data is finalized, so a region's calendar never changes within a session.
const cache = new Map<string, CalendarFrpMap>();

export function calendarFrpKey(year: number, month: number): string {
  return `${year}-${month}`;
}

interface LoadedCalendar {
  region: string;
  cells: CalendarFrpMap;
}

/**
 * The cells to show for `region`. They come from the live archive, so demo mode gets none: the demo
 * region has the same id as a real one (Turkey), and a cache filled before switching to demo would
 * otherwise put real FRP next to synthetic counts.
 */
export function selectCalendarFrp(
  isDemo: boolean,
  region: string,
  cached: CalendarFrpMap | undefined,
  loaded: LoadedCalendar | null,
): CalendarFrpMap {
  if (isDemo) return EMPTY;
  if (cached) return cached;
  return loaded?.region === region ? loaded.cells : EMPTY;
}

/**
 * Per-month FRP metrics from `/api/calendar`, keyed by `calendarFrpKey`. Region ids are the
 * archive country names, so they are passed as `country` unchanged. This is enrichment only:
 * while loading or after a failure the map is empty and the calendar renders from series data.
 */
export function useCalendarFrp(region: string): CalendarFrpMap {
  const isDemo = useAppStore((state) => state.dataMode === "demo");
  const [loaded, setLoaded] = useState<LoadedCalendar | null>(null);

  useEffect(() => {
    if (isDemo || !region || cache.has(region)) return;
    const controller = new AbortController();
    fetchCalendar({ country: region }, controller.signal)
      .then((res) => {
        const cells = new Map<string, CalendarCell>();
        for (const cell of res.calendar) cells.set(calendarFrpKey(cell.year, cell.month), cell);
        cache.set(region, cells);
        setLoaded({ region, cells });
      })
      .catch((err: unknown) => {
        if (!isAbortError(err)) setLoaded(null);
      });
    return () => controller.abort();
  }, [region, isDemo]);

  return selectCalendarFrp(isDemo, region, cache.get(region), loaded);
}
