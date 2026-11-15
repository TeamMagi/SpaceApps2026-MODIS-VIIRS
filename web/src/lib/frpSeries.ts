import type { MonthCell } from "../types";

export interface AnnualFrpPoint {
  year: number;
  mean: number | null;
  lower: number | null;
  upper: number | null;
  months: number;
}

interface YearAcc {
  year: number;
  weightSum: number;
  meanSum: number;
  lowerSum: number;
  upperSum: number;
  allHaveBounds: boolean;
  months: number;
}

export function annualMeanFrp(months: readonly MonthCell[]): AnnualFrpPoint[] {
  const byYear = new Map<number, YearAcc>();

  for (const m of months) {
    let acc = byYear.get(m.year);
    if (!acc) {
      acc = {
        year: m.year,
        weightSum: 0,
        meanSum: 0,
        lowerSum: 0,
        upperSum: 0,
        allHaveBounds: true,
        months: 0,
      };
      byYear.set(m.year, acc);
    }

    if (
      !m.hasData ||
      !Number.isFinite(m.harmonizedCount) ||
      m.harmonizedCount <= 0 ||
      m.meanFrp === null ||
      !Number.isFinite(m.meanFrp)
    ) {
      continue;
    }

    const w = m.harmonizedCount;
    acc.weightSum += w;
    acc.meanSum += w * m.meanFrp;
    acc.months += 1;

    // Count-weighted averaging of monthly bounds treats months as fully correlated, which is conservative and matches the systematic (linear) term in crates/core/src/uncertainty.rs.
    if (
      acc.allHaveBounds &&
      m.meanFrpLower !== null &&
      Number.isFinite(m.meanFrpLower) &&
      m.meanFrpUpper !== null &&
      Number.isFinite(m.meanFrpUpper)
    ) {
      acc.lowerSum += w * m.meanFrpLower;
      acc.upperSum += w * m.meanFrpUpper;
    } else {
      acc.allHaveBounds = false;
    }
  }

  const out: AnnualFrpPoint[] = [];
  for (const acc of byYear.values()) {
    if (acc.months === 0 || acc.weightSum <= 0) {
      out.push({
        year: acc.year,
        mean: null,
        lower: null,
        upper: null,
        months: 0,
      });
      continue;
    }

    out.push({
      year: acc.year,
      mean: acc.meanSum / acc.weightSum,
      lower: acc.allHaveBounds ? acc.lowerSum / acc.weightSum : null,
      upper: acc.allHaveBounds ? acc.upperSum / acc.weightSum : null,
      months: acc.months,
    });
  }

  return out;
}
