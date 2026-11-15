/**
 * BottomBar — 44 px strip; where the Z-scan ends.
 *
 * Left: the single honest limit — disclaimer (never repeated next to numbers).
 * Right: secondary actions (low-confidence filter switch, compact CSV and GeoJSON downloads with filename in
 *   title/aria-description, copy link) and primary briefMe action (opens "briefing").
 * The filter switch lives here rather than in a drawer so its state stays visible next to every
 *   number it changes; while it is on, the strip also says how many detections it removed.
 * Mobile: stacked, actions full width, above the docked toggle. Neutral styling; the briefing
 * button is the single primary action and never uses the accent.
 */

import { useCopy } from "../../hooks/useCopy";
import { useValidationGate } from "../../hooks/useValidationGate";
import { useCopyLink } from "../../hooks/useCopyLink";
import { useExportView } from "../../hooks/useExportView";
import { EXPORT_FORMATS } from "../../lib/export";
import { useMapPoints } from "../../hooks/useRegionData";
import MethodsLink from "../Methods/MethodsLink";
import { useAppStore } from "../../store/useAppStore";
import styles from "./BottomBar.module.css";

export default function BottomBar() {
  const { t, fmt } = useCopy();
  const gate = useValidationGate();
  const gateText = gate?.decision === "pass"
    ? t("validationGatePass", { error: fmt.number(gate.modelError, 4) })
    : gate?.decision === "inconclusive"
      ? t("validationGateInconclusive")
      : gate?.decision === "fail"
        ? t("validationGateFail")
        : t("validationGateUnavailable");
  const showDrawer = useAppStore((state) => state.showDrawer);
  const briefingOpen = useAppStore((state) => state.openDrawer === "briefing");
  const { fileNames, statuses, canDownload, failed, download } = useExportView();
  const { copied, copyLink } = useCopyLink();
  const minConfidence = useAppStore((state) => state.minConfidence);
  const setMinConfidence = useAppStore((state) => state.setMinConfidence);
  const startTour = useAppStore((state) => state.startTour);
  const tourDescription = t("tourStartDescription");
  const points = useMapPoints();
  const filterOn = minConfidence !== "low";
  const filterDescription = t("confidenceFilterDescription");
  const excludedHere =
    filterOn && points.hasData && points.filters.minConfidence === minConfidence
      ? points.filters.excludedDetections
      : null;

  return (
    <div className={styles.bar}>
      <p className={styles.disclaimer}>
        {t("disclaimer")}{" "}
        <MethodsLink section="limits" className={styles.disclaimerLink}>
          {t("disclaimerLink")}
        </MethodsLink>
        {" "}
        <span className={styles.gate} data-decision={gate?.decision} role="status" aria-live="polite" aria-atomic="true">
          {gateText}
        </span>
      </p>
      <div className={styles.actions}>
        <button
          type="button"
          className={styles.action}
          aria-pressed={filterOn}
          title={filterDescription}
          aria-description={filterDescription}
          onClick={() => setMinConfidence(filterOn ? "low" : "nominal")}
        >
          {filterOn ? t("confidenceFilterOn") : t("confidenceFilterOff")}
        </button>
        {excludedHere !== null && (
          <span className={styles.filterNote} role="status">
            {t("confidenceFilterExcluded", { count: excludedHere })}
          </span>
        )}
        {EXPORT_FORMATS.map((format) => {
          const description = t("exportCsvDescription", { file: fileNames[format] });
          const loading = statuses[format] === "loading";
          return (
            <button
              key={format}
              type="button"
              className={styles.action}
              disabled={!canDownload || loading}
              aria-busy={loading}
              aria-label={format === "csv" ? t("exportCsv") : t("exportGeojson")}
              title={description}
              aria-description={description}
              onClick={() => download(format)}
            >
              {loading ? t("exportPreparing") : format === "csv" ? t("exportCsvShort") : t("exportGeojsonShort")}
            </button>
          );
        })}
        {failed && (
          <span className={styles.filterNote} role="alert">
            {t("exportFailed")}
          </span>
        )}
        <button type="button" className={styles.action} data-copied={copied} onClick={copyLink}>
          {copied ? t("briefingLinkCopied") : t("briefingCopyLink")}
        </button>
        <span className="sr-only" role="status">
          {copied ? t("briefingLinkCopied") : ""}
        </span>
        <button
          type="button"
          className={styles.action}
          title={tourDescription}
          aria-description={tourDescription}
          onClick={startTour}
        >
          {t("tourStart")}
        </button>
        <button
          type="button"
          className={styles.primary}
          aria-haspopup="dialog"
          aria-expanded={briefingOpen}
          onClick={() => showDrawer("briefing")}
        >
          {t("briefMe")}
        </button>
      </div>
    </div>
  );
}
