import assert from "node:assert/strict";
import { buildCardModel, platformLabel, fromLiveStats, metricValue, removalRatio, sparkWindow, trendVs, type StatsDataset, type StatsMonth } from "../src/lib/stats.ts";
import { mockAnnualStats, mockMonthlyStats, mockStatsDataset } from "../src/mocks/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

const near = (actual: number | null, expected: number, tolerance = 1e-9): void => {
  assert.ok(actual !== null && Math.abs(actual - expected) <= tolerance, `${actual} != ${expected}`);
};
const month = (over: Partial<StatsMonth> = {}): StatsMonth => ({
  year: 2020, month: 7, modisCovered: true, viirsCovered: true,
  rawCount: 50, rawFrpSum: 1300, harmonizedCount: 30, harmonizedMeanFrp: 100, platforms: ["ModisAqua", "ViirsSNPP"],
  ...over,
});
const dataset = (months: StatsMonth[]): StatsDataset => ({ source: "live", months, coverageThrough: null, generatedAt: null });
const index = (months: StatsMonth[]) => new Map(months.map(m => [m.year * 12 + m.month - 1, m]));

check("removal ratio is 0-1 and null for raw 0", () => {
  near(removalRatio(50, 30), 0.4);
  near(removalRatio(10, 10), 0);
  assert.equal(removalRatio(0, 0), null);
  assert.equal(metricValue(month({ rawCount: 0, harmonizedCount: 0 }), "removed", "harmonized"), null);
  assert.equal(metricValue(month(), "removed", "raw"), null);
});

check("harmonized count 0 in a covered month is a real zero; uncovered is null", () => {
  assert.equal(metricValue(month({ harmonizedCount: 0 }), "detections", "harmonized"), 0);
  near(metricValue(month({ harmonizedCount: 0 }), "removed", "harmonized"), 1);
  assert.equal(metricValue(month({ modisCovered: false, viirsCovered: false }), "detections", "raw"), null);
});

check("raw mean FRP is total / total; harmonized mean is the cell value, null when missing", () => {
  near(metricValue(month({ rawCount: 4, rawFrpSum: 100 }), "meanFrp", "raw"), 25);
  assert.equal(metricValue(month({ rawCount: 0, rawFrpSum: 0 }), "meanFrp", "raw"), null);
  near(metricValue(month(), "meanFrp", "harmonized"), 100);
  assert.equal(metricValue(month({ harmonizedMeanFrp: null }), "meanFrp", "harmonized"), null);
});

check("platforms: unknown is null, empty is zero, same in both modes", () => {
  assert.equal(metricValue(month({ platforms: null }), "platforms", "raw"), null);
  assert.equal(metricValue(month({ platforms: [] }), "platforms", "raw"), 0);
  assert.equal(metricValue(month(), "platforms", "raw"), 2);
  assert.equal(metricValue(month(), "platforms", "harmonized"), 2);
});

check("trend: normal up/down, equal flat", () => {
  const prev = month({ year: 2019, rawCount: 40 });
  const up = trendVs(index([prev, month({ rawCount: 50 })]), { year: 2020, month: 7 }, "detections", "raw");
  assert.equal(up.kind === "change" && up.direction, "up");
  near(up.kind === "change" ? up.ratio : null, 0.25);
  const down = trendVs(index([prev, month({ rawCount: 20 })]), { year: 2020, month: 7 }, "detections", "raw");
  assert.equal(down.kind === "change" && down.direction, "down");
  const flat = trendVs(index([prev, month({ rawCount: 40 })]), { year: 2020, month: 7 }, "detections", "raw");
  assert.equal(flat.kind === "change" && flat.direction, "flat");
});

check("removed trend is absolute percentage points, zero base allowed", () => {
  const prev = month({ year: 2019, rawCount: 100, harmonizedCount: 60 });
  const t = trendVs(index([prev, month({ rawCount: 100, harmonizedCount: 50 })]), { year: 2020, month: 7 }, "removed", "harmonized");
  assert.ok(t.kind === "change" && t.unit === "points" && t.direction === "up");
  near(t.kind === "change" ? t.ratio : null, 0.1);
  const z = trendVs(index([month({ year: 2019, rawCount: 10, harmonizedCount: 10 }), month()]), { year: 2020, month: 7 }, "removed", "harmonized");
  assert.ok(z.kind === "change");
  const r = trendVs(index([month({ year: 2019, rawCount: 40 }), month()]), { year: 2020, month: 7 }, "detections", "raw");
  assert.ok(r.kind === "change" && r.unit === "relative");
});

check("trend: zero base, uncovered or missing previous year, different coverage, missing value", () => {
  const sel = { year: 2020, month: 7 };
  const zero = trendVs(index([month({ year: 2019, rawCount: 0 }), month()]), sel, "detections", "raw");
  assert.deepEqual(zero.kind === "none" && zero.reason, "zeroBase");
  const none = trendVs(index([month()]), sel, "detections", "raw");
  assert.deepEqual(none.kind === "none" && none.reason, "noPrevious");
  const unc = trendVs(index([month({ year: 2019, modisCovered: false, viirsCovered: false }), month()]), sel, "detections", "raw");
  assert.deepEqual(unc.kind === "none" && unc.reason, "notCovered");
  const diff = trendVs(index([month({ year: 2019, viirsCovered: false }), month()]), sel, "detections", "raw");
  assert.deepEqual(diff.kind === "none" && diff.reason, "differentCoverage");
  const miss = trendVs(index([month({ year: 2019, platforms: null }), month()]), sel, "platforms", "raw");
  assert.deepEqual(miss.kind === "none" && miss.reason, "missingValue");
});

