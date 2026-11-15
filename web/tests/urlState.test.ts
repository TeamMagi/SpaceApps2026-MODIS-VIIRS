import assert from "node:assert/strict";
import { parseHash, serializeHash, type UrlState } from "../src/lib/urlState.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const BASE_STATE: UrlState = {
  region: "Turkey",
  mode: "harmonized",
  month: { year: 2021, month: 8 },
  lang: "en",
};

check("urlState omits default palette from serialized hash", () => {
  const hashDefault = serializeHash({ ...BASE_STATE, palette: "default" });
  const hashUndefined = serializeHash(BASE_STATE);

  assert.equal(hashDefault.includes("palette="), false);
  assert.equal(hashUndefined.includes("palette="), false);
  assert.equal(hashDefault, hashUndefined);
  assert.equal(parseHash(hashDefault).palette, undefined);
});

check("urlState round-trips viridis and cividis palettes", () => {
  for (const palette of ["viridis", "cividis"] as const) {
    const state: UrlState = { ...BASE_STATE, palette };
    const hash = serializeHash(state);
    assert.equal(hash.includes(`palette=${palette}`), true);

    const parsed = parseHash(hash);
    assert.equal(parsed.region, state.region);
    assert.equal(parsed.mode, state.mode);
    assert.deepEqual(parsed.month, state.month);
    assert.equal(parsed.lang, state.lang);
    assert.equal(parsed.palette, palette);
  }
});

check("urlState ignores invalid palette values in hash", () => {
  for (const bogus of ["plasma", "inferno", "VIRIDIS", "", "unknown"]) {
    const parsed = parseHash(`#region=Turkey&mode=raw&lang=tr&palette=${bogus}`);
    assert.equal(parsed.region, "Turkey");
    assert.equal(parsed.mode, "raw");
    assert.equal(parsed.lang, "tr");
    assert.equal(parsed.palette, undefined);
  }
});

check("urlState accepts explicit default palette when present in hash", () => {
  const parsed = parseHash("#region=Turkey&mode=harmonized&lang=en&palette=default");
  assert.equal(parsed.palette, "default");
});
