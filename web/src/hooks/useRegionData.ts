import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";
import { formatRegionName, t } from "../content/copy";
import {
  DEFAULT_REGION,
  getExplicitHashRegion,
  readRawHashRegion,
  setValidRegionIds,
  setValidYearRange,
  syncUrlHash,
} from "../lib/urlState";
import {
  ApiError,
  type ApiErrorCode,
  fetchPoints,
  fetchRegions,
  fetchSeries,
  isAbortError,
  type PointsResponse,
  type RegionsResponse,
  type SeriesResponse,
} from "../services/api";
import { useAppStore } from "../store/useAppStore";
import type {
  ApiErrorInfo,
  AsyncStatus,
  BriefingMetrics,
  ConfidenceFilterInfo,
  Lang,
  MinConfidence,
  MapPointSet,
  MonthCell,
  MonthRef,
  PipelineStage,
  PointStages,
  Region,
  RegionData,
  RegionSummary,
  RegionTotals,
  SeriesCoverage,
} from "../types";

// --- Shared reactive cache ---------------------------------------------------

interface StoredError {
  readonly kind: "unreachable" | "server";
  readonly code: ApiErrorCode;
  readonly status: number | null;
  readonly detail: string;
}

interface CacheEntry<T> {
  status: AsyncStatus;
  data: T | null;
  error: StoredError | null;
  controller: AbortController | null;
}

const listeners = new Set<() => void>();
let snapshotVersion = 0;

function emitChange(): void {
  snapshotVersion += 1;
  for (const listener of listeners) {
    listener();
  }
}

