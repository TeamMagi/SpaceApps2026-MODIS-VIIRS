/**
 * AppShell — page layout only; owns no state and renders whatever is passed into its slots.
 *
 * Desktop (>= 1024 px, no page scroll): header | compact command row (headline · toggle · count) |
 * [drift note, only while the Orbital Drift strip shows] | full-bleed map | slim bottom bar. The insight dock floats over the map.
 * World view (`worldView`): the command row and bottom bar collapse to zero height and become inert,
 * leaving header + map. Both stay mounted. Entering a country opens the command row with a CSS intro.
 * Mobile (single column): header, stacked command row, map (>= 60dvh in region view), bottom bar.
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
  /** Optional slim note between the command row and the map; renders nothing (and takes no space) when empty. */
  drift?: ReactNode;
  map: ReactNode;
  bottomBar: ReactNode;
  overlays?: ReactNode;
  worldView: boolean;
}

export default function AppShell({
  header,
  headline,
  toggle,
  count,
  drift,
  map,
  bottomBar,
  overlays,
  worldView,
}: AppShellProps) {
  return (
    <div className={styles.shell} data-view={worldView ? "world" : "region"}>
      <header className={styles.header}>{header}</header>
      <main className={styles.content}>
        <div className={styles.command} inert={worldView} aria-hidden={worldView}>
          <div className={styles.collapseInner}>
            <div className={styles.commandRow}>
              <div className={styles.headline}>
                <ErrorBoundary>{headline}</ErrorBoundary>
              </div>
              <div className={styles.toggle} data-tour="toggle">
                <ErrorBoundary>{toggle}</ErrorBoundary>
              </div>
              <div className={styles.count}>
                <ErrorBoundary>{count}</ErrorBoundary>
              </div>
            </div>
          </div>
        </div>
        <div className={styles.drift} inert={worldView}>
          <ErrorBoundary>{drift}</ErrorBoundary>
        </div>
        <div className={styles.map} data-tour="map">
          <ErrorBoundary>{map}</ErrorBoundary>
        </div>
      </main>
      <footer className={styles.bottom} inert={worldView} aria-hidden={worldView}>
        <div className={`${styles.collapseInner} ${styles.bottomInner}`}>{bottomBar}</div>
      </footer>
      <ErrorBoundary>{overlays}</ErrorBoundary>
    </div>
  );
}
