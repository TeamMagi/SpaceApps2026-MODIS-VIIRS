import { memo } from "react";
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

function MapControls({
  canZoomIn,
  canZoomOut,
  isAtFit,
  onZoomIn,
  onZoomOut,
  onReset,
}: MapControlsProps) {
  const { t } = useCopy();

  return (
    <div
      className={styles.controls}
      role="group"
      aria-label={t("mapZoomLabel")}
      data-map-ui="true"
    >
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
    </div>
  );
}

export default memo(MapControls);
