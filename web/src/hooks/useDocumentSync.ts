import { useEffect } from "react";
import { t } from "../content/copy";
import { storeTheme, useAppStore } from "../store/useAppStore";

/**
 * Mirrors app-level state onto <html>: `lang`, `data-theme`, `data-presentation`, and the page
 * title. Mount once (App does).
 */
export function useDocumentSync(): void {
  const lang = useAppStore((state) => state.lang);
  const theme = useAppStore((state) => state.theme);
  const presentation = useAppStore((state) => state.presentation);

  useEffect(() => {
    document.documentElement.lang = lang;
    document.title = `${t(lang, "appName")} · ${t(lang, "appTagline")}`;
  }, [lang]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    storeTheme(theme);
  }, [theme]);

  useEffect(() => {
    if (presentation) document.documentElement.dataset.presentation = "true";
    else delete document.documentElement.dataset.presentation;
  }, [presentation]);
}
