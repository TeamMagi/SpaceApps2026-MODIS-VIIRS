/**
 * The single clock behind the Raw <-> Harmonized choreography.
 *
 * - `useHarmonizeClock()` owns the timers. Mount it ONCE (App does). It reacts to store changes:
 *   a new `targetMode` starts a run; a press during a run reverses it; any snap (region or language
 *   change, URL change) cancels pending timers and jumps to the final state.
 * - `useHarmonizeTransition(select)` is the read side for components. The store changes on every
 *   clock tick, so each component selects only the fields it draws and re-renders only when those
 *   change. Only the pipeline strip should select `litNodes` / `activeStage`.
 *
 * Beats of the first harmonize (full, 1400 ms): detect 0-400, merge 400-1000, settle 1000-1400.
 * Later runs use the same beats scaled to 500 ms; prefers-reduced-motion gives a 150 ms
 * crossfade (merge only). The current run's beats are also written to the CSS variables
 * --run-detect / --run-merge / --run-settle on <html>; they are 0ms while idle so a snap never
 * animates.
 */

import { useEffect } from "react";
import { useShallow } from "zustand/react/shallow";
import { PIPELINE_NODE_COUNT, useAppStore, type AppState, type ClockStep, type TransitionRun } from "../store/useAppStore";
import type { ActivePhase, Phase, PhaseDurations, PipelineStageKey, ViewMode } from "../types";

export const FULL_DURATIONS: PhaseDurations = { detect: 400, merge: 600, settle: 400 };
export const SHORT_DURATIONS: PhaseDurations = { detect: 143, merge: 214, settle: 143 };
export const REDUCED_DURATIONS: PhaseDurations = { detect: 0, merge: 150, settle: 0 };
const IDLE_DURATIONS: PhaseDurations = { detect: 0, merge: 0, settle: 0 };

const PHASE_ORDER: readonly ActivePhase[] = ["detect", "merge", "settle"];

export const PIPELINE_ORDER: readonly PipelineStageKey[] = ["collect", "dedup", "angle", "grid", "match", "fuse", "calendar"];
export const APPLIED_PIPELINE_ORDER: readonly PipelineStageKey[] = ["collect", "dedup", "angle", "match", "fuse", "calendar"];

export function isPlannedStage(key: PipelineStageKey): boolean {
  return key === "grid";
}

/** When each applied pipeline node lights in the full run (ms): 1-2 in detect, 3-5 across merge, 6 at settle. */
const FULL_NODE_TIMES = [0, 200, 400, 625, 825, 1000];

function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
}

interface Beat {
  phase: ActivePhase;
  start: number;
  duration: number;
}

function beatsFor(run: TransitionRun): Beat[] {
  let start = 0;
  return PHASE_ORDER.map((phase) => {
    const skipped = PHASE_ORDER.indexOf(phase) < PHASE_ORDER.indexOf(run.startPhase);
    const duration = skipped ? 0 : run.durations[phase];
    const beat = { phase, start, duration };
    start += duration;
    return beat;
  });
}

/** Maps a time on the full 1400 ms timeline onto this run's (possibly shorter or truncated) timeline. */
function mapFullTime(fullTime: number, beats: readonly Beat[]): number {
  let fullStart = 0;
  for (const beat of beats) {
    const fullDuration = FULL_DURATIONS[beat.phase];
    if (fullTime < fullStart + fullDuration) {
      return beat.start + ((fullTime - fullStart) / fullDuration) * beat.duration;
    }
    fullStart += fullDuration;
  }
  return beats[beats.length - 1].start + beats[beats.length - 1].duration;
}

function writeRunCssVars(d: PhaseDurations): void {
  if (typeof document === "undefined") return;
  const style = document.documentElement.style;
  style.setProperty("--run-detect", `${d.detect}ms`);
  style.setProperty("--run-merge", `${d.merge}ms`);
  style.setProperty("--run-settle", `${d.settle}ms`);
}

let runSeq = 0;