function subscribeCache(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function getSnapshotVersion(): number {
  return snapshotVersion;
}

let regionsEntry: CacheEntry<RegionsResponse> = {
  status: "loading",
  data: null,
  error: null,
  controller: null,
};
let regionsRequested = false;
let regionsToken = 0;
let regionsSubscribers = 0;

const MAX_SERIES_CACHE = 24;
const MAX_POINTS_CACHE = 24;

const seriesCache = new Map<string, CacheEntry<SeriesResponse>>();
let activeSeriesRegion: string | null = null;
let seriesToken = 0;
const seriesSubscribers = new Map<string, number>();

const pointsCache = new Map<string, CacheEntry<PointsResponse>>();
let activePointsKey: string | null = null;
let pointsToken = 0;
const pointsSubscribers = new Map<string, number>();

function touchCacheEntry<T>(cache: Map<string, CacheEntry<T>>, key: string, entry: CacheEntry<T>): void {
  cache.delete(key);
  cache.set(key, entry);
}

function evictCacheLru<T>(
  cache: Map<string, CacheEntry<T>>,
  subscribers: ReadonlyMap<string, number>,
  maxReady: number,
): void {
  let readyCount = 0;
  for (const entry of cache.values()) {
    if (entry.status === "ready") {
      readyCount += 1;
    }
  }
  if (readyCount <= maxReady) return;

  for (const [key, entry] of cache.entries()) {
    if (readyCount <= maxReady) break;
    if (entry.status !== "ready" || entry.controller !== null || (subscribers.get(key) ?? 0) > 0) {
      continue;
    }
    cache.delete(key);
    readyCount -= 1;
  }
}

function toStoredError(err: unknown): StoredError {
  if (err instanceof ApiError) {
    return { kind: err.kind, code: err.code, status: err.status, detail: err.message };
  }
  if (err instanceof Error && err.message.trim().length > 0) {
    return { kind: "server", code: "server", status: null, detail: err.message.trim() };
  }
  return { kind: "unreachable", code: "unreachable", status: null, detail: "" };
}

function formatStoredError(err: StoredError | null, lang: Lang): ApiErrorInfo | null {
  if (!err) return null;
  switch (err.code) {
    case "unreachable":
      return { kind: "unreachable", message: t(lang, "stateErrorUnreachable") };
    case "region_too_large":
      return { kind: "server", message: t(lang, "stateErrorRegionTooLarge") };
    case "invalid_response":
      return { kind: "server", message: t(lang, "stateErrorInvalidResponse") };
    case "server": {
      const message =
        err.status !== null
          ? t(lang, "stateErrorServerStatus", { status: String(err.status) })
          : err.detail || t(lang, "errorLoading");
      return { kind: "server", message };
    }
  }
}

function reconcileRegionWithList(regions: readonly RegionSummary[]): void {
  if (regions.length === 0) return;
  const selectable = regions.filter((r) => !r.tooLarge);
  const pool = selectable.length > 0 ? selectable : regions;
  const validIds = pool.map((r) => r.id);
  setValidRegionIds(validIds);

  const defaultRegion = pool.find((r) => r.id === DEFAULT_REGION) ?? pool[0];
  const defaultId = defaultRegion.id;

  const explicitRegion = getExplicitHashRegion();
  const current = useAppStore.getState().region;

  if (explicitRegion && validIds.includes(explicitRegion)) {
    if (current !== explicitRegion) {
      useAppStore.getState().setRegion(explicitRegion);
    }
    return;
  }

  const currentSummary = regions.find((r) => r.id === current);
  const currentIsValid = Boolean(currentSummary && (!currentSummary.tooLarge || selectable.length === 0));
  if (!currentIsValid) {
    useAppStore.getState().setRegion(defaultId);
  }

  const rawHashRegion = typeof window === "undefined" ? null : readRawHashRegion(window.location.hash);
  if (rawHashRegion && !validIds.includes(rawHashRegion)) {
    const s = useAppStore.getState();
    syncUrlHash({
      region: s.region,
      mode: s.mode,
      month: s.selectedMonth,
      lang: s.lang,
      minConfidence: s.minConfidence,
    });
  }
}

function startRegionsFetch(force: boolean): void {
  if (!force && regionsRequested) return;
  regionsRequested = true;
  regionsEntry.controller?.abort();

  const token = ++regionsToken;
  const controller = new AbortController();
  regionsEntry = {
    status: "loading",
    data: null,
    error: null,
    controller,
  };
  emitChange();

  fetchRegions(controller.signal)
    .then((res) => {
      if (controller.signal.aborted || token !== regionsToken) return;
      regionsEntry = {
        status: res.regions.length === 0 ? "empty" : "ready",
        data: res,
        error: null,
        controller: null,
      };
      reconcileRegionWithList(res.regions);
      emitChange();
    })
    .catch((err: unknown) => {
      if (isAbortError(err) || controller.signal.aborted || token !== regionsToken) {
        return;
      }
      regionsEntry = {
        status: "error",
        data: null,
        error: toStoredError(err),
        controller: null,
      };
      emitChange();
    });
}

function acquireRegions(): void {
  regionsSubscribers += 1;
  startRegionsFetch(false);
}

function releaseRegions(): void {
  regionsSubscribers = Math.max(0, regionsSubscribers - 1);
  if (regionsSubscribers === 0 && regionsEntry.controller) {
    regionsToken += 1;
    regionsEntry.controller.abort();
    regionsRequested = false;
    regionsEntry = {
      status: "loading",
      data: null,
      error: null,
      controller: null,
    };
  }
}

function startSeriesFetch(region: string, force: boolean): void {
  if (!region) return;
  if (activeSeriesRegion && activeSeriesRegion !== region) {
    const prev = seriesCache.get(activeSeriesRegion);
    if (prev?.controller) {
      seriesToken += 1;
      prev.controller.abort();
    }
    if (prev && (prev.status === "loading" || prev.status === "error")) {
      seriesCache.delete(activeSeriesRegion);
    }
  }
  activeSeriesRegion = region;

  const existing = seriesCache.get(region);
  if (!force && existing) {
    touchCacheEntry(seriesCache, region, existing);
    return;
  }

  existing?.controller?.abort();
  const token = ++seriesToken;
  const controller = new AbortController();
  touchCacheEntry(seriesCache, region, {
    status: "loading",
    data: null,
    error: null,
    controller,
  });
  emitChange();

  fetchSeries(region, controller.signal)
    .then((res) => {
      if (controller.signal.aborted || token !== seriesToken || activeSeriesRegion !== region) return;
      const isEmpty =
        (res.years.length === 0 && res.months.length === 0) ||
        res.years.every((y) => y.raw === 0 && y.harmonized === 0);
      touchCacheEntry(seriesCache, region, {
        status: isEmpty ? "empty" : "ready",
        data: res,
        error: null,
        controller: null,
      });
      evictCacheLru(seriesCache, seriesSubscribers, MAX_SERIES_CACHE);

      if (!isEmpty) {
        const coverage = deriveCoverage(res, regionsEntry.data?.regions.find((r) => r.id === region));
        const bounds = deriveYearBounds(res, coverage);
        if (bounds.firstYear !== null && bounds.lastYear !== null) {
          setValidYearRange(bounds.firstYear, bounds.lastYear);

          const selectedMonth = useAppStore.getState().selectedMonth;
          if (selectedMonth) {
            const cells = buildCalendarMonths(res, bounds.firstYear, bounds.lastYear);
            const validMonth = cells.some(
              (m) => m.hasData && m.year === selectedMonth.year && m.month === selectedMonth.month,
            );
            if (!validMonth) {
              useAppStore.getState().selectMonth(null);
            }
          }
        }
      }
      emitChange();
    })
    .catch((err: unknown) => {
      if (isAbortError(err) || controller.signal.aborted || token !== seriesToken || activeSeriesRegion !== region) {
        return;
      }
      seriesCache.set(region, {
        status: "error",
        data: null,
        error: toStoredError(err),
        controller: null,
      });
      emitChange();
    });
}

function acquireSeries(region: string): void {
  if (!region) return;
  seriesSubscribers.set(region, (seriesSubscribers.get(region) ?? 0) + 1);
  startSeriesFetch(region, false);
}

function releaseSeries(region: string): void {
  if (!region) return;
  const next = (seriesSubscribers.get(region) ?? 0) - 1;
  if (next > 0) {
    seriesSubscribers.set(region, next);
    return;
  }
  seriesSubscribers.delete(region);
  const entry = seriesCache.get(region);
  if (entry?.controller) {
    seriesToken += 1;
    entry.controller.abort();
    seriesCache.delete(region);
  } else if (entry?.status === "ready") {
    evictCacheLru(seriesCache, seriesSubscribers, MAX_SERIES_CACHE);
  }
}

function makePointsKey(region: string, year: number, month: number | null, minConfidence: MinConfidence): string {
  return `${region}:${year}:${month ?? "all"}:${minConfidence}`;
}

function startPointsFetch(
  region: string,
  year: number,
  month: number | null,
  minConfidence: MinConfidence,
  force: boolean,
): void {
  const key = makePointsKey(region, year, month, minConfidence);
  if (activePointsKey && activePointsKey !== key) {
    const prev = pointsCache.get(activePointsKey);
    if (prev?.controller) {
      pointsToken += 1;
      prev.controller.abort();
    }
    if (prev && (prev.status === "loading" || prev.status === "error")) {
      pointsCache.delete(activePointsKey);
    }
  }
  activePointsKey = key;

  const existing = pointsCache.get(key);
  if (!force && existing) {
    touchCacheEntry(pointsCache, key, existing);
    return;
  }

  existing?.controller?.abort();
  const token = ++pointsToken;
  const controller = new AbortController();
  touchCacheEntry(pointsCache, key, {
    status: "loading",
    data: null,
    error: null,
    controller,
  });
  emitChange();

  fetchPoints({ region, year, month, minConfidence }, controller.signal)
    .then((res) => {
      if (controller.signal.aborted || token !== pointsToken || activePointsKey !== key) return;
      const isEmpty = (res.raw.length === 0 && res.harmonized.length === 0) || res.bbox === null;
      touchCacheEntry(pointsCache, key, {
        status: isEmpty ? "empty" : "ready",
        data: res,
        error: null,
        controller: null,
      });
      evictCacheLru(pointsCache, pointsSubscribers, MAX_POINTS_CACHE);
      emitChange();
    })
    .catch((err: unknown) => {
      if (isAbortError(err) || controller.signal.aborted || token !== pointsToken || activePointsKey !== key) {
        return;
      }
      pointsCache.set(key, {
        status: "error",
        data: null,
        error: toStoredError(err),
        controller: null,
      });
      emitChange();
    });
}

function acquirePoints(region: string, year: number, month: number | null, minConfidence: MinConfidence): void {
  const key = makePointsKey(region, year, month, minConfidence);
  pointsSubscribers.set(key, (pointsSubscribers.get(key) ?? 0) + 1);
  startPointsFetch(region, year, month, minConfidence, false);
}

function releasePoints(key: string): void {
  const next = (pointsSubscribers.get(key) ?? 0) - 1;
  if (next > 0) {
    pointsSubscribers.set(key, next);
    return;
  }
  pointsSubscribers.delete(key);
  const entry = pointsCache.get(key);
  if (entry?.controller) {
    pointsToken += 1;
    entry.controller.abort();
    pointsCache.delete(key);
  } else if (entry?.status === "ready") {
    evictCacheLru(pointsCache, pointsSubscribers, MAX_POINTS_CACHE);
  }
}

// --- Derivations -------------------------------------------------------------

const EMPTY_STAGES: PointStages = {
  rawModis: 0,
  rawViirs: 0,
  interscanRemoved: 0,
  fleetMerged: 0,
  matchedPairs: 0,
  harmonized: 0,
};
const UNFILTERED: ConfidenceFilterInfo = { minConfidence: "low", excludedDetections: 0 };
const EMPTY_TOTALS: RegionTotals = {
  modis: 0,
  viirs: 0,
  raw: 0,
  harmonized: 0,
  interscanRemoved: 0,
  fleetMerged: 0,
  matched: 0,
  doubleCounts: 0,
  doubleCountShare: 0,
};

function deriveCoverage(series: SeriesResponse | null, summary: RegionSummary | undefined): SeriesCoverage {
  if (series?.coverage && (series.coverage.modis || series.coverage.viirs)) {
    return series.coverage;
  }
  if (series && series.years.length > 0) {
    const modisYears = series.years.filter((y) => y.rawModis > 0);
    const viirsYears = series.years.filter((y) => y.rawViirs > 0);
    return {
      modis:
        modisYears.length > 0
          ? { first: modisYears[0].year, last: modisYears[modisYears.length - 1].year }
          : summary?.modisYears
            ? { first: summary.modisYears[0], last: summary.modisYears[1] }
            : null,
      viirs:
        viirsYears.length > 0
          ? { first: viirsYears[0].year, last: viirsYears[viirsYears.length - 1].year }
          : summary?.viirsYears
            ? { first: summary.viirsYears[0], last: summary.viirsYears[1] }
            : null,
    };
  }
  return {
    modis: summary?.modisYears ? { first: summary.modisYears[0], last: summary.modisYears[1] } : null,
    viirs: summary?.viirsYears ? { first: summary.viirsYears[0], last: summary.viirsYears[1] } : null,
  };
}

function deriveYearBounds(
  series: SeriesResponse | null,
  coverage: SeriesCoverage,
): { firstYear: number | null; lastYear: number | null } {
  const firstCandidates: number[] = [];
  const lastCandidates: number[] = [];
  if (series && series.years.length > 0) {
    firstCandidates.push(series.years[0].year);
    lastCandidates.push(series.years[series.years.length - 1].year);
  }
  if (coverage.modis) {
    firstCandidates.push(coverage.modis.first);
    lastCandidates.push(coverage.modis.last);
  }
  if (coverage.viirs) {
    firstCandidates.push(coverage.viirs.first);
    lastCandidates.push(coverage.viirs.last);
  }
  if (firstCandidates.length === 0 || lastCandidates.length === 0) {
    return { firstYear: null, lastYear: null };
  }
  return {
    firstYear: Math.min(...firstCandidates),
    lastYear: Math.max(...lastCandidates),
  };
}

function parseYmFromIsoDate(dateStr: string | null): number | null {
  if (!dateStr) return null;
  const match = /^(\d{4})-(\d{2})/.exec(dateStr);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  if (year <= 0 || month < 1 || month > 12) return null;
  return year * 12 + month;
}

function slugifyRegion(id: string): string {
  return id.toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_+|_+$/g, "") || "region";
}

