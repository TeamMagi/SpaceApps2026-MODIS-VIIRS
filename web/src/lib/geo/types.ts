export type Position = readonly [lon: number, lat: number];

export interface GeoFeature {
  code: string;
  name: string;
  regionId: string | null;
  bbox: readonly [west: number, south: number, east: number, north: number];
  polygons: readonly (readonly (readonly Position[])[])[];
}

export interface GeoMeta {
  package: string;
  packageVersion: string;
  source: string;
  sourceVersion: string;
  license: string;
  resolution: string;
  simplification: string;
}

export interface GeoDataset {
  meta: GeoMeta;
  features: readonly GeoFeature[];
}
