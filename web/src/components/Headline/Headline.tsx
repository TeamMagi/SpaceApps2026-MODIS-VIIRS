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
import { headlineFigure } from "../../lib/headline";
import styles from "./Headline.module.css";

const FROM_TOKEN = "\u0000FROM\u0000";
const TO_TOKEN = "\u0000TO\u0000";
const PCT_TOKEN = "\u0000PCT\u0000";
const RANGE_TOKEN = "\u0000RANGE\u0000";

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

function renderTokenized(
  input: string,
  tokens: ReadonlyArray<readonly [token: string, node: ReactNode]>,
): ReactNode {
  const parts: ReactNode[] = [];
  let cursor = 0;
  while (cursor < input.length) {
    let nextIdx = -1;
    let matched: (typeof tokens)[number] | null = null;
    for (const entry of tokens) {
      const idx = input.indexOf(entry[0], cursor);
      if (idx !== -1 && (nextIdx === -1 || idx < nextIdx)) {
        nextIdx = idx;
        matched = entry;
      }
    }
    if (nextIdx === -1 || !matched) {
      parts.push(input.slice(cursor));
      break;
    }
    if (nextIdx > cursor) {
      parts.push(input.slice(cursor, nextIdx));
    }
    parts.push(matched[1]);
    cursor = nextIdx + matched[0].length;
  }
  return parts;
}

function renderHarmonizedHeadline(template: string, vars: { pct: string; from: string; to: string }): ReactNode {
  const fromIdx = template.indexOf(FROM_TOKEN);
  const toIdx = template.indexOf(TO_TOKEN);
  let between = "";
  let normalized = template;
  if (fromIdx !== -1 && toIdx > fromIdx) {
    between = template.slice(fromIdx + FROM_TOKEN.length, toIdx);
    normalized = template.slice(0, fromIdx) + RANGE_TOKEN + template.slice(toIdx + TO_TOKEN.length);
  }

  return renderTokenized(normalized, [
    [
      RANGE_TOKEN,
      <span key="range" className={styles.nowrap}>
        {vars.from}
        {between}
        <span className={styles.year}>{vars.to}</span>
      </span>,
    ],
    [
      PCT_TOKEN,
      <span key="pct" className={styles.nowrap}>
        {vars.pct}
      </span>,
    ],
    [FROM_TOKEN, vars.from],
    [
      TO_TOKEN,
      <span key="to" className={styles.year}>
        {vars.to}
      </span>,
    ],
  ]);
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

  const fromStr = String(cliff.fromYear);
  const yearStr = String(cliff.toYear);
  const figure = headlineFigure(cliff, revealAnswer);
  if (figure.kind === "noBaseline") {
    return (
      <h1 className={styles.headline} data-state="empty">
        <span className={styles.placeholder}>{t("headlineNoBaseline", { from: fromStr, to: yearStr })}</span>
      </h1>
    );
  }
  const pctStr = figure.kind === "harmonized" ? fmt.signedPercent(figure.ratio) : "";
  const text =
    figure.kind === "harmonized"
      ? t("headlineHarmonized", { pct: pctStr, from: fromStr, to: yearStr })
      : figure.dropped
        ? t("headlineRawDropped", { x: figure.absPct, year: yearStr })
        : t("headlineRaw", { x: figure.absPct, year: yearStr });
  const content = revealAnswer
    ? renderHarmonizedHeadline(
        t("headlineHarmonized", { pct: PCT_TOKEN, from: FROM_TOKEN, to: TO_TOKEN }),
        { pct: pctStr, from: fromStr, to: yearStr },
      )
    : renderWithYearAccent(text, yearStr);

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
              {content}
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
