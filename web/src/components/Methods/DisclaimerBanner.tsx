/**
 * DisclaimerBanner — the methods page's honest-limit notice, right under the top bar.
 * Expanded: title + full note. Collapsed: the one-line disclaimer; the choice persists in localStorage.
 * Neutral surface and a quiet left rule, no accent and no alarm styling. Print always shows the full note.
 */

import { useId, useState } from "react";
import { useCopy } from "../../hooks/useCopy";
import styles from "./DisclaimerBanner.module.css";

const STORAGE_KEY = "pyrosync.methodsDisclaimerCollapsed";

function readCollapsed(): boolean {
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

function storeCollapsed(collapsed: boolean): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, collapsed ? "1" : "0");
  } catch {
    // Storage can be unavailable (private mode); the choice then lasts for this visit only.
  }
}

export default function DisclaimerBanner() {
  const { t } = useCopy();
  const bodyId = useId();
  const [collapsed, setCollapsed] = useState(readCollapsed);

  const toggle = () => {
    storeCollapsed(!collapsed);
    setCollapsed(!collapsed);
  };

  return (
    <aside className={styles.banner} data-collapsed={collapsed} aria-label={t("methodsDisclaimerTitle")}>
      <div className={styles.text}>
        {collapsed && <p className={styles.line}>{t("disclaimer")}</p>}
        <div id={bodyId} className={styles.full} hidden={collapsed}>
          <p className={styles.title}>{t("methodsDisclaimerTitle")}</p>
          <p className={styles.body}>{t("methodsDisclaimerBody")}</p>
        </div>
      </div>
      <button type="button" className={styles.toggle} aria-expanded={!collapsed} aria-controls={bodyId} onClick={toggle}>
        {collapsed ? t("methodsDisclaimerShow") : t("methodsDisclaimerHide")}
      </button>
    </aside>
  );
}
