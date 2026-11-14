import { matchRegionsToFeatures } from "../src/lib/geo/regionMatch.ts";
import type { GeoDataset, GeoFeature } from "../src/lib/geo/types.ts";

/** Features the real matcher links to an archive region id; empty when the region is unknown. */
export function featuresForRegion(dataset: GeoDataset, regionId: string): GeoFeature[] {
  const match = matchRegionsToFeatures(dataset.features, [{ id: regionId, name: regionId }]);
  return (match.featuresByRegion.get(regionId) ?? []).map((i) => dataset.features[i]);
}
