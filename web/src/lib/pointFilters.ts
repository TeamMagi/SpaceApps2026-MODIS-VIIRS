import { useSyncExternalStore } from "react";
import { DayNight } from "../types";

export type DayNightFilter = "all" | "day" | "night";

export interface PointFilters {
  /** Points with FRP (MW) below this are hidden; 0 shows all. */
  readonly minFrp: number;
  readonly dayNight: DayNightFilter;
}

export const DEFAULT_POINT_FILTERS: PointFilters = { minFrp: 0, dayNight: "all" };

export const MIN_FRP_STEPS: readonly number[] = [0, 5, 10, 25, 50, 100];

let current: PointFilters = DEFAULT_POINT_FILTERS;

const listeners = new Set<() => void>();

export function getPointFilters(): PointFilters {
  return current;
}

export function setPointFilters(patch: Partial<PointFilters>): void {
  const next: PointFilters = {
    minFrp: patch.minFrp ?? current.minFrp,
    dayNight: patch.dayNight ?? current.dayNight,
  };
  if (next.minFrp === current.minFrp && next.dayNight === current.dayNight) return;
  current = next;
  for (const listener of listeners) listener();
}

export function resetPointFilters(): void {
  if (current === DEFAULT_POINT_FILTERS) return;
  current = DEFAULT_POINT_FILTERS;
  for (const listener of listeners) listener();
}

export function subscribePointFilters(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function usePointFilters(): PointFilters {
  return useSyncExternalStore(subscribePointFilters, getPointFilters, () => DEFAULT_POINT_FILTERS);
}

/** Returns the input array itself when no filter is active (no copy). */
export function filterPoints<T extends { readonly frp: number; readonly daynight: DayNight }>(
  points: readonly T[],
  filters: PointFilters,
): readonly T[] {
  const { minFrp, dayNight } = filters;
  if (minFrp <= 0 && dayNight === "all") return points;
  const wanted = dayNight === "day" ? DayNight.Day : DayNight.Night;
  const out: T[] = [];
  for (const point of points) {
    if (point.frp < minFrp) continue;
    if (dayNight !== "all" && point.daynight !== wanted) continue;
    out.push(point);
  }
  return out;
}

/** minFrp > 0 and dayNight !== "all" each count as one. */
export function activeFilterCount(filters: PointFilters): number {
  return (filters.minFrp > 0 ? 1 : 0) + (filters.dayNight !== "all" ? 1 : 0);
}
