import { memo, useState } from "react";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import { FRP_BREAKS } from "../../lib/scales";
import styles from "./MapLegend.module.css";

export interface SensorFilter {
  readonly modis: boolean;
  readonly viirs: boolean;
}

export interface MapLegendProps {
  readonly sensorFilter?: SensorFilter;
  readonly onSensorFilterChange?: (next: SensorFilter) => void;
}

const DEFAULT_SENSOR_FILTER: SensorFilter = { modis: true, viirs: true };
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

/** Never allow both sensors off: clicking the sole active sensor restores both. */
function nextSensorFilter(current: SensorFilter, target: keyof SensorFilter): SensorFilter {
  const other: keyof SensorFilter = target === "modis" ? "viirs" : "modis";
  if (current[target] && !current[other]) {
    return { modis: true, viirs: true };
  }
  return { ...current, [target]: !current[target] };
}

function MapLegend({
  sensorFilter = DEFAULT_SENSOR_FILTER,
  onSensorFilterChange,
}: MapLegendProps) {
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

  const effectiveSensorFilter = geometryHarmonized ? DEFAULT_SENSOR_FILTER : sensorFilter;

  const handleToggleSensor = (target: keyof SensorFilter) => {
    if (geometryHarmonized) return;
    onSensorFilterChange?.(nextSensorFilter(effectiveSensorFilter, target));
  };

  return (
    <div
      className={styles.legend}
      data-harmonized={geometryHarmonized}
      data-rings={showDuplicateRings}
      data-open={isOpen}
      data-map-ui="true"
    >
      <div className={styles.header}>
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
          <div className={styles.rawSection} aria-hidden={geometryHarmonized}>
            <div className={styles.collapsibleInner}>
              <p className={styles.filterHint}>{t("mapLegendFilterHint")}</p>
            </div>
          </div>
        )}
      </div>

      {isOpen && (
        <div className={styles.body}>
          <div className={styles.rawSection} aria-hidden={geometryHarmonized}>
            <div className={styles.collapsibleInner}>
              <div className={styles.sensorGroup}>
                <button
                  type="button"
                  className={styles.sensorChip}
                  aria-pressed={effectiveSensorFilter.modis}
                  tabIndex={geometryHarmonized ? -1 : undefined}
                  onClick={() => handleToggleSensor("modis")}
                >
                  <span className={styles.markWrap} aria-hidden="true">
                    <span className={styles.modis} />
                  </span>
                  <span className={styles.sensorLabel}>{modisLabel}</span>
                </button>

                <button
                  type="button"
                  className={styles.sensorChip}
                  aria-pressed={effectiveSensorFilter.viirs}
                  tabIndex={geometryHarmonized ? -1 : undefined}
                  onClick={() => handleToggleSensor("viirs")}
                >
                  <span className={styles.markWrap} aria-hidden="true">
                    <span className={styles.viirs} />
                  </span>
                  <span className={styles.sensorLabel}>{viirsLabel}</span>
                </button>
              </div>
            </div>
          </div>

          <div className={styles.harmonizedSection} aria-hidden={!geometryHarmonized}>
            <div className={styles.collapsibleInner}>
              <div className={styles.harmonizedBlock}>
                <div className={styles.row}>
                  <span className={styles.markWrap} aria-hidden="true">
                    <span className={styles.harmonizedDot} />
                  </span>
                  <span>{t("mapLegendHarmonized")}</span>
                </div>

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
              </div>
            </div>
          </div>

          <div
            className={styles.ringsSection}
            aria-hidden={!geometryHarmonized && !showDuplicateRings}
          >
            <div className={styles.collapsibleInner}>
              <div className={styles.ringRow} data-active={showDuplicateRings}>
                <span className={styles.markWrap} aria-hidden="true">
                  <span className={styles.ring} />
                </span>
                <span>{t("mapLegendDuplicate")}</span>
              </div>
            </div>
          </div>

          <p className={styles.dotNote}>{t("mapLegendDotNote")}</p>
        </div>
      )}
    </div>
  );
}

export default memo(MapLegend);