check("sparkline window is the 12 months ending at the selection, gaps are null", () => {
  const w = sparkWindow({ year: 2020, month: 3 });
  assert.equal(w.length, 12);
  assert.deepEqual(w[0], { year: 2019, month: 4 });
  assert.deepEqual(w[11], { year: 2020, month: 3 });
  const model = buildCardModel(dataset([month({ year: 2020, month: 2, rawCount: 7 }), month({ year: 2020, month: 3, rawCount: 9 })]), { year: 2020, month: 3 }, "raw");
  const card = model.cards.find(c => c.id === "detections")!;
  assert.deepEqual(card.spark.slice(0, 9), Array(9).fill(null));
  assert.deepEqual(card.spark.slice(10), [7, 9]);
  assert.equal(card.value, 9);
  assert.equal(model.cards.find(c => c.id === "removed")!.hidden, true);
});

const liveJson = (platformCounts?: unknown) => ({
  calendar: [{ year: 2020, month: 7, count: 30, count_normalized: 25, total_frp: 2500, frp_per_detection_eq: 100 }],
  raw_monthly: [
    { year: 2020, month: 6, modis_covered: true, modis_count: 3, modis_frp_sum: 30, viirs_covered: true, viirs_count: 1, viirs_frp_sum: 10, ...(platformCounts === undefined ? {} : { platform_counts: platformCounts }) },
    { year: 2020, month: 7, modis_covered: true, modis_count: 10, modis_frp_sum: 500, viirs_covered: true, viirs_count: 40, viirs_frp_sum: 800, ...(platformCounts === undefined ? {} : { platform_counts: platformCounts }) },
    { year: 2020, month: 8, modis_covered: false, modis_count: 0, modis_frp_sum: 0, viirs_covered: false, viirs_count: 0, viirs_frp_sum: 0 },
  ],
});

check("parse live with platform_counts", () => {
  const d = fromLiveStats(liveJson({ ModisAqua: 10, ViirsSNPP: 40, ViirsNOAA20: 0 }));
  const m = d.months[1]!;
  assert.equal(m.rawCount, 50);
  near(m.rawFrpSum, 1300);
  assert.equal(m.harmonizedCount, 30);
  near(m.harmonizedMeanFrp, 100);
  assert.deepEqual(m.platforms, ["ModisAqua", "ViirsSNPP"]);
  assert.equal(d.months[0]!.harmonizedCount, 0);
  assert.equal(d.months[0]!.harmonizedMeanFrp, null);
  assert.deepEqual(d.coverageThrough, { year: 2020, month: 7 });
});

check("parse live without platform_counts leaves platforms unknown", () => {
  const d = fromLiveStats(liveJson());
  assert.equal(d.months[1]!.platforms, null);
  assert.equal(metricValue(d.months[1], "platforms", "raw"), null);
  assert.throws(() => fromLiveStats({ calendar: [] }), /raw_monthly/);
});

check("mock card model matches fixture totals", () => {
  const annual = mockMonthlyStats.reduce((a, s) => ({ raw: a.raw + s.rawCount, harm: a.harm + s.harmonizedCount }), { raw: 0, harm: 0 });
  assert.equal(annual.raw, mockAnnualStats.rawCount);
  assert.equal(annual.harm, mockAnnualStats.harmonizedCount);
  const sel = { year: 2021, month: 1 };
  const snap = mockMonthlyStats[0]!;
  const raw = buildCardModel(mockStatsDataset, sel, "raw");
  const harm = buildCardModel(mockStatsDataset, sel, "harmonized");
  assert.equal(raw.cards[0]!.value, snap.rawCount);
  assert.equal(harm.cards[0]!.value, snap.harmonizedCount);
  near(harm.cards[1]!.value, (snap.rawCount - snap.harmonizedCount) / snap.rawCount);
  near(raw.cards[3]!.value, snap.rawFrpSum / snap.rawCount, 1e-2);
  assert.equal(raw.cards[2]!.value, snap.platforms.length);
  assert.equal(harm.cards[2]!.value, raw.cards[2]!.value);
  let total = 0;
  for (let m = 1; m <= 12; m++) total += buildCardModel(mockStatsDataset, { year: 2021, month: m }, "raw").cards[0]!.value ?? 0;
  assert.equal(total, 800);
  total = 0;
  for (let m = 1; m <= 12; m++) total += buildCardModel(mockStatsDataset, { year: 2021, month: m }, "harmonized").cards[0]!.value ?? 0;
  assert.equal(total, 500);
  assert.equal(mockStatsDataset.generatedAt, "2026-11-15T00:00:00Z");
});

check("mock platforms are unknown outside the detailed year", () => {
  const card = buildCardModel(mockStatsDataset, { year: 2020, month: 6 }, "raw").cards[2]!;
  assert.equal(card.value, null);
});

check("platform labels and stable order", () => {
  assert.equal(platformLabel("ViirsNOAA20"), "NOAA-20");
  const d = fromLiveStats(liveJson({ ViirsNOAA20: 1, ViirsSNPP: 1, ModisAqua: 1, ModisTerra: 1, ViirsNOAA21: 1 }));
  assert.deepEqual(d.months[1]!.platforms, ["ModisTerra", "ModisAqua", "ViirsSNPP", "ViirsNOAA20", "ViirsNOAA21"]);
});

