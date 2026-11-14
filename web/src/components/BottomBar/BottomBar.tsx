/**
 * BottomBar — 44 px strip; where the Z-scan ends.
 *
 * Left: the single honest limit — disclaimer + disclaimerLink (opens the "methods" drawer).
 *   Said once here, never repeated next to numbers.
 * Right: exportCsv button (useExportView: downloads the map's current points; the file name is
 *   shown up front in mono, e.g. pyrosync_turkiye_2020-07.csv) and briefMe (opens "briefing").
 * Mobile: stacked, actions full width, above the docked toggle. Neutral styling; the briefing
 * button may be the stronger of the two, but never uses the accent.
 */

import { useCopy } from "../../hooks/useCopy";
import { useCopyLink } from "../../hooks/useCopyLink";
import { useExportView } from "../../hooks/useExportView";
import { useAppStore } from "../../store/useAppStore";
import styles from "./BottomBar.module.css";

export default function BottomBar() {
  const { t } = useCopy();
  const showDrawer = useAppStore((state) => state.showDrawer);
  const methodsOpen = useAppStore((state) => state.openDrawer === "methods");
  const briefingOpen = useAppStore((state) => state.openDrawer === "briefing");
  const { fileName, canDownload, download } = useExportView();
  const { copied, copyLink } = useCopyLink();

  return (
    <div className={styles.bar}>
      <p className={styles.disclaimer}>
        {t("disclaimer")}{" "}
        <button
          type="button"
          className={styles.link}
          aria-haspopup="dialog"
          aria-expanded={methodsOpen}
          onClick={() => showDrawer("methods")}
        >
          {t("disclaimerLink")}
        </button>
      </p>
      <div className={styles.actions}>
        <button type="button" className={styles.action} disabled={!canDownload} onClick={download}>
          {t("exportCsv")}
          <span className={styles.file}>{fileName}</span>
        </button>
        <button type="button" className={styles.action} data-copied={copied} onClick={copyLink}>
          {copied ? t("briefingLinkCopied") : t("briefingCopyLink")}
        </button>
        <span className="sr-only" role="status">
          {copied ? t("briefingLinkCopied") : ""}
        </span>
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
