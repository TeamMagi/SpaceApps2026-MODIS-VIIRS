import assert from "node:assert/strict";
import { performance } from "node:perf_hooks";
import { createTranslator } from "../src/content/copy.ts";
import { createFormatter } from "../src/lib/format.ts";
import {
  RADIAL_CENTER,
  RADIAL_INNER_RADIUS,
  RADIAL_MAX_RADIUS,
  RADIAL_MIN_POSITIVE_RADIUS,
  RADIAL_NO_DATA_RADIUS,
  RADIAL_ZERO_COUNT_RADIUS,
  computeMaxMonthCount,
  computeMonthNormalDeltaPct,
  computeMonthlyBaselines,
  computeSliceAngles,
  computeSliceRadius,
  computeSparseYearTicks,
  computeYearTotal,
  describeAnnularSector,
  describeRadialArc,
  formatSliceDetailLine,
  isTooltipAnchorCurrent,
  polarToCartesian,
  resolveActiveYear,
  resolveFocusMonth,
  resolvePlaybackStep,
  resolvePlaybackToggle,
  resolveVisibleYearTicks,
  sliceColorVar,
  stepYear,
} from "../src/lib/radialCalendar.ts";
import type { MonthCell } from "../src/types/index.ts";

const t0 = performance.now();

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

function makeCell(overrides: Partial<MonthCell> & { year: number; month: number }): MonthCell {
  return {
    hasData: true,
    rawModis: 0,
    rawViirs: 0,
    rawCount: 0,
    harmonizedCount: 0,
    anomalyRaw: null,
    anomalyHarmonized: null,
    ...overrides,
  };
}

check("computeMonthlyBaselines excludes no-record months and includes true zero months", () => {
  const cells: MonthCell[] = [
    // July across 4 years: 2000 has no satellite record, 2001 has 0, 2002 has 2000, 2003 has 4000
    makeCell({ year: 2000, month: 7, hasData: false, rawCount: 9999, harmonizedCount: 9999 }),
    makeCell({ year: 2001, month: 7, hasData: true, rawCount: 0, harmonizedCount: 0 }),
    makeCell({ year: 2002, month: 7, hasData: true, rawCount: 2000, harmonizedCount: 1500 }),
    makeCell({ year: 2003, month: 7, hasData: true, rawCount: 4000, harmonizedCount: 3000 }),
  ];

  const baselines = computeMonthlyBaselines(cells);
  assert.equal(baselines.length, 12);

  const july = baselines[6];
  assert.equal(july.month, 7);
  assert.equal(july.sampleCount, 3);
  assert.equal(july.meanRaw, 2000); // (0 + 2000 + 4000) / 3
  assert.equal(july.meanHarmonized, 1500); // (0 + 1500 + 3000) / 3

  const jan = baselines[0];
  assert.equal(jan.month, 1);
  assert.equal(jan.sampleCount, 0);
  assert.equal(jan.meanRaw, null);
  assert.equal(jan.meanHarmonized, null);
});

check("computeMonthNormalDeltaPct calculates percentage from true monthly average, not z-score", () => {
  // Example: mean = 2296.48, count = 3261 -> +42%
  assert.equal(computeMonthNormalDeltaPct(3261, 2296.48), 42);
  assert.equal(computeMonthNormalDeltaPct(70, 100), -30);
  assert.equal(computeMonthNormalDeltaPct(100, 100), 0);
  assert.equal(computeMonthNormalDeltaPct(0, 100), -100);
  assert.equal(computeMonthNormalDeltaPct(100, 0), null);
  assert.equal(computeMonthNormalDeltaPct(100, null), null);
});

check("computeSliceAngles places January at top-right and advances clockwise through December at top-left", () => {
  const jan = computeSliceAngles(1);
  const feb = computeSliceAngles(2);
  const apr = computeSliceAngles(4);
  const jul = computeSliceAngles(7);
  const dec = computeSliceAngles(12);

  assert.ok(jan.startAngle > 0 && jan.endAngle < Math.PI / 6);
  assert.ok(jan.midAngle < feb.midAngle && feb.midAngle < apr.midAngle && apr.midAngle < jul.midAngle && jul.midAngle < dec.midAngle);

  const janPt = polarToCartesian(RADIAL_CENTER, RADIAL_CENTER, 100, jan.midAngle);
  assert.ok(janPt.x > RADIAL_CENTER && janPt.y < RADIAL_CENTER, "January must be in top-right quadrant");

  const julPt = polarToCartesian(RADIAL_CENTER, RADIAL_CENTER, 100, jul.midAngle);
  assert.ok(julPt.x < RADIAL_CENTER && julPt.y > RADIAL_CENTER, "July must be in bottom-left quadrant");

  const decPt = polarToCartesian(RADIAL_CENTER, RADIAL_CENTER, 100, dec.midAngle);
  assert.ok(decPt.x < RADIAL_CENTER && decPt.y < RADIAL_CENTER, "December must be in top-left quadrant");
});

