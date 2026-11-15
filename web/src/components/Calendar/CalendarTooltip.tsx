/**
 * CalendarTooltip — one line about a calendar cell, plain meaning first, statistic second:
 *   "Aug 2021 · 1,120 detections · well above normal for August (+2.3σ)".
 */

import type { CopyApi } from "../../hooks/useCopy";
import { useCopy } from "../../hooks/useCopy";
import {
  formatFrpInterval,
  relativeUncertaintyWidth,
  WIDE_UNCERTAINTY_THRESHOLD,
} from "../../lib/format";
import { anomalyCopyKey } from "../../lib/scales";
import type { CalendarCell, MonthCell } from "../../types";
import styles from "./CalendarTooltip.module.css";

type TipCopy = Pick<CopyApi, "t" | "fmt">;

/** Headline is FRP per detection; mean FRP is native-pixel and jumps at the 2012 VIIRS pixel-size change. */
function formatCalendarFrpText(frp: CalendarCell | undefined, { t, fmt }: TipCopy): string {
  if (!frp) return "";
  const parts: string[] = [];
  if (frp.frpPerDetectionEq !== null) {
    parts.push(t("calendarFrpEq", { value: fmt.number(frp.frpPerDetectionEq, 1) }));
  }
  const total = fmt.number(frp.totalFrp, 1);
  if (frp.totalFrpLower !== null && frp.totalFrpUpper !== null) {
    parts.push(
      t("calendarFrpTotalBand", {
        value: total,
        low: fmt.number(frp.totalFrpLower, 1),
        high: fmt.number(frp.totalFrpUpper, 1),
      }),
    );
  } else {
    parts.push(t("calendarFrpTotal", { value: total }));
  }
  return parts.join(" · ");
}

function formatCalendarNativeText(frp: CalendarCell | undefined, { t, fmt }: TipCopy): string {
  if (!frp) return "";
  const value = fmt.number(frp.meanFrp, 1);
  if (frp.meanFrpLower !== null && frp.meanFrpUpper !== null) {
    return t("calendarFrpNativeBand", {
      value,
      low: fmt.number(frp.meanFrpLower, 1),
      high: fmt.number(frp.meanFrpUpper, 1),
    });
  }
  return t("calendarFrpNative", { value });
}

export function formatCalendarCellText(cell: MonthCell, harmonized: boolean, copy: TipCopy, frp?: CalendarCell): string {
  const base = formatCalendarCellBase(cell, harmonized, copy);
  if (!harmonized || !cell.hasData) return base;
  return [base, formatCalendarFrpText(frp, copy), formatCalendarNativeText(frp, copy)].filter(Boolean).join(" · ");
}

function formatCalendarCellBase(cell: MonthCell, harmonized: boolean, copy: TipCopy): string {
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
  frp?: CalendarCell;
  position: { x: number; y: number };
  bounds?: { width: number; cellHeight: number };
}

export default function CalendarTooltip({ id, cell, harmonized, frp, position, bounds }: CalendarTooltipProps) {
  const { lang, t, fmt } = useCopy();
  const text = formatCalendarCellBase(cell, harmonized, { t, fmt });
  const showFrp = harmonized && cell.hasData;
  const frpText = showFrp ? formatCalendarFrpText(frp, { t, fmt }) : "";
  const nativeText = showFrp ? formatCalendarNativeText(frp, { t, fmt }) : "";
  const fallbackMeanFrp = showFrp && !nativeText && cell.meanFrp !== null;
  const uncertMean = frp ? frp.meanFrp : cell.meanFrp;
  const uncertLower = frp ? frp.meanFrpLower : cell.meanFrpLower;
  const uncertUpper = frp ? frp.meanFrpUpper : cell.meanFrpUpper;
  const relWidth =
    showFrp && uncertMean !== null
      ? relativeUncertaintyWidth(uncertLower, uncertUpper, uncertMean)
      : null;
  const isWideUncert = relWidth !== null && relWidth > WIDE_UNCERTAINTY_THRESHOLD;

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
      {frpText && <span className={styles.frpLine}>{frpText}</span>}
      {nativeText && <span className={styles.nativeLine}>{nativeText}</span>}
      {fallbackMeanFrp && cell.meanFrp !== null && (
        <span className={styles.frpLine}>
          {formatFrpInterval(cell.meanFrp, cell.meanFrpLower, cell.meanFrpUpper, lang)}
        </span>
      )}
      {isWideUncert && (
        <span className={styles.wideHint}>
          <strong>{t("uncertaintyWideTitle")}:</strong> {t("uncertaintyWideBody")}
        </span>
      )}
      <span className={styles.arrow} style={{ left: `calc(50% + ${arrowOffset - 140}px)` }} aria-hidden="true" />
    </div>
  );
}
