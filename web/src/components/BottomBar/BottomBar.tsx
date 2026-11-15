/**
 * BottomBar — 36 px strip; where the Z-scan ends. Shown only while a country is selected;
 *   hidden in the opening world view.
 *
 * Left: the single honest limit — disclaimer (never repeated next to numbers).
 * Right: CSV and GeoJSON downloads, secondary actions, and the primary briefing action.
 * Mobile: stacked, actions full width, above the docked toggle. Neutral styling; the briefing
 * button is the single primary action and never uses the accent.
 */

import { useCopy } from "../../hooks/useCopy";
import { useValidationGate } from "../../hooks/useValidationGate";
import { useCopyLink } from "../../hooks/useCopyLink";
import { useExportView } from "../../hooks/useExportView";
import { EXPORT_FORMATS } from "../../lib/export";
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
  const startTour = useAppStore((state) => state.startTour);
  const tourDescription = t("tourStartDescription");

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
        {failed && <span className={styles.filterNote} role="alert">{t("exportFailed")}</span>}
        <details className={styles.more}>
          <summary className={styles.action}>{t("bottomBarMore")}</summary>
          <div className={styles.moreMenu}>
            <button type="button" className={styles.menuAction} data-copied={copied} onClick={copyLink}>
              {copied ? t("briefingLinkCopied") : t("briefingCopyLink")}
            </button>
            <button
              type="button"
              className={styles.menuAction}
              title={tourDescription}
              aria-description={tourDescription}
              onClick={startTour}
            >
              {t("tourStart")}
            </button>
            {failed && <span className={styles.filterNote} role="alert">{t("exportFailed")}</span>}
            <span className="sr-only" role="status">
              {copied ? t("briefingLinkCopied") : ""}
            </span>
          </div>
        </details>
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
