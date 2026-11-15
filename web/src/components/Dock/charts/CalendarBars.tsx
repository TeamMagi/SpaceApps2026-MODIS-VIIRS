import { useMemo, type KeyboardEvent } from "react";
import { useCopy } from "../../../hooks/useCopy";
import { useHarmonizeTransition } from "../../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../../hooks/useRegionData";
import { anomalyTone, monthAverages } from "../../../lib/dockCharts";
import { resolveActiveYear } from "../../../lib/radialCalendar";
import { useAppStore } from "../../../store/useAppStore";
import styles from "./charts.module.css";
import { ChartNote, VIEW_H, VIEW_W, Y_BASE } from "./shared";

const HOT = "#f37819";
const NORMAL = "#6b7280";
const AVG = "#d0d6e2";
const BAR_W = 28;
const STEP = 38;
const X_START = 36;
const BAR_MAX_H = 190;
const MONTHS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12] as const;

export default function CalendarBars() {
  const { t, fmt } = useCopy();
  const { geometryHarmonized } = useHarmonizeTransition((v) => ({ geometryHarmonized: v.geometryHarmonized }));
  const { status, hasData, months, firstYear, lastYear, defaultMapMonth } = useRegionData();
  const selectedMonth = useAppStore((s) => s.selectedMonth);
  const calendarYear = useAppStore((s) => s.calendarYear);
  const toggleMonth = useAppStore((s) => s.toggleMonth);

  const year = resolveActiveYear({ viewYear: calendarYear, selectedMonth, defaultMapMonth, firstYear, lastYear });

  const bars = useMemo(() => {
    if (year === null) return [];
    const avgs = monthAverages(months, year, geometryHarmonized);
    return MONTHS.map((m) => {
      const c = months.find((x) => x.year === year && x.month === m);
      const count = c?.hasData ? (geometryHarmonized ? c.harmonizedCount : c.rawCount) : 0;
      const z = c ? (geometryHarmonized ? c.anomalyHarmonized : c.anomalyRaw) : null;
      return { month: m, hasData: c?.hasData === true, count, avg: avgs[m - 1], tone: anomalyTone(z) };
    });
  }, [months, year, geometryHarmonized]);

  if (status === "loading") return <ChartNote text={t("loadingFirms")} />;
  if (status === "error") return <ChartNote text={t("errorLoading")} />;
  if (!hasData || year === null || firstYear === null || lastYear === null || bars.length === 0) {
    return <ChartNote text={t("dockNoData")} />;
  }

  const maxVal = Math.max(1, ...bars.map((b) => Math.max(b.count, b.avg ?? 0)));
  const h = (v: number) => (BAR_MAX_H * v) / maxVal;
  const busiest = bars.reduce((best, b) => (b.count > best.count ? b : best), bars[0]);
  const hasOthers = bars.some((b) => b.avg !== null);
  const mapMonth = selectedMonth ?? defaultMapMonth;
  const selectedHere = mapMonth && mapMonth.year === year ? mapMonth.month : null;

  const activate = (month: number, hasBarData: boolean) => {
    if (hasBarData) toggleMonth({ year, month });
  };
  const onKey = (e: KeyboardEvent<SVGGElement>, month: number, hasBarData: boolean) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      activate(month, hasBarData);
    }
  };

  return (
    <div>
      <p className={styles.lead}>
        {t("chartLeadCalendar", {
          year: String(year),
          month: fmt.monthName(busiest.month, "long"),
          n: fmt.number(busiest.count),
        })}
      </p>
      <svg
        className={styles.svg}
        viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
        role="group"
        aria-label={t("chartAriaCalendar", { year: String(year) })}
      >
        <line x1={30} x2={494} y1={Y_BASE} y2={Y_BASE} className={styles.base} />
        {bars.map((b, i) => {
          const x = X_START + i * STEP;
          const bh = h(b.count);
          const tip = t("chartMonthTip", {
            month: fmt.monthName(b.month, "long"),
            year: String(year),
            n: fmt.number(b.count),
            avg: b.avg === null ? "–" : fmt.number(Math.round(b.avg)),
          });
          const selected = selectedHere === b.month;
          return (
            <g
              key={b.month}
              role="button"
              tabIndex={0}
              aria-label={tip}
              aria-pressed={selected}
              aria-disabled={!b.hasData ? true : undefined}
              className={styles.bar}
              onClick={() => activate(b.month, b.hasData)}
              onKeyDown={(e) => onKey(e, b.month, b.hasData)}
            >
              <title>{tip}</title>
              <rect x={x - 5} y={10} width={BAR_W + 10} height={Y_BASE + 24 - 10} className={styles.barFocus} />
              <rect x={x} y={Y_BASE - bh} width={BAR_W} height={bh} fill={b.tone === "hot" ? HOT : NORMAL} />
              {b.avg !== null && (
                <line
                  x1={x - 3}
                  x2={x + BAR_W + 3}
                  y1={Y_BASE - h(b.avg)}
                  y2={Y_BASE - h(b.avg)}
                  stroke={AVG}
                  strokeWidth={2}
                />
              )}
              <text
                x={x + BAR_W / 2}
                y={228}
                className={`${styles.monthLabel} ${selected ? styles.monthLabelSelected : ""}`}
              >
                {fmt.monthName(b.month, "short")}
              </text>
            </g>
          );
        })}
      </svg>
      <div className={styles.legend}>
        <span>
          <span className={`${styles.swatch} ${styles.swatchHot}`} />
          {t("chartLegendAbove")}
        </span>
        <span>
          <span className={`${styles.swatch} ${styles.swatchNormal}`} />
          {t("chartLegendNear")}
        </span>
        {hasOthers && (
          <span>
            <span className={styles.swatchTick} />
            {t("chartLegendAverage", { first: String(firstYear), last: String(year - 1) })}
          </span>
        )}
      </div>
    </div>
  );
}
