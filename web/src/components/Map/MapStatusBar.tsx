import { memo, useMemo } from "react";
import { formatApiError } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { useRegionData } from "../../hooks/useRegionData";
import { useAppStore } from "../../store/useAppStore";
import type { ApiErrorInfo, AsyncStatus, MonthRef } from "../../types";
import type { DrawStats, SensorFilter } from "./engine/types";
import styles from "./MapStatusBar.module.css";

const DEFAULT_SENSOR_FILTER: SensorFilter = { modis: true, viirs: true };

function monthKey(m: MonthRef): number {
  return m.year * 12 + m.month;
}

export interface MapStatusBarProps {
  readonly regionName: string;
  readonly periodText: string;
  readonly revealAnswer: boolean;
  readonly sensorFilter?: SensorFilter;
  readonly status: AsyncStatus;
  readonly error: ApiErrorInfo | null;
  readonly onRetry: () => void;
  readonly rawTotal: number;
  readonly harmonizedTotal: number;
  readonly rawLoaded: number;
  readonly harmonizedLoaded: number;
  readonly truncated: boolean;
  readonly drawStats: DrawStats;
  readonly selectedMonth: MonthRef | null;
  readonly onClearMonth: () => void;
}

function MapStatusBar({
  regionName,
  periodText,
  revealAnswer,
  sensorFilter = DEFAULT_SENSOR_FILTER,
  status,
  error,
  onRetry,
  rawTotal,
  harmonizedTotal,
  rawLoaded,
  harmonizedLoaded,
  truncated,
  drawStats,
  selectedMonth,
  onClearMonth,
}: MapStatusBarProps) {
  const { t, fmt } = useCopy();
  const { months, defaultMapMonth } = useRegionData();
  const selectMonth = useAppStore((state) => state.selectMonth);

  const availableMonths = useMemo(
    () => months.filter((m) => m.hasData),
    [months],
  );

  const activeMonth = selectedMonth ?? defaultMapMonth;
  const periodLabel = activeMonth ? fmt.monthYear(activeMonth, "long") : periodText;

  const { prevMonth, nextMonth } = useMemo(() => {
    if (!activeMonth || availableMonths.length === 0) {
      return { prevMonth: null as MonthRef | null, nextMonth: null as MonthRef | null };
    }
    const activeYm = monthKey(activeMonth);
    let prev: (typeof availableMonths)[number] | null = null;
    let prevYm = Number.NEGATIVE_INFINITY;
    let next: (typeof availableMonths)[number] | null = null;
    let nextYm = Number.POSITIVE_INFINITY;

    for (const m of availableMonths) {
      const ym = monthKey(m);
      if (ym < activeYm && ym > prevYm) {
        prev = m;
        prevYm = ym;
      } else if (ym > activeYm && ym < nextYm) {
        next = m;
        nextYm = ym;
      }
    }

    return {
      prevMonth: prev ? { year: prev.year, month: prev.month } : null,
      nextMonth: next ? { year: next.year, month: next.month } : null,
    };
  }, [activeMonth, availableMonths]);

  const activeTotal = revealAnswer ? harmonizedTotal : rawTotal;
  const activeLoaded = revealAnswer ? harmonizedLoaded : rawLoaded;
  const isSampled = status === "ready" && drawStats.drawn < drawStats.inView;
  const hasUnknownTruncatedTotal =
    status === "ready" && truncated && activeTotal === 0 && activeLoaded > 0;
  const isTruncatedSubset =
    status === "ready" &&
    truncated &&
    activeLoaded > 0 &&
    (hasUnknownTruncatedTotal || activeLoaded < activeTotal);
  const showSampleLoaded = !isSampled && isTruncatedSubset;

  const effectiveSensorFilter = revealAnswer ? DEFAULT_SENSOR_FILTER : sensorFilter;
  const singleSensor: "modis" | "viirs" | null =
    effectiveSensorFilter.modis && !effectiveSensorFilter.viirs
      ? "modis"
      : effectiveSensorFilter.viirs && !effectiveSensorFilter.modis
        ? "viirs"
        : null;

  const countText = revealAnswer
    ? t("mapStatusHarmonizedDetections", { count: harmonizedTotal })
    : singleSensor === "modis"
      ? t("mapStatusModisDetections", { count: rawTotal })
      : singleSensor === "viirs"
        ? t("mapStatusViirsDetections", { count: rawTotal })
        : t("mapStatusDetections", { count: rawTotal });

  return (
    <div className={styles.statusBar} data-map-ui="true">
      <div className={styles.barInner}>
        <div className={styles.primaryGroup}>
          <span className={styles.regionName}>{regionName}</span>
          {periodLabel && (
            <div className={styles.monthStepper} role="group" aria-label={t("mapMonthStepperLabel")}>
              <button
                type="button"
                className={styles.stepButton}
                aria-label={t("mapPrevMonth")}
                title={t("mapPrevMonth")}
                disabled={prevMonth === null}
                onClick={() => {
                  if (prevMonth) selectMonth(prevMonth);
                }}
              >
                <span aria-hidden="true">‹</span>
              </button>
              <span className={styles.period}>{periodLabel}</span>
              <button
                type="button"
                className={styles.stepButton}
                aria-label={t("mapNextMonth")}
                title={t("mapNextMonth")}
                disabled={nextMonth === null}
                onClick={() => {
                  if (nextMonth) selectMonth(nextMonth);
                }}
              >
                <span aria-hidden="true">›</span>
              </button>
              {selectedMonth !== null && (
                <button
                  type="button"
                  className={styles.clearMonthButton}
                  aria-label={t("mapMonthReset")}
                  title={t("mapMonthReset")}
                  onClick={onClearMonth}
                >
                  <span aria-hidden="true">×</span>
                </button>
              )}
            </div>
          )}
          <span className={styles.modeBadge} data-harmonized={revealAnswer}>
            {revealAnswer ? t("mapStatusModeHarmonized") : t("mapStatusModeRaw")}
          </span>
        </div>

        <div className={styles.countsGroup}>
          {status === "loading" && (
            <span className={styles.statusText} role="status">
              {t("mapStatusLoading")}
            </span>
          )}

          {status === "error" && (
            <div className={styles.errorGroup} role="alert">
              <span className={styles.errorText}>{formatApiError(error, t)}</span>
              <button type="button" className={styles.actionButton} onClick={onRetry}>
                {t("retryAction")}
              </button>
            </div>
          )}

          {status === "empty" && (
            <span className={styles.statusText} role="status">
              {t("mapStatusEmpty")}
            </span>
          )}

          {status === "ready" && (
            <>
              {!hasUnknownTruncatedTotal && (
                <span className={styles.countValue}>{countText}</span>
              )}
              {(isSampled || showSampleLoaded) && (
                <>
                  {!hasUnknownTruncatedTotal && (
                    <span className={styles.separator} aria-hidden="true">
                      ·
                    </span>
                  )}
                  <span
                    className={styles.subsetNote}
                    title={
                      isSampled
                        ? t("mapStatusDrawnTitle")
                        : t("mapStatusSampleLoadedTitle")
                    }
                  >
                    {isSampled
                      ? t("mapStatusDrawn", { drawn: drawStats.drawn })
                      : t("mapStatusSampleLoaded", { loaded: activeLoaded })}
                  </span>
                </>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export default memo(MapStatusBar);
