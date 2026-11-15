import { useSyncExternalStore } from "react";

/** "simple": three summary cards that expand one at a time. "detailed": the full pipeline, chart and calendar column. */
export type DockMode = "simple" | "detailed";

const STORAGE_KEY = "pyrosync.dock";

function readStored(): DockMode {
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "detailed" ? "detailed" : "simple";
  } catch {
    return "simple";
  }
}

let mode: DockMode = typeof window === "undefined" ? "simple" : readStored();

const listeners = new Set<() => void>();

export function getDockMode(): DockMode {
  return mode;
}

export function setDockMode(next: DockMode): void {
  if (mode === next) return;
  mode = next;
  try {
    window.localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // Private windows may refuse storage; the choice still holds for this page load.
  }
  for (const listener of listeners) listener();
}

export function subscribeDockMode(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useDockMode(): DockMode {
  return useSyncExternalStore(subscribeDockMode, getDockMode, () => "simple");
}
