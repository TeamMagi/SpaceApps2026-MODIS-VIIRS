export type PanelId = "detections" | "frp" | "calendar";

export const PANEL_IDS: readonly PanelId[] = ["detections", "frp", "calendar"];

/** Polyline path for a w x h sparkline. Nulls break the line; a flat series is drawn through the middle. */
export function sparklinePath(values: readonly (number | null)[], w: number, h: number): string {
  const finite = values.filter((v): v is number => v !== null && Number.isFinite(v));
  if (finite.length === 0) return "";
  const min = Math.min(...finite);
  const max = Math.max(...finite);
  const span = max - min;
  const n = values.length;
  const parts: string[] = [];
  let pen = false;
  values.forEach((v, i) => {
    if (v === null || !Number.isFinite(v)) {
      pen = false;
      return;
    }
    const x = n === 1 ? w / 2 : (i / (n - 1)) * w;
    const y = span === 0 ? h / 2 : h - ((v - min) / span) * h;
    parts.push(`${pen ? "L" : "M"}${x.toFixed(1)} ${y.toFixed(1)}`);
    pen = true;
  });
  return parts.join(" ");
}

interface MonthCount {
  year: number;
  month: number;
  hasData: boolean;
  rawCount: number;
  harmonizedCount: number;
}

/** Month (1-12) with the most detections in `year`, or null when the year has none. Ties go to the earlier month. */
export function busiestMonth(
  months: readonly MonthCount[],
  year: number,
  harmonized = true,
): number | null {
  let best: number | null = null;
  let bestCount = 0;
  for (const m of months) {
    if (m.year !== year || !m.hasData) continue;
    const count = harmonized ? m.harmonizedCount : m.rawCount;
    if (count > bestCount || (count === bestCount && best !== null && count > 0 && m.month < best)) {
      best = m.month;
      bestCount = count;
    }
  }
  return best;
}

export function parsePanel(hashValue: string | null): PanelId | null {
  return PANEL_IDS.find((id) => id === hashValue) ?? null;
}

interface MonthAnomaly extends MonthCount {
  anomalyRaw: number | null;
  anomalyHarmonized: number | null;
}

/** Whether each calendar month of `year` is clearly above its usual level (z >= 1) in the chosen view. */
export function hotMonths(months: readonly MonthAnomaly[], year: number, harmonized = true): boolean[] {
  const out = new Array<boolean>(12).fill(false);
  for (const m of months) {
    if (m.year !== year || !m.hasData || m.month < 1 || m.month > 12) continue;
    const z = harmonized ? m.anomalyHarmonized : m.anomalyRaw;
    out[m.month - 1] = z !== null && z >= 1;
  }
  return out;
}
