import type { CalendarCell, HarmonizedHotspot, HarmonizeResponse } from "../types";

export const SAMPLE_CALENDAR: CalendarCell[] = [
  { year: 2020, month: 1, count: 142, mean_frp: 28.3, anomaly: -0.12 },
  { year: 2020, month: 2, count: 98, mean_frp: 22.1, anomaly: -0.35 },
  { year: 2020, month: 3, count: 215, mean_frp: 31.7, anomaly: 0.08 },
  { year: 2020, month: 4, count: 340, mean_frp: 45.2, anomaly: 0.42 },
  { year: 2020, month: 5, count: 520, mean_frp: 58.9, anomaly: 0.71 },
  { year: 2020, month: 6, count: 680, mean_frp: 72.4, anomaly: 1.15 },
  { year: 2020, month: 7, count: 890, mean_frp: 88.1, anomaly: 1.82 },
  { year: 2020, month: 8, count: 1120, mean_frp: 102.6, anomaly: 2.31 },
  { year: 2020, month: 9, count: 745, mean_frp: 67.3, anomaly: 0.95 },
  { year: 2020, month: 10, count: 310, mean_frp: 39.8, anomaly: 0.18 },
  { year: 2020, month: 11, count: 178, mean_frp: 26.4, anomaly: -0.21 },
  { year: 2020, month: 12, count: 125, mean_frp: 23.7, anomaly: -0.38 },
];

export const SAMPLE_HOTSPOTS: HarmonizedHotspot[] = [
  { lat: 39.92, lon: 32.85, frp: 45.2, frp_corrected: 38.7, confidence: "High" as any, source: "Matched" as any, acquired: "2020-07-15T12:30:00Z", daynight: "Day" as any },
  { lat: 37.01, lon: 35.32, frp: 28.1, frp_corrected: 24.3, confidence: "Nominal" as any, source: "ViirsOnly" as any, acquired: "2020-07-15T13:15:00Z", daynight: "Day" as any },
  { lat: 40.18, lon: 29.06, frp: 62.8, frp_corrected: 55.1, confidence: "High" as any, source: "ModisOnly" as any, acquired: "2020-08-02T01:45:00Z", daynight: "Night" as any },
];

export const SAMPLE_RESPONSE: HarmonizeResponse = {
  raw: { modis_count: 2340, viirs_count: 4180, total_raw: 6520 },
  harmonized: { count: 5363, hotspots: SAMPLE_HOTSPOTS },
  calendar: SAMPLE_CALENDAR,
};
