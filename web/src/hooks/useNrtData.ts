import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";
import { t } from "../content/copy";
import {
  ApiError,
  fetchCompare,
  fetchHarmonize,
  fetchStats,
  isAbortError,
} from "../services/api";
import { useAppStore } from "../store/useAppStore";
import type {
  ApiErrorInfo,
  AsyncStatus,
  BBox,
  CompareResponse,
  HarmonizeResponse,
  MinConfidence,
  StatsResponse,
} from "../types";

// --- Reactive cache (same pattern as useRegionData) --------------------------

interface StoredError {
  readonly kind: "unreachable" | "server";
  readonly status: number | null;
  readonly detail: string;
}

interface CacheEntry<T> {
  status: AsyncStatus;
  data: T | null;
  error: StoredError | null;
}

interface NrtCacheState {
  harmonize: CacheEntry<HarmonizeResponse>;
  compare: CacheEntry<CompareResponse>;
  stats: CacheEntry<StatsResponse>;
  controller: AbortController | null;
  bboxKey: string | null;
}

const listeners = new Set<() => void>();
let snapshotVersion = 0;

function emitChange(): void {
  snapshotVersion += 1;
  for (const listener of listeners) {
    listener();
  }
}

function subscribeNrt(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function getSnapshotVersion(): number {
  return snapshotVersion;
}

function emptyEntry<T>(): CacheEntry<T> {
  return { status: "loading", data: null, error: null };
}

let nrtCache: NrtCacheState = {
  harmonize: emptyEntry(),
  compare: emptyEntry(),
  stats: emptyEntry(),
  controller: null,
  bboxKey: null,
};
let nrtToken = 0;
let subscriberCount = 0;

function bboxKey(bbox: BBox, days: number, minConfidence: MinConfidence): string {
  return `${bbox.west},${bbox.south},${bbox.east},${bbox.north}:${days}:${minConfidence}`;
}

function toStoredError(err: unknown): StoredError {
  if (err instanceof ApiError) {
    return { kind: err.kind, status: err.status, detail: err.message };
  }
  if (err instanceof Error && err.message.trim().length > 0) {
    return { kind: "server", status: null, detail: err.message.trim() };
  }
  return { kind: "unreachable", status: null, detail: "" };
}

function formatNrtError(err: StoredError | null, lang: "en" | "tr"): ApiErrorInfo | null {
  if (!err) return null;
  if (err.kind === "unreachable") {
    return { kind: "unreachable", message: t(lang, "stateErrorUnreachable") };
  }
  const message =
    err.status !== null
      ? t(lang, "stateErrorServerStatus", { status: String(err.status) })
      : err.detail || t(lang, "errorLoading");
  return { kind: "server", message };
}

function startNrtFetch(bbox: BBox, days: number, minConfidence: MinConfidence): void {
  const key = bboxKey(bbox, days, minConfidence);
  if (nrtCache.bboxKey === key && nrtCache.harmonize.status !== "error") {
    return;
  }

  nrtCache.controller?.abort();
  const token = ++nrtToken;
  const controller = new AbortController();
  const signal = controller.signal;

  const params = {
    west: bbox.west,
    south: bbox.south,
    east: bbox.east,
    north: bbox.north,
    days,
    minConfidence,
  };

  nrtCache = {
    harmonize: emptyEntry(),
    compare: emptyEntry(),
    stats: emptyEntry(),
    controller,
    bboxKey: key,
  };
  emitChange();

  const settle = <T>(
    field: "harmonize" | "compare" | "stats",
    result: T | null,
    error: StoredError | null,
  ): void => {
    if (signal.aborted || token !== nrtToken) return;
    const entry: CacheEntry<T> = {
      status: error ? "error" : result ? "ready" : "empty",
      data: result,
      error,
    };
    nrtCache = { ...nrtCache, [field]: entry };
    const allDone =
      nrtCache.harmonize.status !== "loading" &&
      nrtCache.compare.status !== "loading" &&
      nrtCache.stats.status !== "loading";
    if (allDone) {
      nrtCache = { ...nrtCache, controller: null };
    }
    emitChange();
  };

  fetchHarmonize(params, signal)
    .then((res) => settle("harmonize", res, null))
    .catch((err: unknown) => {
      if (isAbortError(err) || signal.aborted || token !== nrtToken) return;
      settle<HarmonizeResponse>("harmonize", null, toStoredError(err));
    });

  fetchCompare(params, signal)
    .then((res) => settle("compare", res, null))
    .catch((err: unknown) => {
      if (isAbortError(err) || signal.aborted || token !== nrtToken) return;
      settle<CompareResponse>("compare", null, toStoredError(err));
    });

  fetchStats(params, signal)
    .then((res) => settle("stats", res, null))
    .catch((err: unknown) => {
      if (isAbortError(err) || signal.aborted || token !== nrtToken) return;
      settle<StatsResponse>("stats", null, toStoredError(err));
    });
}

function abortNrt(): void {
  if (nrtCache.controller) {
    nrtToken += 1;
    nrtCache.controller.abort();
    nrtCache = {
      harmonize: emptyEntry(),
      compare: emptyEntry(),
      stats: emptyEntry(),
      controller: null,
      bboxKey: null,
    };
    emitChange();
  }
}

// --- Public hook -------------------------------------------------------------

export interface NrtData {
  status: AsyncStatus;
  error: ApiErrorInfo | null;
  unavailableReason: string | null;
  retry: () => void;
  harmonize: HarmonizeResponse | null;
  compare: CompareResponse | null;
  stats: StatsResponse | null;
}

export function useNrtData(bbox: BBox | null): NrtData {
  useSyncExternalStore(subscribeNrt, getSnapshotVersion, getSnapshotVersion);
  const lang = useAppStore((state) => state.lang);
  const nrtDays = useAppStore((state) => state.nrtDays);
  const dataMode = useAppStore((state) => state.dataMode);
  const isDemo = dataMode === "demo";
  const minConfidence = useAppStore((state) => state.minConfidence);

  useEffect(() => {
    if (isDemo) {
      abortNrt();
      return;
    }
    subscriberCount += 1;
    if (bbox) {
      startNrtFetch(bbox, nrtDays, minConfidence);
    } else {
      abortNrt();
    }
    return () => {
      subscriberCount -= 1;
      if (subscriberCount <= 0) {
        subscriberCount = 0;
        abortNrt();
      }
    };
  }, [isDemo, bbox?.west, bbox?.south, bbox?.east, bbox?.north, nrtDays, minConfidence]);

  const retry = useCallback(() => {
    if (isDemo) return;
    if (bbox) {
      nrtCache.controller?.abort();
      nrtCache = { ...nrtCache, bboxKey: null, controller: null };
      startNrtFetch(bbox, nrtDays, minConfidence);
    }
  }, [isDemo, bbox?.west, bbox?.south, bbox?.east, bbox?.north, nrtDays, minConfidence]);

  const { harmonize, compare, stats } = nrtCache;

  const mergedStatus: AsyncStatus = useMemo(() => {
    if (isDemo) return "empty";
    if (!bbox) return "loading";
    const statuses = [harmonize.status, compare.status, stats.status];
    if (statuses.some((s) => s === "error")) return "error";
    if (statuses.some((s) => s === "loading")) return "loading";
    if (statuses.every((s) => s === "empty")) return "empty";
    return "ready";
  }, [isDemo, bbox, harmonize.status, compare.status, stats.status]);

  const rawError =
    harmonize.error ?? compare.error ?? stats.error ?? null;
  const error = useMemo(() => (isDemo ? null : formatNrtError(rawError, lang)), [isDemo, rawError, lang]);
  const unavailableReason = isDemo ? t(lang, "nrtDemoUnavailable") : null;

  return useMemo<NrtData>(
    () => ({
      status: mergedStatus,
      error,
      unavailableReason,
      retry,
      harmonize: isDemo ? null : harmonize.data,
      compare: isDemo ? null : compare.data,
      stats: isDemo ? null : stats.data,
    }),
    [mergedStatus, error, unavailableReason, retry, isDemo, harmonize.data, compare.data, stats.data],
  );
}
