import type { RegionId, ValidationBaseline, ValidationDecision, ValidationGate, ValidationRegionGate } from "../types";

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function finite(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isDecision(value: unknown): value is ValidationDecision {
  return value === "pass" || value === "inconclusive" || value === "fail";
}

export function regionGateDecision(baselines: readonly ValidationBaseline[]): ValidationDecision {
  if (baselines.some((baseline) => baseline.decision === "fail")) return "fail";
  if (baselines.length !== 2 || baselines.some((baseline) => baseline.decision === "inconclusive")) {
    return "inconclusive";
  }
  return "pass";
}

function parseBaseline(value: unknown): ValidationBaseline | null {
  const wire = record(value);
  if (!wire || (wire.name !== "b0_unnormalized" && wire.name !== "b1_persistence")) return null;
  const ci = wire.ci90;
  if (!finite(wire.baseline_error) || wire.baseline_error < 0 || !Array.isArray(ci) || ci.length !== 2) return null;
  const [lo, hi] = ci as unknown[];
  if (!finite(lo) || !finite(hi) || lo > hi) return null;
  const decision = lo > 0 ? "pass" : hi < 0 ? "fail" : "inconclusive";
  if (wire.decision !== decision) return null;
  return { name: wire.name, baselineError: wire.baseline_error, ci90: [lo, hi], decision };
}

export function parseValidationGate(value: unknown): ValidationGate | null {
  const wire = record(value);
  if (!wire || typeof wire.generated !== "string" || !Number.isFinite(Date.parse(wire.generated))
    || typeof wire.method !== "string") return null;
  const regions = record(wire.regions);
  if (!regions) return null;
  const parsed: Record<string, ValidationRegionGate> = Object.create(null) as Record<string, ValidationRegionGate>;
  for (const [name, value] of Object.entries(regions)) {
    const region = record(value);
    if (!region || !finite(region.model_error) || region.model_error < 0 || !isDecision(region.decision)
      || !Array.isArray(region.baselines) || region.baselines.length !== 2) return null;
    const baselines = region.baselines.map(parseBaseline);
    const [b0, b1] = baselines;
    if (!b0 || !b1 || b0.name === b1.name) return null;
    parsed[name] = {
      modelError: region.model_error,
      baselines: [b0, b1],
      decision: regionGateDecision([b0, b1]),
    };
  }
  return { generated: wire.generated, method: wire.method, regions: parsed };
}

export function validationForRegion(gate: ValidationGate | null, region: RegionId): ValidationRegionGate | null {
  return gate && Object.prototype.hasOwnProperty.call(gate.regions, region) ? gate.regions[region] : null;
}

let gateRequest: Promise<ValidationGate | null> | undefined;

export function loadValidationGate(): Promise<ValidationGate | null> {
  gateRequest ??= (async () => {
    try {
      const response = await fetch("/validation/gate.json");
      return response.ok ? parseValidationGate(await response.json()) : null;
    } catch {
      return null;
    }
  })();
  return gateRequest;
}
