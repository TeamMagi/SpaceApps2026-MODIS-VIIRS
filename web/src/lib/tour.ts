export const TOUR_STEP_MS = 15000;

export type TourTarget = "toggle" | "chart" | "map" | "calendar" | "methods";

export type TourStepId = "toggle" | "lens" | "case" | "validation";

export interface TourStep {
  id: TourStepId;
  startSec: number;
  targets: readonly TourTarget[];
  route: "dashboard" | "methods";
}

export const TOUR_STEPS: readonly TourStep[] = [
  { id: "toggle", startSec: 0, targets: ["toggle", "chart"], route: "dashboard" },
  { id: "lens", startSec: 15, targets: ["map"], route: "dashboard" },
  { id: "case", startSec: 30, targets: ["chart", "calendar"], route: "dashboard" },
  { id: "validation", startSec: 45, targets: ["methods"], route: "methods" },
];

/** Index of the step after `index`, or null once the last step is done. */
export function nextTourStep(index: number): number | null {
  return index + 1 < TOUR_STEPS.length ? index + 1 : null;
}

export function prevTourStep(index: number): number {
  return Math.max(0, index - 1);
}

export function tourTotalMs(): number {
  return TOUR_STEPS.length * TOUR_STEP_MS;
}
