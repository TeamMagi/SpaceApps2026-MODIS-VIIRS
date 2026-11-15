/**
 * Headline — the page's <h1>: a two-line question in Raw, the harmonized change in Harmonized.
 *
 * Switches only when `revealAnswer` flips (start of settle on the way in, immediately on the way
 * back). Any partial-year caveat sits in the inline "i" info button's tooltip/aria-label.
 */

import type { ReactNode } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { formatApiError } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import { headlineFigure } from "../../lib/headline";
import styles from "./Headline.module.css";

const PCT_TOKEN = "\u0000PCT\u0000";
const PCT_PATTERN = /%\u0000PCT\u0000|\u0000PCT\u0000%|\u0000PCT\u0000/;

function renderHighlightedPct(template: string, pctText: string, className: string): ReactNode {
  const match = PCT_PATTERN.exec(template);
  if (!match) return template;
  const idx = match.index;
  return (
    <>
      {template.slice(0, idx)}
      <span className={className}>{pctText}</span>
      {template.slice(idx + match[0].length)}
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
      <h1 className={styles.stateHeadline} data-state="loading">
        <span className={styles.placeholder}>{t("headlineLoading")}</span>
      </h1>
    );
  }

  if (status === "error") {
    return (
      <div className={styles.stateWrap} role="status">
        <h1 className={styles.stateHeadline} data-state="error">
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
      <h1 className={styles.stateHeadline} data-state="empty">
        <span className={styles.placeholder}>{t("headlineEmpty")}</span>
      </h1>
    );
  }

  if (!cliff) {
    return (
      <h1 className={styles.stateHeadline} data-state="empty">
        <span className={styles.placeholder}>{t("headlineNoCliff")}</span>
      </h1>
    );
  }

  const yearStr = String(cliff.toYear);
  const fromStr = String(cliff.fromYear);
  const figure = headlineFigure(cliff, revealAnswer);
  if (figure.kind === "noBaseline") {
    return (
      <h1 className={styles.stateHeadline} data-state="empty">
        <span className={styles.placeholder}>{t("headlineNoBaseline", { from: fromStr, to: yearStr })}</span>
      </h1>
    );
  }
  const rawDropped = figure.kind === "raw" && figure.dropped;
  const rawAbsPct = figure.kind === "raw" ? figure.absPct : 0;
  const rawPctText = figure.kind === "raw" ? fmt.percent(figure.absPct / 100, 0) : "";
  const harmPctText = figure.kind === "harmonized" ? fmt.signedPercent(figure.ratio) : "";

  const line1Plain = revealAnswer
    ? t("headlineHarmonized", { pct: harmPctText })
    : rawDropped
      ? t("headlineRawDropped", { x: rawAbsPct, year: yearStr })
      : t("headlineRaw", { x: rawAbsPct, year: yearStr });

  const line1Content = revealAnswer
    ? renderHighlightedPct(t("headlineHarmonized", { pct: PCT_TOKEN }), harmPctText, styles.harmAccent)
    : renderHighlightedPct(
        rawDropped
          ? t("headlineRawDropped", { x: PCT_TOKEN, year: yearStr })
          : t("headlineRaw", { x: PCT_TOKEN, year: yearStr }),
        rawPctText,
        styles.rawAccent,
      );

  const line2Text = revealAnswer
    ? rawDropped
      ? t("headlineHarmonizedSubDropped", { rawPct: rawPctText })
      : t("headlineHarmonizedSub", { rawPct: rawPctText })
    : rawDropped
      ? t("headlineRawDroppedSub")
      : t("headlineRawSub");

  const cliffYearPoint = yearly.find((y) => y.year === cliff.toYear);
  const isCliffPartial = cliffYearPoint?.partial === true;
  const partialSensor = cliffYearPoint && cliffYearPoint.rawViirs > 0 ? "VIIRS" : "MODIS";
  const partialNote = isCliffPartial
    ? t("headlinePartialNote", { year: yearStr, sensor: partialSensor })
    : null;

  const swapMs = reduceMotion || idle ? 0 : revealAnswer ? durations.settle : durations.settle || durations.merge;
  const swapSeconds = swapMs / 1000;
  const slideY = reduceMotion ? 0 : 6;

  return (
    <div className={`${styles.wrap} ${styles.fade}`} key="region">
      <span className="sr-only" aria-live="polite">
        {`${line1Plain} ${line2Text}`}
      </span>
      <h1 className={styles.headline} data-answer={revealAnswer}>
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
              {line1Content}
            </motion.span>
          </AnimatePresence>
        </span>
        <span className="sr-only">{line1Plain}</span>
      </h1>
      <p className={styles.subline}>
        <span className={styles.sublineText}>{line2Text}</span>
        {partialNote && (
          <button
            type="button"
            className={styles.infoButton}
            aria-label={partialNote}
            title={partialNote}
          >
            i
          </button>
        )}
      </p>
    </div>
  );
}
