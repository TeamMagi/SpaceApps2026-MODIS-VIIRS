/**
 * AppShell — page layout only; owns no state and renders whatever is passed into its slots.
 *
 * Desktop (>= 1024 px, reference 1366x768, no page scroll):
 *   header 48 | command row 96 (headline 40% · toggle · count) | pipeline 36 |
 *   main: map 58% left, right column = cliff chart 220 + burn calendar (rest, >= 260) | bottom bar 44.
 *   The 40/18/42 column split makes the toggle column end exactly where the 58% map ends.
 * Mobile (single column, in this order): headline, count, map (clamp 360px-640px / 60dvh),
 *   chart, calendar, pipeline (vertical), bottom bar. The toggle is docked full-width at the
 *   bottom (thumb zone).
 * `overlays` (drawers, toasts) render after the grid.
 */

import { Component, type ErrorInfo, type ReactNode } from "react";
import { useCopy } from "../../hooks/useCopy";
import { useAppStore } from "../../store/useAppStore";
import styles from "./AppShell.module.css";

interface ErrorBoundaryInnerProps {
  resetKey: string;
  errorText: string;
  reloadText: string;
  children: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
}

class ErrorBoundaryInner extends Component<ErrorBoundaryInnerProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { hasError: false };

  static getDerivedStateFromError(): ErrorBoundaryState {
    return { hasError: true };
  }

  componentDidUpdate(prevProps: ErrorBoundaryInnerProps): void {
    if (this.state.hasError && prevProps.resetKey !== this.props.resetKey) {
      this.setState({ hasError: false });
    }
  }

  componentDidCatch(_error: unknown, _info: ErrorInfo): void {
    // Panel errors are isolated so the rest of the shell remains usable.
  }

  handleReload = (): void => {
    this.setState({ hasError: false });
    if (typeof window !== "undefined") {
      window.location.reload();
    }
  };

  render(): ReactNode {
    if (this.state.hasError) {
      return (
        <div className={styles.errorBox} role="alert">
          <p className={styles.errorText}>{this.props.errorText}</p>
          <button type="button" className={styles.reloadButton} onClick={this.handleReload}>
            {this.props.reloadText}
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

export function ErrorBoundary({ children }: { children: ReactNode }) {
  const { t } = useCopy();
  const region = useAppStore((state) => state.region);
  const selectedMonth = useAppStore((state) => state.selectedMonth);
  const resetKey = `${region}:${selectedMonth ? `${selectedMonth.year}-${selectedMonth.month}` : "all"}`;
  return (
    <ErrorBoundaryInner resetKey={resetKey} errorText={t("errorLoading")} reloadText={t("reloadAction")}>
      {children}
    </ErrorBoundaryInner>
  );
}

export interface AppShellProps {
  header: ReactNode;
  headline: ReactNode;
  toggle: ReactNode;
  count: ReactNode;
  pipeline: ReactNode;
  map: ReactNode;
  chart: ReactNode;
  calendar: ReactNode;
  bottomBar: ReactNode;
  overlays?: ReactNode;
}

export default function AppShell({
  header,
  headline,
  toggle,
  count,
  pipeline,
  map,
  chart,
  calendar,
  bottomBar,
  overlays,
}: AppShellProps) {
  return (
    <div className={styles.shell}>
      <header className={styles.header}>{header}</header>
      <main className={styles.content}>
        <div className={styles.headline}>
          <ErrorBoundary>{headline}</ErrorBoundary>
        </div>
        <div className={styles.toggle} data-tour="toggle">
          <ErrorBoundary>{toggle}</ErrorBoundary>
        </div>
        <div className={styles.count}>
          <ErrorBoundary>{count}</ErrorBoundary>
        </div>
        <div className={styles.pipeline}>
          <ErrorBoundary>{pipeline}</ErrorBoundary>
        </div>
        <div className={styles.map} data-tour="map">
          <ErrorBoundary>{map}</ErrorBoundary>
        </div>
        <div className={styles.chart} data-tour="chart">
          <ErrorBoundary>{chart}</ErrorBoundary>
        </div>
        <div className={styles.calendar} data-tour="calendar">
          <ErrorBoundary>{calendar}</ErrorBoundary>
        </div>
      </main>
      <footer className={styles.bottom}>{bottomBar}</footer>
      <ErrorBoundary>{overlays}</ErrorBoundary>
    </div>
  );
}
