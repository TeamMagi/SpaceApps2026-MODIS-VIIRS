import pointsJson from './hotspots.json';
import seriesJson from './calendar.json';
import statsJson from './stats.json';
import metadataJson from './metadata.json';
import { Confidence, DayNight, HotspotSource, Sensor, type AnalyticsSnapshot, type FrpMonthlyPoint } from '../types';
import type { PointsResponse, SeriesResponse } from '../services/api';

function enumValue<T extends string>(values: readonly T[], value: string): T {
  const found = values.find(candidate => candidate === value);
  if (found === undefined) throw new Error(`Invalid fixture enum: ${value}`);
  return found;
}

export const mockPoints: PointsResponse = {
  ...pointsJson,
  raw: pointsJson.raw.map(point => ({ ...point,
    sensor: enumValue(['MODIS', 'VIIRS'] as const, point.sensor),
    confidence: enumValue(Object.values(Confidence), point.confidence),
    daynight: enumValue(Object.values(DayNight), point.daynight),
  })),
  harmonized: pointsJson.harmonized.map(point => ({ ...point,
    confidence: enumValue(Object.values(Confidence), point.confidence),
    daynight: enumValue(Object.values(DayNight), point.daynight),
    source: enumValue(Object.values(HotspotSource), point.source),
    platforms: point.platforms.map(value => enumValue(Object.values(Sensor), value)),
  })),
  stages: { ...pointsJson.stages, staticMasked: 0, samaRemoved: 0, viirsAggregated: 0 },
  filters: { minConfidence: 'low', excludedDetections: 0 },
  tropicalSwathGap: null,
};

function snapshot(value: typeof statsJson.annual): AnalyticsSnapshot {
  return { ...value, platforms: value.platforms.map(p => enumValue(Object.values(Sensor), p)) };
}

const { frpMonths, ...seriesOnly } = seriesJson;
export const mockSeries: SeriesResponse = {
  ...seriesOnly,
  years: seriesOnly.years.map(year => ({ ...year, staticMasked: 0, samaRemoved: 0, viirsAggregated: 0 })),
};
export const mockFrpMonthly: readonly FrpMonthlyPoint[] = frpMonths;
export const mockAnnualStats = snapshot(statsJson.annual);
export const mockMonthlyStats: readonly AnalyticsSnapshot[] = statsJson.months.map(value => ({
  ...value, platforms: value.platforms.map(p => enumValue(Object.values(Sensor), p)),
}));
export const mockMetadata = metadataJson;
