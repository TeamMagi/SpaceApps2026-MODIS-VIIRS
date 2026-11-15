/**
 * SensorCharts — the dashboard's "Sensors" view: three compact hand-written SVG charts behind sub-tabs
 * (monthly raw bars by sensor, matched-pair FRP scatter, source donut). MODIS is always drawn hollow /
 * hatched and VIIRS filled, so the sensors differ by more than colour.
 */

import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { formatApiError } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useMapPoints, useRegionData } from "../../hooks/useRegionData";
import {
  buildPairScatter,
  buildSourceDonut,
  monthlyBarsForYear,
  type DonutKey,
  type PairScatter,
  type SensorMonthBar,
  type SourceDonut,
} from "../../lib/sensorCharts";
import { resolveActiveYear } from "../../lib/radialCalendar";
import { useAppStore } from "../../store/useAppStore";
import styles from "./SensorCharts.module.css";

type SensorView = "monthly" | "pairs" | "sources";

const VIEWS: readonly SensorView[] = ["monthly", "pairs", "sources"];
const PAD_LEFT = 38;
const PAD_RIGHT = 6;
const PAD_TOP = 8;
const PAD_BOTTOM = 16;
const FALLBACK_SIZE = { width: 360, height: 130 };

function useElementSize() {
  const ref = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState(FALLBACK_SIZE);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const update = () => {
      const rect = el.getBoundingClientRect();
      const width = Math.round(rect.width);
      const height = Math.round(rect.height);
      if (width > 0 && height > 0) {
        setSize((prev) => (prev.width === width && prev.height === height ? prev : { width, height }));
      }
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return { ref, size };
}

function niceCeil(value: number): number {
  if (value <= 0) return 10;
  const mag = 10 ** Math.floor(Math.log10(value));
  const norm = value / mag;
  const step = [1, 1.5, 2, 3, 5, 8, 10].find((s) => norm <= s) ?? 10;
  return step * mag;
}

function compactNumber(value: number): string {
  if (value >= 1_000_000) return `${+(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1000) return `${+(value / 1000).toFixed(1)}K`;
  return String(value);
}

function SensorGlyph({ sensor }: { sensor: "modis" | "viirs" }) {
  return <span className={sensor === "modis" ? styles.glyphModis : styles.glyphViirs} aria-hidden="true" />;
}

// --- Monthly bars -------------------------------------------------------------

function MonthlyBars({ bars, year }: { bars: readonly SensorMonthBar[]; year: number }) {
  const { t, fmt } = useCopy();
  const { ref, size } = useElementSize();
  const hintId = useId();
  const [active, setActive] = useState<number | null>(null);

  const max = niceCeil(Math.max(1, ...bars.flatMap((b) => [b.modis, b.viirs])));
  const plotW = Math.max(40, size.width - PAD_LEFT - PAD_RIGHT);
  const plotH = Math.max(30, size.height - PAD_TOP - PAD_BOTTOM);
  const colW = plotW / 12;
  const barW = Math.max(2, Math.min(10, colW * 0.34));
  const yAt = (v: number) => PAD_TOP + plotH - (v / max) * plotH;
  const baseline = yAt(0);

  const describe = (b: SensorMonthBar) =>
    b.hasData
      ? t("sensorBarsTooltip", {
          month: fmt.monthName(b.month),
          modis: fmt.number(b.modis),
          viirs: fmt.number(b.viirs),
        })
      : t("sensorBarsNoData", { month: fmt.monthName(b.month) });

  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "ArrowRight" || e.key === "ArrowUp") {
      e.preventDefault();
      setActive((prev) => Math.min(11, (prev ?? -1) + 1));
    } else if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
      e.preventDefault();
      setActive((prev) => Math.max(0, (prev ?? 12) - 1));
    } else if (e.key === "Home") {
      e.preventDefault();
      setActive(0);
    } else if (e.key === "End") {
      e.preventDefault();
      setActive(11);
    } else if (e.key === "Escape") {
      setActive(null);
    }
  };

  const activeBar = active !== null ? bars[active] : null;
  const activeX = active !== null ? PAD_LEFT + colW * (active + 0.5) : 0;
  const activeTop = activeBar ? yAt(Math.max(activeBar.modis, activeBar.viirs)) : 0;
  const tooltipHalf = 90;
  const tooltipLeft = Math.max(tooltipHalf, Math.min(size.width - tooltipHalf, activeX));
  const below = activeTop < 34;

  return (
    <div className={styles.chartArea}>
      <div
        ref={ref}
        className={styles.canvas}
        tabIndex={0}
        role="group"
        aria-label={t("sensorBarsLabel", { year: String(year) })}
        aria-describedby={hintId}
        onKeyDown={handleKeyDown}
        onFocus={() => setActive((prev) => prev ?? 0)}
        onBlur={() => setActive(null)}
        onPointerLeave={() => setActive(null)}
      >
        <svg className={styles.svg} width={size.width} height={size.height} aria-hidden="true" focusable="false">
          {[0, 0.5, 1].map((f) => (
            <g key={f}>
              <line x1={PAD_LEFT} x2={size.width - PAD_RIGHT} y1={yAt(max * f)} y2={yAt(max * f)} className={f === 0 ? styles.axis : styles.grid} />
              <text x={PAD_LEFT - 4} y={yAt(max * f) + 3} textAnchor="end" className={styles.tick}>
                {compactNumber(max * f)}
              </text>
            </g>
          ))}
          {bars.map((b, i) => {
            const cx = PAD_LEFT + colW * (i + 0.5);
            const hM = baseline - yAt(b.modis);
            const hV = baseline - yAt(b.viirs);
            return (
              <g key={b.month} data-active={active === i ? "true" : undefined}>
                <rect
                  x={PAD_LEFT + colW * i}
                  y={PAD_TOP}
                  width={colW}
                  height={plotH}
                  className={styles.hit}
                  onPointerEnter={() => setActive(i)}
                  onPointerMove={() => setActive(i)}
                />
                {b.hasData ? (
                  <>
                    <rect x={cx - barW - 0.5} y={yAt(b.modis)} width={barW} height={hM} className={styles.barModis} />
                    <rect x={cx + 0.5} y={yAt(b.viirs)} width={barW} height={hV} className={styles.barViirs} />
                  </>
                ) : (
                  <line x1={cx - barW} x2={cx + barW} y1={baseline - 1} y2={baseline - 1} className={styles.missing} />
                )}
                <text x={cx} y={size.height - 4} textAnchor="middle" className={styles.tick}>
                  {fmt.monthName(b.month, "short")}
                </text>
              </g>
            );
          })}
        </svg>
        {activeBar && (
          <div
            className={styles.tooltip}
            data-placement={below ? "below" : "above"}
            style={{ left: tooltipLeft, top: activeTop }}
            role="status"
          >
            {describe(activeBar)}
          </div>
        )}
        <span id={hintId} className={styles.srOnly}>
          {t("sensorBarsHint")}
        </span>
      </div>
      <div className={styles.srOnly}>
      <table>
        <caption>{t("sensorBarsTitle", { year: String(year) })}</caption>
        <thead>
          <tr>
            <th scope="col">{t("analyticsMonthHeader")}</th>
            <th scope="col">{t("sensorLegendModis")}</th>
            <th scope="col">{t("sensorLegendViirs")}</th>
          </tr>
        </thead>
        <tbody>
          {bars.map((b) => (
            <tr key={b.month}>
              <th scope="row">{fmt.monthName(b.month)}</th>
              <td>{b.hasData ? fmt.number(b.modis) : t("analyticsUnavailable")}</td>
              <td>{b.hasData ? fmt.number(b.viirs) : t("analyticsUnavailable")}</td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
    </div>
  );
}

// --- Pair scatter -------------------------------------------------------------

function PairsScatter({ scatter, truncated, limit }: { scatter: PairScatter; truncated: boolean; limit: number }) {
  const { t, fmt } = useCopy();
  const { ref, size } = useElementSize();
  const { pairs, medianRatio } = scatter;

  const plotSide = Math.max(30, Math.min(size.height - PAD_TOP - PAD_BOTTOM, size.width * 0.5 - PAD_LEFT - PAD_RIGHT));

  const { lo, hi } = useMemo(() => {
    let min = Infinity;
    let max = -Infinity;
    for (const p of pairs) {
      min = Math.min(min, p.modisFrp, p.viirsFrp);
      max = Math.max(max, p.modisFrp, p.viirsFrp);
    }
    if (!Number.isFinite(min)) return { lo: 1, hi: 10 };
    const l = Math.floor(Math.log10(min));
    const h = Math.max(l + 1, Math.ceil(Math.log10(max)));
    return { lo: l, hi: h };
  }, [pairs]);

  const scale = (v: number) => (Math.log10(v) - lo) / (hi - lo);
  const x0 = PAD_LEFT;
  const y0 = PAD_TOP;
  const xAt = (v: number) => x0 + scale(v) * plotSide;
  const yAt = (v: number) => y0 + plotSide - scale(v) * plotSide;
  const decades = Array.from({ length: hi - lo + 1 }, (_, i) => lo + i);

  const dots = useMemo(
    () => pairs.map((p) => `M${xAt(p.modisFrp).toFixed(1)} ${yAt(p.viirsFrp).toFixed(1)}h0`).join(""),
    [pairs, lo, hi, plotSide],
  );

  const decadeLabel = (d: number) => fmt.number(10 ** d, d < 0 ? -d : 0);
  const labelEvery = decades.length > 5 ? 2 : 1;

  return (
    <div className={styles.chartArea}>
      <div ref={ref} className={styles.split}>
        <svg
          className={styles.svgFixed}
          width={PAD_LEFT + plotSide + PAD_RIGHT}
          height={PAD_TOP + plotSide + PAD_BOTTOM}
          role="img"
          aria-label={t("sensorPairsLabel")}
        >
          {decades.map((d, i) => (
            <g key={d}>
              <line x1={xAt(10 ** d)} x2={xAt(10 ** d)} y1={y0} y2={y0 + plotSide} className={i === 0 ? styles.axis : styles.grid} />
              <line x1={x0} x2={x0 + plotSide} y1={yAt(10 ** d)} y2={yAt(10 ** d)} className={i === 0 ? styles.axis : styles.grid} />
              {i % labelEvery === 0 && (
                <>
                  <text x={xAt(10 ** d)} y={y0 + plotSide + 11} textAnchor="middle" className={styles.tick}>
                    {decadeLabel(d)}
                  </text>
                  <text x={x0 - 4} y={yAt(10 ** d) + 3} textAnchor="end" className={styles.tick}>
                    {decadeLabel(d)}
                  </text>
                </>
              )}
            </g>
          ))}
          <line x1={x0} y1={y0 + plotSide} x2={x0 + plotSide} y2={y0} className={styles.oneToOne} />
          <path d={dots} className={styles.dots} />
        </svg>
        <div className={styles.stats}>
          <p className={styles.statLine}>
            <strong>{t("sensorPairsCount", { count: fmt.number(pairs.length) })}</strong>
          </p>
          {medianRatio !== null && (
            <p className={styles.statLine}>{t("sensorPairsMedian", { ratio: fmt.number(medianRatio, 2) })}</p>
          )}
          <p className={styles.statLine}>
            <span className={styles.oneToOneSwatch} aria-hidden="true" /> {t("sensorOneToOne")} · x: {t("sensorAxisModis")} · y:{" "}
            {t("sensorAxisViirs")}
          </p>
          <p className={styles.caption}>{t("sensorPairsCaption")}</p>
          {truncated && <p className={styles.caption}>{t("sensorTruncated", { limit: fmt.number(limit) })}</p>}
        </div>
      </div>
      <div className={styles.srOnly}>
      <table>
        <caption>{t("sensorPairsTitle")}</caption>
        <tbody>
          <tr>
            <th scope="row">{t("sensorPairsCountHeader")}</th>
            <td>{fmt.number(pairs.length)}</td>
          </tr>
          {medianRatio !== null && (
            <tr>
              <th scope="row">{t("sensorPairsMedianHeader")}</th>
              <td>{fmt.number(medianRatio, 2)}</td>
            </tr>
          )}
        </tbody>
      </table>
      </div>
    </div>
  );
}

// --- Donut --------------------------------------------------------------------

const DONUT_RADIUS = 38;
const DONUT_WIDTH = 16;

function DonutChart({ donut, truncated, limit }: { donut: SourceDonut; truncated: boolean; limit: number }) {
  const { t, fmt } = useCopy();
  const hatchId = useId();
  const { ref, size } = useElementSize();
  const side = Math.max(60, Math.min(size.height, size.width * 0.5));

  const labels: Record<DonutKey, string> = {
    modisOnly: t("analyticsModisOnly"),
    viirsOnly: t("analyticsViirsOnly"),
    matched: t("analyticsMatched"),
    modis: t("sensorLegendModis"),
    viirs: t("sensorLegendViirs"),
  };
  const arcClass = (key: DonutKey) =>
    key === "modis" || key === "modisOnly" ? undefined : key === "matched" ? styles.arcMatched : styles.arcViirs;

  let offset = 0;
  const title = donut.kind === "harmonized" ? t("analyticsDonut") : t("sensorDonutRawTitle");

  return (
    <div className={styles.chartArea}>
      <div ref={ref} className={styles.split}>
        <svg
          className={styles.svgFixed}
          width={side}
          height={side}
          viewBox="0 0 100 100"
          role="img"
          aria-label={title}
        >
          <defs>
            <pattern id={hatchId} width="5" height="5" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
              <rect width="5" height="5" className={styles.hatchBg} />
              <line x1="0" y1="0" x2="0" y2="5" className={styles.hatchLine} />
            </pattern>
          </defs>
          {donut.total > 0 &&
            donut.segments.map((s) => {
              const share = s.share * 100;
              const start = offset;
              offset += share;
              if (share <= 0) return null;
              const isModis = s.key === "modis" || s.key === "modisOnly";
              return (
                <circle
                  key={s.key}
                  cx="50"
                  cy="50"
                  r={DONUT_RADIUS}
                  fill="none"
                  strokeWidth={DONUT_WIDTH}
                  pathLength="100"
                  strokeDasharray={`${share} ${100 - share}`}
                  strokeDashoffset={-start}
                  transform="rotate(-90 50 50)"
                  className={arcClass(s.key)}
                  stroke={isModis ? `url(#${hatchId})` : undefined}
                />
              );
            })}
          <text x="50" y="52" textAnchor="middle" className={styles.donutTotal}>
            {compactNumber(donut.total)}
          </text>
        </svg>
        <div className={styles.stats}>
          <ul className={styles.donutLegend}>
            {donut.segments.map((s) => (
              <li key={s.key} className={styles.donutRow}>
                <span className={s.key === "matched" ? styles.glyphMatched : s.key.startsWith("modis") ? styles.glyphModis : styles.glyphViirs} aria-hidden="true" />
                <span>{labels[s.key]}</span>
                <span className={styles.donutValue}>
                  {fmt.number(s.count)} · {fmt.percent(s.share, 1)}
                </span>
              </li>
            ))}
          </ul>
          {donut.kind === "raw" && <p className={styles.caption}>{t("sensorDonutRawNote")}</p>}
          {donut.kind === "harmonized" && truncated && (
            <p className={styles.caption}>{t("sensorTruncated", { limit: fmt.number(limit) })}</p>
          )}
        </div>
      </div>
      <div className={styles.srOnly}>
      <table>
        <caption>{title}</caption>
        <thead>
          <tr>
            <th scope="col">{t("analyticsSourceHeader")}</th>
            <th scope="col">{t("analyticsCountHeader")}</th>
            <th scope="col">{t("analyticsShareHeader")}</th>
          </tr>
        </thead>
        <tbody>
          {donut.segments.map((s) => (
            <tr key={s.key}>
              <th scope="row">{labels[s.key]}</th>
              <td>{fmt.number(s.count)}</td>
              <td>{fmt.percent(s.share, 1)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
    </div>
  );
}

// --- Container ----------------------------------------------------------------

export default function SensorCharts() {
  const { t, fmt } = useCopy();
  const revealAnswer = useHarmonizeTransition((v) => v.revealAnswer);
  const region = useRegionData();
  const points = useMapPoints();
  const calendarYear = useAppStore((state) => state.calendarYear);
  const selectedMonth = useAppStore((state) => state.selectedMonth);
  const [view, setView] = useState<SensorView>("monthly");
  const tabRefs = useRef<Record<SensorView, HTMLButtonElement | null>>({ monthly: null, pairs: null, sources: null });

  const year = useMemo(
    () =>
      selectedMonth?.year ??
      resolveActiveYear({
        viewYear: calendarYear,
        selectedMonth: null,
        defaultMapMonth: region.defaultMapMonth,
        firstYear: region.firstYear,
        lastYear: region.lastYear,
      }),
    [selectedMonth, calendarYear, region.defaultMapMonth, region.firstYear, region.lastYear],
  );

  const bars = useMemo(() => (year !== null ? monthlyBarsForYear(region.months, year) : []), [region.months, year]);
  const scatter = useMemo(
    () => buildPairScatter(revealAnswer, points.harmonized, points.raw),
    [revealAnswer, points.harmonized, points.raw],
  );
  const donut = useMemo(
    () => buildSourceDonut(revealAnswer, points.harmonized, points.stages),
    [revealAnswer, points.harmonized, points.stages],
  );

  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const index = VIEWS.indexOf(view);
    let next: number | null = null;
    if (e.key === "ArrowRight") next = (index + 1) % VIEWS.length;
    else if (e.key === "ArrowLeft") next = (index + VIEWS.length - 1) % VIEWS.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = VIEWS.length - 1;
    if (next === null) return;
    e.preventDefault();
    e.stopPropagation();
    setView(VIEWS[next]);
    tabRefs.current[VIEWS[next]]?.focus();
  };

  const tabLabels: Record<SensorView, string> = {
    monthly: t("sensorSubMonthly"),
    pairs: t("sensorSubPairs"),
    sources: t("sensorSubSources"),
  };

  const periodText =
    points.period.year === null
      ? ""
      : points.period.month !== null
        ? fmt.monthYear({ year: points.period.year, month: points.period.month })
        : String(points.period.year);

  const title =
    view === "monthly"
      ? year !== null
        ? t("sensorBarsTitle", { year: String(year) })
        : t("sensorBarsLabel", { year: "" }).trim()
      : view === "pairs"
        ? t("sensorPairsTitle")
        : revealAnswer
          ? t("analyticsDonut")
          : t("sensorDonutRawTitle");

  const dataSource = view === "monthly" ? region : points;
  const pointsView = view !== "monthly";
  const hasContent = view === "monthly" ? bars.some((b) => b.hasData) : points.hasData;

  const renderBody = () => {
    if (dataSource.status === "error") {
      return (
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{formatApiError(dataSource.error, t)}</p>
          <button type="button" className={styles.retry} onClick={dataSource.retry}>
            {t("retryAction")}
          </button>
        </div>
      );
    }
    if (dataSource.status === "loading" && !hasContent) {
      return (
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{t("loadingFirms")}</p>
        </div>
      );
    }
    if (!hasContent || (view === "monthly" && year === null)) {
      return (
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{t("emptyState")}</p>
        </div>
      );
    }
    if (view === "monthly" && year !== null) return <MonthlyBars bars={bars} year={year} />;
    if (view === "pairs") {
      if (!scatter) {
        return (
          <div className={styles.stateBox} role="status">
            <p className={styles.stateText}>{t("sensorPairsReveal")}</p>
          </div>
        );
      }
      if (scatter.pairs.length === 0) {
        return (
          <div className={styles.stateBox} role="status">
            <p className={styles.stateText}>{t("sensorPairsEmpty")}</p>
          </div>
        );
      }
      return <PairsScatter scatter={scatter} truncated={points.truncated} limit={points.limit} />;
    }
    return <DonutChart donut={donut} truncated={points.truncated} limit={points.limit} />;
  };

  return (
    <figure className={styles.chart} aria-label={t("sensorChartsLabel")} data-loading={dataSource.status === "loading" ? "true" : undefined}>
      <div className={styles.header}>
        <figcaption className={styles.title}>
          {title}
          {pointsView && periodText ? ` · ${periodText}` : ""}
        </figcaption>
        <div className={styles.legend}>
          {view === "monthly" && (
            <>
              <span className={styles.legendItem}>
                <SensorGlyph sensor="modis" />
                {t("sensorLegendModis")}
              </span>
              <span className={styles.legendItem}>
                <SensorGlyph sensor="viirs" />
                {t("sensorLegendViirs")}
              </span>
            </>
          )}
          <div role="group" aria-label={t("sensorSubSwitchLabel")} className={styles.segmented} onKeyDown={handleKeyDown}>
            {VIEWS.map((v) => (
              <button
                key={v}
                ref={(el) => {
                  tabRefs.current[v] = el;
                }}
                type="button"
                className={styles.tab}
                aria-pressed={view === v}
                onClick={() => setView(v)}
              >
                {tabLabels[v]}
              </button>
            ))}
          </div>
        </div>
      </div>
      <div className={styles.body}>{renderBody()}</div>
    </figure>
  );
}
