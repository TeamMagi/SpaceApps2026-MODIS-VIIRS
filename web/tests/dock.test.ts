import assert from "node:assert/strict";
import { busiestMonth, hotMonths, parsePanel, sparklinePath } from "../src/lib/dock.ts";
import { parseHash, serializeHash } from "../src/lib/urlState.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const cell = (year: number, month: number, raw: number, harm: number, hasData = true) => ({
  year,
  month,
  hasData,
  rawCount: raw,
  harmonizedCount: harm,
});

check("sparklinePath scales min..max into the box", () => {
  assert.equal(sparklinePath([0, 10], 72, 28), "M0.0 28.0 L72.0 0.0");
});

check("sparklinePath breaks the path at nulls", () => {
  assert.equal(sparklinePath([1, null, 3, 5], 30, 10), "M0.0 10.0 M20.0 5.0 L30.0 0.0");
});

check("sparklinePath centres a flat series and handles empties", () => {
  assert.equal(sparklinePath([4, 4, 4], 20, 10), "M0.0 5.0 L10.0 5.0 L20.0 5.0");
  assert.equal(sparklinePath([], 20, 10), "");
  assert.equal(sparklinePath([null, null], 20, 10), "");
  assert.equal(sparklinePath([7], 20, 10), "M10.0 5.0");
});

check("busiestMonth picks the peak of the year in the chosen view", () => {
  const months = [cell(2020, 3, 5, 9), cell(2020, 7, 8, 6), cell(2021, 1, 99, 99)];
  assert.equal(busiestMonth(months, 2020), 3);
  assert.equal(busiestMonth(months, 2020, false), 7);
});

check("busiestMonth returns null without detections", () => {
  assert.equal(busiestMonth([cell(2020, 1, 0, 0), cell(2020, 2, 4, 4, false)], 2020), null);
  assert.equal(busiestMonth([], 2020), null);
});

check("parsePanel accepts only known ids", () => {
  assert.equal(parsePanel("frp"), "frp");
  assert.equal(parsePanel("calendar"), "calendar");
  assert.equal(parsePanel("detections"), "detections");
  assert.equal(parsePanel("nope"), null);
  assert.equal(parsePanel(null), null);
});

check("hotMonths flags months with z >= 1 in the chosen view", () => {
  const m = (month: number, zr: number | null, zh: number | null) => ({
    ...cell(2020, month, 1, 1),
    anomalyRaw: zr,
    anomalyHarmonized: zh,
  });
  const months = [m(1, 1.5, 0.2), m(2, 0.9, 1), m(3, null, null), { ...m(4, 3, 3), hasData: false }];
  const harmonized = hotMonths(months, 2020);
  assert.deepEqual(harmonized.slice(0, 4), [false, true, false, false]);
  assert.equal(hotMonths(months, 2020, false)[0], true);
  assert.equal(hotMonths(months, 2019).some(Boolean), false);
});

check("the panel id round-trips through the URL hash", () => {
  const base = { region: "Turkey", mode: "raw" as const, month: null, lang: "en" as const, minConfidence: "low" as const };
  const hash = serializeHash({ ...base, panel: "frp" });
  assert.match(hash, /panel=frp/);
  assert.equal(parseHash(hash).panel, "frp");
  assert.doesNotMatch(serializeHash({ ...base, panel: null }), /panel=/);
  assert.equal(parseHash("#region=Turkey").panel, null);
});
