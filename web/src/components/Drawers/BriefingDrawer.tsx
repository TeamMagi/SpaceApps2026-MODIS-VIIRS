/**
 * BriefingDrawer — regional summary derived from the real archive series and points.
 *
 * Harmonized (revealAnswer): the finding (briefingFinding) and the full metric list.
 * Raw: only what the satellites reported (raw total, MODIS/VIIRS split, raw cliff change) and an
 *   invitation to harmonize; the answer is never shown before the user asks for it with the toggle.
 */

import { formatApiError } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { useCopyLink } from "../../hooks/useCopyLink";
import { useExportView } from "../../hooks/useExportView";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData, useRegions } from "../../hooks/useRegionData";
import { useAppStore } from "../../store/useAppStore";
import Drawer from "./Drawer";
import styles from "./BriefingDrawer.module.css";

export default function BriefingDrawer() {
  const { t, fmt } = useCopy();
  const dataMode = useAppStore((state) => state.dataMode);
  const { source } = useRegions();
  const { status, hasData, error, retry, region, totals, cliff, briefing } = useRegionData();
  const revealAnswer = useHarmonizeTransition((v) => v.revealAnswer);
  const { canDownload, download } = useExportView();
  const { copied, copyLink } = useCopyLink();

  const canShowMetrics = status === "ready" && hasData;

  const rawMetrics: Array<readonly [string, string]> = [
    [t("briefingMetricRaw"), fmt.number(totals.raw)],
    [t("briefingMetricSensors"), t("countRawSubline", { modis: totals.modis, viirs: totals.viirs })],
  ];

  const harmonizedMetrics: Array<readonly [string, string]> = [
    [t("briefingMetricRaw"), fmt.number(totals.raw)],
    [t("briefingMetricUnique"), fmt.number(totals.harmonized)],
    [t("briefingMetricDouble"), `${fmt.number(totals.doubleCounts)} (${fmt.percent(totals.doubleCountShare)})`],
  ];

  if (cliff !== null && briefing.rawJumpPct !== null && briefing.harmonizedJumpPct !== null) {
    const rawJumpLabel = t("briefingMetricJumpRaw", {
      from: String(cliff.fromYear),
      to: String(cliff.toYear),
    });
    const harmJumpLabel = t("briefingMetricJumpHarmonized", {
      from: String(cliff.fromYear),
      to: String(cliff.toYear),
    });
    const rawJumpValue = `${fmt.number(cliff.rawFrom)} → ${fmt.number(cliff.rawTo)} (${fmt.signedPercent(briefing.rawJumpPct)})`;
    const harmJumpValue = `${fmt.number(cliff.harmonizedFrom)} → ${fmt.number(cliff.harmonizedTo)} (${fmt.signedPercent(briefing.harmonizedJumpPct)})`;
    rawMetrics.push([rawJumpLabel, rawJumpValue]);
    harmonizedMetrics.push([rawJumpLabel, rawJumpValue]);
    harmonizedMetrics.push([harmJumpLabel, harmJumpValue]);
  }

  if (briefing.peakMonth !== null) {
    harmonizedMetrics.push([
      t("briefingMetricPeak"),
      `${fmt.monthYear(briefing.peakMonth, "long")} · ${fmt.number(briefing.peakMonth.count)}`,
    ]);
  }
  if (cliff !== null && briefing.meanAnnualHarmonized !== null) {
    harmonizedMetrics.push([
      t("briefingMetricMeanAnnual", { year: String(cliff.toYear) }),
      fmt.number(briefing.meanAnnualHarmonized),
    ]);
  }
  if (briefing.frpAngleCorrectionPct !== null) {
    harmonizedMetrics.push([
      t("receiptPeriodAngle"),
      t("receiptAngleValue", { pct: fmt.signedPercent(briefing.frpAngleCorrectionPct) }),
    ]);
  }

  const metrics: ReadonlyArray<readonly [string, string]> = revealAnswer ? harmonizedMetrics : rawMetrics;

  const findingText = !canShowMetrics
    ? status === "error"
      ? formatApiError(error, t)
      : status === "empty"
        ? t("emptyState")
        : t("loadingFirms")
    : revealAnswer
      ? t("briefingFinding", {
          region: region.displayName,
          pct: fmt.percent(totals.doubleCountShare),
          removed: totals.doubleCounts,
          raw: totals.raw,
        })
      : t("briefingRawInvite");

  return (
    <Drawer
      id="briefing"
      title={t("briefingTitle")}
      footer={
        <>
          <button
            type="button"
            className={styles.primaryAction}
            disabled={!canShowMetrics}
            onClick={() => window.print()}
          >
            {t("briefingPrint")}
          </button>
          <button type="button" className={styles.action} onClick={copyLink}>
            {t("briefingCopyLink")}
          </button>
          <button type="button" className={styles.action} disabled={!canDownload} onClick={download}>
            {t("exportCsv")}
          </button>
          <span className={styles.status} role="status">
            {copied ? t("briefingLinkCopied") : ""}
          </span>
        </>
      }
    >
      <div className={styles.findingCard}>
        <p className={styles.finding}>{findingText}</p>
        {status === "error" && (
          <button type="button" className={styles.action} onClick={retry}>
            {t("retryAction")}
          </button>
        )}
      </div>
      {canShowMetrics && (
        <dl className={styles.metrics}>
          {metrics.map(([label, value]) => (
            <div key={label} className={styles.metric}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      )}
      <p className={styles.sourceLine}>
        {dataMode === "demo" ? t("briefingSourceDemo") : t("briefingSourceLive", { source })}
      </p>
    </Drawer>
  );
}
