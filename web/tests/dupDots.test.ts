import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  DOT_GRID_COLS,
  DOT_GRID_TOTAL,
  computeDisappearingDotCount,
  selectDisappearingDotIndices,
} from "../src/lib/dupDots.ts";
import { createTranslator } from "../src/content/copy.ts";

describe("dupDots", () => {
  it("rounds 20 * removed / raw and clamps positive removals to 1..19", () => {
    assert.equal(computeDisappearingDotCount(1157, 6520), 4);
    assert.equal(computeDisappearingDotCount(1, 1000), 1);
    assert.equal(computeDisappearingDotCount(999, 1000), 19);
    assert.equal(computeDisappearingDotCount(1200, 1000), 19);
  });

  it("returns 0 when raw or removed is zero, negative, or non-finite", () => {
    assert.equal(computeDisappearingDotCount(0, 1000), 0);
    assert.equal(computeDisappearingDotCount(10, 0), 0);
    assert.equal(computeDisappearingDotCount(-5, 1000), 0);
    assert.equal(computeDisappearingDotCount(Number.NaN, 1000), 0);
    assert.equal(computeDisappearingDotCount(100, Number.POSITIVE_INFINITY), 0);
  });

  it("picks exact, deterministic, evenly spread indices in 0..19", () => {
    for (let n = 0; n < DOT_GRID_TOTAL; n++) {
      const a = selectDisappearingDotIndices(n);
      const b = selectDisappearingDotIndices(n);
      assert.equal(a.size, n, `expected ${n} unique indices`);
      assert.deepEqual([...a], [...b], "must be deterministic");
      for (const idx of a) {
        assert.ok(Number.isInteger(idx) && idx >= 0 && idx < DOT_GRID_TOTAL);
        if (n <= 18) {
          assert.notEqual(idx % DOT_GRID_COLS, 0, `count ${n} should leave col 0 as a left neighbour`);
        }
      }
    }

    assert.deepEqual([...selectDisappearingDotIndices(4)], [3, 7, 13, 17]);
  });

  it("provides non-empty EN and TR copy for the top-section strings without exclamation marks", () => {
    for (const lang of ["en", "tr"] as const) {
      const t = createTranslator(lang);
      const samples: Array<[string, string]> = [
        ["headlineRaw", t("headlineRaw", { pct: "180%", year: "2012" })],
        ["headlineRawDropped", t("headlineRawDropped", { pct: "12%", year: "2012" })],
        ["headlineHarmonized", t("headlineHarmonized", { pct: "+42%" })],
        ["headlineRawSub", t("headlineRawSub")],
        ["headlineRawDroppedSub", t("headlineRawDroppedSub")],
        ["headlineHarmonizedSub", t("headlineHarmonizedSub", { rawPct: "180%" })],
        ["headlineHarmonizedSubDropped", t("headlineHarmonizedSubDropped", { rawPct: "12%" })],
        ["countUnitRaw", t("countUnitRaw")],
        ["countUnitHarmonized", t("countUnitHarmonized")],
        ["countMutedRaw", t("countMutedRaw", { from: "2000", to: "2025" })],
        ["countMutedRawNoYears", t("countMutedRawNoYears")],
        ["countMutedHarmonized", t("countMutedHarmonized", { removed: "1,157", share: "17.7%", from: "2000", to: "2025" })],
        ["countMutedHarmonizedNoYears", t("countMutedHarmonizedNoYears", { removed: "1,157", share: "17.7%" })],
        ["countDotsAriaRaw", t("countDotsAriaRaw", { raw: "6,520" })],
        ["countDotsAriaHarmonized", t("countDotsAriaHarmonized", { kept: 16, harmonized: "5,363", removed: "1,157", share: "17.7%" })],
      ];

      for (const [key, text] of samples) {
        assert.ok(text.length > 0, `${lang}.${key}`);
        assert.equal(text.includes("!"), false, `${lang}.${key} must not contain exclamation mark`);
      }
    }
  });
});
