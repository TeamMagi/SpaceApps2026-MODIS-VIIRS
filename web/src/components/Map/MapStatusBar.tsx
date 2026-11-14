import { memo } from "react";
import { formatApiError } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import type { ApiErrorInfo, AsyncStatus, MonthRef } from "../../types";
import type { DrawStats } from "./engine/types";
import styles from "./MapStatusBar.module.css";

export interface MapStatusBarProps {
  readonly regionName: string;
  readonly periodText: string;
  readonly revealAnswer: boolean;
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

  const activeTotal = revealAnswer ? harmonizedTotal : rawTotal;
  const activeLoaded = revealAnswer ? harmonizedLoaded : rawLoaded;
  const isSampled = status === "ready" && drawStats.drawn < drawStats.inView;

  return (
    <div className={styles.statusBar} data-map-ui="true">
      <div className={styles.primaryGroup}>
        <span className={styles.regionName}>{regionName}</span>
        {periodText && status !== "loading" && (
          <span className={styles.period}>{t("mapPeriod", { period: periodText })}</span>
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
            <span className={styles.countValue}>
              {revealAnswer
                ? t("mapStatusHarmonizedDetections", { count: harmonizedTotal })
                : t("mapStatusDetections", { count: rawTotal })}
            </span>
            {truncated && (
              <span className={styles.subsetNote}>
                {t("mapTruncatedNote", {
                  shown: activeLoaded,
                  total: activeTotal > 0 ? activeTotal : activeLoaded,
                })}
              </span>
            )}
            {isSampled && (
              <span className={styles.subsetNote}>
                {t("mapStatusSampledNote", {
                  drawn: drawStats.drawn,
                  inView: drawStats.inView,
                })}
              </span>
            )}
          </>
        )}

        {selectedMonth && (
          <div className={styles.filterChip}>
            <span className={styles.filterText}>
              {t("mapMonthFilter", {
                month: fmt.monthYear(selectedMonth, "long"),
              })}
            </span>
            <button type="button" className={styles.actionButton} onClick={onClearMonth}>
              {t("mapMonthFilterClear")}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

export default memo(MapStatusBar);
