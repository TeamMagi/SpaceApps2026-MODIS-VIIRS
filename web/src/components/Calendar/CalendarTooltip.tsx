/**
 * CalendarTooltip — one line about a calendar cell, plain meaning first, statistic second:
 *   "Aug 2021 · 1,120 detections · well above normal for August (+2.3σ)".
 */

import type { CopyApi } from "../../hooks/useCopy";
import { useCopy } from "../../hooks/useCopy";
import { anomalyCopyKey } from "../../lib/scales";
import type { MonthCell } from "../../types";
import styles from "./CalendarTooltip.module.css";

export function formatCalendarCellText(cell: MonthCell, harmonized: boolean, copy: Pick<CopyApi, "t" | "fmt">): string {
  const { t, fmt } = copy;
  const date = fmt.monthYear(cell);
  const count = harmonized ? cell.harmonizedCount : cell.rawCount;
  const z = harmonized ? cell.anomalyHarmonized : cell.anomalyRaw;

  if (!cell.hasData) {
    return t("calendarNoRecord", { month: date });
  }
  if (z === null) {
    if (count === 0) {
      return t("calendarEmptyCell", { month: date });
    }
    return t("calendarNoAnomalyCell", { date, count });
  }
  return t("calendarTooltip", {
    date,
    count,
    anomaly: t(anomalyCopyKey(z), { month: fmt.monthNameInSentence(cell.month) }),
    sigma: fmt.sigma(z),
  });
}

export interface CalendarTooltipProps {
  id: string;
  cell: MonthCell;
  harmonized: boolean;
  position: { x: number; y: number };
  bounds?: { width: number; cellHeight: number };
}

export default function CalendarTooltip({ id, cell, harmonized, position, bounds }: CalendarTooltipProps) {
  const { t, fmt } = useCopy();
  const text = formatCalendarCellText(cell, harmonized, { t, fmt });

  const flipBottom = position.y < 52;
  const top = flipBottom ? position.y + (bounds?.cellHeight ?? 18) : position.y;
  const panelWidth = bounds?.width ?? 480;
  const halfMax = Math.min(140, Math.max(80, panelWidth / 2 - 8));
  const clampedX = Math.max(halfMax, Math.min(panelWidth - halfMax, position.x));
  const arrowOffset = Math.max(12, Math.min(268, 140 + (position.x - clampedX)));

  return (
    <div
      id={id}
      role="tooltip"
      className={styles.tooltip}
      data-placement={flipBottom ? "bottom" : "top"}
      style={{ left: clampedX, top }}
    >
      <span>{text}</span>
      <span className={styles.arrow} style={{ left: `calc(50% + ${arrowOffset - 140}px)` }} aria-hidden="true" />
    </div>
  );
}
