import { Component, type ErrorInfo, type ReactNode } from "react";
import { t } from "../../content/copy";
import { readUrlState } from "../../lib/urlState";
import type { Lang } from "../../types";
import styles from "./ErrorBoundary.module.css";

export interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
}

function resolveFallbackLang(): Lang {
  try {
    const urlLang = readUrlState().lang;
    if (urlLang === "en" || urlLang === "tr") return urlLang;
  } catch {
    // URL parsing can fail if location is unavailable; fall back to document/English.
  }
  if (typeof document !== "undefined" && document.documentElement.lang === "tr") {
    return "tr";
  }
  return "en";
}

export default class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { hasError: false };

  static getDerivedStateFromError(): ErrorBoundaryState {
    return { hasError: true };
  }

  componentDidCatch(_error: unknown, _info: ErrorInfo): void {
    // Error boundary prevents a white screen on unhandled render exceptions.
  }

  private handleReload = (): void => {
    if (typeof window !== "undefined") {
      window.location.reload();
    }
  };

  render(): ReactNode {
    if (!this.state.hasError) {
      return this.props.children;
    }

    const lang = resolveFallbackLang();

    return (
      <main className={styles.page} role="alert">
        <section className={styles.card}>
          <h1 className={styles.title}>{t(lang, "stateErrorBoundaryTitle")}</h1>
          <p className={styles.body}>{t(lang, "stateErrorBoundaryBody")}</p>
          <button type="button" className={styles.reload} onClick={this.handleReload}>
            {t(lang, "reloadAction")}
          </button>
        </section>
      </main>
    );
  }
}
