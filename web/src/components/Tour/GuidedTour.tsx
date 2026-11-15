/**
 * GuidedTour — the floating card of the 60-second tour. Non-modal: the dashboard stays usable.
 * It holds no timers (hooks/useGuidedTour owns them); the progress bar is a CSS animation that
 * restarts per step (keyed) and pauses with the tour.
 */

import { useEffect, useRef, type KeyboardEvent } from "react";
import { useCopy } from "../../hooks/useCopy";
import { nextTourStep, prevTourStep, TOUR_STEP_MS, TOUR_STEPS } from "../../lib/tour";
import { useAppStore } from "../../store/useAppStore";
import styles from "./GuidedTour.module.css";

/** Element focused when the tour started; module-level so the card can remount across routes. */
let returnFocusTo: HTMLElement | null = null;

export default function GuidedTour() {
  const { t } = useCopy();
  const tourStep = useAppStore((state) => state.tourStep);
  const paused = useAppStore((state) => state.tourPaused);
  const tourRun = useAppStore((state) => state.tourRun);
  const setTourStep = useAppStore((state) => state.setTourStep);
  const setTourPaused = useAppStore((state) => state.setTourPaused);
  const endTour = useAppStore((state) => state.endTour);
  const cardRef = useRef<HTMLDivElement>(null);
  const active = tourStep !== null;

  useEffect(() => {
    if (!active) return;
    if (returnFocusTo === null && document.activeElement instanceof HTMLElement) {
      returnFocusTo = document.activeElement;
    }
    cardRef.current?.focus();
    return () => {
      if (useAppStore.getState().tourStep !== null) return;
      const target = returnFocusTo;
      returnFocusTo = null;
      if (target && document.contains(target)) target.focus();
    };
  }, [active]);

  if (tourStep === null) return null;
  const step = TOUR_STEPS[tourStep];
  if (!step) return null;

  const total = TOUR_STEPS.length;
  const next = nextTourStep(tourStep);
  const isLast = next === null;
  const goNext = () => (next === null ? endTour() : setTourStep(next));
  const goBack = () => setTourStep(prevTourStep(tourStep));

  const copyByStep = {
    toggle: { title: t("tourStepToggleTitle"), body: t("tourStepToggleBody") },
    lens: { title: t("tourStepLensTitle"), body: t("tourStepLensBody", { lens: t("mapFootprintOpen") }) },
    case: { title: t("tourStepCaseTitle"), body: t("tourStepCaseBody") },
    validation: { title: t("tourStepValidationTitle"), body: t("tourStepValidationBody") },
  } as const;
  const { title, body } = copyByStep[step.id];
  const n = tourStep + 1;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      endTour();
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      goNext();
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      goBack();
    }
  };

  return (
    <>
      <span className="sr-only" role="status">
        {t("tourAnnounce", { n, total, title })}
      </span>
      <div
        ref={cardRef}
        className={styles.card}
        role="dialog"
        aria-modal="false"
        aria-labelledby="tour-title"
        tabIndex={-1}
        onKeyDown={onKeyDown}
      >
        <div className={styles.head}>
          <p className={styles.meta}>
            {t("tourStepOf", { n, total })} ·{" "}
            {t("tourTimeWindow", {
              from: String(step.startSec),
              to: String(step.startSec + TOUR_STEP_MS / 1000),
            })}
          </p>
          <button type="button" className={styles.close} aria-label={t("tourSkip")} onClick={endTour}>
            ×
          </button>
        </div>
        <h2 id="tour-title" className={styles.title}>
          {title}
        </h2>
        <p className={styles.body}>{body}</p>
        <div className={styles.track} aria-hidden="true">
          <div key={`${tourRun}:${tourStep}`} className={styles.fill} data-paused={paused} />
        </div>
        <div className={styles.actions}>
          <button type="button" className={styles.button} disabled={tourStep === 0} onClick={goBack}>
            {t("tourBack")}
          </button>
          <button
            type="button"
            className={styles.button}
            aria-pressed={paused}
            onClick={() => setTourPaused(!paused)}
          >
            {paused ? t("tourResume") : t("tourPause")}
          </button>
          <button type="button" className={styles.primary} onClick={goNext}>
            {isLast ? t("tourFinish") : t("tourNext")}
          </button>
        </div>
      </div>
    </>
  );
}
