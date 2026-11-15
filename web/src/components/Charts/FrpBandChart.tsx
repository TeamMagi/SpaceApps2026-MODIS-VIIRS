/**
 * FrpBandChart — annual count-weighted mean FRP with a +/-1 sigma confidence band.
 * Hand-written SVG matching CliffChart's layout, ResizeObserver sizing, and keyboard slider.
 */

import { animate, motion, useMotionValue, useReducedMotion } from "motion/react";
import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { formatApiError } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import { WIDE_UNCERTAINTY_THRESHOLD, formatFrpInterval, relativeUncertaintyWidth } from "../../lib/format";
import { annualMeanFrp, type AnnualFrpPoint } from "../../lib/frpSeries";
import { resolveActiveYear } from "../../lib/radialCalendar";
import { yearTicks } from "../../lib/yearTicks";
import { resolveMonthForYear, useAppStore } from "../../store/useAppStore";
import styles from "./FrpBandChart.module.css";

const PAD_LEFT = 44;
const PAD_RIGHT = 8;
const PAD_TOP = 20;
const PAD_BOTTOM = 20;
const MIN_TICK_GAP_PX = 36;
const TOOLTIP_FLIP_Y = PAD_TOP + 50;
const DEFAULT_SIZE = { width: 460, height: 150 };

function niceCeil(value: number): number {
  if (value <= 0) return 10;
  const exp = Math.floor(Math.log10(value));
  const mag = 10 ** exp;
  const norm = value / mag;
  const steps = [1.2, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10];
  const step = steps.find((s) => norm <= s) ?? 10;
  return step * mag;
}

interface MeanDot {
  x: number;
  y: number;
  partial: boolean;
  year: number;
}

function buildMeanPaths(
  points: readonly AnnualFrpPoint[],
  partialYears: ReadonlySet<number>,
  xAt: (index: number) => number,
  yAt: (value: number) => number,
): { solidD: string; partialD: string; dots: MeanDot[] } {
  const solidParts: string[] = [];
  const partialParts: string[] = [];
  const dots: MeanDot[] = [];

  for (let i = 0; i < points.length; i++) {
    const curr = points[i];
    if (curr.mean === null) continue;

    dots.push({
      x: xAt(i),
      y: yAt(curr.mean),
      partial: partialYears.has(curr.year),
      year: curr.year,
    });

    if (i > 0) {
      const prev = points[i - 1];
      if (prev.mean !== null) {
        const seg = `M${xAt(i - 1).toFixed(1)} ${yAt(prev.mean).toFixed(1)} L${xAt(i).toFixed(1)} ${yAt(curr.mean).toFixed(1)}`;
        if (partialYears.has(prev.year) || partialYears.has(curr.year)) {
          partialParts.push(seg);
        } else {
          solidParts.push(seg);
        }
      }
    }
  }

  return {
    solidD: solidParts.join(" "),
    partialD: partialParts.join(" "),
    dots,
  };
}

