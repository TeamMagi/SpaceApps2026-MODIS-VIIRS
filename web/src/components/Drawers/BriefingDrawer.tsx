/**
 * BriefingDrawer — regional summary derived from the real archive series and points.
 *
 * Harmonized (revealAnswer): the finding (briefingFinding), the full metric list and, for Türkiye,
 *   the 2021 Mediterranean fires case study (S7).
 * Raw: only what the satellites reported (raw total, MODIS/VIIRS split, raw cliff change, raw
 *   last-7-days count, most active sensor) and an invitation to harmonize; the answer is never
 *   shown before the user asks for it with the toggle.
 */

import { formatApiError } from "../../content/copy";
import { useCalendarFrp } from "../../hooks/useCalendarFrp";
import { useCaseStudy } from "../../hooks/useCaseStudy";
import { useCopy } from "../../hooks/useCopy";
import { useCopyLink } from "../../hooks/useCopyLink";
import { useExportView } from "../../hooks/useExportView";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useMapPoints, useRegionData, useRegions } from "../../hooks/useRegionData";
import {
  historicalDeviation,
  platformLabel,
  lastSevenDays,
  mostActiveSensor,
  sensorCounts,
  topPlatform,
  yearlyTrend,
} from "../../lib/briefingMetrics";
import { EXPORT_FORMATS } from "../../lib/export";
import { formatDayRange, formatSigma, formatYearSpan } from "../../lib/format";
import { useAppStore } from "../../store/useAppStore";
import Drawer from "./Drawer";
import styles from "./BriefingDrawer.module.css";

const CASE_STUDY_REGION = "Turkey";

export default function BriefingDrawer() {
  const { t, fmt, lang } = useCopy();
  const dataMode = useAppStore((state) => state.dataMode);
  const { source } = useRegions();
  const { status, hasData, error, retry, region, totals, cliff, briefing, yearly, months } = useRegionData();
  const points = useMapPoints();
  const calendar = useCalendarFrp(region.id);
  const revealAnswer = useHarmonizeTransition((v) => v.revealAnswer);
  const { fileNames, statuses, canDownload, failed, download } = useExportView();
  const { copied, copyLink } = useCopyLink();

  const canShowMetrics = status === "ready" && hasData;
  const isCaseStudyRegion = region.id === CASE_STUDY_REGION;
  const caseStudy = useCaseStudy(revealAnswer && canShowMetrics && isCaseStudyRegion);

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

  const last7Label = (window: { start: string; end: string }) =>
    t("briefingMetricLast7", { range: formatDayRange(lang, window.start, window.end) });
  const last7Raw = lastSevenDays(points, false);
  const last7Harmonized = lastSevenDays(points, true);
  const counts = sensorCounts(points, totals);
  const activity = mostActiveSensor(counts);
  const sensorScope =
    counts.period === null || counts.period.year === null
      ? t("briefingScopeAllYears")
      : counts.period.month === null
        ? String(counts.period.year)
        : fmt.monthYear({ year: counts.period.year, month: counts.period.month });
  const sensorLabel = t("briefingMetricSensor", { scope: sensorScope });
  const sensorBase = activity ? { sensor: activity.sensor, share: fmt.percent(activity.share, 0) } : null;

  if (last7Raw) rawMetrics.push([last7Label(last7Raw), fmt.number(last7Raw.count)]);
  if (sensorBase) rawMetrics.push([sensorLabel, t("briefingSensorValue", sensorBase)]);

  if (last7Harmonized) harmonizedMetrics.push([last7Label(last7Harmonized), fmt.number(last7Harmonized.count)]);
  const deviation = historicalDeviation(points.period, months, yearly);
  if (deviation) {
    const period =
      deviation.period.month === null
        ? String(deviation.period.year)
        : fmt.monthYear({ year: deviation.period.year, month: deviation.period.month }, "long");
    harmonizedMetrics.push([
      t("briefingMetricDeviation", { period }),
      t("briefingDeviationValue", { pct: fmt.signedPercent(deviation.ratio), mean: fmt.number(deviation.mean) }),
    ]);
  }
  if (sensorBase) {
    const platform = topPlatform(points);
    harmonizedMetrics.push([
      sensorLabel,
      platform
        ? t("briefingSensorValuePlatform", { ...sensorBase, platform: platformLabel(platform) })
        : t("briefingSensorValue", sensorBase),
    ]);
  }
  const trend = yearlyTrend(yearly, calendar);
  if (trend) {
    const labelVars = { from: String(trend.fromYear), to: String(trend.toYear) };
    const pct = fmt.signedPercent(trend.perYear);
    harmonizedMetrics.push([
      trend.normalized ? t("briefingMetricTrendNormalized", labelVars) : t("briefingMetricTrend", labelVars),
      trend.direction === "flat"
        ? t("briefingTrendFlat")
        : trend.direction === "rising"
          ? t("briefingTrendRising", { pct })
          : t("briefingTrendFalling", { pct }),
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
          {EXPORT_FORMATS.map((format) => {
            const loading = statuses[format] === "loading";
            return (
              <button
                key={format}
                type="button"
                className={styles.action}
                disabled={!canDownload || loading}
                aria-busy={loading}
                title={fileNames[format]}
                onClick={() => download(format)}
              >
                {loading ? t("exportPreparing") : format === "csv" ? t("exportCsv") : t("exportGeojson")}
              </button>
            );
          })}
          <span className={styles.status} role="status">
            {copied ? t("briefingLinkCopied") : failed ? t("exportFailed") : ""}
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
      {canShowMetrics && isCaseStudyRegion && (
        <section className={styles.caseStudy} aria-labelledby="case-study-title">
          <h3 id="case-study-title" className={styles.caseStudyTitle}>
            {t("caseStudyTitle")}
          </h3>
          {!revealAnswer && <p className={styles.caseStudyNote}>{t("caseStudyRawInvite")}</p>}
          {revealAnswer && caseStudy && (
            <>
              <dl className={styles.metrics}>
                {(
                  [
                    ["caseStudyAntalya", caseStudy.antalyaAugust],
                    ["caseStudyMugla", caseStudy.muglaAugust],
                  ] as const
                ).map(([labelKey, row]) => (
                  <div key={labelKey} className={styles.metric}>
                    <dt>{t(labelKey)}</dt>
                    <dd>
                      {t("caseStudyRowValue", {
                        count: row.count,
                        z: formatSigma(lang, row.anomalyZ),
                        rank: row.rankAllMonths,
                        total: row.ofAllMonths,
                      })}
                    </dd>
                  </div>
                ))}
                <div className={styles.metric}>
                  <dt>{t("caseStudyMediterranean")}</dt>
                  <dd>
                    {t("caseStudyMediterraneanValue", {
                      july: formatSigma(lang, caseStudy.mediterraneanJulyZ),
                      august: formatSigma(lang, caseStudy.mediterraneanAugustZ),
                    })}
                  </dd>
                </div>
              </dl>
              <p className={styles.caseStudyNote}>
                {t("caseStudySource", { years: caseStudy.years ? formatYearSpan(caseStudy.years) : "—" })}
              </p>
            </>
          )}
        </section>
      )}
      <p className={styles.sourceLine}>
        {dataMode === "demo" ? t("briefingSourceDemo") : t("briefingSourceLive", { source })}
      </p>
    </Drawer>
  );
}