function computeFrpCorrectionRatio(points: PointsResponse | null): number | null {
  if (!points || points.harmonized.length === 0) return null;
  let sumRaw = 0;
  let sumCorrected = 0;
  for (const h of points.harmonized) {
    sumRaw += h.frp;
    sumCorrected += h.frpCorrected;
  }
  if (sumRaw <= 0) return null;
  return (sumCorrected - sumRaw) / sumRaw;
}

function findDefaultMapMonth(series: SeriesResponse | null): MonthRef | null {
  if (!series || series.months.length === 0) return null;

  let targetYear: number | null = null;
  for (let i = series.years.length - 1; i >= 0; i--) {
    if (!series.years[i].partial && series.years[i].raw > 0) {
      targetYear = series.years[i].year;
      break;
    }
  }
  if (targetYear === null && series.years.length > 0) {
    targetYear = series.years[series.years.length - 1].year;
  }

  const candidateMonths =
    targetYear !== null ? series.months.filter((m) => m.year === targetYear && m.rawTotal > 0) : [];
  const pool = candidateMonths.length > 0 ? candidateMonths : series.months;

  let best = pool[0];
  for (const m of pool) {
    if (m.rawTotal > best.rawTotal) {
      best = m;
    }
  }
  return best ? { year: best.year, month: best.month } : null;
}

