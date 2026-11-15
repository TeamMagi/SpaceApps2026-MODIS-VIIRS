import assert from "node:assert/strict";
import { parseHash, serializeHash, type UrlState } from "../src/lib/urlState.ts";
import { DEFAULT_PIPELINE_CONFIG } from "../src/lib/pipelineOverrides.ts";
import type { PipelineConfig } from "../src/types/index.ts";

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

check("urlState round-trips pipeline overrides using short hash keys", () => {
  const overrides = {
    dedupTimeSecs: 15,
    dedupOverlap: 0.55,
    matchRadiusKm: 4.5,
    matchWindowSecs: 5400,
    fleetRadiusKm: 3.5,
  };
  const hash = serializeHash({ ...BASE_STATE, overrides });
  assert.equal(hash.includes("dt=15"), true);
  assert.equal(hash.includes("ov=0.55"), true);
  assert.equal(hash.includes("mr=4.5"), true);
  assert.equal(hash.includes("mw=5400"), true);
  assert.equal(hash.includes("fr=3.5"), true);

  const parsed = parseHash(hash);
  assert.deepEqual(parsed.overrides, overrides);
});

check("urlState omits pipeline override values equal to server defaults", () => {
  const defaultOverrides = {
    dedupTimeSecs: DEFAULT_PIPELINE_CONFIG.dedup.maxTimeDiffSecs,
    dedupOverlap: DEFAULT_PIPELINE_CONFIG.dedup.minOverlapRatio,
    matchRadiusKm: DEFAULT_PIPELINE_CONFIG.matching.radiusKm,
    matchWindowSecs: DEFAULT_PIPELINE_CONFIG.matching.windowSecs,
    fleetRadiusKm: DEFAULT_PIPELINE_CONFIG.fleet.radiusKm,
  };
  const hashWithDefaults = serializeHash({ ...BASE_STATE, overrides: defaultOverrides });
  const hashWithout = serializeHash(BASE_STATE);
  assert.equal(hashWithDefaults, hashWithout);
  for (const key of ["dt=", "ov=", "mr=", "mw=", "fr="]) {
    assert.equal(hashWithDefaults.includes(key), false);
  }

  const customConfig: PipelineConfig = {
    ...DEFAULT_PIPELINE_CONFIG,
    dedup: { maxTimeDiffSecs: 12, minOverlapRatio: 0.4 },
  };
  const hashCustom = serializeHash(
    { ...BASE_STATE, overrides: { dedupTimeSecs: 12, dedupOverlap: 0.4, fleetRadiusKm: 4 } },
    customConfig,
  );
  assert.equal(hashCustom.includes("dt="), false);
  assert.equal(hashCustom.includes("ov="), false);
  assert.equal(hashCustom.includes("fr=4"), true);
});

check("urlState ignores out-of-range and non-numeric pipeline overrides", () => {
  const badHash =
    "#region=Turkey&mode=harmonized&lang=en&dt=0&ov=0.95&mr=abc&mw=1000&fr=-2";
  const parsedOut = parseHash(badHash);
  assert.equal(parsedOut.overrides, undefined);

  const floatIntAndNanHash =
    "#region=Turkey&mode=harmonized&lang=en&dt=8.5&ov=NaN&mr=Infinity&mw=3600.5&fr=1.5";
  const parsedPartial = parseHash(floatIntAndNanHash);
  assert.deepEqual(parsedPartial.overrides, { fleetRadiusKm: 1.5 });

  const emptyHash = "#region=Turkey&dt=&ov=0.05&mr=10.5&mw=21601&fr=5.5";
  assert.equal(parseHash(emptyHash).overrides, undefined);
});
