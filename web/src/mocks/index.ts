import pointsJson from './hotspots.json';
import seriesJson from './calendar.json';
import statsJson from './stats.json';
import metadataJson from './metadata.json';
import { Confidence, DayNight, HotspotSource, Sensor, type AnalyticsSnapshot, type FrpMonthlyPoint } from '../types';
import type { FrpSeriesPoint } from '../lib/frpSeries';
import type { StatsDataset } from '../lib/stats';
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
};

function snapshot(value: typeof statsJson.annual): AnalyticsSnapshot {
  return { ...value, platforms: value.platforms.map(p => enumValue(Object.values(Sensor), p)) };
}

const { frpMonths, ...seriesOnly } = seriesJson;
export const mockSeries: SeriesResponse = seriesOnly;
export const mockFrpMonthly: readonly FrpMonthlyPoint[] = frpMonths;
export const mockAnnualStats = snapshot(statsJson.annual);
export const mockMonthlyStats: readonly AnalyticsSnapshot[] = statsJson.months.map(value => ({
  ...value, platforms: value.platforms.map(p => enumValue(Object.values(Sensor), p)),
}));
export const mockMetadata = metadataJson;

export function fromMock(rows: readonly FrpMonthlyPoint[]): FrpSeriesPoint[] {
  return rows.map(row => {
    const modisObservable = row.year > 2000 || (row.year === 2000 && row.month >= 11);
    return {
      year: row.year,
      month: row.month,
      modis: modisObservable ? { count: row.rawModisCount, frpSum: row.rawModisFrpSum } : null,
      viirs: row.year >= 2012 ? { count: row.rawViirsCount, frpSum: row.rawViirsFrpSum } : null,
      harmonized: { count: row.harmonizedCount, countNormalized: row.harmonizedCount, totalFrp: row.harmonizedFrpSum, totalFrpLower: null, totalFrpUpper: null },
    };
  });
}

/** Card dataset from the fixture; platforms are known only for the detailed year (2021), unknown elsewhere. */
export function fromMockStats(rows: readonly FrpMonthlyPoint[], detail: readonly AnalyticsSnapshot[], generatedAt: string): StatsDataset {
  const platformsByMonth = new Map(detail.map(s => [s.scope.year * 12 + (s.scope.month ?? 1), s.platforms]));
  const months = rows.map(row => ({
    year: row.year,
    month: row.month,
    modisCovered: row.year > 2000 || (row.year === 2000 && row.month >= 11),
    viirsCovered: row.year >= 2012,
    rawCount: row.rawModisCount + row.rawViirsCount,
    rawFrpSum: row.rawModisFrpSum + row.rawViirsFrpSum,
    harmonizedCount: row.harmonizedCount,
    harmonizedMeanFrp: row.meanFrpHarmonized,
    platforms: platformsByMonth.get(row.year * 12 + row.month) ?? null,
  }));
  const last = months[months.length - 1];
  return { source: 'fixture', months, coverageThrough: last ? { year: last.year, month: last.month } : null, generatedAt };
}

export const mockStatsDataset = fromMockStats(mockFrpMonthly, mockMonthlyStats, mockMetadata.generatedAt);
