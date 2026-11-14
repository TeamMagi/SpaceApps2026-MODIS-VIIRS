import { memo, useEffect, useId, useRef, useState, type FocusEvent } from "react";
import { useCopy } from "../../hooks/useCopy";
import styles from "./MapControls.module.css";

export interface MapControlsProps {
  readonly canZoomIn: boolean;
  readonly canZoomOut: boolean;
  readonly isAtFit: boolean;
  readonly onZoomIn: () => void;
  readonly onZoomOut: () => void;
  readonly onReset: () => void;
}

const SHORTCUTS = [
  { keys: "mapShortcutPanKeys", action: "mapShortcutPanAction" },
  { keys: "mapShortcutZoomKeys", action: "mapShortcutZoomAction" },
  { keys: "mapShortcutFitKeys", action: "mapShortcutFitAction" },
  { keys: "mapShortcutNavKeys", action: "mapShortcutNavAction" },
  { keys: "mapShortcutEscKeys", action: "mapShortcutEscAction" },
] as const;

function MapControls({
  canZoomIn,
  canZoomOut,
  isAtFit,
  onZoomIn,
  onZoomOut,
  onReset,
}: MapControlsProps) {
  const { t } = useCopy();
  const popoverId = useId();
  const helpWrapRef = useRef<HTMLDivElement | null>(null);
  const helpButtonRef = useRef<HTMLButtonElement | null>(null);

  const [isHelpOpen, setIsHelpOpen] = useState(false);

  useEffect(() => {
    if (!isHelpOpen) return;

    const handlePointerDown = (event: PointerEvent) => {
      if (helpWrapRef.current && !helpWrapRef.current.contains(event.target as Node)) {
        setIsHelpOpen(false);
      }
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      setIsHelpOpen(false);
      helpButtonRef.current?.focus({ preventScroll: true });
    };

    document.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown, true);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [isHelpOpen]);

  const handleHelpBlur = (event: FocusEvent<HTMLDivElement>) => {
    if (isHelpOpen && !event.currentTarget.contains(event.relatedTarget)) {
      setIsHelpOpen(false);
    }
  };

  return (
    <div className={styles.controlsWrap} data-map-ui="true">
      <div className={styles.controls} role="group" aria-label={t("mapZoomLabel")}>
        <button
          type="button"
          className={styles.button}
          aria-label={t("mapZoomIn")}
          title={t("mapZoomIn")}
          disabled={!canZoomIn}
          onClick={onZoomIn}
        >
          <span aria-hidden="true">+</span>
        </button>
        <button
          type="button"
          className={styles.button}
          aria-label={t("mapZoomOut")}
          title={t("mapZoomOut")}
          disabled={!canZoomOut}
          onClick={onZoomOut}
        >
          <span aria-hidden="true">−</span>
        </button>
        <button
          type="button"
          className={`${styles.button} ${styles.fitButton}`}
          aria-label={t("mapZoomReset")}
          title={t("mapFitCountry")}
          disabled={isAtFit}
          onClick={onReset}
        >
          <span aria-hidden="true">{t("mapFitCountry")}</span>
        </button>
        <div ref={helpWrapRef} className={styles.helpWrap} onBlur={handleHelpBlur}>
          <button
            ref={helpButtonRef}
            type="button"
            className={styles.button}
            aria-label={t("mapShortcutsLabel")}
            title={t("mapShortcutsLabel")}
            aria-expanded={isHelpOpen}
            aria-controls={isHelpOpen ? popoverId : undefined}
            onClick={() => setIsHelpOpen((prev) => !prev)}
          >
            <span aria-hidden="true">?</span>
          </button>

          {isHelpOpen && (
            <div
              id={popoverId}
              className={styles.popover}
              role="region"
              aria-label={t("mapShortcutsLabel")}
            >
              <p className={styles.popoverTitle}>{t("mapShortcutsLabel")}</p>
              <dl className={styles.shortcutList}>
                {SHORTCUTS.map((item) => (
                  <div key={item.keys} className={styles.shortcutRow}>
                    <dt className={styles.shortcutKeyWrap}>
                      <kbd className={styles.kbd}>{t(item.keys)}</kbd>
                    </dt>
                    <dd className={styles.shortcutAction}>{t(item.action)}</dd>
                  </div>
                ))}
              </dl>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default memo(MapControls);
