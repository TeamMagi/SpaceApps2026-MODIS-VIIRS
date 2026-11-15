import type { Translate } from "../../content/copy";
import type { TropicalSwathGapInfo } from "../../types";

/**
 * Calendar note for the MODIS tropical swath gap, or null outside the belt. It only describes the
 * sensor limitation (no harmonized count or total), so it is safe to show in Raw mode.
 */
export function formatSwathGapNote(gap: TropicalSwathGapInfo | null, t: Translate): string | null {
  if (!gap) return null;
  // A sliver of belt still deserves the note, so it never rounds down to "0%".
  const share = Math.min(100, Math.max(1, Math.round(gap.beltShare * 100)));
  const vars = {
    share,
    km: gap.swathGapKm,
    low: gap.missedFrpLowPct,
    high: gap.missedFrpHighPct,
  };
  if (gap.compensation) {
    const scale = Math.round((gap.compensation.factor - 1) * 100);
    return t("calendarSwathGapNoteCompensated", { ...vars, scale });
  }
  return t("calendarSwathGapNote", vars);
}
