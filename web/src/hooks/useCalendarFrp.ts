import { useEffect, useState } from "react";
import { fetchCalendar, isAbortError } from "../services/api";
import type { CalendarCell } from "../types";

export type CalendarFrpMap = ReadonlyMap<string, CalendarCell>;

const EMPTY: CalendarFrpMap = new Map();

// Archive data is finalized, so a region's calendar never changes within a session.
const cache = new Map<string, CalendarFrpMap>();

export function calendarFrpKey(year: number, month: number): string {
  return `${year}-${month}`;
}

/**
 * Per-month FRP metrics from `/api/calendar`, keyed by `calendarFrpKey`. Region ids are the
 * archive country names, so they are passed as `country` unchanged. This is enrichment only:
 * while loading or after a failure the map is empty and the calendar renders from series data.
 */
export function useCalendarFrp(region: string): CalendarFrpMap {
  const [loaded, setLoaded] = useState<{ region: string; cells: CalendarFrpMap } | null>(null);

  useEffect(() => {
    if (!region || cache.has(region)) return;
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
  }, [region]);

  const cached = cache.get(region);
  if (cached) return cached;
  return loaded?.region === region ? loaded.cells : EMPTY;
}
