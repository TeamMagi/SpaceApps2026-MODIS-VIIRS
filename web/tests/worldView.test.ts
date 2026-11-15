import assert from "node:assert/strict";
import { test } from "node:test";
import { fitDistance, viewHalfAngle } from "../src/components/Map/globe/geoMath";
import { worldFrame } from "../src/components/Map/globe/worldView";
import { parseHash } from "../src/lib/urlState";

test("world frame is a unit direction whose fit shows the whole disc", () => {
  const f = worldFrame();
  assert.ok(Math.abs(Math.hypot(...f.direction) - 1) < 1e-9);
  const half = viewHalfAngle((30 * Math.PI) / 180, 1.6) * 0.86;
  const d = fitDistance(f.radius, half);
  assert.ok(Math.asin(1 / d) <= half + 1e-9);
});

test("a hash naming a region means the region is picked", () => {
  assert.equal(parseHash("#region=Turkey&lang=tr").region, "Turkey");
  assert.equal(parseHash("#lang=tr").region, undefined);
});
