/**
 * BottomBar — 44 px strip; where the Z-scan ends.
 *
 * Left: the single honest limit — disclaimer (never repeated next to numbers).
 * Right: secondary actions (low-confidence filter switch, compact CSV download with filename in
 *   title/aria-description, copy link) and primary briefMe action (opens "briefing").
 * The filter switch lives here rather than in a drawer so its state stays visible next to every
 *   number it changes; while it is on, the strip also says how many detections it removed.
 * Mobile: stacked, actions full width, above the docked toggle. Neutral styling; the briefing
 * button is the single primary action and never uses the accent.
 */

import { useCopy } from "../../hooks/useCopy";
import { useCopyLink } from "../../hooks/useCopyLink";
import { useExportView } from "../../hooks/useExportView";
import { useMapPoints } from "../../hooks/useRegionData";
import MethodsLink from "../Methods/MethodsLink";
import { useAppStore } from "../../store/useAppStore";
import styles from "./BottomBar.module.css";

export default function BottomBar() {
  const { t } = useCopy();
  const showDrawer = useAppStore((state) => state.showDrawer);
  const briefingOpen = useAppStore((state) => state.openDrawer === "briefing");
  const { fileName, canDownload, download } = useExportView();
  const { copied, copyLink } = useCopyLink();
  const csvDescription = t("exportCsvDescription", { file: fileName });
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
        <button
          type="button"
          className={styles.action}
          disabled={!canDownload}
          title={csvDescription}
          aria-description={csvDescription}
          onClick={download}
        >
          {t("exportCsvShort")}
        </button>
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
