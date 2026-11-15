import assert from "node:assert/strict";
import { formatSwathGapNote } from "../src/components/Calendar/swathGapNote.ts";
import { createTranslator } from "../src/content/copy.ts";
import { adaptTropicalSwathGap } from "../src/services/api.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const wire = {
  belt_abs_lat: 30,
  belt_share: 0.42,
  basis: "detections",
  swath_gap_km: 388.1,
  missed_frp_low_pct: 10.7,
  missed_frp_high_pct: 12.5,
  compensation: null,
  note: "…",
  reference: "[14] …",
};

check("a well-formed flag is adapted and an absent or malformed one is dropped", () => {
  const gap = adaptTropicalSwathGap(wire);
  assert.deepEqual(gap, {
    beltAbsLat: 30,
    beltShare: 0.42,
    basis: "detections",
    swathGapKm: 388.1,
    missedFrpLowPct: 10.7,
    missedFrpHighPct: 12.5,
    compensation: null,
  });
  assert.equal(adaptTropicalSwathGap(null), null);
  assert.equal(adaptTropicalSwathGap(undefined), null);
  assert.equal(adaptTropicalSwathGap({ ...wire, belt_share: 0 }), null);
  assert.equal(adaptTropicalSwathGap({ ...wire, basis: "country" }), null);
  const on = adaptTropicalSwathGap({ ...wire, compensation: { factor: 1.1312, factor_sigma: 0.0115 } });
  assert.deepEqual(on?.compensation, { factor: 1.1312, factorSigma: 0.0115 });
});

check("no note outside the tropical belt", () => {
  assert.equal(formatSwathGapNote(null, createTranslator("en")), null);
});

check("the note states the belt share and the documented 10.7–12.5% range in EN and TR", () => {
  const gap = adaptTropicalSwathGap(wire);
  const en = formatSwathGapNote(gap, createTranslator("en")) ?? "";
  assert.match(en, /42% of this region/);
  assert.match(en, /10\.7–12\.5%/);
  assert.match(en, /388\.1 km/);
  assert.match(en, /not corrected/);
  const tr = formatSwathGapNote(gap, createTranslator("tr")) ?? "";
  assert.match(tr, /%42'si/);
  assert.match(tr, /%10,7–12,5/);
});

check("a sliver of belt never reads as 0% and compensation is described once", () => {
  const sliver = adaptTropicalSwathGap({ ...wire, belt_share: 0.001 });
  assert.match(formatSwathGapNote(sliver, createTranslator("en")) ?? "", /1% of this region/);
  const on = adaptTropicalSwathGap({ ...wire, compensation: { factor: 1.1312, factor_sigma: 0.0115 } });
  const text = formatSwathGapNote(on, createTranslator("en")) ?? "";
  assert.match(text, /scaled up by 13%/);
  assert.doesNotMatch(text, /not corrected/);
});