function buildCalendarMonths(
  series: SeriesResponse,
  firstYear: number | null,
  lastYear: number | null,
): readonly MonthCell[] {
  if (firstYear === null || lastYear === null) return [];
  if (series.months.length === 0 && series.years.length === 0) return [];
  const byKey = new Map<string, SeriesResponse["months"][number]>();
  let minMonthYm = Number.POSITIVE_INFINITY;
  let maxMonthYm = Number.NEGATIVE_INFINITY;
  for (const m of series.months) {
    byKey.set(`${m.year}-${m.month}`, m);
    const ym = m.year * 12 + m.month;
    if (ym < minMonthYm) minMonthYm = ym;
    if (ym > maxMonthYm) maxMonthYm = ym;
  }

  let startYm = minMonthYm;
  let endYm = maxMonthYm;
  if (series.years.length > 0) {
    const firstY = series.years[0];
    const parsedFirst = parseYmFromIsoDate(firstY.firstDate);
    const yStart = parsedFirst ?? (firstY.partial ? minMonthYm : firstY.year * 12 + 1);
    if (yStart < startYm) startYm = yStart;

    const lastY = series.years[series.years.length - 1];
    const parsedLast = parseYmFromIsoDate(lastY.lastDate);
    const yEnd = parsedLast ?? (lastY.partial ? maxMonthYm : lastY.year * 12 + 12);
    if (yEnd > endYm) endYm = yEnd;
  }
  if (series.coverage?.modis) {
    if (!Number.isFinite(startYm)) startYm = series.coverage.modis.first * 12 + 1;
    if (!Number.isFinite(endYm)) endYm = series.coverage.modis.last * 12 + 12;
  }

  const cells: MonthCell[] = [];
  for (let year = firstYear; year <= lastYear; year++) {
    for (let month = 1; month <= 12; month++) {
      const found = byKey.get(`${year}-${month}`);
      if (found) {
        cells.push({
          year,
          month,
          hasData: true,
          rawModis: found.rawModis,
          rawViirs: found.rawViirs,
          rawCount: found.rawTotal,
          harmonizedCount: found.harmonized,
          anomalyRaw: found.anomalyRaw,
          anomalyHarmonized: found.anomalyHarmonized,
        });
      } else {
        const ym = year * 12 + month;
        const inCoverage = ym >= startYm && ym <= endYm;
        cells.push({
          year,
          month,
          hasData: inCoverage,
          rawModis: 0,
          rawViirs: 0,
          rawCount: 0,
          harmonizedCount: 0,
          anomalyRaw: null,
          anomalyHarmonized: null,
        });
      }
    }
  }
  return cells;
}

