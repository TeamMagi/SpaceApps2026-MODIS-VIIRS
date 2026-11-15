import { useCallback, useSyncExternalStore } from "react";
import { DEFAULT_PIPELINE_CONFIG } from "../lib/pipelineOverrides";
import { parseRoute, type Route } from "../lib/route";
import { getPanel } from "../lib/dockPanel";
import { serializeHash } from "../lib/urlState";
import { useAppStore } from "../store/useAppStore";

function subscribe(onChange: () => void): () => void {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

const getHash = () => window.location.hash;
const getServerHash = () => "";

/**
 * Hash routing (`#/methods[/section]`) instead of pathname routing: the backend has no working
 * SPA fallback (K9), so a path like /methods would 404 on reload or when shared.
 */
export function useRoute(): Route {
  const hash = useSyncExternalStore(subscribe, getHash, getServerHash);
  return parseRoute(hash);
}

/** The dashboard hash rebuilt from the store (region, mode, month, language, palette, data mode, overrides, confidence). */
export function dashboardHashFromStore(): string {
  const s = useAppStore.getState();
  return serializeHash(
    {
      region: s.region,
      panel: getPanel(),
      mode: s.mode,
      month: s.selectedMonth,
      lang: s.lang,
      palette: s.palette,
      data: s.dataMode === "demo" && s.dataModeSource === "manual" ? "demo" : undefined,
      overrides: s.pipelineOverrides,
      minConfidence: s.minConfidence,
      fusionModel: s.fusionModel,
    },
    s.serverConfig ?? DEFAULT_PIPELINE_CONFIG,
  );
}

/**
 * Returns to the dashboard by rebuilding its hash from the store rather than calling
 * history.back(): the store kept region, mode, month and any language change made on the
 * methods page, while an older history entry would revert that language.
 */
export function useLeaveMethods(): () => void {
  return useCallback(() => {
    window.location.hash = dashboardHashFromStore();
  }, []);
}
