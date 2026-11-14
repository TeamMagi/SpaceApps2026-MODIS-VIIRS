import { useCallback, useEffect } from "react";
import { resolvePlaybackStep, resolvePlaybackToggle } from "../lib/radialCalendar";
import { useAppStore } from "../store/useAppStore";

const PLAYBACK_STEP_MS = 650;
const PLAYBACK_STEP_REDUCED_MS = 950;

function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
}

export interface UseYearPlaybackOptions {
  readonly activeYear?: number | null;
  readonly firstYear: number | null;
  readonly lastYear: number | null;
  readonly enabled: boolean;
  readonly onYearChange?: (year: number) => void;
}

export interface UseYearPlaybackResult {
  readonly isPlaying: boolean;
  readonly stop: () => void;
  readonly togglePlay: () => void;
}

/**
 * Steps through archive years on user request only (never auto-starts).
 * Cleans up its timer on pause, region/bounds change, reaching `lastYear`, or unmount.
 */
export function useYearPlayback({
  activeYear,
  firstYear,
  lastYear,
  enabled,
  onYearChange,
}: UseYearPlaybackOptions): UseYearPlaybackResult {
  const isPlaying = useAppStore((state) => state.yearPlaying);
  const setYearPlaying = useAppStore((state) => state.setYearPlaying);
  const stopYearPlayback = useAppStore((state) => state.stopYearPlayback);
  const calendarYear = useAppStore((state) => state.calendarYear);
  const setCalendarYear = useAppStore((state) => state.setCalendarYear);
  const currentYear = activeYear !== undefined ? activeYear : calendarYear;

  const canPlay = enabled && firstYear !== null && lastYear !== null && firstYear < lastYear;

  const commitYear = useCallback(
    (year: number) => {
      setCalendarYear(year);
      onYearChange?.(year);
    },
    [setCalendarYear, onYearChange],
  );

  const stop = useCallback(() => {
    stopYearPlayback();
  }, [stopYearPlayback]);

  const togglePlay = useCallback(() => {
    const { nextPlaying, restartYear } = resolvePlaybackToggle({
      isPlaying,
      canPlay,
      activeYear: currentYear,
      firstYear,
      lastYear,
    });
    if (restartYear !== null) commitYear(restartYear);
    setYearPlaying(nextPlaying);
  }, [canPlay, isPlaying, firstYear, lastYear, currentYear, commitYear, setYearPlaying]);

  useEffect(() => {
    if (!canPlay && isPlaying) {
      stopYearPlayback();
    }
  }, [canPlay, isPlaying, stopYearPlayback]);

  useEffect(() => () => stopYearPlayback(), [stopYearPlayback]);

  useEffect(() => {
    if (!isPlaying || !canPlay || firstYear === null || lastYear === null) return;
    if (currentYear !== null && currentYear >= lastYear) {
      stopYearPlayback();
      return;
    }

    const delay = prefersReducedMotion() ? PLAYBACK_STEP_REDUCED_MS : PLAYBACK_STEP_MS;
    const timerId = window.setTimeout(() => {
      const { nextYear, finished } = resolvePlaybackStep(currentYear, firstYear, lastYear);
      commitYear(nextYear);
      if (finished) {
        stopYearPlayback();
      }
    }, delay);

    return () => {
      window.clearTimeout(timerId);
    };
  }, [isPlaying, canPlay, currentYear, firstYear, lastYear, commitYear, stopYearPlayback]);

  return { isPlaying, stop, togglePlay };
}
