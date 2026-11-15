import type { CliffSummary } from "../types";

export type HeadlineFigure =
  | { readonly kind: "noBaseline" }
  | { readonly kind: "raw"; readonly absPct: number; readonly dropped: boolean }
  | { readonly kind: "harmonized"; readonly ratio: number };

/**
 * The one figure the headline states: the raw change in Raw, the harmonized change once revealed.
 * The server sends a null change when the earlier year has no detections to divide by; that must
 * read as "no baseline", never as a change of 0 %.
 */
export function headlineFigure(cliff: CliffSummary, revealAnswer: boolean): HeadlineFigure {
  if (revealAnswer) {
    return cliff.harmonizedChangeRatio === null
      ? { kind: "noBaseline" }
      : { kind: "harmonized", ratio: cliff.harmonizedChangeRatio };
  }
  return cliff.rawChangePct === null
    ? { kind: "noBaseline" }
    : { kind: "raw", absPct: Math.round(Math.abs(cliff.rawChangePct)), dropped: cliff.rawChangePct < 0 };
}
