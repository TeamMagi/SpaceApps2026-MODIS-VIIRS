import assert from "node:assert/strict";
import { createTranslator } from "../src/content/copy.ts";
import { parseHash, serializeHash } from "../src/lib/urlState.ts";
import { adaptMinConfidence } from "../src/services/api.ts";
import { useAppStore } from "../src/store/useAppStore.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

check("the default filter keeps share links unchanged", () => {
  const hash = serializeHash({ region: "Turkey", mode: "raw", month: null, lang: "en", minConfidence: "low" });
  assert.equal(hash, "#region=Turkey&mode=raw&lang=en");
  assert.equal(parseHash(hash).minConfidence, "low");
});

check("an active filter round-trips through the URL hash", () => {
  const hash = serializeHash({
    region: "Turkey",
    mode: "harmonized",
    month: { year: 2021, month: 8 },
    lang: "tr",
    minConfidence: "nominal",
  });
  assert.match(hash, /conf=nominal/);
  assert.equal(parseHash(hash).minConfidence, "nominal");
  assert.equal(parseHash("#region=Turkey&conf=high").minConfidence, "high");
});

check("an unknown conf value is ignored rather than guessed", () => {
  assert.equal(parseHash("#region=Turkey&conf=medium").minConfidence, undefined);
});

check("store applies the filter from the URL and toggles it", () => {
  const store = useAppStore.getState();
  assert.equal(store.minConfidence, "low");
  store.applyUrlState({ minConfidence: "nominal" });
  assert.equal(useAppStore.getState().minConfidence, "nominal");
  useAppStore.getState().setMinConfidence("low");
  assert.equal(useAppStore.getState().minConfidence, "low");
});

check("responses from servers without a filters field read as unfiltered", () => {
  assert.equal(adaptMinConfidence(undefined), "low");
  assert.equal(adaptMinConfidence("nominal"), "nominal");
  assert.equal(adaptMinConfidence("bogus"), "low");
});

check("filter and atmospheric copy exist in both languages", () => {
  for (const lang of ["en", "tr"] as const) {
    const t = createTranslator(lang);
    assert.ok(t("confidenceFilterOn").length > 0);
    assert.ok(t("confidenceFilterExcluded", { count: 3 }).includes("3"));
    assert.ok(t("methodsAtmosphereNote").includes("7–22"));
  }
});
