import { useCallback, useEffect, useMemo, useRef, type ReactNode } from "react";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import { annualMeanFrp } from "../../lib/annualFrp";
import { busiestMonth, hotMonths, PANEL_IDS, sparklinePath, type PanelId } from "../../lib/dock";
import { setDockMode, useDockMode, type DockMode } from "../../lib/dockMode";
import { setPanel, usePanel } from "../../lib/dockPanel";
import { resolveActiveYear } from "../../lib/radialCalendar";
import { useAppStore } from "../../store/useAppStore";
import DetailedDock from "./DetailedDock";
import CalendarBars from "./charts/CalendarBars";
import DetectionsChart from "./charts/DetectionsChart";
import FrpChart from "./charts/FrpChart";
import styles from "./InsightDock.module.css";

const SPARK_W = 84;
const SPARK_H = 30;
const SPARK_PAD = 2;

function Sparkline({ values, color }: { values: readonly (number | null)[]; color: string }) {
  const d = sparklinePath(values, SPARK_W - 2 * SPARK_PAD, SPARK_H - 2 * SPARK_PAD);
  return (
    <svg width={SPARK_W} height={SPARK_H} viewBox={`0 0 ${SPARK_W} ${SPARK_H}`} aria-hidden="true">
      {d && (
        <path
          d={d}
          transform={`translate(${SPARK_PAD} ${SPARK_PAD})`}
          fill="none"
          stroke={color}
          strokeWidth="1.8"
          strokeLinejoin="round"
          strokeLinecap="round"
        />
      )}
    </svg>
  );
}

const BODIES: Record<PanelId, ReactNode> = {
  detections: <DetectionsChart />,
  frp: <FrpChart />,
  calendar: <CalendarBars />,
};

/**
 * Three summary cards over the map that expand into one panel. Hidden until the viewer has
 * picked a country; picking another country (or going back to the world) closes the panel.
 */
