/**
 * Header — top bar, 60 px on desktop.
 *
 * Left to right: wordmark (appName; not the page heading, the Headline owns <h1>), RegionPicker,
 * ModeBadge, Methods link (navMethods, anchor to the #/methods page), LangSwitch.
 * Mobile: wordmark + badge + Methods + LangSwitch on the first line, RegionPicker full width below.
 * Neutral greys plus the fire-orange flame mark.
 */

import { useCopy } from "../../hooks/useCopy";
import { useAppStore } from "../../store/useAppStore";
import MethodsLink from "../Methods/MethodsLink";
import LangSwitch from "./LangSwitch";
import ModeBadge from "./ModeBadge";
import RegionPicker from "./RegionPicker";
import styles from "./Header.module.css";

export default function Header() {
  const { t } = useCopy();
  const settingsOpen = useAppStore((state) => state.openDrawer === "settings");
  const showDrawer = useAppStore((state) => state.showDrawer);

  return (
    <div className={styles.header}>
      <span className={styles.wordmark}>
        <svg className={styles.logo} width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M12 3c1 3.5 5 5.5 5 10a5 5 0 0 1-10 0c0-2 1-3.5 2-4.5 0 2 1 3 2 3 0-3-1-5.5 1-8.5z" />
        </svg>
        {t("appName")}
      </span>
      <div className={styles.regions}>
        <RegionPicker />
      </div>
      <div className={styles.tools}>
        <ModeBadge />
        <MethodsLink className={styles.methods}>{t("navMethods")}</MethodsLink>
        <button
          type="button"
          className={styles.methods}
          aria-haspopup="dialog"
          aria-expanded={settingsOpen}
          onClick={() => showDrawer("settings")}
        >
          {t("navSettings")}
        </button>
        <LangSwitch />
      </div>
    </div>
  );
}
