/**
 * ViewToggle — segmented pill control for Raw vs. Harmonized (Von Restorff primary action).
 *
 * `aria-pressed` follows `targetMode`, which flips synchronously on press, so the control answers
 * within one frame while the choreography runs. Pressing H toggles the same store action.
 */

import { useId } from "react";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import styles from "./ViewToggle.module.css";

export default function ViewToggle() {
  const { t } = useCopy();
  const { targetMode, toggle } = useHarmonizeTransition((v) => ({
    targetMode: v.targetMode,
    toggle: v.toggle,
  }));
  const hintId = useId();
  const harmonized = targetMode === "harmonized";
  const hintText = harmonized ? t("toggleHintRaw") : t("toggleHint");

  return (
    <div className={styles.wrap}>
      <div className={styles.segmented} role="group" aria-label={t("toggleAriaLabel")}>
        <button
          type="button"
          className={styles.button}
          data-side="raw"
          aria-pressed={!harmonized}
          title={hintText}
          aria-describedby={hintId}
          onClick={() => {
            if (harmonized) toggle();
          }}
        >
          <span className={styles.dots} aria-hidden="true">
            <span className={`${styles.dot} ${styles.rawDot} ${styles.dot1}`} />
            <span className={`${styles.dot} ${styles.rawDot} ${styles.dot2}`} />
            <span className={`${styles.dot} ${styles.rawDot} ${styles.dot3}`} />
          </span>
          <span className={styles.label}>{t("toggleRawLabel")}</span>
        </button>

        <button
          type="button"
          className={styles.button}
          data-side="harmonized"
          aria-pressed={harmonized}
          title={hintText}
          aria-describedby={hintId}
          onClick={() => {
            if (!harmonized) toggle();
          }}
        >
          <span className={styles.dots} aria-hidden="true">
            <span className={`${styles.dot} ${styles.harmDot} ${styles.dot1}`} />
            <span className={`${styles.dot} ${styles.harmDot} ${styles.dot2}`} />
            <span className={`${styles.dot} ${styles.harmDot} ${styles.dot3}`} />
          </span>
          <span className={styles.label}>{t("toggleHarmonizedLabel")}</span>
        </button>
      </div>
      <span id={hintId} className="sr-only">
        {hintText}
      </span>
    </div>
  );
}
