import assert from "node:assert/strict";
import { createTranslator } from "../src/content/copy.ts";
import { headlineFigure } from "../src/lib/headline.ts";
import type { CliffSummary } from "../src/types/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

function cliff(overrides: Partial<CliffSummary> = {}): CliffSummary {
  return {
    fromYear: 2011,
    toYear: 2012,
    year: 2012,
    rawFrom: 100,
    rawTo: 280,
    rawChangePct: 180,
    rawChangeRatio: 1.8,
    harmonizedFrom: 100,
    harmonizedTo: 142,
    harmonizedChangePct: 42,
    harmonizedChangeRatio: 0.42,
    ...overrides,
  };
}

check("Raw states the raw change and says whether it was a jump or a drop", () => {
  assert.deepEqual(headlineFigure(cliff(), false), { kind: "raw", absPct: 180, dropped: false });
  assert.deepEqual(headlineFigure(cliff({ rawChangePct: -35.4, rawChangeRatio: -0.354 }), false), {
    kind: "raw",
    absPct: 35,
    dropped: true,
  });
});

check("Harmonized states the harmonized change ratio", () => {
  assert.deepEqual(headlineFigure(cliff(), true), { kind: "harmonized", ratio: 0.42 });
});

check("a year with nothing to divide by is a missing baseline, never a 0% change", () => {
  const noRawBase = cliff({ rawFrom: 0, rawChangePct: null, rawChangeRatio: null });
  assert.deepEqual(headlineFigure(noRawBase, false), { kind: "noBaseline" });
  const noHarmonizedBase = cliff({ harmonizedFrom: 0, harmonizedChangePct: null, harmonizedChangeRatio: null });
  assert.deepEqual(headlineFigure(noHarmonizedBase, true), { kind: "noBaseline" });
});

check("each mode reads only its own figure, so Raw cannot be affected by (or leak) the harmonized one", () => {
  const harmonizedMissing = cliff({ harmonizedChangePct: null, harmonizedChangeRatio: null });
  assert.equal(headlineFigure(harmonizedMissing, false).kind, "raw");
  const rawMissing = cliff({ rawChangePct: null, rawChangeRatio: null });
  assert.equal(headlineFigure(rawMissing, true).kind, "harmonized");
});

check("the missing-baseline headline names both years in English and Turkish without a percentage", () => {
  const en = createTranslator("en")("headlineNoBaseline", { from: "2011", to: "2012" });
  assert.ok(en.includes("2011") && en.includes("2012"), en);
  assert.ok(!/\d\s*%/.test(en), en);
  const tr = createTranslator("tr")("headlineNoBaseline", { from: "2011", to: "2012" });
  assert.ok(tr.startsWith("2011'de "), tr);
  assert.ok(tr.includes("2012"), tr);
});