export default function InsightDock() {
  const { t, fmt } = useCopy();
  const regionPicked = useAppStore((s) => (s as { regionPicked?: boolean }).regionPicked ?? false);
  const region = useAppStore((s) => s.region);
  const expanded = usePanel();
  const mode = useDockMode();
  const { yearly, months, cliff, status, firstYear, lastYear, defaultMapMonth } = useRegionData();
  const geometryHarmonized = useHarmonizeTransition((v) => v.geometryHarmonized);
  const selectedMonth = useAppStore((s) => s.selectedMonth);
  const calendarYear = useAppStore((s) => s.calendarYear);
  const cardRefs = useRef<Partial<Record<PanelId, HTMLButtonElement | null>>>({});
  const closeRef = useRef<HTMLButtonElement | null>(null);
  const lastExpanded = useRef<PanelId | null>(null);
  const lastRegion = useRef(region);

  const collapse = useCallback(() => setPanel(null), []);

  useEffect(() => {
    if (lastRegion.current !== region) {
      lastRegion.current = region;
      setPanel(null);
    }
  }, [region]);

  useEffect(() => {
    if (!regionPicked) setPanel(null);
  }, [regionPicked]);

  useEffect(() => {
    if (expanded) {
      closeRef.current?.focus();
    } else if (lastExpanded.current) {
      cardRefs.current[lastExpanded.current]?.focus();
    }
    lastExpanded.current = expanded;
  }, [expanded]);

  useEffect(() => {
    if (!expanded) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") collapse();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [expanded, collapse]);

  const year = resolveActiveYear({ viewYear: calendarYear, selectedMonth, defaultMapMonth, firstYear, lastYear });

  const summaries = useMemo(() => {
    const counts = yearly.map((p) => (geometryHarmonized ? p.harmonized : p.raw));
    const ratio = cliff ? (geometryHarmonized ? cliff.harmonizedChangeRatio : cliff.rawChangeRatio) : null;
    const jump =
      cliff && ratio !== null
        ? t(ratio >= 0 ? "dockJump" : "dockDrop", { x: fmt.number(Math.abs(ratio) * 100), year: String(cliff.year) })
        : t("dockNoData");

    const frp = annualMeanFrp(months);
    const frpValues = frp.map((p) => p.mean);
    const lastFrp = [...frp].reverse().find((p) => p.mean !== null);
    const frpText = lastFrp
      ? t("dockFrpValue", { value: fmt.number(lastFrp.mean ?? 0, 1), year: String(lastFrp.year) })
      : t("dockNoData");

    const peak = year === null ? null : busiestMonth(months, year, geometryHarmonized);
    const peakText = peak === null ? t("dockNoData") : t("dockBusiest", { month: fmt.monthNameInSentence(peak) });
    const hot = year === null ? new Array<boolean>(12).fill(false) : hotMonths(months, year, geometryHarmonized);
    return { counts, jump, frpValues, frpText, peakText, hot };
  }, [yearly, months, cliff, geometryHarmonized, year, t, fmt]);

  if (!regionPicked || status === "loading" || status === "error") return null;

  const yearText = year === null ? "" : String(year);
  const titles: Record<PanelId, string> = {
    detections: t("dockDetectionsTitle"),
    frp: t("dockFrpTitle"),
    calendar: t("dockCalendarTitle", { year: yearText }),
  };
  const tabs: Record<PanelId, string> = {
    detections: t("dockTabDetections"),
    frp: t("dockTabFrp"),
    calendar: t("dockTabCalendar"),
  };
  const sentences: Record<PanelId, string> = {
    detections: summaries.jump,
    frp: summaries.frpText,
    calendar: summaries.peakText,
  };

  const visual = (id: PanelId): ReactNode => {
    if (id === "detections") return <Sparkline values={summaries.counts} color="#f37819" />;
    if (id === "frp") return <Sparkline values={summaries.frpValues} color="#fcb519" />;
    return (
      <span className={styles.strip} aria-hidden="true">
        {summaries.hot.map((hot, i) => (
          <span key={i} className={styles.stripBar} data-hot={hot ? "true" : undefined} />
        ))}
      </span>
    );
  };

  const chooseMode = (next: DockMode) => {
    setPanel(null);
    setDockMode(next);
  };

  return (
    <div className={styles.dock} data-mode={mode} role="region" aria-label={t("dockLabel")}>
      <div className={styles.modeSwitch} role="group" aria-label={t("dockModeLabel")}>
        {(["simple", "detailed"] as const).map((m) => (
          <button
            key={m}
            type="button"
            className={styles.modeButton}
            aria-pressed={mode === m}
            onClick={() => chooseMode(m)}
          >
            {t(m === "simple" ? "dockModeSimple" : "dockModeDetailed")}
          </button>
        ))}
      </div>
      {mode === "detailed" ? (
        <DetailedDock />
      ) : expanded === null ? (
        <div className={styles.stack}>
          {PANEL_IDS.map((id) => (
            <button
              key={id}
              ref={(el) => {
                cardRefs.current[id] = el;
              }}
              type="button"
              className={styles.card}
              aria-expanded={false}
              data-tour={id === "calendar" ? "calendar" : "chart"}
              onClick={() => setPanel(id)}
            >
              <span className={styles.cardHead}>
                <span className={styles.cardTitle}>{titles[id]}</span>
                <span className={styles.cardOpen}>{t("dockOpen")}</span>
              </span>
              <span className={styles.cardBody}>
                <span className={styles.cardText}>{sentences[id]}</span>
                <span className={styles.cardVisual}>{visual(id)}</span>
              </span>
            </button>
          ))}
        </div>
      ) : (
        <div
          className={styles.panel}
          role="dialog"
          aria-label={titles[expanded]}
          data-tour={expanded === "calendar" ? "calendar" : "chart"}
        >
          <div className={styles.tabs} role="tablist">
            {PANEL_IDS.map((id) => (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={id === expanded}
                className={styles.tab}
                onClick={() => setPanel(id)}
              >
                {tabs[id]}
              </button>
            ))}
            <span className={styles.spacer} />
            <button ref={closeRef} type="button" className={styles.close} aria-label={t("dockClose")} onClick={collapse}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
                <path d="M6 6l12 12M18 6L6 18" />
              </svg>
            </button>
          </div>
          <h2 className={styles.panelTitle}>{titles[expanded]}</h2>
          <div className={styles.panelBody} role="tabpanel">
            {BODIES[expanded]}
          </div>
        </div>
      )}
    </div>
  );
}
