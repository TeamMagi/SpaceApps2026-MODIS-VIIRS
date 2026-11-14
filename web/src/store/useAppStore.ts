import { create } from "zustand";
import { DEFAULT_REGION, readUrlState, type UrlState } from "../lib/urlState";
import type {
  ActivePhase,
  DrawerId,
  Lang,
  MonthCell,
  MonthRef,
  Phase,
  PhaseDurations,
  RegionId,
  Theme,
  ViewMode,
} from "../types";

/**
 * Resolves which month to select on the map when jumping to `year`:
 * keeps `currentMonth` if it has data in `year`, otherwise picks the month in `year`
 * with the most detections (using `harmonizedCount` only when `harmonized` is true).
 */
export function resolveMonthForYear(params: {
  year: number;
  months: readonly MonthCell[];
  currentMonth: number | null;
  harmonized: boolean;
}): MonthRef | null {
  const { year, months, currentMonth, harmonized } = params;
  const yearCells = months.filter((c) => c.year === year && c.hasData);
  if (yearCells.length === 0) return null;

  if (currentMonth !== null) {
    const matching = yearCells.find((c) => c.month === currentMonth);
    if (matching) {
      return { year, month: matching.month };
    }
  }

  let best = yearCells[0];
  let bestCount = harmonized ? best.harmonizedCount : best.rawCount;
  for (let i = 1; i < yearCells.length; i++) {
    const cell = yearCells[i];
    const count = harmonized ? cell.harmonizedCount : cell.rawCount;
    if (count > bestCount) {
      best = cell;
      bestCount = count;
    }
  }
  return { year, month: best.month };
}

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
  calendarYear: number | null;
  yearPlaying: boolean;
  openDrawer: DrawerId | null;
  presentation: boolean;
  nrtDays: number;

  setRegion: (region: RegionId) => void;
  /** The single user-facing mode action (button click, H key). */
  toggleMode: () => void;
  setLang: (lang: Lang) => void;
  setTheme: (theme: Theme) => void;
  selectMonth: (month: MonthRef | null) => void;
  /** Selects the month, or clears the selection when it is already selected. */
  toggleMonth: (month: MonthRef) => void;
  setCalendarYear: (year: number | null) => void;
  setYearPlaying: (playing: boolean) => void;
  stopYearPlayback: () => void;
  showDrawer: (id: DrawerId) => void;
  closeDrawer: () => void;
  toggleDrawer: (id: DrawerId) => void;
  setPresentation: (on: boolean) => void;
  togglePresentation: () => void;
  setNrtDays: (days: number) => void;
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
  calendarYear: initialUrl.month?.year ?? null,
  yearPlaying: false,
  openDrawer: null,
  presentation: false,
  nrtDays: 2,

  setRegion: (region) => {
    const state = get();
    if (state.region === region) return;
    set({ region, selectedMonth: null, calendarYear: null, yearPlaying: false, ...snapped(state) });
  },
  toggleMode: () => set((s) => ({ targetMode: s.targetMode === "raw" ? "harmonized" : "raw" })),
  setLang: (lang) => {
    const state = get();
    if (state.lang === lang) return;
    set({ lang, ...snapped(state) });
  },
  setTheme: (theme) => set({ theme }),
  selectMonth: (month) =>
    set((s) => ({
      selectedMonth: month,
      calendarYear: month ? month.year : s.calendarYear,
    })),
  toggleMonth: (month) =>
    set((s) => ({
      selectedMonth:
        s.selectedMonth && s.selectedMonth.year === month.year && s.selectedMonth.month === month.month ? null : month,
      calendarYear: month.year,
    })),
  setCalendarYear: (calendarYear) => set({ calendarYear }),
  setYearPlaying: (yearPlaying) => {
    if (get().yearPlaying !== yearPlaying) set({ yearPlaying });
  },
  stopYearPlayback: () => {
    if (get().yearPlaying) set({ yearPlaying: false });
  },
  showDrawer: (id) => set({ openDrawer: id }),
  closeDrawer: () => set({ openDrawer: null }),
  toggleDrawer: (id) => set((s) => ({ openDrawer: s.openDrawer === id ? null : id })),
  setPresentation: (presentation) => set({ presentation }),
  togglePresentation: () => set((s) => ({ presentation: !s.presentation })),
  setNrtDays: (days) => set({ nrtDays: Math.max(1, Math.min(5, Math.round(days))) }),
  applyUrlState: (url) => {
    const state = get();
    const nextRegion = url.region ?? state.region;
    const regionChanged = nextRegion !== state.region;
    const nextSelectedMonth =
      url.month !== undefined
        ? url.month
        : regionChanged
          ? null
          : state.selectedMonth;
    const nextCalendarYear =
      url.month !== undefined
        ? url.month
          ? url.month.year
          : regionChanged
            ? null
            : state.calendarYear
        : regionChanged
          ? null
          : state.calendarYear;
    set({
      region: nextRegion,
      lang: url.lang ?? state.lang,
      selectedMonth: nextSelectedMonth,
      calendarYear: nextCalendarYear,
      yearPlaying: regionChanged ? false : state.yearPlaying,
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