// --- Public hooks ------------------------------------------------------------

export interface UseRegionsResult {
  status: AsyncStatus;
  regionsEmpty: boolean;
  source: string;
  regions: readonly RegionSummary[];
  error: ApiErrorInfo | null;
  retry: () => void;
}

// Shared so consumers memoizing on `regions` stay stable while the list is loading.
const NO_REGIONS: readonly RegionSummary[] = [];

export function useRegions(): UseRegionsResult {
  useSyncExternalStore(subscribeCache, getSnapshotVersion, getSnapshotVersion);
  const lang = useAppStore((state) => state.lang);

  useEffect(() => {
    acquireRegions();
    return () => {
      releaseRegions();
    };
  }, []);

  const retry = useCallback(() => {
    const wasEmpty = regionsEntry.status === "empty";
    startRegionsFetch(true);
    if (!wasEmpty) {
      const currentRegion = useAppStore.getState().region;
      const seriesEntry = seriesCache.get(currentRegion);
      if (!seriesEntry || seriesEntry.status === "error") {
        startSeriesFetch(currentRegion, true);
      }
    }
  }, []);

  const regionsEmpty = regionsEntry.status === "empty";
  const rawError = regionsEntry.error;
  const error = useMemo(() => formatStoredError(rawError, lang), [rawError, lang]);

  return {
    status: regionsEntry.status,
    regionsEmpty,
    source: regionsEntry.data?.source ?? "FIRMS archive",
    regions: regionsEntry.status === "ready" && regionsEntry.data ? regionsEntry.data.regions : NO_REGIONS,
    error,
    retry,
  };
}

