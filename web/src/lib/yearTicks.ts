/**
 * Year labels for the time axis: first, last, the cliff year (when present), and every fifth year
 * in between unless it would crowd the cliff or the last year.
 */
export function yearTicks(first: number, last: number, cliffYear: number | null = null): Set<number> {
  const ticks = new Set<number>([first, last]);
  if (cliffYear !== null && cliffYear >= first && cliffYear <= last) {
    ticks.add(cliffYear);
  }
  for (let year = first + 5; year < last; year += 5) {
    const nearCliff = cliffYear !== null && Math.abs(year - cliffYear) < 2;
    if (!nearCliff && Math.abs(year - last) >= 2) ticks.add(year);
  }
  return ticks;
}
