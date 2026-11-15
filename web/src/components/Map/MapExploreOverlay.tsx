import { memo, type ReactNode } from "react";
import { useCopy } from "../../hooks/useCopy";
import { useRegionData } from "../../hooks/useRegionData";
import { useAppStore } from "../../store/useAppStore";
import styles from "./MapExploreOverlay.module.css";

export interface MapExploreOverlayProps {
  readonly regionName: string;
  /** Country view: rendered right of the World button instead of the region/month label (the map passes <MapStatusBar/>). */
  readonly children?: ReactNode;
}

function HandIcon() {
  return (
    <svg className={styles.hand} viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
      <path
        d="M9 11V5.5a1.5 1.5 0 0 1 3 0V11m0-1.5a1.5 1.5 0 0 1 3 0V11m0-.5a1.5 1.5 0 0 1 3 0V15a6 6 0 0 1-6 6h-1.2a5 5 0 0 1-4-2L4.5 15.5a1.5 1.5 0 0 1 2.3-1.9L9 16"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** Whole-Earth hint before a country is picked; a way back to the world plus the month label after. */
function MapExploreOverlay({ regionName, children }: MapExploreOverlayProps) {
  const { t, fmt } = useCopy();
  const regionPicked = useAppStore((s) => s.regionPicked);
  const showWorld = useAppStore((s) => s.showWorld);
  const selectedMonth = useAppStore((s) => s.selectedMonth);
  const { defaultMapMonth } = useRegionData();
  const month = selectedMonth ?? defaultMapMonth;

  if (!regionPicked) {
    return (
      <p className={styles.hint} data-map-ui="true">
        <HandIcon />
        <span>{t("mapWorldHint")}</span>
      </p>
    );
  }
  return (
    <div className={styles.topLeft} data-map-ui="true">
      <button type="button" className={styles.worldButton} aria-label={t("mapWorldButtonLabel")} onClick={showWorld}>
        <span aria-hidden="true">‹</span> {t("mapWorldButton")}
      </button>
      {children ?? (
        <span className={styles.label}>
          {regionName}
          {month ? ` · ${fmt.monthYear(month, "long")}` : ""}
        </span>
      )}
    </div>
  );
}

export default memo(MapExploreOverlay);
