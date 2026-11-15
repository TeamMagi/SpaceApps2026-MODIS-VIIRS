import { memo, useMemo, useState } from "react";
import { useCopy, type CopyApi } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import {
  activeFilterCount,
  MIN_FRP_STEPS,
  resetPointFilters,
  setPointFilters,
  usePointFilters,
  type DayNightFilter,
} from "../../lib/pointFilters";
import { FRP_BREAKS } from "../../lib/scales";
import { useAppStore } from "../../store/useAppStore";
import type { MinConfidence, Palette } from "../../types";
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
const STORAGE_KEY = "pyrosync.legendOpen";

function readStoredOpen(): boolean {
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

function writeStoredOpen(open: boolean): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, open ? "1" : "0");
  } catch {
    // storage unavailable (private mode): the preference just is not remembered
  }
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
  const minConfidence = useAppStore((state) => state.minConfidence);
  const setMinConfidence = useAppStore((state) => state.setMinConfidence);
  const filters = usePointFilters();
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

  const [isOpen, setIsOpen] = useState<boolean>(readStoredOpen);

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

  const activeCount = activeFilterCount(filters) + (minConfidence !== "low" ? 1 : 0);

  const timeOptions: readonly { value: DayNightFilter; label: string }[] = [
    { value: "all", label: t("filterTimeAll") },
    { value: "day", label: t("filterTimeDay") },
    { value: "night", label: t("filterTimeNight") },
  ];
  const confidenceOptions: readonly { value: MinConfidence; label: string }[] = [
    { value: "low", label: t("filterConfidenceAll") },
    { value: "nominal", label: t("filterConfidenceNominal") },
    { value: "high", label: t("filterConfidenceHigh") },
  ];

  const frpStepIndex = Math.max(0, MIN_FRP_STEPS.indexOf(filters.minFrp));

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
        onClick={() => {
          const next = !isOpen;
          setIsOpen(next);
          writeStoredOpen(next);
        }}
      >
        <span className={styles.toggleLabel}>{t("legendFiltersTitle")}</span>
        {activeCount > 0 && (
          <span className={styles.badge}>{t("filterActiveCount", { n: String(activeCount) })}</span>
        )}
        {!isOpen && (
          <span className={styles.preview} aria-hidden="true">
            {FRP_STEPS.map((step) => (
              <span key={step} className={styles.swatch} data-bucket={step} />
            ))}
          </span>
        )}
        <span className={styles.chevron} aria-hidden="true">
          {isOpen ? "▾" : "▸"}
        </span>
      </button>

      {isOpen && (
        <div className={styles.body}>
          <section className={styles.section}>
            <div className={styles.sectionHead}>
              <h3 className={styles.sectionTitle}>{t("legendSectionFrp")}</h3>
              <span className={styles.meta}>
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
          </section>

          <section className={styles.section} aria-hidden={geometryHarmonized}>
            <h3 className={styles.sectionTitle}>{t("legendSectionSensors")}</h3>
            <div className={styles.chipRow}>
              <button
                type="button"
                className={styles.chip}
                aria-pressed={effectiveSensorFilter.modis}
                disabled={geometryHarmonized}
                tabIndex={geometryHarmonized ? -1 : undefined}
                onClick={() => handleToggleSensor("modis")}
              >
                <span className={styles.markWrap} aria-hidden="true">
                  <span className={styles.modis} />
                </span>
                <span>{modisLabel}</span>
              </button>
              <button
                type="button"
                className={styles.chip}
                aria-pressed={effectiveSensorFilter.viirs}
                disabled={geometryHarmonized}
                tabIndex={geometryHarmonized ? -1 : undefined}
                onClick={() => handleToggleSensor("viirs")}
              >
                <span className={styles.markWrap} aria-hidden="true">
                  <span className={styles.viirs} />
                </span>
                <span>{viirsLabel}</span>
              </button>
            </div>
            {!geometryHarmonized && <p className={styles.note}>{t("mapLegendFilterHint")}</p>}
          </section>

          <section className={styles.section}>
            <h3 className={styles.sectionTitle}>{t("legendSectionFilters")}</h3>

            <div className={styles.field}>
              <label className={styles.fieldLabel} htmlFor="legend-min-frp">
                {t("filterMinFrp")}
              </label>
              <div className={styles.rangeWrap}>
                <input
                  id="legend-min-frp"
                  className={styles.slider}
                  type="range"
                  min={0}
                  max={MIN_FRP_STEPS.length - 1}
                  step={1}
                  value={frpStepIndex}
                  onChange={(event) =>
                    setPointFilters({ minFrp: MIN_FRP_STEPS[Number(event.target.value)] ?? 0 })
                  }
                />
                <output className={styles.value} htmlFor="legend-min-frp">
                  {filters.minFrp > 0 ? `≥ ${fmt.number(filters.minFrp)} MW` : t("filterTimeAll")}
                </output>
              </div>
            </div>

            <div className={styles.field} role="group" aria-label={t("filterTime")}>
              <span className={styles.fieldLabel}>{t("filterTime")}</span>
              <div className={styles.segment}>
                {timeOptions.map((option) => (
                  <button
                    key={option.value}
                    type="button"
                    className={styles.segmentButton}
                    aria-pressed={filters.dayNight === option.value}
                    onClick={() => setPointFilters({ dayNight: option.value })}
                  >
                    {option.label}
                  </button>
                ))}
              </div>
            </div>

            <div className={styles.field} role="group" aria-label={t("filterConfidence")}>
              <span className={styles.fieldLabel}>{t("filterConfidence")}</span>
              <div className={styles.segment}>
                {confidenceOptions.map((option) => (
                  <button
                    key={option.value}
                    type="button"
                    className={styles.segmentButton}
                    aria-pressed={minConfidence === option.value}
                    onClick={() => setMinConfidence(option.value)}
                  >
                    {option.label}
                  </button>
                ))}
              </div>
            </div>

            <button
              type="button"
              className={styles.resetButton}
              disabled={activeCount === 0}
              onClick={() => {
                resetPointFilters();
                setMinConfidence("low");
              }}
            >
              {t("filterReset")}
            </button>
          </section>

          <section className={styles.section}>
            <div
              className={styles.ringRow}
              data-active={showDuplicateRings}
              hidden={!geometryHarmonized && !showDuplicateRings}
            >
              <span className={styles.markWrap} aria-hidden="true">
                <span className={styles.ring} />
              </span>
              <span>{t("mapLegendDuplicate")}</span>
            </div>
            <p className={styles.note}>{t("mapLegendDotNote")}</p>
          </section>
        </div>
      )}
    </div>
  );
}

export default memo(MapLegend);
