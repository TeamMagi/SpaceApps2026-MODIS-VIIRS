/**
 * PipelineStrip — the 7 harmonization steps (Zeigarnik: empty in Raw, fills on harmonize).
 *
 * Uses real stage removal counts: stages from /api/points when a period is filtered on the map,
 * and archive-wide interscan_removed / matched from /api/series.
 */

import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { PIPELINE_COPY } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import {
  APPLIED_PIPELINE_ORDER,
  isPlannedStage,
  PIPELINE_ORDER,
  useHarmonizeTransition,
} from "../../hooks/useHarmonizeTransition";
import { useMapPoints, useRegionData } from "../../hooks/useRegionData";
import { PIPELINE_NODE_COUNT, useAppStore } from "../../store/useAppStore";
import styles from "./PipelineStrip.module.css";

export default function PipelineStrip() {
  const { t, fmt } = useCopy();
  const { litNodes, activeStage, mode, revealAnswer, durations } = useHarmonizeTransition((v) => ({
    litNodes: v.litNodes,
    activeStage: v.activeStage,
    mode: v.mode,
    revealAnswer: v.revealAnswer,
    durations: v.durations,
  }));
  const { status, hasData, stages, totals } = useRegionData();
  const points = useMapPoints();
  const selectedMonth = useAppStore((state) => state.selectedMonth);
  const reduceMotion = useReducedMotion();

  const count =
    litNodes === 0
      ? t("pipelineCountIdle")
      : litNodes === PIPELINE_NODE_COUNT
        ? t("pipelineCountDone")
        : t("pipelineCount", { n: litNodes });

  // Follows the committed mode, which changes once per run (at its end), so it is announced once.
  const announced = mode === "harmonized" ? t("pipelineCountDone") : t("pipelineCountIdle");

  const activeStatus = activeStage ? t(PIPELINE_COPY[activeStage].progress) : null;

  const usePeriodStages = selectedMonth !== null && points.status === "ready" && points.hasData;
  const dedupRemoved = usePeriodStages
    ? points.stages.interscanRemoved
    : (stages.find((stage) => stage.key === "dedup")?.removed ?? 0);
  const matchRemoved = usePeriodStages
    ? points.stages.matchedPairs
    : (stages.find((stage) => stage.key === "match")?.removed ?? 0);
  const totalRemoved = usePeriodStages ? dedupRemoved + matchRemoved : totals.doubleCounts;

  const canShowTotals =
    revealAnswer &&
    status === "ready" &&
    hasData &&
    (selectedMonth === null || (points.status === "ready" && points.hasData));
  const totalStatus =
    canShowTotals ? t("pipelineProgressTotal", { removed: fmt.signed(-totalRemoved) }) : null;
  const split =
    canShowTotals
      ? t("pipelineSplit", { dedup: fmt.signed(-dedupRemoved), match: fmt.signed(-matchRemoved) })
      : null;

  const pulseSeconds = reduceMotion ? 0 : Math.min(0.28, (durations.merge || 200) / 1000);
  const fadeSeconds = reduceMotion ? 0.15 : 0.16;

  return (
    <div className={styles.strip}>
      <ol className={styles.nodes} role="list" aria-label={t("pipelineLabel")}>
        {PIPELINE_ORDER.map((key, index) => {
          const planned = isPlannedStage(key);
          const appliedIndex = APPLIED_PIPELINE_ORDER.indexOf(key);
          const lit = !planned && appliedIndex >= 0 && appliedIndex < litNodes;
          const active = !planned && key === activeStage;
          return (
            <li
              key={key}
              className={styles.node}
              data-lit={lit}
              data-active={active}
              data-planned={planned ? "true" : undefined}
            >
              {index > 0 && (
                <span className={styles.connector} aria-hidden="true">
                  <span className={styles.connectorFill} />
                </span>
              )}
              <motion.span
                className={styles.index}
                aria-hidden="true"
                animate={{ scale: active && !reduceMotion ? [1, 1.14, 1] : 1 }}
                transition={{ duration: pulseSeconds, ease: [0.2, 0, 0, 1] }}
              >
                {index + 1}
              </motion.span>
              <span className={styles.labels}>
                <span className={styles.name}>{t(PIPELINE_COPY[key].name)}</span>
                <span className={styles.tech}>
                  {t(PIPELINE_COPY[key].tech)}
                  {planned && <span className={styles.plannedBadge}>{t("pipelinePlannedBadge")}</span>}
                </span>
                <AnimatePresence>
                  {active && activeStatus && (
                    <motion.span
                      key={key}
                      className={styles.nodeProgress}
                      data-align={index >= 5 ? "end" : "start"}
                      initial={{ opacity: 0, y: reduceMotion ? 0 : -2 }}
                      animate={{ opacity: 1, y: 0 }}
                      exit={{ opacity: 0, y: reduceMotion ? 0 : -2 }}
                      transition={{ duration: fadeSeconds, ease: [0.2, 0, 0, 1] }}
                    >
                      {activeStatus}
                    </motion.span>
                  )}
                </AnimatePresence>
              </span>
            </li>
          );
        })}
      </ol>
      <div className={styles.meta}>
        {totalStatus && (
          <span className={styles.status}>
            {totalStatus}
            {split && <span className={styles.split}>{split}</span>}
          </span>
        )}
        <span className={styles.count}>{count}</span>
        <span className="sr-only" aria-live="polite">
          {announced}
        </span>
      </div>
    </div>
  );
}
