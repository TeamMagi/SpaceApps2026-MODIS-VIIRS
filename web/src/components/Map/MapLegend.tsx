import { memo, useMemo, useState } from "react";
import { useCopy, type CopyApi } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import { FRP_BREAKS } from "../../lib/scales";
import { useAppStore } from "../../store/useAppStore";
import type { Palette } from "../../types";
import styles from "./MapLegend.module.css";

export interface SensorFilter {
  readonly modis: boolean;
  readonly viirs: boolean;
}

export interface MapLegendProps {
  readonly sensorFilter?: SensorFilter;
  readonly onSensorFilterChange?: (next: SensorFilter) => void;
  readonly frpBreaks?: readonly number[];
}

function paletteLabelKey(
  palette: Palette,
): "paletteNameDefault" | "paletteNameViridis" | "paletteNameCividis" {
  switch (palette) {
    case "viridis":
      return "paletteNameViridis";
    case "cividis":
      return "paletteNameCividis";
    case "default":
      return "paletteNameDefault";
  }
}

function formatBreakValue(value: number, fmt: CopyApi["fmt"]): string {
  if (value < 10 && !Number.isInteger(value)) {
    return fmt.number(value, 1);
  }
  return fmt.number(Math.round(value));
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
  frpBreaks = FRP_BREAKS,
}: MapLegendProps) {
  const { t, fmt } = useCopy();
  const { coverage } = useRegionData();
  const palette = useAppStore((state) => state.palette);
  const { geometryHarmonized, showDuplicateRings } = useHarmonizeTransition((v) => ({
    geometryHarmonized: v.geometryHarmonized,
    showDuplicateRings: v.showDuplicateRings,
  }));

  const activeBreaks = frpBreaks.length === FRP_BREAKS.length ? frpBreaks : FRP_BREAKS;
  const isQuantile = frpBreaks !== FRP_BREAKS && frpBreaks.length === FRP_BREAKS.length;
  const paletteName = t(paletteLabelKey(palette));
  const scaleModeText = isQuantile ? t("mapLegendScaleQuantile") : t("mapLegendScaleFixed");

  const formattedTicks = useMemo(
    () => activeBreaks.map((val) => formatBreakValue(val, fmt)),
    [activeBreaks, fmt],
  );
  const bucketRanges = useMemo(
    () =>
      FRP_STEPS.map((_step, index) => {
        if (index === 0) {
          return t("mapLegendFrpRange", { min: fmt.number(0), max: formattedTicks[0] });
        }
        if (index === FRP_STEPS.length - 1) {
          return t("mapLegendFrpRangePlus", { min: formattedTicks[formattedTicks.length - 1] });
        }
        return t("mapLegendFrpRange", {
          min: formattedTicks[index - 1],
          max: formattedTicks[index],
        });
      }),
    [formattedTicks, fmt, t],
  );

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
                  <div className={styles.frpHeader}>
                    <p className={styles.title}>{t("mapLegendFrp")}</p>
                    <span className={styles.paletteMeta}>
                      {paletteName} · {scaleModeText}
                    </span>
                  </div>
                  <div className={styles.ramp} role="img" aria-label={`${paletteName} · ${scaleModeText}`}>
                    {FRP_STEPS.map((step, index) => (
                      <span
                        key={step}
                        className={styles.swatch}
                        data-bucket={step}
                        title={bucketRanges[index]}
                      />
                    ))}
                  </div>
                  <div className={styles.ticks}>
                    {formattedTicks.map((label, index) => (
                      <span key={`${index}-${label}`}>{label}</span>
                    ))}
                  </div>
                  <div className={styles.rangeRow} aria-hidden="true">
                    <span>{bucketRanges[0]}</span>
                    <span>{bucketRanges[bucketRanges.length - 1]}</span>
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
