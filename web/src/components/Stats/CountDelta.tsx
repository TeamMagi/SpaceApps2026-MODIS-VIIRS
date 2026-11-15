/**
 * CountDelta — top-right all-years count summary with a 2x10 visual dot reduction.
 *
 * Raw mode shows 20 fire-orange dots, the raw total, and "Including duplicates · {from}–{to}".
 * Harmonized mode slides N of the 20 dots left onto their neighbour (proportional to removed / raw)
 * while turning the rest --color-accent, and reveals the harmonized total + Receipt popover.
 */

import { type CSSProperties, useEffect, useId, useLayoutEffect, useMemo, useRef } from "react";
import { animate, AnimatePresence, useReducedMotion } from "motion/react";
import { formatApiError } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import {
  DOT_GRID_TOTAL,
  computeDisappearingDotCount,
  selectDisappearingDotIndices,
} from "../../lib/dupDots";
import { useAppStore } from "../../store/useAppStore";
import Receipt from "./Receipt";
import styles from "./CountDelta.module.css";

export default function CountDelta() {
  const { t, fmt } = useCopy();
  const { geometryHarmonized, revealAnswer } = useHarmonizeTransition((v) => ({
    geometryHarmonized: v.geometryHarmonized,
    revealAnswer: v.revealAnswer,
  }));
  const { status, hasData, error, retry, totals, firstYear, lastYear } = useRegionData();
  const reduceMotion = useReducedMotion();
  const receiptOpen = useAppStore((state) => state.openDrawer === "receipt");
  const toggleDrawer = useAppStore((state) => state.toggleDrawer);
  const closeDrawer = useAppStore((state) => state.closeDrawer);
  const receiptId = useId();
  const uniqueRef = useRef<HTMLSpanElement | null>(null);
  const regionPicked = useAppStore((state) => state.regionPicked);
  // The ignition intro plays once per world -> country entry; the last dot's animationend ends it.
  const introPending = useRef(false);
  if (!regionPicked) introPending.current = true;
  const intro = introPending.current ? "true" : undefined;

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

  const removedDots = useMemo(
    () => computeDisappearingDotCount(totals.doubleCounts, totals.raw),
    [totals.doubleCounts, totals.raw],
  );
  const disappearingIndices = useMemo(
    () => selectDisappearingDotIndices(removedDots),
    [removedDots],
  );

  if (status === "error") {
    return (
      <div className={styles.count} role="status">
        <p className={styles.mutedLine}>{formatApiError(error, t)}</p>
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
        <p className={styles.mutedLine}>{t("emptyState")}</p>
      </div>
    );
  }

  const shareText = fmt.percent(totals.doubleCountShare);
  const keptDots = DOT_GRID_TOTAL - removedDots;
  const dotsAriaLabel = revealAnswer
    ? t("countDotsAriaHarmonized", {
        kept: keptDots,
        harmonized: totals.harmonized,
        removed: totals.doubleCounts,
        share: shareText,
      })
    : t("countDotsAriaRaw", { raw: totals.raw });

  const mutedText = revealAnswer
    ? firstYear !== null && lastYear !== null
      ? t("countMutedHarmonized", {
          removed: totals.doubleCounts,
          share: shareText,
          from: String(firstYear),
          to: String(lastYear),
        })
      : t("countMutedHarmonizedNoYears", {
          removed: totals.doubleCounts,
          share: shareText,
        })
    : firstYear !== null && lastYear !== null
      ? t("countMutedRaw", { from: String(firstYear), to: String(lastYear) })
      : t("countMutedRawNoYears");

  const dotGrid = (
    <div
      className={styles.dotGrid}
      role="img"
      aria-label={dotsAriaLabel}
      data-harmonized={geometryHarmonized}
      onAnimationEnd={(event) => {
        if (event.target === event.currentTarget.lastElementChild) introPending.current = false;
      }}
    >
      {Array.from({ length: DOT_GRID_TOTAL }, (_, idx) => (
        <span
          key={idx}
          className={styles.dot}
          style={{ "--i": idx } as CSSProperties}
          data-removed={disappearingIndices.has(idx) ? "true" : undefined}
        />
      ))}
    </div>
  );

  if (!revealAnswer) {
    return (
      <div className={styles.count} data-intro={intro}>
        {dotGrid}
        <p className={styles.totalLine}>
          <span className={styles.totalNumber}>{fmt.number(totals.raw)}</span>{" "}
          <span className={styles.totalUnit}>{t("countUnitRaw")}</span>
        </p>
        <p className={styles.mutedLine}>{mutedText}</p>
      </div>
    );
  }

  return (
    <div className={styles.anchor}>
      <button
        type="button"
        className={styles.count}
        data-intro={intro}
        aria-expanded={receiptOpen}
        aria-controls={receiptId}
        title={t("countOpenReceipt")}
        onClick={() => toggleDrawer("receipt")}
      >
        {dotGrid}
        <span className={styles.totalLine}>
          <span ref={uniqueRef} className={styles.totalNumber}>
            {fmt.number(totals.harmonized)}
          </span>{" "}
          <span className={styles.totalUnit}>{t("countUnitHarmonized")}</span>
        </span>
        <span className={styles.mutedLine}>{mutedText}</span>
      </button>
      <AnimatePresence>{receiptOpen && <Receipt key="receipt" id={receiptId} />}</AnimatePresence>
    </div>
  );
}
