/** Mirrors `k_ring_for_radius` in crates/core/src/h3index.rs: `floor(radius × 1.15 / 0.64 km) + 2`. */
const MIN_CELL_WIDTH_KM = 0.64;
const COVERAGE_MARGIN = 1.15;
const RING_SLACK = 2;

/** Smallest H3 k-ring that covers every candidate within `radiusKm` of a detection. */
export function kRingForRadius(radiusKm: number): number {
  return Math.floor((radiusKm * COVERAGE_MARGIN) / MIN_CELL_WIDTH_KM) + RING_SLACK;
}
