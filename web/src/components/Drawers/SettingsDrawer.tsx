import { useEffect, useId, useRef, useState } from "react";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { flushPipelineOverrides, useMapPoints } from "../../hooks/useRegionData";
import type { FusionLabelKey } from "../../lib/fusionModels";
import {
  DEFAULT_PIPELINE_CONFIG,
  hasPipelineOverrides,
  PIPELINE_PARAM_SPECS,
  type PipelineParamSpec,
  type PipelineUnitKind,
} from "../../lib/pipelineOverrides";
import { monthName } from "../../lib/format";
import { useAppStore } from "../../store/useAppStore";
import { DEFAULT_FUSION_MODEL, type FusionModel, type Palette, type PipelineOverrideKey, type Theme } from "../../types";
import Drawer from "./Drawer";
import styles from "./SettingsDrawer.module.css";

type SettingsLabelKey =
  | "settingsDedupTimeLabel"
  | "settingsDedupOverlapLabel"
  | "settingsMatchRadiusLabel"
  | "settingsMatchWindowLabel"
  | "settingsFleetRadiusLabel";

type SettingsUnitKey = "settingsUnitSec" | "settingsUnitPct" | "settingsUnitKm" | "settingsUnitMin";
type SettingsHintKey = "settingsHintSec" | "settingsHintPct" | "settingsHintKm" | "settingsHintMin";
type SettingsValueKey = "settingsValueSec" | "settingsValuePct" | "settingsValueKm" | "settingsValueMin";

const FUSION_OPTIONS: readonly {
  value: FusionModel;
  label: FusionLabelKey;
  hint: "fusionViirsPreferredHint" | "fusionEqualWeightHint" | "fusionModisCalibratedHint";
}[] = [
  { value: "viirs_preferred", label: "fusionViirsPreferred", hint: "fusionViirsPreferredHint" },
  { value: "equal_weight", label: "fusionEqualWeight", hint: "fusionEqualWeightHint" },
  { value: "modis_calibrated", label: "fusionModisCalibrated", hint: "fusionModisCalibratedHint" },
];

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

const LABEL_KEY: Record<PipelineOverrideKey, SettingsLabelKey> = {
  dedupTimeSecs: "settingsDedupTimeLabel",
  dedupOverlap: "settingsDedupOverlapLabel",
  matchRadiusKm: "settingsMatchRadiusLabel",
  matchWindowSecs: "settingsMatchWindowLabel",
  fleetRadiusKm: "settingsFleetRadiusLabel",
};

const UNIT_KEYS: Record<
  PipelineUnitKind,
  { unit: SettingsUnitKey; hint: SettingsHintKey; value: SettingsValueKey }
> = {
  sec: { unit: "settingsUnitSec", hint: "settingsHintSec", value: "settingsValueSec" },
  pct: { unit: "settingsUnitPct", hint: "settingsHintPct", value: "settingsValuePct" },
  km: { unit: "settingsUnitKm", hint: "settingsHintKm", value: "settingsValueKm" },
  min: { unit: "settingsUnitMin", hint: "settingsHintMin", value: "settingsValueMin" },
};

function ratioChange(next: number, base: number): number {
  return base === 0 ? 0 : (next - base) / base;
}

function clampUiValue(spec: PipelineParamSpec, raw: number): number {
  const clamped = Math.min(spec.uiMax, Math.max(spec.uiMin, raw));
  return spec.uiInteger ? Math.round(clamped) : Math.round(clamped * 100) / 100;
}

