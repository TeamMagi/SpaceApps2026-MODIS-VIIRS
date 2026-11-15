/**
 * Header — top bar, 48 px on desktop.
 *
 * Left to right: wordmark (appName; not the page heading, the Headline owns <h1>), RegionPicker,
 * ModeBadge, Methods button (navMethods, opens the "methods" drawer via store.showDrawer), LangSwitch.
 * Mobile: wordmark + badge + Methods + LangSwitch on the first line, RegionPicker full width below.
 * Neutral greys only; no accent colour here (Von Restorff).
 */

import { useCopy } from "../../hooks/useCopy";
import { useAppStore } from "../../store/useAppStore";
import LangSwitch from "./LangSwitch";
import ModeBadge from "./ModeBadge";
import RegionPicker from "./RegionPicker";
import styles from "./Header.module.css";

export default function Header() {
  const { t } = useCopy();
  const methodsOpen = useAppStore((state) => state.openDrawer === "methods");
  const settingsOpen = useAppStore((state) => state.openDrawer === "settings");
  const showDrawer = useAppStore((state) => state.showDrawer);

  return (
    <div className={styles.header}>
      <span className={styles.wordmark}>
        <span className={styles.logo} aria-hidden="true" />
        {t("appName")}
      </span>
      <div className={styles.regions}>
        <RegionPicker />
      </div>
      <div className={styles.tools}>
        <ModeBadge />
        <button
          type="button"
          className={styles.methods}
          aria-haspopup="dialog"
          aria-expanded={methodsOpen}
          onClick={() => showDrawer("methods")}
        >
          {t("navMethods")}
        </button>
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