check("computeSliceRadius preserves distinction between no-data, true zero, and positive counts within [46, 138]", () => {
  const rNoData = computeSliceRadius(false, 0, 5000);
  const rZero = computeSliceRadius(true, 0, 5000);
  const rLow = computeSliceRadius(true, 100, 5000);
  const rMean = computeSliceRadius(true, 1000, 5000);
  const rHigh = computeSliceRadius(true, 3000, 5000);
  const rMax = computeSliceRadius(true, 5000, 5000);

  assert.equal(rNoData, RADIAL_NO_DATA_RADIUS);
  assert.equal(rZero, RADIAL_ZERO_COUNT_RADIUS);
  assert.ok(RADIAL_INNER_RADIUS < rNoData && rNoData < rZero);
  assert.ok(rZero < RADIAL_MIN_POSITIVE_RADIUS && RADIAL_MIN_POSITIVE_RADIUS <= rLow);
  assert.ok(rLow < rMean && rMean < rHigh && rHigh < rMax);
  assert.equal(rMax, RADIAL_MAX_RADIUS);
});

check("describeAnnularSector and describeRadialArc emit valid SVG path strings", () => {
  const angles = computeSliceAngles(7);
  const sector = describeAnnularSector(
    RADIAL_CENTER,
    RADIAL_CENTER,
    RADIAL_INNER_RADIUS,
    RADIAL_MAX_RADIUS,
    angles.startAngle,
    angles.endAngle,
  );
  assert.ok(sector.startsWith("M ") && sector.endsWith(" Z"));
  assert.ok(sector.includes(`A ${RADIAL_MAX_RADIUS} ${RADIAL_MAX_RADIUS}`));
  assert.ok(sector.includes(`A ${RADIAL_INNER_RADIUS} ${RADIAL_INNER_RADIUS}`));

  const arc = describeRadialArc(RADIAL_CENTER, RADIAL_CENTER, 95, angles.startAngle, angles.endAngle);
  assert.ok(arc.startsWith("M ") && arc.includes("A 95 95"));
});

check("sliceColorVar distinguishes no-record, true zero, below normal, neutral, and above normal", () => {
  assert.equal(sliceColorVar(false, 0, null, 100), "var(--anom-none)");
  assert.equal(sliceColorVar(true, 0, null, 100), "var(--anom-neg-3)");
  assert.equal(sliceColorVar(true, 0, null, 0), "var(--anom-0)");
  assert.equal(sliceColorVar(true, 40, -1.5, 100), "var(--anom-neg-2)");
  assert.equal(sliceColorVar(true, 250, 2.4, 100), "var(--anom-pos-3)");
  assert.equal(sliceColorVar(true, 102, 0.1, 100), "var(--anom-0)");
});

check("resolveActiveYear, stepYear, resolveFocusMonth, and computeSparseYearTicks handle navigation and bounds", () => {
  assert.equal(
    resolveActiveYear({
      viewYear: null,
      selectedMonth: { year: 2021, month: 7 },
      defaultMapMonth: { year: 2024, month: 8 },
      firstYear: 2000,
      lastYear: 2025,
    }),
    2021,
  );
  assert.equal(
    resolveActiveYear({
      viewYear: 2015,
      selectedMonth: { year: 2021, month: 7 },
      defaultMapMonth: { year: 2024, month: 8 },
      firstYear: 2000,
      lastYear: 2025,
    }),
    2015,
  );
  assert.equal(stepYear(2000, -1, 2000, 2025), 2000);
  assert.equal(stepYear(2025, 1, 2000, 2025), 2025);
  assert.equal(stepYear(2021, -1, 2000, 2025), 2020);

  const ticks = computeSparseYearTicks(2000, 2025, 2012, 2021);
  assert.deepEqual([...ticks].sort((a, b) => a - b), [2000, 2012, 2021, 2025]);

  const yearCells = Array.from({ length: 12 }, (_, i) =>
    makeCell({ year: 2021, month: i + 1, rawCount: (i + 1) === 8 ? 500 : 10, harmonizedCount: (i + 1) === 8 ? 400 : 8 }),
  );
  assert.equal(
    resolveFocusMonth({
      yearCells,
      selectedMonth: null,
      defaultMapMonth: null,
      hoveredMonth: null,
      focusedMonth: null,
      harmonized: false,
    }),
    8,
  );
  assert.equal(computeYearTotal(2021, [], yearCells, false), 500 + 11 * 10);
  assert.equal(computeMaxMonthCount(yearCells, computeMonthlyBaselines(yearCells), false), 500);
});

