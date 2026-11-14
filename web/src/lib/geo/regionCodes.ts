import type { GeoDataset, GeoFeature } from "./types.ts";

interface RegionCodeEntry {
  readonly numericId: string;
  readonly code: string;
  readonly regionId: string;
}

const REGION_CODE_TABLE: readonly RegionCodeEntry[] = [
  { numericId: "792", code: "TUR", regionId: "Turkey" },
  { numericId: "300", code: "GRC", regionId: "Greece" },
  { numericId: "100", code: "BGR", regionId: "Bulgaria" },
  { numericId: "196", code: "CYP", regionId: "Cyprus" },
  { numericId: "268", code: "GEO", regionId: "Georgia" },
  { numericId: "051", code: "ARM", regionId: "Armenia" },
  { numericId: "031", code: "AZE", regionId: "Azerbaijan" },
  { numericId: "364", code: "IRN", regionId: "Iran" },
  { numericId: "368", code: "IRQ", regionId: "Iraq" },
  { numericId: "760", code: "SYR", regionId: "Syria" },
  { numericId: "643", code: "RUS", regionId: "Russian_Federation" },
];

// Natural Earth encodes Northern Cyprus without an ISO numeric id; mapped alongside CYP.
const NORTH_CYPRUS_CODE = "CYN";

export const regionIdToCodes: Readonly<Record<string, readonly string[]>> = (() => {
  const map: Record<string, string[]> = {};
  for (const entry of REGION_CODE_TABLE) {
    const codes = map[entry.regionId] ?? (map[entry.regionId] = []);
    codes.push(entry.code);
  }
  map["Cyprus"]?.push(NORTH_CYPRUS_CODE);
  return map;
})();

export function isMappedRegion(regionId: string): boolean {
  return Object.prototype.hasOwnProperty.call(regionIdToCodes, regionId);
}

export function featuresForRegion(dataset: GeoDataset, regionId: string): GeoFeature[] {
  if (!isMappedRegion(regionId)) return [];
  return dataset.features.filter((feature) => feature.regionId === regionId);
}
