export enum Sensor {
  ModisTerra = "ModisTerra",
  ModisAqua = "ModisAqua",
  ViirsSNPP = "ViirsSNPP",
  ViirsNOAA20 = "ViirsNOAA20",
  ViirsNOAA21 = "ViirsNOAA21",
}

export enum Confidence {
  Low = "Low",
  Nominal = "Nominal",
  High = "High",
}

export enum DayNight {
  Day = "Day",
  Night = "Night",
}

export enum HotspotSource {
  ModisOnly = "ModisOnly",
  ViirsOnly = "ViirsOnly",
  Matched = "Matched",
}

export interface Hotspot {
  lat: number;
  lon: number;
  frp: number;
  confidence: Confidence;
  sensor: Sensor;
  acquired: string;
  daynight: DayNight;
}

export interface HarmonizedHotspot {
  lat: number;
  lon: number;
  frp: number;
  frp_corrected: number;
  confidence: Confidence;
  source: HotspotSource;
  acquired: string;
  daynight: DayNight;
}

export interface CalendarCell {
  year: number;
  month: number;
  count: number;
  mean_frp: number;
  anomaly: number | null;
}

export interface HarmonizeResponse {
  raw: {
    modis_count: number;
    viirs_count: number;
    total_raw: number;
  };
  harmonized: {
    count: number;
    hotspots: HarmonizedHotspot[];
  };
  calendar: CalendarCell[];
}

export interface HealthResponse {
  status: string;
  version: string;
}

export type ViewMode = "raw" | "harmonized";
