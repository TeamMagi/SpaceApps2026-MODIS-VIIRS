/**
 * CountDelta — top right count summary and duplicate-removal delta.
 *
 * Raw mode: shows the raw total and the MODIS/VIIRS split only; nothing harmonized is in the DOM
 *   while resting in Raw (harmonized geometry mounts at the merge beat, and harmonized counts and
 *   the receipt appear at settle when revealAnswer becomes true).
 * Harmonized: "{raw} detections → {unique} harmonized detections" plus the removed duplicates line.
 *   When revealed, the block is a button that toggles the Receipt.
 */

import { useEffect, useId, useLayoutEffect, useRef } from "react";
import { animate, AnimatePresence, useReducedMotion } from "motion/react";
import { formatApiError } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import { useAppStore } from "../../store/useAppStore";
import Receipt from "./Receipt";
import styles from "./CountDelta.module.css";

const UNIQUE_TOKEN = "\u0000UNIQUE\u0000";

export default function CountDelta() {
  const { t, fmt } = useCopy();
  const revealAnswer = useHarmonizeTransition((v) => v.revealAnswer);
  const { status, hasData, error, retry, totals } = useRegionData();
  const reduceMotion = useReducedMotion();
  const receiptOpen = useAppStore((state) => state.openDrawer === "receipt");
  const toggleDrawer = useAppStore((state) => state.toggleDrawer);
  const closeDrawer = useAppStore((state) => state.closeDrawer);
  const receiptId = useId();
  const uniqueRef = useRef<HTMLSpanElement | null>(null);

  useLayoutEffect(() => {
    const node = uniqueRef.current;
    if (!revealAnswer || !node || !hasData) return;
    const write = (value: number) => {
      node.textContent = fmt.number(Math.round(value));
    };
    const { phase, run } = useAppStore.getState();
    const settleMs = run?.durations.settle ?? 0;
    if (phase !== "settle" || settleMs <= 0 || reduceMotion) {
      write(totals.harmonized);
      return;
    }
    write(totals.raw);
    const controls = animate(totals.raw, totals.harmonized, {
      duration: settleMs / 1000,
      ease: [0.2, 0, 0, 1],
      onUpdate: write,
    });
    return () => controls.stop();
  }, [revealAnswer, hasData, totals.raw, totals.harmonized, fmt, reduceMotion]);

  useEffect(() => {
    if (!revealAnswer && receiptOpen) closeDrawer();
  }, [revealAnswer, receiptOpen, closeDrawer]);

  if (status === "error") {
    return (
      <div className={styles.count} role="status">
        <p className={styles.subline}>{formatApiError(error, t)}</p>
        <button type="button" className={styles.retry} onClick={retry}>
          {t("retryAction")}
        </button>
      </div>
    );
  }

  if (status === "loading") {
    return (
      <div className={styles.count} role="status">
        <p className={styles.skeleton}>{t("loadingFirms")}</p>
      </div>
    );
  }

  if (status === "empty" || !hasData) {
    return (
      <div className={styles.count} role="status">
        <p className={styles.subline}>{t("emptyState")}</p>
      </div>
    );
  }

  if (!revealAnswer) {
    return (
      <div className={styles.count}>
        <p className={styles.summary}>{t("countRawSummary", { raw: totals.raw })}</p>
        <p className={styles.subline}>{t("countRawSubline", { modis: totals.modis, viirs: totals.viirs })}</p>
      </div>
    );
  }

  const summaryTemplate = t("countSummary", { raw: fmt.number(totals.raw), unique: UNIQUE_TOKEN });
  const [summaryPrefix, summarySuffix = ""] = summaryTemplate.split(UNIQUE_TOKEN);

  return (
    <div className={styles.anchor}>
      <button
        type="button"
        className={styles.count}
        aria-expanded={receiptOpen}
        aria-controls={receiptId}
        title={t("countOpenReceipt")}
        onClick={() => toggleDrawer("receipt")}
      >
        <span className={styles.summary}>
          {summaryPrefix}
          <span ref={uniqueRef} className={styles.unique} />
          {summarySuffix}
        </span>
        <span className={styles.subline}>
          {t("countSubline", { dup: totals.doubleCounts, pct: fmt.percent(totals.doubleCountShare) })}
        </span>
      </button>
      <AnimatePresence>{receiptOpen && <Receipt key="receipt" id={receiptId} />}</AnimatePresence>
    </div>
  );
}
