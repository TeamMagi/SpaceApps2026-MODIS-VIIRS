import { create } from "zustand";
import { DEFAULT_REGION, readUrlState, type UrlState } from "../lib/urlState";
import type {
  ActivePhase,
  DrawerId,
  Lang,
  MonthRef,
  Phase,
  PhaseDurations,
  RegionId,
  Theme,
  ViewMode,
} from "../types";

/** One Raw <-> Harmonized run, written by the transition clock (hooks/useHarmonizeTransition). */
export interface TransitionRun {
  id: number;
  target: ViewMode;
  /** `detect` for a fresh run; `merge` when a press reversed a run already past detect. */
  startPhase: ActivePhase;
  durations: PhaseDurations;
  /** True only for the first full 1400 ms harmonize choreography of the session. */
  full: boolean;
}

export const PIPELINE_NODE_COUNT = 6;

/** One tick of the transition clock: whatever changes at that moment, applied in one update. */
export interface ClockStep {
  phase?: Phase;
  litNodes?: number;
}

export interface AppState {
  region: RegionId;
  /** Committed mode: changes only when a transition finishes (or on a snap). */
  mode: ViewMode;
  /** Requested mode: flips synchronously on toggle so the switch responds within one frame. */
  targetMode: ViewMode;
  phase: Phase;
  /** Whether the full 1400 ms choreography has completed once; later runs use the short version. */
  hasPlayedFullIntro: boolean;
  /** Applied pipeline nodes lit, 0..6. Written by the clock. */
  litNodes: number;
  run: TransitionRun | null;
  /** Increments on every snap; tells the clock to drop pending timers. */
  snapSeq: number;

  lang: Lang;
  theme: Theme;
  selectedMonth: MonthRef | null;
  openDrawer: DrawerId | null;
  presentation: boolean;

  setRegion: (region: RegionId) => void;
  /** The single user-facing mode action (button click, H key). */
  toggleMode: () => void;
  setLang: (lang: Lang) => void;
  setTheme: (theme: Theme) => void;
  selectMonth: (month: MonthRef | null) => void;
  /** Selects the month, or clears the selection when it is already selected. */
  toggleMonth: (month: MonthRef) => void;
  showDrawer: (id: DrawerId) => void;
  closeDrawer: () => void;
  toggleDrawer: (id: DrawerId) => void;
  setPresentation: (on: boolean) => void;
  togglePresentation: () => void;
  /** Applies a parsed URL hash; mode changes snap without animation. */
  applyUrlState: (url: Partial<UrlState>) => void;
  /** Jumps to the final state of the current target, cancelling any run. */
  snap: () => void;

  // Transition clock only. Components must not call these.
  clockBegin: (run: TransitionRun, step: ClockStep) => void;
  clockStep: (step: ClockStep) => void;
  clockFinish: () => void;
}

const THEME_KEY = "pyrosync.theme";

function readStoredTheme(): Theme {
  try {
    const value = window.localStorage.getItem(THEME_KEY);
    return value === "light" || value === "dark" ? value : "dark";
  } catch {
    return "dark";
  }
}

export function storeTheme(theme: Theme): void {
  try {
    window.localStorage.setItem(THEME_KEY, theme);
  } catch {
    // Storage can be unavailable (private mode); the theme then lasts for this visit only.
  }
}

function litFor(mode: ViewMode): number {
  return mode === "harmonized" ? PIPELINE_NODE_COUNT : 0;
}

function snapped(state: Pick<AppState, "targetMode" | "snapSeq">, targetMode = state.targetMode) {
  return {
    targetMode,
    mode: targetMode,
    phase: "idle" as Phase,
    run: null,
    litNodes: litFor(targetMode),
    snapSeq: state.snapSeq + 1,
  };
}

const initialUrl = readUrlState();
const initialMode: ViewMode = initialUrl.mode ?? "raw";

export const useAppStore = create<AppState>()((set, get) => ({
  region: initialUrl.region ?? DEFAULT_REGION,
  mode: initialMode,
  targetMode: initialMode,
  phase: "idle",
  hasPlayedFullIntro: false,
  litNodes: litFor(initialMode),
  run: null,
  snapSeq: 0,

  lang: initialUrl.lang ?? "en",
  theme: typeof window === "undefined" ? "dark" : readStoredTheme(),
  selectedMonth: initialUrl.month ?? null,
  openDrawer: null,
  presentation: false,

  setRegion: (region) => {
    const state = get();
    if (state.region === region) return;
    set({ region, selectedMonth: null, ...snapped(state) });
  },
  toggleMode: () => set((s) => ({ targetMode: s.targetMode === "raw" ? "harmonized" : "raw" })),
  setLang: (lang) => {
    const state = get();
    if (state.lang === lang) return;
    set({ lang, ...snapped(state) });
  },
  setTheme: (theme) => set({ theme }),
  selectMonth: (month) => set({ selectedMonth: month }),
  toggleMonth: (month) =>
    set((s) => ({
      selectedMonth:
        s.selectedMonth && s.selectedMonth.year === month.year && s.selectedMonth.month === month.month ? null : month,
    })),
  showDrawer: (id) => set({ openDrawer: id }),
  closeDrawer: () => set({ openDrawer: null }),
  toggleDrawer: (id) => set((s) => ({ openDrawer: s.openDrawer === id ? null : id })),
  setPresentation: (presentation) => set({ presentation }),
  togglePresentation: () => set((s) => ({ presentation: !s.presentation })),
  applyUrlState: (url) => {
    const state = get();
    const nextRegion = url.region ?? state.region;
    const regionChanged = nextRegion !== state.region;
    set({
      region: nextRegion,
      lang: url.lang ?? state.lang,
      selectedMonth:
        url.month !== undefined
          ? url.month
          : regionChanged
            ? null
            : state.selectedMonth,
      ...snapped(state, url.mode ?? state.targetMode),
    });
  },
  snap: () => set((s) => snapped(s)),

  clockBegin: (run, step) => set({ run, ...step }),
  clockStep: (step) => set(step),
  clockFinish: () =>
    set((s) => {
      if (!s.run) return {};
      return {
        mode: s.run.target,
        phase: "idle",
        litNodes: litFor(s.run.target),
        run: null,
        hasPlayedFullIntro: s.hasPlayedFullIntro || s.run.full,
      };
    }),
}));
