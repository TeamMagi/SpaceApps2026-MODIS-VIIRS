import assert from "node:assert/strict";
import {
  alphaForTier,
  UNCERT_ALPHA_STEPS,
  UNCERT_MID_MAX,
  UNCERT_TIGHT_MAX,
  uncertaintyAlpha,
  uncertaintyTierFor,
} from "../src/components/Map/engine/renderer.ts";
import {
  WIDE_UNCERTAINTY_THRESHOLD,
  formatFrpInterval,
  relativeUncertaintyWidth,
} from "../src/lib/format.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

check("WIDE_UNCERTAINTY_THRESHOLD matches design specification", () => {
  assert.equal(WIDE_UNCERTAINTY_THRESHOLD, 0.30);
});

check("relativeUncertaintyWidth computes width ratio and rejects invalid inputs", () => {
  const width = relativeUncertaintyWidth(38.7, 51.8, 45.2);
  assert.ok(width !== null);
  assert.ok(Math.abs(width - (51.8 - 38.7) / 45.2) < 1e-12);

  assert.equal(relativeUncertaintyWidth(10, 10, 20), 0);
  assert.equal(relativeUncertaintyWidth(null, 50, 40), null);
  assert.equal(relativeUncertaintyWidth(30, null, 40), null);
  assert.equal(relativeUncertaintyWidth(30, 50, null), null);
  assert.equal(relativeUncertaintyWidth(undefined, 50, 40), null);
  assert.equal(relativeUncertaintyWidth(30, undefined, 40), null);
  assert.equal(relativeUncertaintyWidth(30, 50, undefined), null);
  assert.equal(relativeUncertaintyWidth(Number.NaN, 50, 40), null);
  assert.equal(relativeUncertaintyWidth(30, Number.POSITIVE_INFINITY, 40), null);
  assert.equal(relativeUncertaintyWidth(30, 50, Number.NaN), null);
  assert.equal(relativeUncertaintyWidth(30, 50, 0), null);
  assert.equal(relativeUncertaintyWidth(30, 50, -5), null);
});

check("formatFrpInterval formats EN and TR full intervals with nbsp and en dash", () => {
  assert.equal(
    formatFrpInterval(45.2, 38.7, 51.8, "en"),
    "45.2\u00A0MW (likely 38.7\u201351.8)",
  );
  assert.equal(
    formatFrpInterval(45.2, 38.7, 51.8, "tr"),
    "45,2\u00A0MW (olası aralık 38,7\u201351,8)",
  );
});

check("formatFrpInterval formats EN and TR compact intervals", () => {
  assert.equal(
    formatFrpInterval(45.2, 38.7, 51.8, "en", true),
    "45.2\u00A0MW (38.7\u201351.8)",
  );
  assert.equal(
    formatFrpInterval(45.2, 38.7, 51.8, "tr", true),
    "45,2\u00A0MW (38,7\u201351,8)",
  );
});

check("formatFrpInterval falls back to plain value when bounds are missing or non-finite", () => {
  assert.equal(formatFrpInterval(45.2, null, 51.8, "en"), "45.2\u00A0MW");
  assert.equal(formatFrpInterval(45.2, 38.7, null, "en", true), "45.2\u00A0MW");
  assert.equal(formatFrpInterval(45.2, null, null, "tr"), "45,2\u00A0MW");
  assert.equal(formatFrpInterval(45.2, Number.NaN, 51.8, "en"), "45.2\u00A0MW");
  assert.equal(formatFrpInterval(45.2, 38.7, Number.POSITIVE_INFINITY, "tr"), "45,2\u00A0MW");
});

check("uncertaintyTierFor separates VIIRS, nadir MODIS, and off-nadir MODIS at exact boundaries", () => {
  assert.equal(UNCERT_TIGHT_MAX, 0.30);
  assert.equal(UNCERT_MID_MAX, 0.65);

  assert.equal(uncertaintyTierFor(null), 0);
  assert.equal(uncertaintyTierFor(Number.NaN), 0);
  assert.equal(uncertaintyTierFor(0), 0);
  assert.equal(uncertaintyTierFor(0.15), 0);
  assert.equal(uncertaintyTierFor(0.30), 0);
  assert.equal(uncertaintyTierFor(0.30001), 1);
  assert.equal(uncertaintyTierFor(0.57), 1);
  assert.equal(uncertaintyTierFor(0.65), 1);
  assert.equal(uncertaintyTierFor(0.65001), 2);
  assert.equal(uncertaintyTierFor(0.72), 2);
});

check("uncertaintyAlpha respects exact step boundaries and null", () => {
  assert.equal(uncertaintyAlpha(null), 1.0);
  assert.equal(uncertaintyAlpha(Number.NaN), 1.0);
  assert.equal(uncertaintyAlpha(0), 1.0);
  assert.equal(uncertaintyAlpha(0.15), 1.0);
  assert.equal(uncertaintyAlpha(0.30), 1.0);
  assert.equal(uncertaintyAlpha(0.30001), 0.72);
  assert.equal(uncertaintyAlpha(0.57), 0.72);
  assert.equal(uncertaintyAlpha(0.65), 0.72);
  assert.equal(uncertaintyAlpha(0.65001), 0.48);
  assert.equal(uncertaintyAlpha(0.72), 0.48);
  assert.equal(uncertaintyAlpha(0.95), 0.48);
});

check("alphaForTier maps tiers 0/1/2 to the shared opacity steps with tier 0 fallback", () => {
  assert.deepEqual([...UNCERT_ALPHA_STEPS], [1.0, 0.72, 0.48]);
  assert.equal(alphaForTier(0), 1.0);
  assert.equal(alphaForTier(1), 0.72);
  assert.equal(alphaForTier(2), 0.48);
  assert.equal(alphaForTier(-1), 1.0);
  assert.equal(alphaForTier(3), 1.0);
  assert.equal(alphaForTier(Number.NaN), 1.0);
});