export default function SettingsDrawer() {
  const { t, fmt, lang } = useCopy();
  const baseId = useId();
  const openDrawer = useAppStore((state) => state.openDrawer);
  const pipelineOverrides = useAppStore((state) => state.pipelineOverrides);
  const serverConfig = useAppStore((state) => state.serverConfig);
  const setPipelineOverride = useAppStore((state) => state.setPipelineOverride);
  const resetPipelineOverrides = useAppStore((state) => state.resetPipelineOverrides);
  const fusionModel = useAppStore((state) => state.fusionModel);
  const setFusionModel = useAppStore((state) => state.setFusionModel);
  const { revealAnswer } = useHarmonizeTransition((v) => ({ revealAnswer: v.revealAnswer }));
  const theme = useAppStore((state) => state.theme);
  const setTheme = useAppStore((state) => state.setTheme);
  const palette = useAppStore((state) => state.palette);
  const setPalette = useAppStore((state) => state.setPalette);
  const presentation = useAppStore((state) => state.presentation);
  const setPresentation = useAppStore((state) => state.setPresentation);
  const { overrideError, fusionComparison, fusionModel: shownModel, hasData, period, stages } = useMapPoints();

  const [draftTexts, setDraftTexts] = useState<Partial<Record<PipelineOverrideKey, string>>>({});

  // Drop unsaved text drafts whenever the drawer closes or reopens.
  useEffect(() => {
    setDraftTexts({});
  }, [openDrawer]);

  // The comparison carries harmonized numbers, so Raw mode keeps it hidden until the answer is revealed.
  const showComparison =
    revealAnswer && hasData && fusionModel !== DEFAULT_FUSION_MODEL && shownModel === fusionModel;
  const defaults = serverConfig ?? DEFAULT_PIPELINE_CONFIG;
  const hasOverrides = hasPipelineOverrides(pipelineOverrides, defaults);

  // The default-threshold count is only known while no override is active, so it is remembered per month.
  const periodKey = `${period.year}-${period.month}`;
  const baseline = useRef<{ key: string; n: number } | null>(null);
  const customActive = hasOverrides || fusionModel !== DEFAULT_FUSION_MODEL;
  if (hasData && !customActive) baseline.current = { key: periodKey, n: stages.harmonized };
  const defaultCount = baseline.current?.key === periodKey ? baseline.current.n : null;
  const monthLabel =
    period.year === null
      ? ""
      : period.month === null
        ? String(period.year)
        : `${monthName(lang, period.month, "long")} ${period.year}`;

  const commitUi = (spec: PipelineParamSpec, uiValue: number) => {
    setPipelineOverride(spec.key, spec.uiToApi(uiValue));
  };

  const handleReset = () => {
    setDraftTexts({});
    resetPipelineOverrides();
    setFusionModel(DEFAULT_FUSION_MODEL);
    flushPipelineOverrides();
  };

  return (
    <Drawer
      id="settings"
      title={t("settingsTitle")}
      description={t("settingsNote")}
      footer={
        <button
          type="button"
          className={styles.resetButton}
          disabled={!hasOverrides && fusionModel === DEFAULT_FUSION_MODEL}
          onClick={handleReset}
        >
          {t("settingsReset")}
        </button>
      }
    >
      {overrideError && (
        <p className={styles.errorBanner} role="alert">
          {overrideError}
        </p>
      )}

      <h3 className={styles.sectionTitle}>{t("settingsAppearanceTitle")}</h3>
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
                  <span className={styles.swatch} data-palette-preview={option.id} aria-hidden="true" />
                </span>
              </label>
            );
          })}
        </div>
      </fieldset>
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
      <label className={styles.setting}>
        <span className={styles.checkLabel}>
          <input type="checkbox" checked={presentation} onChange={(event) => setPresentation(event.target.checked)} />
          <span>{t("presentationMode")}</span>
        </span>
        <span className={styles.hint}>{t("presentationModeHint")}</span>
      </label>

      <details className={styles.advanced}>
        <summary className={styles.advancedSummary}>{t("settingsAdvancedTitle")}</summary>
        <p className={styles.hint}>{t("settingsAdvancedScope")}</p>
        {hasData && (
          <p className={styles.live} aria-live="polite">
            {t("settingsAdvancedLive", { month: monthLabel, n: fmt.number(stages.harmonized) })}
            {customActive &&
              (defaultCount !== null ? (
                <span className={styles.liveDefault}>
                  {" ("}
                  {t("settingsAdvancedDefault", { n: fmt.number(defaultCount) })}
                  {")"}
                </span>
              ) : (
                <span className={styles.badge}>{t("settingsAdvancedCustom")}</span>
              ))}
          </p>
        )}
      <fieldset className={styles.fusion}>
        <legend className={styles.fusionLegend}>{t("fusionTitle")}</legend>
        <p className={styles.hint}>{t("fusionNote")}</p>
        {FUSION_OPTIONS.map((option) => (
          <label key={option.value} className={styles.fusionOption}>
            <input
              type="radio"
              name={`${baseId}-fusion`}
              className={styles.fusionRadio}
              value={option.value}
              checked={fusionModel === option.value}
              onChange={() => setFusionModel(option.value)}
            />
            <span className={styles.fusionText}>
              <span className={styles.paramLabel}>{t(option.label)}</span>
              <span className={styles.hint}>{t(option.hint)}</span>
            </span>
          </label>
        ))}
        {showComparison && fusionComparison && (
          <div className={styles.fusionResult} aria-live="polite">
            <p className={styles.hint}>
              {t("fusionComparison", {
                count: fmt.number(fusionComparison.count),
                baseline: fmt.number(fusionComparison.baselineCount),
                delta: fmt.signedPercent(ratioChange(fusionComparison.count, fusionComparison.baselineCount)),
                frpDelta: fmt.signedPercent(
                  ratioChange(fusionComparison.totalFrp, fusionComparison.baselineTotalFrp),
                ),
              })}
            </p>
            {fusionComparison.model === "modis_calibrated" && (
              <p className={styles.hint}>
                {t("fusionDropped", { n: fmt.number(fusionComparison.droppedBelowModisLimit) })}
              </p>
            )}
          </div>
        )}
      </fieldset>

      <div className={styles.list}>
        {PIPELINE_PARAM_SPECS.map((spec) => {
          const label = t(LABEL_KEY[spec.key]);
          const unitCopy = UNIT_KEYS[spec.unit];
          const unitLabel = t(unitCopy.unit);
          const apiValue = pipelineOverrides[spec.key] ?? spec.getDefaultApi(defaults);
          const uiValue = spec.apiToUi(apiValue);
          const defaultUiValue = spec.apiToUi(spec.getDefaultApi(defaults));
          const sliderId = `${baseId}-${spec.key}-range`;
          const numberValue = draftTexts[spec.key] ?? String(uiValue);

          return (
            <div key={spec.key} className={styles.paramCard}>
              <div className={styles.paramHeader}>
                <label htmlFor={sliderId} className={styles.paramLabel}>
                  {label}
                </label>
                <div className={styles.numberGroup}>
                  <input
                    type="number"
                    className={styles.numberInput}
                    aria-label={t("settingsNumberInputAria", { label, unit: unitLabel })}
                    min={spec.uiMin}
                    max={spec.uiMax}
                    step={spec.uiStep}
                    inputMode={spec.uiInteger ? "numeric" : "decimal"}
                    value={numberValue}
                    onChange={(e) => {
                      const raw = e.currentTarget.value;
                      setDraftTexts((prev) => ({ ...prev, [spec.key]: raw }));
                      const parsed = Number(raw);
                      if (raw.trim() !== "" && Number.isFinite(parsed)) {
                        if (parsed >= spec.uiMin && parsed <= spec.uiMax) {
                          const normalized = spec.uiInteger ? Math.round(parsed) : parsed;
                          commitUi(spec, normalized);
                        }
                      }
                    }}
                    onBlur={(e) => {
                      const raw = e.currentTarget.value.trim();
                      setDraftTexts((prev) => {
                        const next = { ...prev };
                        delete next[spec.key];
                        return next;
                      });
                      if (raw === "") return;
                      const parsed = Number(raw);
                      if (Number.isFinite(parsed)) {
                        commitUi(spec, clampUiValue(spec, parsed));
                      }
                    }}
                  />
                  <span className={styles.unit} aria-hidden="true">
                    {unitLabel}
                  </span>
                </div>
              </div>

              <input
                id={sliderId}
                type="range"
                className={styles.slider}
                min={spec.uiMin}
                max={spec.uiMax}
                step={spec.uiStep}
                value={uiValue}
                aria-valuetext={t(unitCopy.value, { value: uiValue })}
                onChange={(e) => {
                  const nextUi = Number(e.currentTarget.value);
                  setDraftTexts((prev) => {
                    if (!(spec.key in prev)) return prev;
                    const next = { ...prev };
                    delete next[spec.key];
                    return next;
                  });
                  if (Number.isFinite(nextUi)) {
                    commitUi(spec, nextUi);
                  }
                }}
              />

              <p className={styles.hint}>
                {t(unitCopy.hint, {
                  default: defaultUiValue,
                  min: spec.uiMin,
                  max: spec.uiMax,
                })}
              </p>
            </div>
          );
        })}
      </div>
      </details>
    </Drawer>
  );
}
