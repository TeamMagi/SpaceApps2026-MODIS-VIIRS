import { useEffect } from "react";
import { t } from "../content/copy";
import { storePalette, storeTheme, useAppStore } from "../store/useAppStore";

/**
 * Mirrors app-level state onto <html>: `lang`, `data-theme`, `data-palette`, `data-presentation`,
 * and the page title. Mount once (App does).
 */
export function useDocumentSync(): void {
  const lang = useAppStore((state) => state.lang);
  const theme = useAppStore((state) => state.theme);
  const palette = useAppStore((state) => state.palette);
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
    if (palette === "default") {
      delete document.documentElement.dataset.palette;
    } else {
      document.documentElement.dataset.palette = palette;
    }
    storePalette(palette);
  }, [palette]);

  useEffect(() => {
    if (presentation) document.documentElement.dataset.presentation = "true";
    else delete document.documentElement.dataset.presentation;
  }, [presentation]);
}