export function useHarmonizeClock(): void {
  useEffect(() => {
    const timers = new Set<number>();
    const clear = () => {
      for (const id of timers) window.clearTimeout(id);
      timers.clear();
    };
    const at = (ms: number, fn: () => void) => {
      const id = window.setTimeout(() => {
        timers.delete(id);
        fn();
      }, ms);
      timers.add(id);
    };

    const startRun = (target: ViewMode, startPhase: ActivePhase) => {
      const store = useAppStore.getState();
      const reduced = prefersReducedMotion();
      const full = target === "harmonized" && startPhase === "detect" && !reduced && !store.hasPlayedFullIntro;
      const durations = reduced ? REDUCED_DURATIONS : full ? FULL_DURATIONS : SHORT_DURATIONS;
      const run: TransitionRun = { id: ++runSeq, target, startPhase, durations, full };
      const beats = beatsFor(run);
      const active = beats.filter((beat) => beat.duration > 0);
      const total = beats.reduce((sum, beat) => sum + beat.duration, 0);

      // Changes due at the same moment go out as one store update (one render), e.g. the settle
      // beat and the seventh node.
      const steps = new Map<number, ClockStep>();
      const add = (ms: number, step: ClockStep) => {
        const key = Math.round(ms);
        if (key >= total) return; // clockFinish writes the final state
        steps.set(key, { ...steps.get(key), ...step });
      };

      active.slice(1).forEach((beat) => add(beat.start, { phase: beat.phase }));
      if (target === "harmonized") {
        FULL_NODE_TIMES.forEach((fullTime, index) => {
          if (index >= store.litNodes) add(mapFullTime(fullTime, beats), { litNodes: index + 1 });
        });
      } else {
        // Going back to raw, the nodes empty in reverse across the merge beat.
        const lit = store.litNodes;
        const merge = beats[1];
        for (let k = 1; k <= lit; k++) {
          add(merge.start + ((k - 1) / lit) * merge.duration, { litNodes: lit - k });
        }
      }

      const first = steps.get(0);
      steps.delete(0);
      writeRunCssVars(durations);
      store.clockBegin(run, { phase: active[0]?.phase ?? "idle", ...first });
      for (const [ms, step] of steps) at(ms, () => useAppStore.getState().clockStep(step));
      at(total, () => {
        writeRunCssVars(IDLE_DURATIONS);
        useAppStore.getState().clockFinish();
      });
    };

    writeRunCssVars(IDLE_DURATIONS);

    const unsubscribe = useAppStore.subscribe((state, prev) => {
      if (state.snapSeq !== prev.snapSeq) {
        clear();
        writeRunCssVars(IDLE_DURATIONS);
        return;
      }
      if (state.targetMode === prev.targetMode) return;
      clear();
      if (state.phase === "idle") {
        if (state.targetMode !== state.mode) startRun(state.targetMode, "detect");
        return;
      }
      // Pressed again mid-run. Nothing has moved during detect, so just cancel; later, animate back.
      if (state.phase === "detect") {
        state.snap();
        return;
      }
      startRun(state.targetMode, "merge");
    });

    return () => {
      clear();
      unsubscribe();
      const state = useAppStore.getState();
      if (state.phase !== "idle" || state.run) state.snap();
    };
  }, []);
}

export interface HarmonizeView {
  mode: ViewMode;
  targetMode: ViewMode;
  phase: Phase;
  /** Beats of the current run (all 0 when idle). Use for motion/CSS durations. */
  durations: PhaseDurations;
  /** Pipeline nodes lit, 0..7. Changes on every clock tick: select it only in the pipeline strip. */
  litNodes: number;
  /** The pipeline stage being worked on right now (for its progress line), or null. */
  activeStage: PipelineStageKey | null;
  /** Rings around double-counted points: detect + merge of a run toward harmonized. */
  showDuplicateRings: boolean;
  /** Map positions/colours and the chart line should be in their harmonized form (flips at merge). */
  geometryHarmonized: boolean;
  /**
   * Harmonized numbers, the answer headline and the receipt may be shown (flips at settle).
   * False for the whole of a run back to raw, so Raw never leaks the answer.
   */
  revealAnswer: boolean;
  toggle: () => void;
}

function viewOf(state: AppState): HarmonizeView {
  const toHarmonized = state.targetMode === "harmonized";
  const idle = state.phase === "idle";
  return {
    mode: state.mode,
    targetMode: state.targetMode,
    phase: state.phase,
    durations: state.run?.durations ?? IDLE_DURATIONS,
    litNodes: state.litNodes,
    activeStage:
      !idle && toHarmonized && state.litNodes > 0 && state.litNodes <= PIPELINE_NODE_COUNT
        ? APPLIED_PIPELINE_ORDER[state.litNodes - 1]
        : null,
    showDuplicateRings: !idle && toHarmonized && (state.phase === "detect" || state.phase === "merge"),
    geometryHarmonized: idle ? state.mode === "harmonized" : state.phase === "detect" ? !toHarmonized : toHarmonized,
    revealAnswer: idle ? state.mode === "harmonized" : state.phase === "settle" && toHarmonized,
    toggle: state.toggleMode,
  };
}

/** `useHarmonizeTransition((v) => ({ revealAnswer: v.revealAnswer }))`; the result is compared shallowly. */
export function useHarmonizeTransition<T>(select: (view: HarmonizeView) => T): T {
  return useAppStore(useShallow((state: AppState) => select(viewOf(state))));
}