export interface UseRegionDataResult extends RegionData {
  regionsEmpty: boolean;
}

export function useRegionData(): UseRegionDataResult {
  useSyncExternalStore(subscribeCache, getSnapshotVersion, getSnapshotVersion);
  const regionId = useAppStore((state) => state.region);
  const lang = useAppStore((state) => state.lang);
  const selectedMonth = useAppStore((state) => state.selectedMonth);
  const minConfidence = useAppStore((state) => state.minConfidence);
  const regionsEmpty = regionsEntry.status === "empty";

  useEffect(() => {
    acquireRegions();
    return () => {
      releaseRegions();
    };
  }, []);

  useEffect(() => {
    if (regionsEmpty) return;
    acquireSeries(regionId);
    return () => {
      releaseSeries(regionId);
    };
  }, [regionId, regionsEmpty]);

  const retry = useCallback(() => {
    const wasEmpty = regionsEntry.status === "empty";
    if (regionsEntry.status === "error" || wasEmpty || !regionsEntry.data) {
      startRegionsFetch(true);
    }
    for (const [key, pt] of pointsCache.entries()) {
      if (key.startsWith(`${regionId}:`) && pt.status === "error") {
        pointsCache.delete(key);
      }
    }
    if (wasEmpty) {
      if (seriesCache.get(regionId)?.status === "error") {
        seriesCache.delete(regionId);
      }
      return;
    }
    startSeriesFetch(regionId, true);
  }, [regionId]);

  const entry = seriesCache.get(regionId);
  const status: AsyncStatus = regionsEmpty
    ? "error"
    : (entry?.status ?? (regionsEntry.status === "error" ? "error" : "loading"));
  const rawError = regionsEmpty
    ? null
    : (entry?.error ?? (regionsEntry.status === "error" ? regionsEntry.error : null));
  const error = useMemo<ApiErrorInfo | null>(
    () =>
      regionsEmpty
        ? { kind: "server", message: t(lang, "stateErrorRegionsEmpty") }
        : formatStoredError(rawError, lang),
    [regionsEmpty, rawError, lang],
  );
  const activeSeries: SeriesResponse | null =
    !regionsEmpty && status === "ready" && entry?.data && entry.data.region === regionId ? entry.data : null;

  const regionSummary = regionsEntry.data?.regions.find((r) => r.id === regionId);
  const rawName = regionSummary?.name ?? activeSeries?.region ?? regionId;
  const region: Region = useMemo(
    () => ({
      id: regionId,
      name: rawName,
      displayName: formatRegionName({ id: regionId, name: rawName }, lang),
      slug: slugifyRegion(regionId),
    }),
    [regionId, rawName, lang],
  );

  const coverage = useMemo(
    () => (activeSeries ? deriveCoverage(activeSeries, regionSummary) : { modis: null, viirs: null }),
    [activeSeries, regionSummary],
  );
  const { firstYear, lastYear } = useMemo(
    () => (activeSeries ? deriveYearBounds(activeSeries, coverage) : { firstYear: null, lastYear: null }),
    [activeSeries, coverage],
  );

  const defaultMapMonth = useMemo(() => findDefaultMapMonth(activeSeries), [activeSeries]);
  const targetMonth = selectedMonth ?? defaultMapMonth;
  const pointsKey = targetMonth ? makePointsKey(regionId, targetMonth.year, targetMonth.month, minConfidence) : null;
  const pointsEntry = pointsKey ? pointsCache.get(pointsKey) : undefined;
  const activePoints =
    pointsEntry?.status === "ready" && pointsEntry.data && pointsEntry.data.region === regionId
      ? pointsEntry.data
      : null;
  const frpAngleCorrectionPct = useMemo(() => computeFrpCorrectionRatio(activePoints), [activePoints]);

  return useMemo<UseRegionDataResult>(() => {
    if (!activeSeries) {
      const fallbackStages: PipelineStage[] = [
        { key: "collect" },
        { key: "dedup" },
        { key: "angle" },
        { key: "grid" },
        { key: "match" },
        { key: "fuse" },
        { key: "calendar" },
      ];
      return {
        status,
        regionsEmpty,
        hasData: false,
        error,
        retry,
        region,
        totals: EMPTY_TOTALS,
        cliff: null,
        coverage,
        yearly: [],
        months: [],
        stages: fallbackStages,
        frpAngleCorrectionPct: null,
        briefing: {
          peakMonth: null,
          meanAnnualHarmonized: null,
          rawJumpPct: null,
          harmonizedJumpPct: null,
          doubleCountShare: 0,
          frpAngleCorrectionPct: null,
        },
        firstYear: null,
        lastYear: null,
        defaultMapMonth: null,
      };
    }

    const years = activeSeries.years;

    let modis = 0;
    let viirs = 0;
    let raw = 0;
    let harmonized = 0;
    let interscanRemoved = 0;
    let fleetMerged = 0;
    let matched = 0;

    for (const y of years) {
      modis += y.rawModis;
      viirs += y.rawViirs;
      raw += y.raw;
      harmonized += y.harmonized;
      interscanRemoved += y.interscanRemoved;
      fleetMerged += y.fleetMerged;
      matched += y.matched;
    }

    const doubleCounts = Math.max(0, raw - harmonized);
    const doubleCountShare = raw > 0 ? doubleCounts / raw : 0;

    const totals: RegionTotals = {
      modis,
      viirs,
      raw,
      harmonized,
      interscanRemoved,
      fleetMerged,
      matched,
      doubleCounts,
      doubleCountShare,
    };

    const months = buildCalendarMonths(activeSeries, firstYear, lastYear);

    let peakMonth: (MonthRef & { count: number }) | null = null;
    for (const m of activeSeries.months) {
      if (m.harmonized > 0 && (!peakMonth || m.harmonized > peakMonth.count)) {
        peakMonth = { year: m.year, month: m.month, count: m.harmonized };
      }
    }

    const cliff = activeSeries.cliff;
    const cliffYear = cliff?.toYear ?? null;
    const completeSinceCliff = cliffYear !== null ? years.filter((y) => y.year >= cliffYear && !y.partial) : [];
    const sinceCliffPool =
      cliffYear !== null
        ? completeSinceCliff.length > 0
          ? completeSinceCliff
          : years.filter((y) => y.year >= cliffYear)
        : [];
    const meanAnnualHarmonized =
      sinceCliffPool.length > 0
        ? Math.round(sinceCliffPool.reduce((acc, y) => acc + y.harmonized, 0) / sinceCliffPool.length)
        : null;

    const stages: PipelineStage[] = [
      { key: "collect" },
      { key: "dedup", removed: interscanRemoved },
      { key: "fleet", removed: fleetMerged },
      { key: "angle", frpDeltaPct: frpAngleCorrectionPct ?? undefined },
      { key: "grid" },
      { key: "match", removed: matched },
      { key: "fuse" },
      { key: "calendar" },
    ];

    const briefing: BriefingMetrics = {
      peakMonth,
      meanAnnualHarmonized,
      rawJumpPct: cliff ? cliff.rawChangeRatio : null,
      harmonizedJumpPct: cliff ? cliff.harmonizedChangeRatio : null,
      doubleCountShare,
      frpAngleCorrectionPct,
    };

    return {
      status,
      regionsEmpty,
      hasData: true,
      error,
      retry,
      region,
      totals,
      cliff,
      coverage,
      yearly: years,
      months,
      stages,
      frpAngleCorrectionPct,
      briefing,
      firstYear,
      lastYear,
      defaultMapMonth,
    };
  }, [
    activeSeries,
    status,
    regionsEmpty,
    error,
    retry,
    region,
    coverage,
    firstYear,
    lastYear,
    frpAngleCorrectionPct,
    defaultMapMonth,
  ]);
}

