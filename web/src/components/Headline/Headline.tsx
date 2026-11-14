/**
 * Headline — the page's <h1>: a question in Raw, the real harmonized change in Harmonized.
 *
 * Switches only when `revealAnswer` flips (start of settle on the way in, immediately on the way
 * back). The cliff year takes the accent to rhyme with the chart marker.
 */

import type { ReactNode } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { formatApiError } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import styles from "./Headline.module.css";

function renderWithYearAccent(text: string, yearText: string): ReactNode {
  const index = text.indexOf(yearText);
  if (index === -1) return text;
  return (
    <>
      {text.slice(0, index)}
      <span className={styles.year}>{yearText}</span>
      {text.slice(index + yearText.length)}
    </>
  );
}

export default function Headline() {
  const { t, fmt } = useCopy();
  const { revealAnswer, idle, durations } = useHarmonizeTransition((v) => ({
    revealAnswer: v.revealAnswer,
    idle: v.phase === "idle",
    durations: v.durations,
  }));
  const { status, hasData, error, retry, cliff, yearly } = useRegionData();
  const reduceMotion = useReducedMotion();

  if (status === "loading") {
    return (
      <h1 className={styles.headline} data-state="loading">
        <span className={styles.placeholder}>{t("headlineLoading")}</span>
      </h1>
    );
  }

  if (status === "error") {
    return (
      <div className={styles.stateWrap} role="status">
        <h1 className={styles.headline} data-state="error">
          <span className={styles.placeholder}>{formatApiError(error, t, "headlineError")}</span>
        </h1>
        <button type="button" className={styles.retry} onClick={retry}>
          {t("retryAction")}
        </button>
      </div>
    );
  }

  if (status === "empty" || !hasData) {
    return (
      <h1 className={styles.headline} data-state="empty">
        <span className={styles.placeholder}>{t("headlineEmpty")}</span>
      </h1>
    );
  }

  if (!cliff) {
    return (
      <h1 className={styles.headline} data-state="empty">
        <span className={styles.placeholder}>{t("headlineNoCliff")}</span>
      </h1>
    );
  }

  const yearStr = String(cliff.toYear);
  const rawAbsPct = Math.round(Math.abs(cliff.rawChangePct));
  const rawText =
    cliff.rawChangePct < 0
      ? t("headlineRawDropped", { x: rawAbsPct, year: yearStr })
      : t("headlineRaw", { x: rawAbsPct, year: yearStr });
  const harmonizedText = t("headlineHarmonized", {
    pct: fmt.signedPercent(cliff.harmonizedChangeRatio),
    from: String(cliff.fromYear),
    to: yearStr,
  });
  const text = revealAnswer ? harmonizedText : rawText;

  const cliffYearPoint = yearly.find((y) => y.year === cliff.toYear);
  const isCliffPartial = cliffYearPoint?.partial === true;
  const partialSensor = cliffYearPoint && cliffYearPoint.rawViirs > 0 ? "VIIRS" : "MODIS";

  const swapMs = reduceMotion || idle ? 0 : revealAnswer ? durations.settle : durations.settle || durations.merge;
  const swapSeconds = swapMs / 1000;
  const slideY = reduceMotion ? 0 : 6;

  return (
    <div className={styles.wrap}>
      <h1 className={styles.headline} data-answer={revealAnswer}>
        <span className="sr-only" aria-live="polite">
          {text}
        </span>
        <span className={styles.stage} aria-hidden="true">
          <AnimatePresence initial={false}>
            <motion.span
              key={revealAnswer ? "answer" : "question"}
              className={styles.line}
              initial={{ opacity: 0, y: slideY }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -slideY }}
              transition={{ duration: swapSeconds, ease: [0.2, 0, 0, 1] }}
            >
              {renderWithYearAccent(text, yearStr)}
            </motion.span>
          </AnimatePresence>
        </span>
      </h1>
      {isCliffPartial && (
        <p className={styles.subline}>
          {t("headlinePartialNote", { year: yearStr, sensor: partialSensor })}
        </p>
      )}
    </div>
  );
}
