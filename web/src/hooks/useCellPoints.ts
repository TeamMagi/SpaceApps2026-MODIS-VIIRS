import { useEffect, useState } from "react";
import { fetchPoints, isAbortError, type PointsResponse } from "../services/api";
import { useAppStore } from "../store/useAppStore";
import type { MonthRef } from "../types";

export type CellPointsStatus = "idle" | "loading" | "ready" | "error";

export interface CellPoints {
  status: CellPointsStatus;
  points: PointsResponse | null;
}

interface Loaded {
  key: string;
  status: "ready" | "error";
  points: PointsResponse | null;
}

// Archive months are final, so a month fetched once is reused until the region or data mode changes.
const cache = new Map<string, PointsResponse>();

function cacheKey(dataMode: string, region: string, month: MonthRef): string {
  return `${dataMode}|${region}|${month.year}-${month.month}`;
}

/**
 * `/api/points` of one month with the default pipeline, for the cell diagnostic drawer: it carries the
 * stage counts (matched records) and the points the MODIS-only / VIIRS-only split is counted from.
 * Fetches only while `enabled`.
 */
export function useCellPoints(month: MonthRef | null, enabled: boolean): CellPoints {
  const region = useAppStore((state) => state.region);
  const dataMode = useAppStore((state) => state.dataMode);
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const key = month ? cacheKey(dataMode, region, month) : null;

  useEffect(() => {
    if (!enabled || !month || !key || cache.has(key)) return;
    const controller = new AbortController();
    fetchPoints({ region, year: month.year, month: month.month }, controller.signal)
      .then((points) => {
        cache.set(key, points);
        setLoaded({ key, status: "ready", points });
      })
      .catch((err: unknown) => {
        if (!isAbortError(err)) setLoaded({ key, status: "error", points: null });
      });
    return () => controller.abort();
  }, [enabled, month, key, region]);

  if (!enabled || !key) return { status: "idle", points: null };
  const cached = cache.get(key);
  if (cached) return { status: "ready", points: cached };
  if (loaded?.key === key) return { status: loaded.status, points: loaded.points };
  return { status: "loading", points: null };
}