function buildBandPaths(
  points: readonly AnnualFrpPoint[],
  xAt: (index: number) => number,
  yAt: (value: number) => number,
  colW: number,
): { bandPolygons: string[]; wideUpperD: string } {
  const bandPolygons: string[] = [];
  const n = points.length;

  let runStart = -1;
  const flushBandRun = (start: number, end: number) => {
    if (start === end) {
      const p = points[start];
      if (p.lower === null || p.upper === null) return;
      const x = xAt(start);
      const halfW = Math.max(3, Math.min(8, colW * 0.25));
      const yU = yAt(p.upper).toFixed(1);
      const yL = yAt(p.lower).toFixed(1);
      bandPolygons.push(
        `M${(x - halfW).toFixed(1)} ${yU} L${(x + halfW).toFixed(1)} ${yU} L${(x + halfW).toFixed(1)} ${yL} L${(x - halfW).toFixed(1)} ${yL} Z`,
      );
      return;
    }

    const upperPts: string[] = [];
    const lowerPts: string[] = [];
    for (let i = start; i <= end; i++) {
      const p = points[i];
      if (p.upper === null || p.lower === null) continue;
      const x = xAt(i).toFixed(1);
      upperPts.push(`${i === start ? "M" : "L"}${x} ${yAt(p.upper).toFixed(1)}`);
    }
    for (let i = end; i >= start; i--) {
      const p = points[i];
      if (p.lower === null) continue;
      lowerPts.push(`L${xAt(i).toFixed(1)} ${yAt(p.lower).toFixed(1)}`);
    }
    if (upperPts.length > 0 && lowerPts.length > 0) {
      bandPolygons.push(`${upperPts.join(" ")} ${lowerPts.join(" ")} Z`);
    }
  };

  for (let i = 0; i < n; i++) {
    const hasBounds = points[i].lower !== null && points[i].upper !== null;
    if (hasBounds) {
      if (runStart < 0) runStart = i;
    } else if (runStart >= 0) {
      flushBandRun(runStart, i - 1);
      runStart = -1;
    }
  }
  if (runStart >= 0) {
    flushBandRun(runStart, n - 1);
  }

  const wideParts: string[] = [];
  let wideStart = -1;
  const flushWideRun = (start: number, end: number) => {
    if (start === end) {
      const p = points[start];
      if (p.upper === null) return;
      const x = xAt(start);
      const halfW = Math.max(4, Math.min(10, colW * 0.3));
      const yU = yAt(p.upper).toFixed(1);
      wideParts.push(`M${(x - halfW).toFixed(1)} ${yU} L${(x + halfW).toFixed(1)} ${yU}`);
      return;
    }
    for (let i = start; i <= end; i++) {
      const p = points[i];
      if (p.upper === null) continue;
      wideParts.push(`${i === start ? "M" : "L"}${xAt(i).toFixed(1)} ${yAt(p.upper).toFixed(1)}`);
    }
  };

  for (let i = 0; i < n; i++) {
    const p = points[i];
    const rel = relativeUncertaintyWidth(p.lower, p.upper, p.mean);
    const isWide = rel !== null && rel > WIDE_UNCERTAINTY_THRESHOLD && p.upper !== null;
    if (isWide) {
      if (wideStart < 0) wideStart = i;
    } else if (wideStart >= 0) {
      flushWideRun(wideStart, i - 1);
      wideStart = -1;
    }
  }
  if (wideStart >= 0) {
    flushWideRun(wideStart, n - 1);
  }

  return { bandPolygons, wideUpperD: wideParts.join(" ") };
}