export interface UseMapPointsResult extends MapPointSet {
  status: AsyncStatus;
  regionsEmpty: boolean;
  hasData: boolean;
  error: ApiErrorInfo | null;
  retry: () => void;
}

/** Points for the map: the selected month, or the month with highest raw_total in the latest complete year. */
export function useMapPoints(): UseMapPointsResult {
  useSyncExternalStore(subscribeCache, getSnapshotVersion, getSnapshotVersion);
  const regionId = useAppStore((state) => state.region);
  const lang = useAppStore((state) => state.lang);
  const selectedMonth = useAppStore((state) => state.selectedMonth);
  const minConfidence = useAppStore((state) => state.minConfidence);
  const {
    defaultMapMonth,
    status: seriesStatus,
    regionsEmpty,
    error: seriesError,
    retry: retrySeries,
  } = useRegionData();

  const targetMonth = selectedMonth ?? defaultMapMonth;
  const targetYear = targetMonth?.year ?? null;
  const targetMonthNum = targetMonth?.month ?? null;

  useEffect(() => {
    if (targetYear === null || seriesStatus === "error" || seriesStatus === "empty") return;
    const key = makePointsKey(regionId, targetYear, targetMonthNum, minConfidence);
    acquirePoints(regionId, targetYear, targetMonthNum, minConfidence);
    return () => {
      releasePoints(key);
    };
  }, [regionId, targetYear, targetMonthNum, minConfidence, seriesStatus]);

  const retry = useCallback(() => {
    if (regionsEntry.status === "error" || regionsEntry.status === "empty" || !regionsEntry.data) {
      startRegionsFetch(true);
    }
    if (seriesStatus === "error") {
      retrySeries();
      return;
    }
    if (targetYear !== null) {
      startPointsFetch(regionId, targetYear, targetMonthNum, minConfidence, true);
    }
  }, [seriesStatus, retrySeries, targetYear, targetMonthNum, minConfidence, regionId]);

  const key = targetYear !== null ? makePointsKey(regionId, targetYear, targetMonthNum, minConfidence) : null;
  const entry = key ? pointsCache.get(key) : undefined;
  const rawPointsError = entry?.error ?? null;
  const pointsError = useMemo(() => formatStoredError(rawPointsError, lang), [rawPointsError, lang]);

  let status: AsyncStatus;
  let error: ApiErrorInfo | null = null;
  if (seriesStatus === "error") {
    status = "error";
    error = seriesError;
  } else if (targetYear === null) {
    status = seriesStatus === "ready" ? "empty" : seriesStatus;
    error = seriesError;
  } else if (!entry) {
    status = "loading";
  } else {
    status = entry.status;
    error = pointsError;
  }

  const activeData =
    status === "ready" && entry?.data && entry.data.region === regionId ? entry.data : null;
  const frpAngleCorrectionPct = useMemo(() => computeFrpCorrectionRatio(activeData), [activeData]);

  return useMemo<UseMapPointsResult>(() => {
    if (!activeData) {
      return {
        status,
        regionsEmpty,
        hasData: false,
        error,
        retry,
        period:
          status === "error" || status === "empty" || targetYear === null
            ? { year: null, month: null }
            : { year: targetYear, month: targetMonthNum },
        raw: [],
        harmonized: [],
        stages: EMPTY_STAGES,
        bbox: null,
        truncated: false,
        limit: 4000,
        frpAngleCorrectionPct: null,
        filters: UNFILTERED,
      };
    }

    return {
      status,
      regionsEmpty,
      hasData: true,
      error,
      retry,
      period: { year: activeData.year, month: activeData.month },
      raw: activeData.raw,
      harmonized: activeData.harmonized,
      stages: activeData.stages,
      bbox: activeData.bbox,
      truncated: activeData.truncated,
      limit: activeData.limit,
      frpAngleCorrectionPct,
      filters: activeData.filters,
    };
  }, [activeData, status, regionsEmpty, error, retry, targetYear, targetMonthNum, frpAngleCorrectionPct]);
}

export function useSelectedMonthCell(): MonthCell | null {
  const month = useAppStore((state) => state.selectedMonth);
  const { months } = useRegionData();
  return useMemo(() => {
    if (!month) return null;
    return months.find((c) => c.year === month.year && c.month === month.month) ?? null;
  }, [month, months]);
}
