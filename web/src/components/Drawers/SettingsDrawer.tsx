import { useEffect, useId, useState } from "react";
import { useCopy } from "../../hooks/useCopy";
import { flushPipelineOverrides, useMapPoints } from "../../hooks/useRegionData";
import {
  DEFAULT_PIPELINE_CONFIG,
  hasPipelineOverrides,
  PIPELINE_PARAM_SPECS,
  type PipelineParamSpec,
  type PipelineUnitKind,
} from "../../lib/pipelineOverrides";
import { useAppStore } from "../../store/useAppStore";
import type { PipelineOverrideKey } from "../../types";
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

function clampUiValue(spec: PipelineParamSpec, raw: number): number {
  const clamped = Math.min(spec.uiMax, Math.max(spec.uiMin, raw));
  return spec.uiInteger ? Math.round(clamped) : Math.round(clamped * 100) / 100;
}

export default function SettingsDrawer() {
  const { t } = useCopy();
  const baseId = useId();
  const openDrawer = useAppStore((state) => state.openDrawer);
  const pipelineOverrides = useAppStore((state) => state.pipelineOverrides);
  const serverConfig = useAppStore((state) => state.serverConfig);
  const setPipelineOverride = useAppStore((state) => state.setPipelineOverride);
  const resetPipelineOverrides = useAppStore((state) => state.resetPipelineOverrides);
  const { overrideError } = useMapPoints();

  const [draftTexts, setDraftTexts] = useState<Partial<Record<PipelineOverrideKey, string>>>({});

  // Drop unsaved text drafts whenever the drawer closes or reopens.
  useEffect(() => {
    setDraftTexts({});
  }, [openDrawer]);

  const defaults = serverConfig ?? DEFAULT_PIPELINE_CONFIG;
  const hasOverrides = hasPipelineOverrides(pipelineOverrides, defaults);

  const commitUi = (spec: PipelineParamSpec, uiValue: number) => {
    setPipelineOverride(spec.key, spec.uiToApi(uiValue));
  };

  const handleReset = () => {
    setDraftTexts({});
    resetPipelineOverrides();
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
          disabled={!hasOverrides}
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
    </Drawer>
  );
}
