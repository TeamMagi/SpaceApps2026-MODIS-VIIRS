import { mockMetadata, mockPoints, mockSeries } from "../mocks";
import type { FetchPointsParams, PointsResponse, RegionsResponse, SeriesResponse } from "../services/api";
import { HotspotSource, type PointStages } from "../types";

export const DEMO_SOURCE_LABEL = "Synthetic demo data (not NASA FIRMS)";

const EMPTY_STAGES: PointStages = {
  rawModis: 0,
  rawViirs: 0,
  interscanRemoved: 0,
  fleetMerged: 0,
  matchedPairs: 0,
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
      interscanRemoved,
      fleetMerged,
      matchedPairs,
      harmonized: harmonized.length,
    },
    bbox: mockPoints.bbox,
  };
}
