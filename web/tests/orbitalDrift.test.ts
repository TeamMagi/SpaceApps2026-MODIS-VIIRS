import assert from "node:assert/strict";
import { createTranslator } from "../src/content/copy.ts";
import {
  buildDriftRows,
  demoPeriodIsFlagged,
  driftStripState,
  formatClock,
  periodHasDriftFlag,
  signedMinuteDiff,
} from "../src/lib/orbitalDrift.ts";
import { adaptOverpass } from "../src/services/api.ts";
import { DayNight, type CalendarCell, type OverpassMedian } from "../src/types/index.ts";

function check(name: string, fn: () => void): void {
  fn();
  console.log(`ok - ${name}`);
}

function median(platform: string, daynight: DayNight, minutes: number, n = 100): OverpassMedian {
  return { platform, daynight, medianLocalSolarTime: formatClock(minutes), medianMinutes: minutes, n };
}

check("clock formatting pads, rounds and wraps", () => {
  assert.equal(formatClock(630), "10:30");
  assert.equal(formatClock(9 * 60 + 15), "09:15");
  assert.equal(formatClock(1439.6), "00:00");
  assert.equal(formatClock(-30), "23:30");
  assert.equal(formatClock(1440 + 90), "01:30");
});

check("signed difference takes the short way round midnight", () => {
  assert.equal(signedMinuteDiff(10 * 60 + 12, 10 * 60 + 30), -18);
  assert.equal(signedMinuteDiff(13 * 60 + 51, 13 * 60 + 30), 21);
  assert.equal(signedMinuteDiff(10, 23 * 60 + 50), 20);
  assert.equal(signedMinuteDiff(23 * 60 + 50, 10), -20);
  assert.equal(signedMinuteDiff(0, 0), 0);
  assert.equal(signedMinuteDiff(720, 0), 720);
});

check("drift rows cover Terra and Aqua only, ordered, against the nominal times", () => {
  const rows = buildDriftRows([
    median("S-NPP", DayNight.Day, 13 * 60 + 30),
    median("Aqua", DayNight.Night, 60 + 40),
    median("Aqua", DayNight.Day, 13 * 60 + 51),
    median("Terra", DayNight.Night, 22 * 60 + 10),
    median("Terra", DayNight.Day, 10 * 60 + 12),
  ]);
  assert.deepEqual(
    rows.map((r) => [r.platform, r.daynight, r.medianTime, r.nominalTime, r.diffMinutes]),
    [
      ["Terra", DayNight.Day, "10:12", "10:30", -18],
      ["Terra", DayNight.Night, "22:10", "22:30", -20],
      ["Aqua", DayNight.Day, "13:51", "13:30", 21],
      ["Aqua", DayNight.Night, "01:40", "01:30", 10],
    ],
  );
  assert.deepEqual(buildDriftRows(null), []);
  assert.deepEqual(buildDriftRows(undefined), []);
});

check("an Aqua night median before midnight is compared with 01:30 across the wrap", () => {
  const [row] = buildDriftRows([median("Aqua", DayNight.Night, 23 * 60 + 50)]);
  assert.equal(row.diffMinutes, -100);
  assert.equal(row.medianTime, "23:50");
});

check("the strip is hidden without the flag or a deviation, and shown for either", () => {
  const near = buildDriftRows([
    median("Terra", DayNight.Day, 10 * 60 + 35),
    median("Aqua", DayNight.Day, 13 * 60 + 25),
  ]);
  const far = buildDriftRows([median("Terra", DayNight.Day, 9 * 60 + 40)]);
  assert.equal(driftStripState({ flagged: false, rows: near, dismissed: false }).visible, false);
  assert.equal(driftStripState({ flagged: false, rows: [], dismissed: false }).visible, false);
  const flagged = driftStripState({ flagged: true, rows: near, dismissed: false });
  assert.deepEqual([flagged.visible, flagged.reason, flagged.showNumbers], [true, "flag", true]);
  const deviating = driftStripState({ flagged: false, rows: far, dismissed: false });
  assert.deepEqual([deviating.visible, deviating.reason], [true, "deviation"]);
  const demo = driftStripState({ flagged: true, rows: [], dismissed: false });
  assert.deepEqual([demo.visible, demo.showNumbers], [true, false]);
  assert.equal(driftStripState({ flagged: true, rows: far, dismissed: true }).visible, false);
});

function cell(year: number, month: number, flags: string[]): CalendarCell {
  return { year, month, qualityFlags: flags } as unknown as CalendarCell;
}

check("the flag is read from the period's calendar cells", () => {
  const cells = [cell(2020, 12, []), cell(2021, 3, ["modis_orbital_drift"]), cell(2021, 4, [])];
  assert.equal(periodHasDriftFlag(cells, { year: 2021, month: null }), true);
  assert.equal(periodHasDriftFlag(cells, { year: 2021, month: 3 }), true);
  assert.equal(periodHasDriftFlag(cells, { year: 2021, month: 4 }), false);
  assert.equal(periodHasDriftFlag(cells, { year: 2020, month: null }), false);
  assert.equal(demoPeriodIsFlagged({ year: 2021, month: 8 }), true);
  assert.equal(demoPeriodIsFlagged({ year: 2019, month: null }), false);
  assert.equal(demoPeriodIsFlagged(null), false);
});

check("overpass adapter: present, empty, malformed entries and a server without the field", () => {
  const wire = [
    { platform: "Terra", daynight: "Day", median_local_solar_time: "10:12", median_minutes: 612.3, n: 812 },
    { platform: "Aqua", daynight: "Night", median_local_solar_time: "01:40", median_minutes: 100, n: 44 },
    { platform: "S-NPP", daynight: "Dusk", median_local_solar_time: "13:30", median_minutes: 810, n: 40 },
    { platform: "Terra" },
  ];
  assert.deepEqual(adaptOverpass(wire), [
    { platform: "Terra", daynight: DayNight.Day, medianLocalSolarTime: "10:12", medianMinutes: 612.3, n: 812 },
    { platform: "Aqua", daynight: DayNight.Night, medianLocalSolarTime: "01:40", medianMinutes: 100, n: 44 },
  ]);
  assert.deepEqual(adaptOverpass([]), []);
  assert.equal(adaptOverpass(undefined), null);
  assert.equal(adaptOverpass(null), null);
  assert.equal(adaptOverpass({}), null);
});

check("drift copy exists in EN and TR and states the caveat and the literature range", () => {
  for (const lang of ["en", "tr"] as const) {
    const t = createTranslator(lang);
    assert.ok(t("driftCaveat").length > 40);
    assert.ok(t("driftFlagBody").includes("2,8") || t("driftFlagBody").includes("2.8"));
    assert.ok(t("driftDeviationBody").includes("5,5") || t("driftDeviationBody").includes("5.5"));
    assert.ok(
      t("driftRow", { platform: "Terra", pass: "x", time: "10:12", nominal: "10:30", diff: "-18" }).includes("10:12"),
    );
    assert.ok(t("methodsBandsConformal").length > 0 && t("methodsBandsDrift").length > 0);
  }
});
