import assert from "node:assert/strict";
import { createTranslator } from "../src/content/copy.ts";
import { resolveMonthForYear, useAppStore } from "../src/store/useAppStore.ts";
import type { MonthCell } from "../src/types/index.ts";

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

check("resolveMonthForYear keeps currently shown month when it has data in the target year", () => {
  const months: MonthCell[] = [
    makeCell({ year: 2021, month: 6, hasData: true, rawCount: 120, harmonizedCount: 80 }),
    makeCell({ year: 2021, month: 8, hasData: true, rawCount: 950, harmonizedCount: 600 }),
  ];

  assert.deepEqual(
    resolveMonthForYear({ year: 2021, months, currentMonth: 6, harmonized: false }),
    { year: 2021, month: 6 },
  );
});

check("resolveMonthForYear falls back to busiest month when current month has no data in the target year", () => {
  const months: MonthCell[] = [
    makeCell({ year: 2000, month: 6, hasData: false, rawCount: 0, harmonizedCount: 0 }),
    makeCell({ year: 2000, month: 11, hasData: true, rawCount: 420, harmonizedCount: 180 }),
    makeCell({ year: 2000, month: 12, hasData: true, rawCount: 310, harmonizedCount: 240 }),
  ];

  // Raw mode picks month 11 (420 > 310)
  assert.deepEqual(
    resolveMonthForYear({ year: 2000, months, currentMonth: 6, harmonized: false }),
    { year: 2000, month: 11 },
  );

  // Harmonized mode picks month 12 (240 > 180)
  assert.deepEqual(
    resolveMonthForYear({ year: 2000, months, currentMonth: 6, harmonized: true }),
    { year: 2000, month: 12 },
  );
});

check("useAppStore shares calendarYear across setCalendarYear, selectMonth, toggleMonth, and setRegion", () => {
  const store = useAppStore.getState();
  store.setRegion("Turkey");
  store.setCalendarYear(2015);
  assert.equal(useAppStore.getState().calendarYear, 2015);

  store.selectMonth({ year: 2021, month: 8 });
  assert.equal(useAppStore.getState().calendarYear, 2021);
  assert.deepEqual(useAppStore.getState().selectedMonth, { year: 2021, month: 8 });

  // Clearing month filter keeps viewed calendarYear
  store.selectMonth(null);
  assert.equal(useAppStore.getState().calendarYear, 2021);
  assert.equal(useAppStore.getState().selectedMonth, null);

  store.toggleMonth({ year: 2012, month: 7 });
  assert.equal(useAppStore.getState().calendarYear, 2012);
  assert.deepEqual(useAppStore.getState().selectedMonth, { year: 2012, month: 7 });

  // Changing region resets both selectedMonth and calendarYear
  store.setRegion("Greece");
  assert.equal(useAppStore.getState().calendarYear, null);
  assert.equal(useAppStore.getState().selectedMonth, null);
});

check("copy keys format calendarYearScope and calendarSliceHint in EN and TR", () => {
  const enT = createTranslator("en");
  const trT = createTranslator("tr");

  assert.equal(enT("calendarYearScope", { year: "2025" }), "in 2025");
  assert.equal(trT("calendarYearScope", { year: "2025" }), "2025'te");
  assert.equal(trT("calendarYearScope", { year: "2012" }), "2012'de");

  assert.equal(enT("calendarSliceHint"), "Click a month to show it on the map");
  assert.equal(trT("calendarSliceHint"), "Haritada görmek için bir aya tıkla");
});

check("resolveMonthForYear returns null when target year has no months with data", () => {
  const months: MonthCell[] = [
    makeCell({ year: 2020, month: 6, hasData: false }),
    makeCell({ year: 2021, month: 8, hasData: true, rawCount: 500, harmonizedCount: 300 }),
  ];

  assert.equal(resolveMonthForYear({ year: 2020, months, currentMonth: 8, harmonized: false }), null);
});

check("useAppStore stops yearPlaying via stopYearPlayback and setRegion", () => {
  const store = useAppStore.getState();
  store.setRegion("Turkey");
  store.setYearPlaying(true);
  assert.equal(useAppStore.getState().yearPlaying, true);

  store.stopYearPlayback();
  assert.equal(useAppStore.getState().yearPlaying, false);

  store.setYearPlaying(true);
  store.setRegion("Greece");
  assert.equal(useAppStore.getState().yearPlaying, false);
});

check("copy keys format cliff chart slider label, hint, and valuetext in EN and TR", () => {
  const enT = createTranslator("en");
  const trT = createTranslator("tr");

  assert.equal(enT("cliffSliderLabel"), "Select year");
  assert.equal(trT("cliffSliderLabel"), "Yıl seç");
  assert.equal(enT("cliffSliderHint"), "Use arrow keys to pick a year");
  assert.equal(trT("cliffSliderHint"), "Yıl seçmek için ok tuşlarını kullan");
  assert.equal(enT("cliffSliderValue", { year: "2021", count: 1200 }), "2021 · 1,200 detections");
  assert.equal(trT("cliffSliderValue", { year: "2021", count: 1200 }), "2021 · 1.200 tespit");
});
