import { useCallback, useSyncExternalStore } from "react";
import { parseRoute, type Route } from "../lib/route";
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

/**
 * Returns to the dashboard by rebuilding its hash from the store rather than calling
 * history.back(): the store kept region, mode, month and any language change made on the
 * methods page, while an older history entry would revert that language.
 */
export function useLeaveMethods(): () => void {
  return useCallback(() => {
    const { region, mode, selectedMonth, lang, minConfidence } = useAppStore.getState();
    window.location.hash = serializeHash({ region, mode, month: selectedMonth, lang, minConfidence });
  }, []);
}
