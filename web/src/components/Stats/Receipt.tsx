/**
 * Receipt — the "correction receipt" opened from CountDelta; only exists once the answer is
 * revealed. Uses real counts from /api/series (yearly interscan_removed & matched) and
 * /api/points (stages for the shown period).
 */

import { motion, useReducedMotion } from "motion/react";
import { formatApiError, PIPELINE_COPY } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { useMapPoints, useRegionData } from "../../hooks/useRegionData";
import { DEFAULT_PIPELINE_CONFIG, hasPipelineOverrides } from "../../lib/pipelineOverrides";
import { summarizePointStages } from "../../lib/receiptCounts";
import { useAppStore } from "../../store/useAppStore";
import styles from "./Receipt.module.css";

export interface ReceiptProps {
  /** Referenced by the opener's aria-controls. */
  id: string;
}

function MaskRows({ staticMasked, samaRemoved }: { staticMasked: number; samaRemoved: number }) {
  const { t, fmt } = useCopy();
  return (
    <>
      {staticMasked > 0 && (
        <div className={styles.row}>
          <dt>{t("receiptStaticMasked")}</dt>
          <dd>{fmt.signed(-staticMasked)}</dd>
        </div>
      )}
      {samaRemoved > 0 && (
        <div className={styles.row}>
          <dt>{t("receiptSamaRemoved")}</dt>
          <dd>{fmt.signed(-samaRemoved)}</dd>
        </div>
      )}
    </>
  );
}

export default function Receipt({ id }: ReceiptProps) {
  const { t, fmt } = useCopy();
  const { status, hasData, error, retry, totals, stages } = useRegionData();
  const points = useMapPoints();
  const closeDrawer = useAppStore((state) => state.closeDrawer);
  const pipelineOverrides = useAppStore((state) => state.pipelineOverrides);
  const serverConfig = useAppStore((state) => state.serverConfig);
  const customThresholds = hasPipelineOverrides(
    pipelineOverrides,
    serverConfig ?? DEFAULT_PIPELINE_CONFIG,
  );
  const reduceMotion = useReducedMotion();
  const slideY = reduceMotion ? 0 : -6;

  const periodLabel =
    points.period.year === null
      ? ""
      : points.period.month === null
        ? String(points.period.year)
        : fmt.monthYear({ year: points.period.year, month: points.period.month }, "long");
  const periodCounts = summarizePointStages(points.stages);

  return (
    <motion.section
      id={id}
      className={styles.receipt}
      aria-label={t("receiptTitle")}
      initial={{ opacity: 0, y: slideY }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: slideY }}
      transition={{ duration: reduceMotion ? 0.15 : 0.18, ease: [0.2, 0, 0, 1] }}
    >
      <div className={styles.head}>
        <h2 className={styles.title}>{t("receiptTitle")}</h2>
        <button type="button" className={styles.close} onClick={closeDrawer}>
          {t("closeAction")}
        </button>
      </div>
      {status === "error" ? (
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{formatApiError(error, t)}</p>
          <button type="button" className={styles.retry} onClick={retry}>
            {t("retryAction")}
          </button>
        </div>
      ) : status === "loading" ? (
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{t("loadingFirms")}</p>
        </div>
      ) : status !== "ready" || !hasData ? (
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{t("emptyState")}</p>
        </div>
      ) : (
        <>
          <dl className={styles.lines}>
            <div className={styles.row}>
              <dt>{t("receiptRaw")}</dt>
              <dd>{fmt.number(totals.raw)}</dd>
            </div>
            <MaskRows staticMasked={totals.staticMasked} samaRemoved={totals.samaRemoved} />
            <div className={styles.row}>
              <dt>{t("receiptDedup")}</dt>
              <dd>{fmt.signed(-totals.doubleCounts)}</dd>
            </div>
            <div className={styles.totalRow}>
              <dt>{t("receiptUnique")}</dt>
              <dd>{fmt.number(totals.harmonized)}</dd>
            </div>
          </dl>
          <h3 className={styles.subtitle}>{t("receiptStagesTitle")}</h3>
          <dl className={styles.lines}>
            <MaskRows staticMasked={totals.staticMasked} samaRemoved={totals.samaRemoved} />
            {stages
              .filter((stage) => stage.removed !== undefined)
              .map((stage) => (
                <div key={stage.key} className={styles.row}>
                  <dt>{t(PIPELINE_COPY[stage.key].name)}</dt>
                  <dd>{fmt.signed(-(stage.removed ?? 0))}</dd>
                </div>
              ))}
          </dl>
          {points.status === "ready" && points.hasData && (
            <>
              <div className={styles.subtitleRow}>
                <h3 className={styles.subtitle}>{t("receiptPeriodTitle", { period: periodLabel })}</h3>
                {customThresholds && (
                  <span className={styles.customBadge}>{t("settingsCustomBadge")}</span>
                )}
              </div>
              <dl className={styles.lines}>
                <div className={styles.row}>
                  <dt>{t("receiptRaw")}</dt>
                  <dd>{fmt.number(periodCounts.raw)}</dd>
                </div>
                <MaskRows
                  staticMasked={periodCounts.staticMasked}
                  samaRemoved={periodCounts.samaRemoved}
                />
                <div className={styles.row}>
                  <dt>{t(PIPELINE_COPY.dedup.name)}</dt>
                  <dd>{fmt.signed(-periodCounts.interscanRemoved)}</dd>
                </div>
                <div className={styles.row}>
                  <dt>{t(PIPELINE_COPY.fleet.name)}</dt>
                  <dd>{fmt.signed(-periodCounts.fleetMerged)}</dd>
                </div>
                <div className={styles.row}>
                  <dt>{t(PIPELINE_COPY.match.name)}</dt>
                  <dd>{fmt.signed(-periodCounts.matchedPairs)}</dd>
                </div>
                <div className={styles.totalRow}>
                  <dt>{t("receiptUnique")}</dt>
                  <dd>{fmt.number(periodCounts.harmonized)}</dd>
                </div>
                {points.frpAngleCorrectionPct !== null && (
                  <div className={styles.row}>
                    <dt>{t("receiptPeriodAngle")}</dt>
                    <dd>{t("receiptAngleValue", { pct: fmt.signedPercent(points.frpAngleCorrectionPct) })}</dd>
                  </div>
                )}
              </dl>
            </>
          )}
        </>
      )}
    </motion.section>
  );
}