export default function FrpBandChart() {
  const { lang, t, fmt } = useCopy();
  const { geometryHarmonized, revealAnswer, durations } = useHarmonizeTransition((v) => ({
    geometryHarmonized: v.geometryHarmonized,
    revealAnswer: v.revealAnswer,
    durations: v.durations,
  }));
  const { status, error, retry, yearly, months, cliff, firstYear, lastYear, defaultMapMonth } = useRegionData();
  const calendarYear = useAppStore((state) => state.calendarYear);
  const selectedMonth = useAppStore((state) => state.selectedMonth);
  const setCalendarYear = useAppStore((state) => state.setCalendarYear);
  const selectMonth = useAppStore((state) => state.selectMonth);
  const stopYearPlayback = useAppStore((state) => state.stopYearPlayback);
  const reducedMotion = useReducedMotion() ?? false;

  const sliderHintId = useId();
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState(DEFAULT_SIZE);
  const [activeIndex, setActiveIndex] = useState<number | null>(null);

  const frpPoints = useMemo(() => annualMeanFrp(months), [months]);
  const n = frpPoints.length;
  const hasAnyMean = useMemo(() => frpPoints.some((p) => p.mean !== null), [frpPoints]);

  const partialYears = useMemo(
    () => new Set(yearly.filter((y) => y.partial).map((y) => y.year)),
    [yearly],
  );
  const hasAnyPartial = useMemo(
    () => frpPoints.some((p) => p.mean !== null && partialYears.has(p.year)),
    [frpPoints, partialYears],
  );

  useEffect(() => {
    const el = wrapRef.current;
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
  }, [n, hasAnyMean]);

  const plotW = Math.max(40, size.width - PAD_LEFT - PAD_RIGHT);
  const plotH = Math.max(40, size.height - PAD_TOP - PAD_BOTTOM);
  const colW = plotW / Math.max(n, 1);

  const niceMax = useMemo(() => {
    const peak = frpPoints.reduce((acc, p) => Math.max(acc, p.mean ?? 0, p.upper ?? 0), 1);
    return niceCeil(peak * 1.06);
  }, [frpPoints]);

  const xAt = useCallback(
    (index: number) => PAD_LEFT + ((index + 0.5) / Math.max(n, 1)) * plotW,
    [n, plotW],
  );
  const yAt = useCallback(
    (value: number) => PAD_TOP + plotH - (value / Math.max(niceMax, 1)) * plotH,
    [niceMax, plotH],
  );

  const bandOpacity = useMotionValue(geometryHarmonized ? 1 : 0);

  useEffect(() => {
    const target = geometryHarmonized ? 1 : 0;
    if (reducedMotion || durations.merge <= 0) {
      bandOpacity.jump(target);
      return;
    }
    const controls = animate(bandOpacity, target, {
      duration: durations.merge / 1000,
      ease: [0.2, 0, 0, 1],
    });
    return () => controls.stop();
  }, [geometryHarmonized, reducedMotion, durations.merge, bandOpacity]);

  const meanPaths = useMemo(
    () => buildMeanPaths(frpPoints, partialYears, xAt, yAt),
    [frpPoints, partialYears, xAt, yAt],
  );
  const bandPaths = useMemo(
    () => buildBandPaths(frpPoints, xAt, yAt, colW),
    [frpPoints, xAt, yAt, colW],
  );

  const cliffYear = cliff?.toYear ?? null;
  const cliffIndex = cliffYear !== null ? frpPoints.findIndex((p) => p.year === cliffYear) : -1;
  const cliffX = cliffIndex >= 0 ? xAt(cliffIndex) : null;

  const selectedYear = useMemo(
    () =>
      resolveActiveYear({
        viewYear: calendarYear,
        selectedMonth,
        defaultMapMonth,
        firstYear,
        lastYear,
      }),
    [calendarYear, selectedMonth, defaultMapMonth, firstYear, lastYear],
  );
  const selectedIndex = useMemo(
    () => (selectedYear !== null ? frpPoints.findIndex((p) => p.year === selectedYear) : -1),
    [frpPoints, selectedYear],
  );
  const selectedX = selectedIndex >= 0 ? xAt(selectedIndex) : null;

  const handleSelectYear = useCallback(
    (year: number) => {
      stopYearPlayback();
      setCalendarYear(year);
      const currentMonth = (selectedMonth ?? defaultMapMonth)?.month ?? null;
      const nextMonth = resolveMonthForYear({
        year,
        months,
        currentMonth,
        harmonized: revealAnswer,
      });
      if (nextMonth) {
        selectMonth(nextMonth);
      }
    },
    [stopYearPlayback, setCalendarYear, selectedMonth, defaultMapMonth, months, revealAnswer, selectMonth],
  );

  const nearCliffDist = cliffX !== null && selectedX !== null ? selectedX - cliffX : null;
  const selectedLabelOverlap = nearCliffDist !== null && Math.abs(nearCliffDist) < MIN_TICK_GAP_PX;
  const selectedLabelX =
    selectedX === null
      ? 0
      : selectedLabelOverlap && cliffX !== null && nearCliffDist !== null
        ? nearCliffDist < 0
          ? Math.min(selectedX, cliffX - 18)
          : Math.max(selectedX, cliffX + 18)
        : selectedX;
  const selectedLabelAnchor =
    selectedLabelOverlap && nearCliffDist !== null
      ? nearCliffDist < 0
        ? "end"
        : "start"
      : "middle";
  const selectedLabelCenterX =
    selectedLabelAnchor === "end"
      ? selectedLabelX - 14
      : selectedLabelAnchor === "start"
        ? selectedLabelX + 14
        : selectedLabelX;

  const yTicks = [0, niceMax / 2, niceMax];
  const xTickYears = useMemo(() => {
    if (n === 0) return [];
    const ticks = yearTicks(frpPoints[0].year, frpPoints[n - 1].year, cliffYear);
    const markersX: number[] = [];
    if (cliffX !== null) markersX.push(cliffX);
    if (selectedYear !== null && selectedYear !== cliffYear && selectedX !== null) {
      markersX.push(selectedLabelCenterX);
    }

    const eligible = frpPoints
      .map((p, index) => ({ year: p.year, index, x: xAt(index) }))
      .filter((item) => {
        if (!ticks.has(item.year)) return false;
        if (cliffYear !== null && item.year === cliffYear) return true;
        if (selectedYear !== null && item.year === selectedYear) return false;
        return markersX.every((mx) => Math.abs(item.x - mx) >= MIN_TICK_GAP_PX);
      });

    const firstItem = eligible[0];
    const lastItem = eligible.length > 1 ? eligible[eligible.length - 1] : null;
    const kept: typeof eligible = [];
    const tryAdd = (item: (typeof eligible)[number]) => {
      if (cliffYear !== null && item.year === cliffYear) {
        kept.push(item);
        return;
      }
      if (kept.every((k) => Math.abs(k.x - item.x) >= MIN_TICK_GAP_PX)) {
        kept.push(item);
      }
    };

    for (const item of eligible) {
      if (cliffYear !== null && item.year === cliffYear) tryAdd(item);
    }
    if (firstItem && firstItem.year !== cliffYear) tryAdd(firstItem);
    if (lastItem && lastItem.year !== cliffYear) tryAdd(lastItem);
    for (const item of eligible) {
      if (item !== firstItem && item !== lastItem && item.year !== cliffYear) {
        tryAdd(item);
      }
    }
    return kept.sort((a, b) => a.index - b.index);
  }, [frpPoints, n, cliffYear, cliffX, selectedYear, selectedX, selectedLabelCenterX, xAt]);

  const handlePointerMove = (e: PointerEvent<SVGSVGElement>) => {
    if (n === 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const relX = e.clientX - rect.left - PAD_LEFT;
    const clamped = Math.max(0, Math.min(plotW - 0.001, relX));
    const idx = Math.min(n - 1, Math.max(0, Math.floor((clamped / plotW) * n)));
    setActiveIndex(idx);
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (n === 0) return;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((prev) => {
        const base = prev ?? (selectedIndex >= 0 ? selectedIndex : -1);
        return Math.min(n - 1, base + 1);
      });
    } else if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((prev) => {
        const base = prev ?? (selectedIndex >= 0 ? selectedIndex : n);
        return Math.max(0, base - 1);
      });
    } else if (e.key === "Home") {
      e.preventDefault();
      setActiveIndex(0);
    } else if (e.key === "End") {
      e.preventDefault();
      setActiveIndex(n - 1);
    } else if (e.key === "Enter" || e.key === " ") {
      const idx = activeIndex ?? (selectedIndex >= 0 ? selectedIndex : null);
      if (idx !== null && frpPoints[idx]) {
        e.preventDefault();
        handleSelectYear(frpPoints[idx].year);
      }
    } else if (e.key === "Escape") {
      setActiveIndex(null);
    }
  };

  const formatPointSummary = useCallback(
    (point: AnnualFrpPoint) => {
      const yearText = partialYears.has(point.year)
        ? t("cliffPartialYear", { year: String(point.year) })
        : String(point.year);
      if (point.mean === null) {
        return t("frpTooltipNoData", { year: yearText });
      }
      const valueText = revealAnswer
        ? formatFrpInterval(point.mean, point.lower, point.upper, lang, true)
        : formatFrpInterval(point.mean, null, null, lang);
      return t("frpTooltip", { year: yearText, value: valueText });
    },
    [partialYears, t, revealAnswer, lang],
  );

  const chartTitle =
    firstYear !== null && lastYear !== null
      ? t("frpChartTitle", { first: String(firstYear), last: String(lastYear) })
      : t("frpChartLabel");

  if (status === "error") {
    return (
      <figure className={styles.chart} aria-label={t("frpChartLabel")}>
        <div className={styles.header}>
          <figcaption className={styles.title}>{chartTitle}</figcaption>
        </div>
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{formatApiError(error, t)}</p>
          <button type="button" className={styles.retry} onClick={retry}>
            {t("retryAction")}
          </button>
        </div>
      </figure>
    );
  }

  if (status === "loading" && n === 0) {
    return (
      <figure className={styles.chart} aria-label={t("frpChartLabel")}>
        <div className={styles.header}>
          <figcaption className={styles.title}>{chartTitle}</figcaption>
        </div>
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{t("loadingFirms")}</p>
        </div>
      </figure>
    );
  }

  if (status === "empty" || n === 0 || !hasAnyMean) {
    return (
      <figure className={styles.chart} aria-label={t("frpChartLabel")}>
        <div className={styles.header}>
          <figcaption className={styles.title}>{chartTitle}</figcaption>
        </div>
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{t("emptyState")}</p>
        </div>
      </figure>
    );
  }

  const activePoint = activeIndex !== null ? frpPoints[activeIndex] ?? null : null;
  const activeX = activeIndex !== null ? xAt(activeIndex) : 0;
  const activeY = activePoint && activePoint.mean !== null ? yAt(activePoint.mean) : PAD_TOP + plotH * 0.5;

  const tooltipBelow = activeY < TOOLTIP_FLIP_Y;
  const tooltipHalfW = Math.min(125, Math.max(60, Math.floor(size.width * 0.28)));
  const tooltipLeft = Math.max(tooltipHalfW, Math.min(size.width - tooltipHalfW, activeX));
  const tooltipTop = tooltipBelow ? Math.max(PAD_TOP + 2, activeY) : activeY;

  const sliderIndex = Math.max(0, Math.min(n - 1, activeIndex ?? (selectedIndex >= 0 ? selectedIndex : 0)));
  const sliderPoint = frpPoints[sliderIndex];
  const sliderValueText = formatPointSummary(sliderPoint);
  const muted = !geometryHarmonized;

  return (
    <figure className={styles.chart} aria-label={t("frpChartLabel")} data-loading={status === "loading" ? "true" : undefined}>
      <div className={styles.header}>
        <figcaption className={styles.title}>{chartTitle}</figcaption>
        <div className={styles.legend}>
          {status === "loading" && <span className={styles.loadingTag}>{t("loadingFirms")}</span>}
          <span className={styles.legendItem}>
            <span className={styles.swatchLine} data-muted={muted ? "true" : undefined} aria-hidden="true" />
            {t("frpLegendMean")}
          </span>
          {revealAnswer && (
            <span className={styles.legendItem}>
              <span className={styles.swatchBand} aria-hidden="true" />
              {t("uncertaintyConfidenceLabel")}
            </span>
          )}
          {hasAnyPartial && (
            <span className={styles.legendItem}>
              <span className={styles.swatchPartial} data-muted={muted ? "true" : undefined} aria-hidden="true" />
              {t("cliffLegendPartial")}
            </span>
          )}
        </div>
      </div>

      <div ref={wrapRef} className={styles.canvasWrap}>
        <svg
          className={styles.svg}
          viewBox={`0 0 ${size.width} ${size.height}`}
          role="img"
          aria-label={t("frpChartLabel")}
          onPointerMove={handlePointerMove}
          onPointerLeave={() => setActiveIndex(null)}
        >
          <text x={PAD_LEFT - 6} y={PAD_TOP - 6} textAnchor="end" className={styles.axisUnit}>
            MW
          </text>

          {yTicks.map((tick) => {
            const y = yAt(tick);
            const digits = tick % 1 !== 0 ? 1 : 0;
            return (
              <g key={tick}>
                <line
                  x1={PAD_LEFT}
                  y1={y}
                  x2={PAD_LEFT + plotW}
                  y2={y}
                  className={tick === 0 ? styles.axisLine : styles.gridLine}
                />
                <text x={PAD_LEFT - 6} y={y + 3} textAnchor="end" className={styles.axisLabel}>
                  {fmt.number(tick, digits)}
                </text>
              </g>
            );
          })}

          {selectedIndex >= 0 && selectedX !== null && (
            <g aria-hidden="true">
              <rect
                x={PAD_LEFT + selectedIndex * colW}
                y={PAD_TOP}
                width={colW}
                height={plotH}
                rx={2}
                className={styles.selectedYearBand}
              />
              <line
                x1={selectedX}
                y1={PAD_TOP}
                x2={selectedX}
                y2={PAD_TOP + plotH}
                className={styles.selectedYearLine}
              />
            </g>
          )}

          {xTickYears.map(({ year, index }) => {
            const x = xAt(index);
            const isCliff = cliffYear !== null && year === cliffYear;
            return (
              <text
                key={year}
                x={x}
                y={PAD_TOP + plotH + 14}
                textAnchor="middle"
                className={isCliff ? `${styles.axisLabel} ${styles.cliffYearLabel}` : styles.axisLabel}
              >
                {year}
              </text>
            );
          })}

          {selectedYear !== null && selectedYear !== cliffYear && selectedX !== null && (
            <text
              x={selectedLabelX}
              y={PAD_TOP + plotH + 14}
              textAnchor={selectedLabelAnchor}
              className={`${styles.axisLabel} ${styles.selectedYearLabel}`}
            >
              {selectedYear}
            </text>
          )}

          {cliffYear !== null && cliffX !== null && (
            <g>
              <line x1={cliffX} y1={PAD_TOP - 4} x2={cliffX} y2={PAD_TOP + plotH} className={styles.cliffLine} />
              <text x={cliffX + 5} y={PAD_TOP - 6} className={styles.cliffLabel}>
                {t("cliffMarker2012", { year: String(cliffYear) })}
              </text>
            </g>
          )}

          <motion.g
            className={styles.bandGroup}
            style={{ opacity: bandOpacity }}
            aria-hidden={!geometryHarmonized}
          >
            {bandPaths.bandPolygons.map((d, idx) => (
              <path key={`band-${idx}`} className={styles.bandPolygon} d={d} />
            ))}
            {bandPaths.wideUpperD && (
              <path className={styles.wideUpperLine} d={bandPaths.wideUpperD} />
            )}
          </motion.g>

          <g>
            {meanPaths.solidD && (
              <path className={styles.seriesLine} data-muted={muted ? "true" : undefined} d={meanPaths.solidD} />
            )}
            {meanPaths.partialD && (
              <path className={styles.partialLine} data-muted={muted ? "true" : undefined} d={meanPaths.partialD} />
            )}
            {meanPaths.dots.map((dot) =>
              dot.partial ? (
                <circle
                  key={`dot-${dot.year}`}
                  cx={dot.x}
                  cy={dot.y}
                  r={3.5}
                  className={styles.partialDot}
                  data-muted={muted ? "true" : undefined}
                />
              ) : (
                <circle
                  key={`dot-${dot.year}`}
                  cx={dot.x}
                  cy={dot.y}
                  r={2.5}
                  className={styles.meanDot}
                  data-muted={muted ? "true" : undefined}
                />
              ),
            )}
          </g>

          {activePoint && (
            <g aria-hidden="true">
              <line x1={activeX} y1={PAD_TOP} x2={activeX} y2={PAD_TOP + plotH} className={styles.hoverGuide} />
              {activePoint.mean !== null && (
                <circle cx={activeX} cy={activeY} r={3.5} className={styles.hoverDot} />
              )}
            </g>
          )}

          <g className={styles.hitGroup}>
            {frpPoints.map((p, i) => (
              <rect
                key={`hit-${p.year}`}
                x={PAD_LEFT + i * colW}
                y={PAD_TOP - 4}
                width={colW}
                height={plotH + PAD_BOTTOM + 4}
                data-year={p.year}
                className={styles.hitCol}
                onPointerEnter={() => setActiveIndex(i)}
                onClick={() => handleSelectYear(p.year)}
              />
            ))}
          </g>
        </svg>

        <div
          role="slider"
          tabIndex={0}
          className={styles.yearSlider}
          aria-label={t("cliffSliderLabel")}
          aria-describedby={sliderHintId}
          aria-orientation="horizontal"
          aria-valuemin={frpPoints[0].year}
          aria-valuemax={frpPoints[n - 1].year}
          aria-valuenow={sliderPoint.year}
          aria-valuetext={sliderValueText}
          onFocus={() => setActiveIndex((prev) => prev ?? (selectedIndex >= 0 ? selectedIndex : 0))}
          onBlur={() => setActiveIndex(null)}
          onKeyDown={handleKeyDown}
        />
        <span id={sliderHintId} className={styles.srOnly}>
          {t("cliffSliderHint")}
        </span>

        {activePoint && (
          <div
            className={styles.tooltip}
            data-placement={tooltipBelow ? "below" : "above"}
            style={{ left: tooltipLeft, top: tooltipTop }}
            role="status"
          >
            {formatPointSummary(activePoint)}
          </div>
        )}
      </div>

      {!revealAnswer && (
        <p className={styles.rawCaption}>{t("frpRawCaption")}</p>
      )}

      <table className={styles.srOnly}>
        <caption>{chartTitle}</caption>
        <thead>
          <tr>
            <th scope="col">{t("cliffTableYear")}</th>
            <th scope="col">{t("frpTableMean")}</th>
            <th scope="col">{t("frpTableLower")}</th>
            <th scope="col">{t("frpTableUpper")}</th>
          </tr>
        </thead>
        <tbody>
          {frpPoints.map((p) => (
            <tr key={p.year}>
              <th scope="row">
                {partialYears.has(p.year) ? t("cliffPartialYear", { year: String(p.year) }) : p.year}
              </th>
              <td>{p.mean !== null ? fmt.number(p.mean, 1) : "—"}</td>
              <td>{revealAnswer && p.lower !== null ? fmt.number(p.lower, 1) : "—"}</td>
              <td>{revealAnswer && p.upper !== null ? fmt.number(p.upper, 1) : "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  );
}
