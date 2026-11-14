/**
 * LangSwitch — EN | TR segmented control.
 *
 * Two buttons with aria-pressed inside a group labelled langSwitchLabel; each button's accessible
 * name is the language's own name (langEnName / langTrName) with a matching `lang` attribute.
 * Calls store.setLang, which snaps any running transition to its final state.
 */

import { useCopy } from "../../hooks/useCopy";
import { useAppStore } from "../../store/useAppStore";
import type { Lang } from "../../types";
import styles from "./LangSwitch.module.css";

const LANGS: readonly Lang[] = ["en", "tr"];

export default function LangSwitch() {
  const { t } = useCopy();
  const lang = useAppStore((state) => state.lang);
  const setLang = useAppStore((state) => state.setLang);

  return (
    <div className={styles.switch} role="group" aria-label={t("langSwitchLabel")}>
      {LANGS.map((l) => (
        <button
          key={l}
          type="button"
          lang={l}
          className={styles.option}
          aria-pressed={l === lang}
          aria-label={l === "en" ? t("langEnName") : t("langTrName")}
          onClick={() => setLang(l)}
        >
          {l === "en" ? t("langEn") : t("langTr")}
        </button>
      ))}
    </div>
  );
}
