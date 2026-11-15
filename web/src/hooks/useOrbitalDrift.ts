import { useMemo } from "react";
import {
  buildDriftRows,
  demoPeriodIsFlagged,
  driftStripState,
  periodHasDriftFlag,
  type DriftRow,
  type DriftStripState,
} from "../lib/orbitalDrift";
import { useAppStore } from "../store/useAppStore";
import { useCalendarFrp } from "./useCalendarFrp";
import { useMapPoints } from "./useRegionData";

export interface OrbitalDrift extends DriftStripState {
  rows: readonly DriftRow[];
}

/**
 * Orbital Drift strip state for the map's loaded period. The medians are raw-observation facts
 * (local solar time of detections), so nothing here depends on Raw or Harmonized mode.
 */
export function useOrbitalDrift(): OrbitalDrift {
  const region = useAppStore((state) => state.region);
  const isDemo = useAppStore((state) => state.dataMode === "demo");
  const dismissed = useAppStore((state) => state.driftDismissed);
  const calendar = useCalendarFrp(region);
  const { period, overpass } = useMapPoints();

  return useMemo(() => {
    const loaded = period.year === null ? null : { year: period.year, month: period.month };
    // Demo mode has no calendar cells and its synthetic timestamps are not overpass times.
    const rows = isDemo ? [] : buildDriftRows(overpass);
    const flagged =
      loaded === null
        ? false
        : isDemo
          ? demoPeriodIsFlagged(loaded)
          : periodHasDriftFlag(calendar.values(), loaded);
    return { ...driftStripState({ flagged, rows, dismissed }), rows };
  }, [period.year, period.month, overpass, isDemo, calendar, dismissed]);
}
