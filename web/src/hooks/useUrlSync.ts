import { useEffect } from "react";
import { isRouteHash } from "../lib/route";
import { parseHash, serializeHash, syncUrlHash, type UrlState } from "../lib/urlState";
import { useAppStore, type AppState } from "../store/useAppStore";

function urlStateOf(state: AppState): UrlState {
  return { region: state.region, mode: state.mode, month: state.selectedMonth, lang: state.lang };
}

/**
 * Keeps region, committed mode, selected month and language in `location.hash` so any view is a
 * shareable link. The store reads the hash once at load; this hook writes on change
 * (replaceState, so toggling does not flood the back button) and applies manual hash edits.
 * Mount once (App does).
 */
export function useUrlSync(): void {
  useEffect(() => {
    const write = (state: AppState) => syncUrlHash(urlStateOf(state));

    write(useAppStore.getState());

    const unsubscribe = useAppStore.subscribe((state, prev) => {
      if (
        state.region !== prev.region ||
        state.mode !== prev.mode ||
        state.selectedMonth !== prev.selectedMonth ||
        state.lang !== prev.lang
      ) {
        // Read fresh state: the transition clock sets state from inside listeners, so `state`
        // may already be stale by the time this listener runs.
        write(useAppStore.getState());
      }
    });

    const onHashChange = () => {
      if (isRouteHash(window.location.hash)) return;
      const state = useAppStore.getState();
      if (window.location.hash === serializeHash(urlStateOf(state))) return;
      state.applyUrlState(parseHash(window.location.hash));
      write(useAppStore.getState());
    };
    window.addEventListener("hashchange", onHashChange);

    return () => {
      unsubscribe();
      window.removeEventListener("hashchange", onHashChange);
    };
  }, []);
}
