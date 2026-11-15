import { create } from "zustand";
import {
  areOverridesEqual,
  DEFAULT_PIPELINE_CONFIG,
  isDefaultOverrideValue,
  normalizeOverrides,
} from "../lib/pipelineOverrides";
import { DEFAULT_REGION, readUrlState, type UrlState } from "../lib/urlState";
import { DEFAULT_FUSION_MODEL, type FusionModel } from "../types";
import type {
  ActivePhase,
  DataMode,
  DataModeSource,
  DrawerId,
  Lang,
  MinConfidence,
  MonthCell,
  MonthRef,
  Palette,
  Phase,
  PhaseDurations,
  PipelineConfig,
  PipelineOverrideKey,
  PipelineOverrides,
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
  palette: Palette;
  dataMode: DataMode;
  dataModeSource: DataModeSource;
  selectedMonth: MonthRef | null;
  calendarYear: number | null;
  yearPlaying: boolean;
  openDrawer: DrawerId | null;
  presentation: boolean;
  nrtDays: number;
  serverConfig: PipelineConfig | null;
  pipelineOverrides: PipelineOverrides;
  /** Lowest FIRMS confidence class sent as `min_confidence`; `low` keeps every detection. */
  minConfidence: MinConfidence;
  /** Harmonization model sent as `fusion_model`; `viirs_preferred` is the standard one. */
  fusionModel: FusionModel;
  /** Index into TOUR_STEPS while the guided tour runs; null when no tour is active. */
  tourStep: number | null;
  tourPaused: boolean;
  /** Increments on every startTour, so restarting while on step 1 re-runs the step. */
  tourRun: number;

  setRegion: (region: RegionId) => void;
  /** The single user-facing mode action (button click, H key). */
  toggleMode: () => void;
  setLang: (lang: Lang) => void;
  setTheme: (theme: Theme) => void;
  setPalette: (palette: Palette) => void;
  setDataMode: (mode: DataMode, source: DataModeSource) => void;
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
  setServerConfig: (config: PipelineConfig) => void;
  setPipelineOverride: (key: PipelineOverrideKey, value: number | undefined) => void;
  resetPipelineOverrides: () => void;
  setMinConfidence: (minConfidence: MinConfidence) => void;
  setFusionModel: (model: FusionModel) => void;
  /** Starts (or restarts) the guided tour at step 1, unpaused, with no drawer open. */
  startTour: () => void;
  setTourStep: (step: number) => void;
  endTour: () => void;
  setTourPaused: (paused: boolean) => void;
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
const PALETTE_KEY = "pyrosync.palette";

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

function readStoredPalette(): Palette {
  try {
    const value = window.localStorage.getItem(PALETTE_KEY);
    return value === "default" || value === "viridis" || value === "cividis" ? value : "default";
  } catch {
    return "default";
  }
}

export function storePalette(palette: Palette): void {
  try {
    window.localStorage.setItem(PALETTE_KEY, palette);
  } catch {
    // Storage can be unavailable (private mode); the palette then lasts for this visit only.
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
const initialDataMode: DataMode = initialUrl.data === "demo" ? "demo" : "live";
const initialDataModeSource: DataModeSource = initialUrl.data === "demo" ? "manual" : "auto";

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
  palette: initialUrl.palette ?? (typeof window === "undefined" ? "default" : readStoredPalette()),
  dataMode: initialDataMode,
  dataModeSource: initialDataModeSource,
  selectedMonth: initialUrl.month ?? null,
  calendarYear: initialUrl.month?.year ?? null,
  yearPlaying: false,
  openDrawer: null,
  presentation: false,
  nrtDays: 2,
  serverConfig: null,
  pipelineOverrides: initialUrl.overrides ? { ...initialUrl.overrides } : {},
  minConfidence: initialUrl.minConfidence ?? "low",
  fusionModel: initialUrl.fusionModel ?? DEFAULT_FUSION_MODEL,
  tourStep: null,
  tourPaused: false,
  tourRun: 0,

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
  setPalette: (palette) => set({ palette }),
  setDataMode: (dataMode, dataModeSource) =>
    set((s) => (s.dataMode === dataMode && s.dataModeSource === dataModeSource ? s : { dataMode, dataModeSource })),
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
  setServerConfig: (config) =>
    set((s) => {
      const nextOverrides = normalizeOverrides(s.pipelineOverrides, config);
      return {
        serverConfig: config,
        pipelineOverrides: areOverridesEqual(s.pipelineOverrides, nextOverrides)
          ? s.pipelineOverrides
          : nextOverrides,
      };
    }),
  setPipelineOverride: (key, value) =>
    set((s) => {
      const defaults = s.serverConfig ?? DEFAULT_PIPELINE_CONFIG;
      const next: PipelineOverrides = { ...s.pipelineOverrides };
      if (value === undefined || !Number.isFinite(value) || isDefaultOverrideValue(key, value, defaults)) {
        delete next[key];
      } else {
        next[key] = value;
      }
      return areOverridesEqual(s.pipelineOverrides, next) ? s : { pipelineOverrides: next };
    }),
  resetPipelineOverrides: () =>
    set((s) => (Object.keys(s.pipelineOverrides).length === 0 ? s : { pipelineOverrides: {} })),
  setMinConfidence: (minConfidence) => {
    if (get().minConfidence !== minConfidence) set({ minConfidence });
  },
  setFusionModel: (model) => {
    if (get().fusionModel !== model) set({ fusionModel: model });
  },
  startTour: () => set((s) => ({ tourStep: 0, tourPaused: false, openDrawer: null, tourRun: s.tourRun + 1 })),
  setTourStep: (tourStep) => set({ tourStep }),
  endTour: () => set({ tourStep: null, tourPaused: false }),
  setTourPaused: (tourPaused) => set({ tourPaused }),
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
    const nextDataMode: DataMode =
      url.data === "demo"
        ? "demo"
        : url.data === "live"
          ? "live"
          : state.dataMode === "demo" && state.dataModeSource === "manual"
            ? "live"
            : state.dataMode;
    const nextDataModeSource: DataModeSource =
      url.data === "demo"
        ? "manual"
        : url.data === "live" || (url.data === undefined && state.dataMode === "demo" && state.dataModeSource === "manual")
          ? "auto"
          : state.dataModeSource;
    const defaults = state.serverConfig ?? DEFAULT_PIPELINE_CONFIG;
    const nextOverrides = normalizeOverrides(url.overrides, defaults);
    set({
      region: nextRegion,
      lang: url.lang ?? state.lang,
      palette: url.palette ?? state.palette,
      dataMode: nextDataMode,
      dataModeSource: nextDataModeSource,
      minConfidence: url.minConfidence ?? state.minConfidence,
      fusionModel: url.fusionModel ?? state.fusionModel,
      selectedMonth: nextSelectedMonth,
      calendarYear: nextCalendarYear,
      yearPlaying: regionChanged ? false : state.yearPlaying,
      pipelineOverrides: areOverridesEqual(state.pipelineOverrides, nextOverrides)
        ? state.pipelineOverrides
        : nextOverrides,
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
