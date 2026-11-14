import { memo, useState } from "react";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import { FRP_BREAKS } from "../../lib/scales";
import styles from "./MapLegend.module.css";

const FRP_STEPS = [1, 2, 3, 4, 5, 6, 7, 8] as const;
const DESKTOP_WIDTH_PX = 1024;
const MIN_OPEN_HEIGHT_PX = 640;

function shouldDefaultOpen(): boolean {
  if (typeof window === "undefined") return false;
  if (typeof window.matchMedia === "function") {
    return window.matchMedia(
      `(min-width: ${DESKTOP_WIDTH_PX}px) and (min-height: ${MIN_OPEN_HEIGHT_PX}px)`,
    ).matches;
  }
  return window.innerWidth >= DESKTOP_WIDTH_PX && window.innerHeight >= MIN_OPEN_HEIGHT_PX;
}

function MapLegend() {
  const { t, fmt } = useCopy();
  const { coverage } = useRegionData();
  const { geometryHarmonized, showDuplicateRings } = useHarmonizeTransition((v) => ({
    geometryHarmonized: v.geometryHarmonized,
    showDuplicateRings: v.showDuplicateRings,
  }));

  const [isOpen, setIsOpen] = useState<boolean>(shouldDefaultOpen);

  const modisLabel = coverage.modis
    ? t("mapLegendModis", {
        first: String(coverage.modis.first),
        last: String(coverage.modis.last),
      })
    : t("mapLegendModisShort");
  const viirsLabel = coverage.viirs
    ? t("mapLegendViirs", {
        first: String(coverage.viirs.first),
        last: String(coverage.viirs.last),
      })
    : t("mapLegendViirsShort");

  return (
    <div
      className={styles.legend}
      data-harmonized={geometryHarmonized}
      data-rings={showDuplicateRings}
      data-open={isOpen}
      data-map-ui="true"
    >
      <button
        type="button"
        className={styles.toggleButton}
        aria-expanded={isOpen}
        onClick={() => setIsOpen((prev) => !prev)}
      >
        <span>{isOpen ? t("mapLegendToggleHide") : t("mapLegendToggleShow")}</span>
        <span className={styles.chevron} aria-hidden="true">
          {isOpen ? "▾" : "▸"}
        </span>
      </button>

      {isOpen && (
        <div className={styles.body}>
          <ul className={styles.shapeList} role="list">
            <li className={styles.row}>
              <span className={styles.markWrap} aria-hidden="true">
                <span className={styles.modis} />
              </span>
              <span>{modisLabel}</span>
            </li>
            <li className={styles.row}>
              <span className={styles.markWrap} aria-hidden="true">
                <span className={styles.viirs} />
              </span>
              <span>{viirsLabel}</span>
            </li>
            <li className={styles.row}>
              <span className={styles.markWrap} aria-hidden="true">
                <span className={styles.harmonizedDot} />
              </span>
              <span>{t("mapLegendHarmonized")}</span>
            </li>
            <li className={styles.row} data-active={showDuplicateRings}>
              <span className={styles.markWrap} aria-hidden="true">
                <span className={styles.ring} />
              </span>
              <span>{t("mapLegendDuplicate")}</span>
            </li>
          </ul>

          <div className={styles.frpSection}>
            <p className={styles.title}>{t("mapLegendFrp")}</p>
            <div className={styles.ramp} aria-hidden="true">
              {FRP_STEPS.map((step) => (
                <span key={step} className={styles.swatch} data-bucket={step} />
              ))}
            </div>
            <div className={styles.ticks}>
              {FRP_BREAKS.map((val) => (
                <span key={val}>{fmt.number(val)}</span>
              ))}
            </div>
          </div>

          <p className={styles.dotNote}>{t("mapLegendDotNote")}</p>
          <p className={styles.helpLine}>{t("mapKeyboardHelp")}</p>
        </div>
      )}
    </div>
  );
}

export default memo(MapLegend);
