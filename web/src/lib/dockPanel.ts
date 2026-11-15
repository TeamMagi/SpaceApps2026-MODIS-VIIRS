import { useSyncExternalStore } from "react";
import { parsePanel, type PanelId } from "./dock";

/** Which insight panel is expanded. Lives outside the app store so it can be shared through the URL hash without widening the store. */
let panel: PanelId | null =
  typeof window === "undefined" ? null : parsePanel(new URLSearchParams(window.location.hash.replace(/^#/, "")).get("panel"));

const listeners = new Set<() => void>();

export function getPanel(): PanelId | null {
  return panel;
}

export function setPanel(next: PanelId | null): void {
  if (panel === next) return;
  panel = next;
  for (const listener of listeners) listener();
}

export function subscribePanel(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function usePanel(): PanelId | null {
  return useSyncExternalStore(subscribePanel, getPanel, () => null);
}
