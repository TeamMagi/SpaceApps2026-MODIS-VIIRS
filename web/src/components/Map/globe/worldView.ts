import { lonLatToVec, type GeoFrame } from "./geoMath";

/** Europe, Africa and Asia face the camera when the whole Earth is shown. */
export const WORLD_VIEW_LON = 35;
export const WORLD_VIEW_LAT = 20;

/** A cap past the horizon makes the scene fit the whole visible disc inside the view cone. */
export function worldFrame(): GeoFrame {
  return { direction: lonLatToVec(WORLD_VIEW_LON, WORLD_VIEW_LAT), radius: Math.PI / 2 };
}

/** Share of the narrower view half-extent the whole-Earth disc fills on screen. */
export const WORLD_DISC_FILL = 0.88;

/** True for the whole-Earth frame, whose disc is sized by `WORLD_DISC_FILL` instead of region padding. */
export function isWorldFrame(frame: GeoFrame): boolean {
  return frame.radius >= Math.PI / 2;
}

/** View half-angle that makes the limb of the Earth span `WORLD_DISC_FILL` of the narrow axis. */
export function worldFitHalfAngle(narrowHalfAngle: number): number {
  return Math.atan(Math.tan(narrowHalfAngle) * WORLD_DISC_FILL);
}