check("formatSliceDetailLine formats TR and EN summary and respects revealAnswer gate", () => {
  const trCopy = { t: createTranslator("tr"), fmt: createFormatter("tr") };
  const enCopy = { t: createTranslator("en"), fmt: createFormatter("en") };

  const july2021 = makeCell({
    year: 2021,
    month: 7,
    hasData: true,
    rawCount: 3261,
    harmonizedCount: 2100,
    anomalyRaw: 2.8, // z-score must NOT be used as percentage
    anomalyHarmonized: 1.9,
  });

  // Raw mode (revealAnswer = false): uses rawCount (3261) and rawMean (2296.48 -> +42%)
  const trRawLine = formatSliceDetailLine(july2021, 2296.48, false, trCopy);
  assert.equal(trRawLine, "Temmuz 2021 · normalinden %42 fazla · 3.261 tespit");

  const enRawLine = formatSliceDetailLine(july2021, 2296.48, false, enCopy);
  assert.equal(enRawLine, "July 2021 · 42% above normal · 3,261 detections");

  // Harmonized mode (revealAnswer = true): uses harmonizedCount (2100) and harmMean (3000 -> -30%)
  const trHarmLine = formatSliceDetailLine(july2021, 3000, true, trCopy);
  assert.equal(trHarmLine, "Temmuz 2021 · normalinden %30 az · 2.100 tespit");

  // No satellite record
  const noDataCell = makeCell({ year: 2000, month: 1, hasData: false });
  assert.equal(
    formatSliceDetailLine(noDataCell, 100, false, trCopy),
    "Ocak 2000 · uydu kaydı yok",
  );
});

check("resolvePlaybackToggle, resolvePlaybackStep, and isTooltipAnchorCurrent keep playback and tooltip synced", () => {
  assert.deepEqual(
    resolvePlaybackToggle({ isPlaying: true, canPlay: true, activeYear: 2010, firstYear: 2000, lastYear: 2025 }),
    { nextPlaying: false, restartYear: null },
  );
  assert.deepEqual(
    resolvePlaybackToggle({ isPlaying: false, canPlay: false, activeYear: 2010, firstYear: 2000, lastYear: 2025 }),
    { nextPlaying: false, restartYear: null },
  );
  assert.deepEqual(
    resolvePlaybackToggle({ isPlaying: false, canPlay: true, activeYear: 2025, firstYear: 2000, lastYear: 2025 }),
    { nextPlaying: true, restartYear: 2000 },
  );
  assert.deepEqual(
    resolvePlaybackToggle({ isPlaying: false, canPlay: true, activeYear: 2010, firstYear: 2000, lastYear: 2025 }),
    { nextPlaying: true, restartYear: null },
  );

  assert.deepEqual(resolvePlaybackStep(null, 2000, 2025), { nextYear: 2001, finished: false });
  assert.deepEqual(resolvePlaybackStep(2023, 2000, 2025), { nextYear: 2024, finished: false });
  assert.deepEqual(resolvePlaybackStep(2024, 2000, 2025), { nextYear: 2025, finished: true });

  assert.equal(isTooltipAnchorCurrent({ year: 2025, month: 6 }, 2025, 6), true);
  assert.equal(isTooltipAnchorCurrent({ year: 2024, month: 6 }, 2025, 6), false);
  assert.equal(isTooltipAnchorCurrent({ year: 2025, month: 7 }, 2025, 6), false);
  assert.equal(isTooltipAnchorCurrent(null, 2025, 6), false);
});

check("resolveVisibleYearTicks prevents tick overlap near cliff and boundary years", () => {
  assert.deepEqual(
    [...resolveVisibleYearTicks(2000, 2025, 2012, 2018)].sort((a, b) => a - b),
    [2000, 2012, 2018, 2025],
  );
  // Active year 2013 is adjacent to cliff year 2012: cliff year stays, active tick hides
  assert.deepEqual(
    [...resolveVisibleYearTicks(2000, 2025, 2012, 2013)].sort((a, b) => a - b),
    [2000, 2012, 2025],
  );
  // Active year 2024 is adjacent to lastYear 2025: active tick stays, lastYear hides
  assert.deepEqual(
    [...resolveVisibleYearTicks(2000, 2025, 2012, 2024)].sort((a, b) => a - b),
    [2000, 2012, 2024],
  );
  // Active year 2001 is adjacent to firstYear 2000: active tick stays, firstYear hides
  assert.deepEqual(
    [...resolveVisibleYearTicks(2000, 2025, 2012, 2001)].sort((a, b) => a - b),
    [2001, 2012, 2025],
  );
});

console.log(`radialCalendar.test.ts completed in ${(performance.now() - t0).toFixed(2)} ms`);
