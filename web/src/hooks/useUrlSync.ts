import { useEffect } from "react";
import { getPanel, setPanel, subscribePanel } from "../lib/dockPanel";
import { DEFAULT_PIPELINE_CONFIG } from "../lib/pipelineOverrides";
import { isRouteHash } from "../lib/route";
import { parseHash, serializeHash, syncUrlHash, type UrlState } from "../lib/urlState";
import { useAppStore, type AppState } from "../store/useAppStore";

function urlStateOf(state: AppState): UrlState {
  return {
    region: state.region,
    panel: getPanel(),
    mode: state.mode,
    month: state.selectedMonth,
    lang: state.lang,
    palette: state.palette,
    data: state.dataMode === "demo" && state.dataModeSource === "manual" ? "demo" : undefined,
    overrides: state.pipelineOverrides,
    minConfidence: state.minConfidence,
    fusionModel: state.fusionModel,
  };
}

/**
 * Keeps region, committed mode, selected month, language, palette, pipeline overrides, harmonization
 * model, manual demo flag and the confidence filter in `location.hash` so any view is a shareable link. The
 * store reads the hash once at load; this hook writes on change (replaceState, so toggling does
 * not flood the back button) and applies manual hash edits. Mount once (App does).
 */
export function useUrlSync(): void {
  useEffect(() => {
    const write = (state: AppState) =>
      syncUrlHash(urlStateOf(state), state.serverConfig ?? DEFAULT_PIPELINE_CONFIG);

    write(useAppStore.getState());

    const unsubscribe = useAppStore.subscribe((state, prev) => {
      if (
        state.region !== prev.region ||
        state.mode !== prev.mode ||
        state.selectedMonth !== prev.selectedMonth ||
        state.lang !== prev.lang ||
        state.palette !== prev.palette ||
        state.dataMode !== prev.dataMode ||
        state.dataModeSource !== prev.dataModeSource ||
        state.pipelineOverrides !== prev.pipelineOverrides ||
        state.serverConfig !== prev.serverConfig ||
        state.minConfidence !== prev.minConfidence ||
        state.fusionModel !== prev.fusionModel
      ) {
        // Read fresh state: the transition clock sets state from inside listeners, so `state`
        // may already be stale by the time this listener runs.
        write(useAppStore.getState());
      }
    });

    const unsubscribePanel = subscribePanel(() => write(useAppStore.getState()));

    const onHashChange = () => {
      if (isRouteHash(window.location.hash)) return;
      const state = useAppStore.getState();
      const defaults = state.serverConfig ?? DEFAULT_PIPELINE_CONFIG;
      if (window.location.hash === serializeHash(urlStateOf(state), defaults)) return;
      const parsed = parseHash(window.location.hash);
      setPanel(parsed.panel ?? null);
      state.applyUrlState(parsed);
      write(useAppStore.getState());
    };
    window.addEventListener("hashchange", onHashChange);

    return () => {
      unsubscribe();
      unsubscribePanel();
      window.removeEventListener("hashchange", onHashChange);
    };
  }, []);
}
