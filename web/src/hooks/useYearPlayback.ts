import { useCallback, useEffect, useState } from "react";
import { resolvePlaybackStep, resolvePlaybackToggle } from "../lib/radialCalendar";

const PLAYBACK_STEP_MS = 650;
const PLAYBACK_STEP_REDUCED_MS = 950;

function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
}

export interface UseYearPlaybackOptions {
  readonly activeYear: number | null;
  readonly firstYear: number | null;
  readonly lastYear: number | null;
  readonly enabled: boolean;
  readonly onYearChange: (year: number) => void;
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
  const [isPlaying, setIsPlaying] = useState(false);

  const canPlay = enabled && firstYear !== null && lastYear !== null && firstYear < lastYear;

  const stop = useCallback(() => {
    setIsPlaying(false);
  }, []);

  const togglePlay = useCallback(() => {
    const { nextPlaying, restartYear } = resolvePlaybackToggle({ isPlaying, canPlay, activeYear, firstYear, lastYear });
    if (restartYear !== null) onYearChange(restartYear);
    setIsPlaying(nextPlaying);
  }, [canPlay, isPlaying, firstYear, lastYear, activeYear, onYearChange]);

  useEffect(() => {
    if (!canPlay && isPlaying) {
      setIsPlaying(false);
    }
  }, [canPlay, isPlaying]);

  useEffect(() => {
    if (!isPlaying || !canPlay || firstYear === null || lastYear === null) return;
    if (activeYear !== null && activeYear >= lastYear) {
      setIsPlaying(false);
      return;
    }

    const delay = prefersReducedMotion() ? PLAYBACK_STEP_REDUCED_MS : PLAYBACK_STEP_MS;
    const timerId = window.setTimeout(() => {
      const { nextYear, finished } = resolvePlaybackStep(activeYear, firstYear, lastYear);
      onYearChange(nextYear);
      if (finished) {
        setIsPlaying(false);
      }
    }, delay);

    return () => {
      window.clearTimeout(timerId);
    };
  }, [isPlaying, canPlay, activeYear, firstYear, lastYear, onYearChange]);

  return { isPlaying, stop, togglePlay };
}
