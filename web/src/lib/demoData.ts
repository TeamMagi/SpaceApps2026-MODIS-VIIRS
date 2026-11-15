import { mockMetadata, mockPoints, mockSeries } from "../mocks";
import type { FetchPointsParams, PointsResponse, RegionsResponse, SeriesResponse } from "../services/api";
import { HotspotSource, type MonthRef, type PointStages } from "../types";

export const DEMO_SOURCE_LABEL = "Synthetic demo data (not NASA FIRMS)";

const EMPTY_STAGES: PointStages = {
  rawModis: 0,
  rawViirs: 0,
  staticMasked: 0,
  samaRemoved: 0,
  interscanRemoved: 0,
  fleetMerged: 0,
  matchedPairs: 0,
  viirsAggregated: 0,
  harmonized: 0,
};

function emptyPointsResponse(params: FetchPointsParams): PointsResponse {
  return {
    region: params.region,
    year: params.year,
    month: params.month ?? null,
    truncated: false,
    limit: mockPoints.limit,
    raw: [],
    harmonized: [],
    stages: EMPTY_STAGES,
    bbox: null,
    filters: { minConfidence: "low", excludedDetections: 0 },
    tropicalSwathGap: null,
  };
}

function monthOfIsoTimestamp(acquired: string): number {
  return Number(acquired.slice(5, 7));
}

function yearOfIsoTimestamp(acquired: string): number {
  return Number(acquired.slice(0, 4));
}

export function buildDemoRegions(): RegionsResponse {
  const regionId = mockSeries.region || mockMetadata.region;
  const modisYears: readonly [number, number] | null = mockSeries.coverage?.modis
    ? [mockSeries.coverage.modis.first, mockSeries.coverage.modis.last]
    : mockMetadata.summaryYears.length >= 2
      ? [mockMetadata.summaryYears[0], mockMetadata.summaryYears[1]]
      : null;
  const viirsYears: readonly [number, number] | null = mockSeries.coverage?.viirs
    ? [mockSeries.coverage.viirs.first, mockSeries.coverage.viirs.last]
    : null;

  return {
    source: DEMO_SOURCE_LABEL,
    regions: [
      {
        id: regionId,
        name: regionId,
        modisYears,
        viirsYears,
        sizeMb: null,
        tooLarge: false,
      },
    ],
  };
}

export function buildDemoSeries(region: string): SeriesResponse {
  if (region === mockSeries.region) {
    return mockSeries;
  }
  return {
    region,
    filters: mockSeries.filters,
    years: [],
    months: [],
    cliff: null,
    coverage: null,
  };
}

export function buildDemoPoints(params: FetchPointsParams): PointsResponse {
  if (params.region !== mockPoints.region || params.year !== mockPoints.year) {
    return emptyPointsResponse(params);
  }

  if (params.month === undefined || params.month === null) {
    return mockPoints;
  }

  if (!Number.isInteger(params.month) || params.month < 1 || params.month > 12) {
    return emptyPointsResponse(params);
  }

  const raw = mockPoints.raw.filter(
    (p) => yearOfIsoTimestamp(p.acquired) === params.year && monthOfIsoTimestamp(p.acquired) === params.month,
  );
  const harmonized = mockPoints.harmonized.filter(
    (p) => yearOfIsoTimestamp(p.acquired) === params.year && monthOfIsoTimestamp(p.acquired) === params.month,
  );

  if (raw.length === 0 && harmonized.length === 0) {
    return emptyPointsResponse(params);
  }

  const rawModis = raw.filter((p) => p.sensor === "MODIS").length;
  const rawViirs = raw.filter((p) => p.sensor === "VIIRS").length;
  const interscanRemoved = raw.filter((p) => p.harmonizedId === null).length;
  const matchedPairs = harmonized.filter((p) => p.source === HotspotSource.Matched).length;
  const fleetMerged = Math.max(0, raw.length - interscanRemoved - matchedPairs - harmonized.length);

  return {
    region: mockPoints.region,
    year: params.year,
    month: params.month,
    truncated: false,
    limit: mockPoints.limit,
    raw,
    harmonized,
    stages: {
      rawModis,
      rawViirs,
      staticMasked: 0,
      samaRemoved: 0,
      interscanRemoved,
      fleetMerged,
      matchedPairs,
      viirsAggregated: 0,
      harmonized: harmonized.length,
    },
    bbox: mockPoints.bbox,
    filters: { minConfidence: "low", excludedDetections: 0 },
    tropicalSwathGap: null,
  };
}

/**
 * Returns the `{ year, month }` covered by the fixture points with the most
 * harmonized detections, so demo mode can land on a populated map view.
 */
export function demoCoveredMonth(): { year: number; month: number } {
  const counts = new Map<string, { year: number; month: number; count: number }>();
  for (const point of mockPoints.harmonized) {
    const year = yearOfIsoTimestamp(point.acquired);
    const month = monthOfIsoTimestamp(point.acquired);
    if (!Number.isInteger(year) || !Number.isInteger(month) || month < 1 || month > 12) continue;
    const key = `${year}-${month}`;
    const existing = counts.get(key);
    if (existing) {
      existing.count += 1;
    } else {
      counts.set(key, { year, month, count: 1 });
    }
  }
  let best: { year: number; month: number; count: number } | null = null;
  for (const entry of counts.values()) {
    if (!best || entry.count > best.count) {
      best = entry;
    }
  }
  return best ? { year: best.year, month: best.month } : { year: mockPoints.year, month: 1 };
}

/**
 * Reports whether the current region, calendar year, and selected month
 * already resolve to non-empty demo fixture points on the map.
 */
export function isDemoPeriodCovered(
  region: string,
  calendarYear: number | null,
  selectedMonth: MonthRef | null,
): boolean {
  if (region !== mockPoints.region || selectedMonth === null) {
    return false;
  }
  if (calendarYear !== null && calendarYear !== selectedMonth.year) {
    return false;
  }
  return (
    buildDemoPoints({
      region,
      year: selectedMonth.year,
      month: selectedMonth.month,
    }).harmonized.length > 0
  );
}
