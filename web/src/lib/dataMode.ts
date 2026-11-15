import type { ApiErrorKind, DataMode, DataModeSource } from "../types";

export interface DataModeState {
  readonly dataMode: DataMode;
  readonly dataModeSource: DataModeSource;
}

export type ProbeOrFetchOutcome =
  | { readonly source: "probe"; readonly ok: true; readonly forceLive?: boolean }
  | { readonly source: "probe"; readonly ok: false; readonly status: number | null; readonly forceLive?: boolean }
  | { readonly source: "fetch"; readonly ok: true }
  | { readonly source: "fetch"; readonly ok: false; readonly kind: ApiErrorKind; readonly status: number | null };

/**
 * Maps a health-probe or API-fetch outcome to the next `(dataMode, dataModeSource)`.
 * Manual demo stays pinned unless `forceLive` is set by an explicit user action.
 */
export function decideNextDataMode(
  current: DataModeState,
  outcome: ProbeOrFetchOutcome,
): DataModeState {
  if (outcome.source === "probe") {
    const manualDemoPinned =
      current.dataMode === "demo" && current.dataModeSource === "manual" && !outcome.forceLive;

    if (outcome.ok) {
      return manualDemoPinned ? current : { dataMode: "live", dataModeSource: "auto" };
    }

    if (outcome.status === null || outcome.status >= 500) {
      return manualDemoPinned ? current : { dataMode: "demo", dataModeSource: "auto" };
    }

    return current;
  }

  if (outcome.ok || current.dataMode !== "live") {
    return current;
  }

  if (outcome.kind === "unreachable") {
    return { dataMode: "demo", dataModeSource: "auto" };
  }

  return current;
}
