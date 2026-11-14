/**
 * ViewToggle — THE primary action and the only saturated, moving element (Von Restorff).
 *
 * aria-checked follows `targetMode`, which flips synchronously on press, so the switch answers
 * within one frame even though the choreography is still running. H does the same
 * (hooks/useKeyboardShortcuts); pressing again mid-transition reverses it (handled by the clock).
 * Never red/green.
 */

import { useId } from "react";
import { motion, useReducedMotion } from "motion/react";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import styles from "./ViewToggle.module.css";

export default function ViewToggle() {
  const { t } = useCopy();
  const { targetMode, toggle } = useHarmonizeTransition((v) => ({ targetMode: v.targetMode, toggle: v.toggle }));
  const reduceMotion = useReducedMotion();
  const hintId = useId();
  const harmonized = targetMode === "harmonized";

  return (
    <div className={styles.wrap}>
      <button
        type="button"
        role="switch"
        aria-checked={harmonized}
        aria-label={t("toggleAriaLabel")}
        aria-describedby={hintId}
        className={styles.switch}
        data-on={harmonized}
        onClick={toggle}
      >
        <span className={styles.side} data-active={!harmonized}>
          <span className={styles.label}>{t("toggleRawLabel")}</span>
          <span className={styles.sub}>{t("toggleRawSub")}</span>
        </span>
        <span className={styles.track} aria-hidden="true">
          <motion.span
            className={styles.thumb}
            initial={false}
            animate={{ x: harmonized ? 20 : 0 }}
            transition={
              reduceMotion
                ? { duration: 0.15 }
                : { type: "spring", stiffness: 520, damping: 34, mass: 0.8 }
            }
          />
        </span>
        <span className={styles.side} data-active={harmonized}>
          <span className={styles.label}>{t("toggleHarmonizedLabel")}</span>
          <span className={styles.sub}>{t("toggleHarmonizedSub")}</span>
        </span>
      </button>
      <p id={hintId} className={styles.hint}>
        {harmonized ? t("toggleHintRaw") : t("toggleHint")}
      </p>
    </div>
  );
}
