import assert from "node:assert/strict";
import test from "node:test";
import reference from "../public/validation/gate.json";
import { createTranslator } from "../src/content/copy";
import {
  loadValidationGate,
  parseValidationGate,
  regionGateDecision,
  validationForRegion,
} from "../src/services/validationGate";
import type { ValidationBaseline, ValidationDecision } from "../src/types";

function baseline(name: ValidationBaseline["name"], decision: ValidationDecision): ValidationBaseline {
  return {
    name,
    baselineError: 0.5,
    ci90: decision === "pass" ? [0.1, 0.3] : decision === "fail" ? [-0.3, -0.1] : [-0.1, 0.1],
    decision,
  };
}

function document(b0: ValidationDecision, b1: ValidationDecision) {
  return {
    generated: "2026-11-15T00:00:00Z",
    method: "year-blocked LOYO, bootstrap 90% CI, B=1000, seed=42",
    regions: {
      Russian_Federation: {
        model_error: 0.0368,
        baselines: [baseline("b0_unnormalized", b0), baseline("b1_persistence", b1)]
          .map((b) => ({ name: b.name, baseline_error: b.baselineError, ci90: [...b.ci90], decision: b.decision })),
        decision: "pass",
      },
    },
  };
}

test("reference file parses all ten archive countries and both passing baselines", () => {
  const gate = parseValidationGate(reference);
  assert.ok(gate);
  assert.equal(gate.generated, "2026-11-15T00:00:00Z");
  assert.equal(Object.keys(gate.regions).length, 10);
  for (const region of Object.values(gate.regions)) {
    assert.equal(region.decision, "pass");
    assert.deepEqual(region.baselines.map((b) => b.name), ["b0_unnormalized", "b1_persistence"]);
  }
  const russia = validationForRegion(gate, "Russian_Federation");
  assert.equal(russia?.modelError, 0.0368);
  assert.deepEqual(russia?.baselines[1].ci90, [0.2535, 0.4714]);
  assert.equal(validationForRegion(gate, "United_States")?.modelError, 0.0343);
  assert.equal(validationForRegion(gate, "Turkey")?.modelError, 0.0556);
});

test("region decisions use fail, inconclusive, pass precedence regardless of baseline order", () => {
  for (const b0 of ["pass", "inconclusive", "fail"] as const) {
    for (const b1 of ["pass", "inconclusive", "fail"] as const) {
      const expected = b0 === "fail" || b1 === "fail" ? "fail"
        : b0 === "inconclusive" || b1 === "inconclusive" ? "inconclusive" : "pass";
      const pair = [baseline("b0_unnormalized", b0), baseline("b1_persistence", b1)];
      assert.equal(regionGateDecision(pair), expected);
      assert.equal(regionGateDecision([...pair].reverse()), expected);
      assert.equal(validationForRegion(parseValidationGate(document(b0, b1)), "Russian_Federation")?.decision, expected);
    }
  }
  assert.equal(regionGateDecision([]), "inconclusive");
});

test("absent files and unknown regions have no validation status", () => {
  assert.equal(validationForRegion(null, "Turkey"), null);
  const gate = parseValidationGate(reference);
  for (const region of ["Greece", "Russian", "Russian Federation", "toString", "__proto__"]) {
    assert.equal(validationForRegion(gate, region), null);
  }
});

test("invalid documents, non-finite numbers and incomplete baselines are unavailable", () => {
  for (const value of [null, [], "{}", {}, { ...reference, generated: "invalid" }, { ...reference, regions: [] }]) {
    assert.equal(parseValidationGate(value), null);
  }
  const mutations = [
    (r: ReturnType<typeof document>["regions"]["Russian_Federation"]) => { r.model_error = NaN; },
    (r: ReturnType<typeof document>["regions"]["Russian_Federation"]) => { r.baselines.pop(); },
    (r: ReturnType<typeof document>["regions"]["Russian_Federation"]) => { r.baselines[1] = r.baselines[0]; },
    (r: ReturnType<typeof document>["regions"]["Russian_Federation"]) => { r.baselines[0].ci90 = [2, 1]; },
    (r: ReturnType<typeof document>["regions"]["Russian_Federation"]) => { r.baselines[0].ci90 = [0, Infinity]; },
    (r: ReturnType<typeof document>["regions"]["Russian_Federation"]) => { r.baselines[0].decision = "fail"; },
  ];
  for (const mutate of mutations) {
    const value = document("pass", "pass");
    mutate(value.regions.Russian_Federation);
    assert.equal(parseValidationGate(value), null);
  }
});

test("a CI touching zero is inconclusive", () => {
  for (const ci of [[0, 0.1], [-0.1, 0], [0, 0]]) {
    const value = document("inconclusive", "pass");
    value.regions.Russian_Federation.baselines[0].ci90 = ci;
    assert.equal(validationForRegion(parseValidationGate(value), "Russian_Federation")?.decision, "inconclusive");
  }
});

test("gate copy covers all four statuses in English and Turkish", () => {
  for (const lang of ["en", "tr"] as const) {
    const t = createTranslator(lang);
    assert.ok(t("validationGatePass", { error: "0.0368" }).includes("0.0368"));
    for (const key of ["validationGateLabel", "validationGateInconclusive", "validationGateFail", "validationGateUnavailable"] as const) {
      assert.ok(t(key).length > 0);
    }
  }
  assert.equal(createTranslator("en")("validationGateFail"), "Does not beat baseline · interpret with care");
});

test("gate loading caches a missing file as unavailable without retrying", async (t) => {
  const fetch = t.mock.method(globalThis, "fetch", async () => new Response(null, { status: 404 }));
  const first = loadValidationGate();
  assert.strictEqual(loadValidationGate(), first);
  assert.equal(await first, null);
  assert.equal(await loadValidationGate(), null);
  assert.equal(fetch.mock.callCount(), 1);
  assert.equal(fetch.mock.calls[0].arguments[0], "/validation/gate.json");
});
