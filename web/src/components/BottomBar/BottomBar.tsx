/**
 * BottomBar — 44 px strip; where the Z-scan ends.
 *
 * Left: the single honest limit — disclaimer (never repeated next to numbers).
 * Right: secondary actions (compact CSV download with filename in title/aria-description,
 *   copy link) and primary briefMe action (opens "briefing").
 * Mobile: stacked, actions full width, above the docked toggle. Neutral styling; the briefing
 * button is the single primary action and never uses the accent.
 */

import { useCopy } from "../../hooks/useCopy";
import { useCopyLink } from "../../hooks/useCopyLink";
import { useExportView } from "../../hooks/useExportView";
import { useAppStore } from "../../store/useAppStore";
import styles from "./BottomBar.module.css";

export default function BottomBar() {
  const { t } = useCopy();
  const showDrawer = useAppStore((state) => state.showDrawer);
  const briefingOpen = useAppStore((state) => state.openDrawer === "briefing");
  const { fileName, canDownload, download } = useExportView();
  const { copied, copyLink } = useCopyLink();
  const csvDescription = t("exportCsvDescription", { file: fileName });

  return (
    <div className={styles.bar}>
      <p className={styles.disclaimer}>{t("disclaimer")}</p>
      <div className={styles.actions}>
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
