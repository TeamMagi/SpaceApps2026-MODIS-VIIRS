import assert from "node:assert/strict";
import test from "node:test";
import { createTranslator } from "../src/content/copy.ts";
import { nextTourStep, prevTourStep, TOUR_STEPS, tourTotalMs } from "../src/lib/tour";

test("tour has four steps in order with 15 s windows", () => {
  assert.deepEqual(
    TOUR_STEPS.map((step) => step.id),
    ["toggle", "lens", "case", "validation"],
  );
  assert.deepEqual(
    TOUR_STEPS.map((step) => step.startSec),
    [0, 15, 30, 45],
  );
  assert.equal(tourTotalMs(), 60000);
});

test("step navigation clamps at both ends", () => {
  assert.equal(nextTourStep(0), 1);
  assert.equal(nextTourStep(3), null);
  assert.equal(prevTourStep(0), 0);
  assert.equal(prevTourStep(2), 1);
});

test("only the validation step lives on the methods route", () => {
  const methods = TOUR_STEPS.filter((step) => step.route === "methods").map((step) => step.id);
  assert.deepEqual(methods, ["validation"]);
});

test("every step has non-empty EN and TR title and body", () => {
  const keys = {
    toggle: ["tourStepToggleTitle", "tourStepToggleBody"],
    lens: ["tourStepLensTitle", "tourStepLensBody"],
    case: ["tourStepCaseTitle", "tourStepCaseBody"],
    validation: ["tourStepValidationTitle", "tourStepValidationBody"],
  } as const;
  for (const lang of ["en", "tr"] as const) {
    const t = createTranslator(lang);
    for (const step of TOUR_STEPS) {
      const [title, body] = keys[step.id];
      assert.ok(t(title).length > 0);
      if (step.id === "lens") assert.ok(t("tourStepLensBody", { lens: "x" }).length > 0);
      else assert.ok((t as (key: string) => string)(body).length > 0);
    }
  }
});

test("the validation step points at the conformal band and the Orbital Drift strip", () => {
  const validation = TOUR_STEPS.find((step) => step.id === "validation");
  assert.ok(validation?.targets.includes("bands"));
  assert.match(createTranslator("en")("tourStepValidationBody"), /Mondrian.*Orbital Drift/);
  assert.match(createTranslator("tr")("tourStepValidationBody"), /Mondrian.*Yörünge Sürüklenmesi/);
});
