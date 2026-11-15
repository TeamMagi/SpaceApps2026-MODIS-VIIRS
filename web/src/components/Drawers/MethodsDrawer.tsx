/**
 * MethodsDrawer — "How three satellites' detections are harmonized" (layer 3 of the information hierarchy).
 *
 * Built on Drawer (id "methods"). Sections in order:
 *   1. methodsCliffDef — the ONLY place "sensor cliff" is defined; methodsSensors.
 *   2. methodsStepsTitle — the 6 applied steps + 1 planned step (H3 grid).
 *   3. methodsLimitsTitle — disclaimer (the honest limit, said once), the one-sided smoke/cloud
 *      FRP bias, why low-confidence detections can be excluded, and the archive source citation.
 *   4. methodsSettingsTitle — theme (themeDark/themeLight → store.setTheme) and presentationMode
 *      (store.setPresentation).
 */

import { useId, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { PIPELINE_COPY } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { isPlannedStage, PIPELINE_ORDER } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import { useAppStore } from "../../store/useAppStore";
import type { Palette, PipelineStageKey, Theme } from "../../types";
import MethodsLink from "../Methods/MethodsLink";
import Drawer from "./Drawer";
import styles from "./MethodsDrawer.module.css";

const THEMES: readonly Theme[] = ["dark", "light"];

const PALETTES: ReadonlyArray<{
  readonly id: Palette;
  readonly labelKey: "paletteDefault" | "paletteViridis" | "paletteCividis";
  readonly descKey: "paletteDefaultDesc" | "paletteViridisDesc" | "paletteCividisDesc";
}> = [
  { id: "default", labelKey: "paletteDefault", descKey: "paletteDefaultDesc" },
  { id: "viridis", labelKey: "paletteViridis", descKey: "paletteViridisDesc" },
  { id: "cividis", labelKey: "paletteCividis", descKey: "paletteCividisDesc" },
];

export default function MethodsDrawer() {
  const { t } = useCopy();
  const { coverage } = useRegionData();
  const dataMode = useAppStore((state) => state.dataMode);
  const theme = useAppStore((state) => state.theme);
  const setTheme = useAppStore((state) => state.setTheme);
  const palette = useAppStore((state) => state.palette);
  const setPalette = useAppStore((state) => state.setPalette);
  const presentation = useAppStore((state) => state.presentation);
  const setPresentation = useAppStore((state) => state.setPresentation);
  const reduceMotion = useReducedMotion();
  const baseId = useId();
  const [openSteps, setOpenSteps] = useState<Partial<Record<PipelineStageKey, boolean>>>({});

  const toggleStep = (key: PipelineStageKey) => {
    setOpenSteps((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  const sourceCitation =
    dataMode === "demo"
      ? t("methodsSourceCitationDemo")
      : coverage.modis
        ? coverage.viirs
          ? t("methodsSourceCitation", {
              modisFirst: String(coverage.modis.first),
              modisLast: String(coverage.modis.last),
              viirsFirst: String(coverage.viirs.first),
              viirsLast: String(coverage.viirs.last),
            })
          : t("methodsSourceCitationModis", {
              modisFirst: String(coverage.modis.first),
              modisLast: String(coverage.modis.last),
            })
        : t("methodsSourceCitationFallback");

  return (
    <Drawer id="methods" title={t("methodsTitle")} description={t("methodsCliffDef")}>
      <p className={styles.lead}>{t("methodsSensors")}</p>
      <p className={styles.fullPage}>
        <MethodsLink className={styles.fullPageLink}>{t("methodsFullPageLink")}</MethodsLink>
      </p>

      <h3 className={styles.heading}>{t("methodsStepsTitle")}</h3>
      <ol className={styles.steps} role="list">
        {PIPELINE_ORDER.map((key, index) => {
          const isOpen = openSteps[key] === true;
          const planned = isPlannedStage(key);
          const panelId = `${baseId}-${key}`;
          return (
            <li
              key={key}
              className={styles.stepItem}
              data-open={isOpen}
              data-planned={planned ? "true" : undefined}
            >
              <button
                type="button"
                className={styles.stepTrigger}
                aria-expanded={isOpen}
                aria-controls={panelId}
                onClick={() => toggleStep(key)}
              >
                <span className={styles.stepIndex} aria-hidden="true">
                  {index + 1}
                </span>
                <span className={styles.stepName}>{t(PIPELINE_COPY[key].name)}</span>
                <span className={styles.tech}>{t(PIPELINE_COPY[key].tech)}</span>
                {planned && <span className={styles.plannedTag}>{t("pipelinePlannedBadge")}</span>}
                <span className={styles.chevron} aria-hidden="true" data-open={isOpen} />
              </button>
              <AnimatePresence initial={false}>
                {isOpen && (
                  <motion.div
                    id={panelId}
                    className={styles.stepPanel}
                    initial={{ height: 0, opacity: 0 }}
                    animate={{ height: "auto", opacity: 1 }}
                    exit={{ height: 0, opacity: 0 }}
                    transition={{ duration: reduceMotion ? 0 : 0.18, ease: [0.2, 0, 0, 1] }}
                  >
                    <p className={styles.stepBody}>{t(PIPELINE_COPY[key].method)}</p>
                  </motion.div>
                )}
              </AnimatePresence>
            </li>
          );
        })}
      </ol>

      <div className={styles.diagrams}>
        <figure className={styles.diagram}>
          <svg
            viewBox="0 0 180 84"
            className={styles.svg}
            role="img"
            aria-label={`${t("pipelineTechAngle")}: ${t("methodsStepAngle")}`}
          >
            <line x1="20" y1="68" x2="164" y2="68" className={styles.axis} />
            <line x1="20" y1="14" x2="20" y2="68" className={styles.axis} />
            <path d="M 24 26 Q 92 28 156 56" className={styles.curveRaw} />
            <line x1="24" y1="26" x2="156" y2="26" className={styles.curveCorrected} />
            <line x1="138" y1="49" x2="138" y2="30" className={styles.arrow} />
            <polyline points="134,34 138,29 142,34" className={styles.arrow} />
            <text x="24" y="79" className={styles.svgLabel}>
              {t("angleDegrees", { value: 0 })}
            </text>
            <text x="148" y="79" className={styles.svgLabel}>
              {t("angleDegrees", { value: 60 })}
            </text>
          </svg>
          <figcaption className={styles.caption}>
            <span className={styles.tech}>{t("pipelineTechAngle")}</span>
            <span>{t("pipelineStepAngle")}</span>
          </figcaption>
        </figure>

        <figure className={styles.diagram} data-planned="true">
          <svg
            viewBox="0 0 180 84"
            className={styles.svg}
            role="img"
            aria-label={`${t("pipelineTechGrid")} (${t("pipelinePlannedBadge")}): ${t("methodsStepGrid")}`}
          >
            <polygon points="62,18 76,26 76,42 62,50 48,42 48,26" className={styles.hex} />
            <polygon points="90,18 104,26 104,42 90,50 76,42 76,26" className={styles.hexActive} />
            <polygon points="118,18 132,26 132,42 118,50 104,42 104,26" className={styles.hex} />
            <polygon points="76,42 90,50 90,66 76,74 62,66 62,50" className={styles.hex} />
            <polygon points="104,42 118,50 118,66 104,74 90,66 90,50" className={styles.hex} />
            <circle cx="85" cy="34" r="4" className={styles.modisDot} />
            <rect x="92" y="31" width="5" height="5" className={styles.viirsDot} />
          </svg>
          <figcaption className={styles.caption}>
            <span className={styles.captionHeader}>
              <span className={styles.tech}>{t("pipelineTechGrid")}</span>
              <span className={styles.plannedTag}>{t("pipelinePlannedBadge")}</span>
            </span>
            <span>{t("pipelineStepGrid")}</span>
          </figcaption>
        </figure>
      </div>

      <h3 className={styles.heading}>{t("methodsLimitsTitle")}</h3>
      <p className={styles.bodyText}>{t("disclaimer")}</p>
      <p className={styles.bodyText}>
        {t("uncertaintyConfidenceLabel")}. {t("uncertaintyWideBody")}
      </p>
      <p className={styles.bodyText}>{t("uncertaintyModelCaveat")}</p>
      <p className={styles.bodyText}>{t("methodsAtmosphereNote")}</p>
      <p className={styles.bodyText}>{t("methodsLowConfidenceNote")}</p>

      <div className={styles.references} aria-label={t("methodsSensors")}>
        <p className={styles.refSource}>{sourceCitation}</p>
      </div>

      <h3 className={styles.heading}>{t("methodsSettingsTitle")}</h3>
      <div className={styles.setting} role="group" aria-label={t("themeLabel")}>
        <span className={styles.settingLabel}>{t("themeLabel")}</span>
        <div className={styles.options}>
          {THEMES.map((option) => (
            <button
              key={option}
              type="button"
              className={styles.option}
              aria-pressed={theme === option}
              onClick={() => setTheme(option)}
            >
              {option === "dark" ? t("themeDark") : t("themeLight")}
            </button>
          ))}
        </div>
      </div>
      <fieldset className={styles.paletteFieldset}>
        <legend className={styles.settingLabel}>{t("paletteLabel")}</legend>
        <div className={styles.paletteList}>
          {PALETTES.map((option) => {
            const selected = palette === option.id;
            return (
              <label
                key={option.id}
                className={styles.paletteOption}
                data-selected={selected ? "true" : undefined}
              >
                <input
                  type="radio"
                  name={`${baseId}-palette`}
                  value={option.id}
                  checked={selected}
                  onChange={() => setPalette(option.id)}
                  className={styles.paletteRadio}
                />
                <span className={styles.paletteText}>
                  <span className={styles.paletteTitle}>{t(option.labelKey)}</span>
                  <span className={styles.paletteDesc}>{t(option.descKey)}</span>
                </span>
              </label>
            );
          })}
        </div>
      </fieldset>
      <label className={`${styles.setting} ${styles.settingInline}`}>
        <input type="checkbox" checked={presentation} onChange={(event) => setPresentation(event.target.checked)} />
        <span>{t("presentationMode")}</span>
        <span className={`${styles.tech} ${styles.settingHint}`}>{t("presentationModeHint")}</span>
      </label>
    </Drawer>
  );
}
