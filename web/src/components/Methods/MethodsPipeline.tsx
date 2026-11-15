/**
 * MethodsPipeline — the seven stages as a flow diagram (numbered nodes, planned stage dashed) plus
 * an accordion with what / why / parameters / input → output / references per stage.
 *
 * Accordion: each trigger is a button inside a heading (aria-expanded, aria-controls); ArrowUp/Down,
 * Home and End move between triggers. Closed panels use `hidden` (no animation to turn off for
 * reduced motion); print opens every panel.
 */

import { useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { PIPELINE_COPY } from "../../content/copy";
import { REFERENCES } from "../../content/references";
import { useCopy } from "../../hooks/useCopy";
import { isPlannedStage, PIPELINE_ORDER } from "../../hooks/useHarmonizeTransition";
import { STAGE_DETAIL, stageParams, type FlowStage } from "./stages";
import type { MethodsConfig } from "./useMethodsConfig";
import styles from "./MethodsPipeline.module.css";

// PIPELINE_ORDER has no fleet stage (see FlowStage), so the narrowing is safe.
const STAGES = PIPELINE_ORDER as readonly FlowStage[];

type OpenMap = Partial<Record<FlowStage, boolean>>;

export default function MethodsPipeline({ config, status, retry }: MethodsConfig) {
  const { t } = useCopy();
  const baseId = useId();
  const listRef = useRef<HTMLOListElement>(null);
  const [open, setOpen] = useState<OpenMap>({});
  const params = useMemo(() => stageParams(config), [config]);
  const allOpen = STAGES.every((key) => open[key] === true);

  const triggerId = (key: FlowStage) => `${baseId}-trigger-${key}`;
  const panelId = (key: FlowStage) => `${baseId}-panel-${key}`;

  const toggle = (key: FlowStage) => setOpen((prev) => ({ ...prev, [key]: !prev[key] }));
  const setAll = (value: boolean) => setOpen(Object.fromEntries(STAGES.map((key) => [key, value])));

  const openFromDiagram = (key: FlowStage) => {
    setOpen((prev) => ({ ...prev, [key]: true }));
    const trigger = document.getElementById(triggerId(key));
    trigger?.scrollIntoView({ block: "center" });
    trigger?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLOListElement>) => {
    const target = event.target;
    if (!(target instanceof HTMLElement) || target.dataset.stageTrigger === undefined) return;
    const triggers = Array.from(listRef.current?.querySelectorAll<HTMLElement>("[data-stage-trigger]") ?? []);
    const index = triggers.indexOf(target);
    let next = -1;
    if (event.key === "ArrowDown") next = Math.min(index + 1, triggers.length - 1);
    else if (event.key === "ArrowUp") next = Math.max(index - 1, 0);
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = triggers.length - 1;
    if (next < 0) return;
    event.preventDefault();
    triggers[next]?.focus();
  };

  return (
    <div>
      <ol className={styles.flow} role="list" aria-label={t("methodsFlowLabel")}>
        {STAGES.map((key, index) => {
          const planned = isPlannedStage(key);
          return (
            <li key={key} className={styles.node} data-planned={planned ? "true" : undefined}>
              <button type="button" className={styles.nodeButton} onClick={() => openFromDiagram(key)}>
                <span className={styles.nodeIndex} aria-hidden="true">
                  {index + 1}
                </span>
                <span className={styles.nodeText}>
                  <span className={styles.nodeName}>{t(PIPELINE_COPY[key].name)}</span>
                  <span className={styles.nodeTech}>
                    {t(PIPELINE_COPY[key].tech)}
                    {planned && <span className={styles.planned}> · {t("pipelinePlannedBadge")}</span>}
                  </span>
                </span>
              </button>
            </li>
          );
        })}
      </ol>

      <ul className={styles.notes} role="list">
        <li>{t("methodsFlowNoteFleet")}</li>
        <li>{t("methodsFlowNoteGrid")}</li>
        <li>{t("methodsFlowNoteModules")}</li>
      </ul>

      <div className={styles.toolbar}>
        <p className={styles.configNote} role="status">
          {status === "fallback" && (
            <>
              {t("methodsConfigFallback")}{" "}
              <button type="button" className={styles.inlineButton} onClick={retry}>
                {t("methodsConfigRetry")}
              </button>
            </>
          )}
          {status === "live" && t("methodsConfigLive")}
        </p>
        <button type="button" className={styles.expandAll} onClick={() => setAll(!allOpen)}>
          {allOpen ? t("methodsCollapseAll") : t("methodsExpandAll")}
        </button>
      </div>

      <ol ref={listRef} className={styles.steps} role="list" onKeyDown={onKeyDown}>
        {STAGES.map((key, index) => {
          const isOpen = open[key] === true;
          const planned = isPlannedStage(key);
          const detail = STAGE_DETAIL[key];
          const refs = REFERENCES.filter((ref) => ref.stages.includes(key));
          return (
            <li key={key} className={styles.step} data-open={isOpen} data-planned={planned ? "true" : undefined}>
              <h3 className={styles.stepHeading}>
                <button
                  type="button"
                  id={triggerId(key)}
                  className={styles.trigger}
                  data-stage-trigger=""
                  aria-expanded={isOpen}
                  aria-controls={panelId(key)}
                  onClick={() => toggle(key)}
                >
                  <span className={styles.stepIndex} aria-hidden="true">
                    {index + 1}
                  </span>
                  <span className={styles.stepName}>{t(PIPELINE_COPY[key].name)}</span>
                  <span className={styles.stepTech}>{t(PIPELINE_COPY[key].tech)}</span>
                  {planned && <span className={styles.plannedTag}>{t("pipelinePlannedBadge")}</span>}
                  <span className={styles.chevron} aria-hidden="true" data-open={isOpen} />
                </button>
              </h3>
              <div
                id={panelId(key)}
                className={styles.panel}
                role="region"
                aria-labelledby={triggerId(key)}
                hidden={!isOpen}
              >
                <p className={styles.label}>{t("methodsLabelWhat")}</p>
                <p className={styles.text}>{t(detail.what)}</p>
                <p className={styles.label}>{t("methodsLabelWhy")}</p>
                <p className={styles.text}>{t(detail.why)}</p>
                <p className={styles.label}>{t("methodsLabelParams")}</p>
                <dl className={styles.params}>
                  {params[key].map((param) => (
                    <div key={param.name} className={styles.param}>
                      <dt>{param.name}</dt>
                      <dd>{param.value}</dd>
                    </div>
                  ))}
                </dl>
                <p className={styles.label}>{t("methodsLabelIo")}</p>
                <p className={styles.text}>{t(detail.io)}</p>
                {refs.length > 0 && (
                  <>
                    <p className={styles.label}>{t("methodsReferencesTitle")}</p>
                    <p className={styles.text}>
                      {refs.map((ref) => (ref.year === null ? ref.authors : `${ref.authors} (${ref.year})`)).join(" · ")}
                    </p>
                  </>
                )}
              </div>
            </li>
          );
        })}
      </ol>
    </div>
  );
}
