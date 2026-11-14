/**
 * BurnCalendar — annular radial "year clock" of monthly fire detections, paired with
 * year navigation/playback and a compact multi-year histogram.
 *
 * Visual geometry (slice radii, dashed normal arcs, bar heights, colours) follows
 * `geometryHarmonized`; numerical text and ARIA labels only flip when `revealAnswer` is true
 * so Raw mode never leaks the harmonized answer.
 */

import {
  memo,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from "react";
import { formatApiError } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData, useSelectedMonthCell } from "../../hooks/useRegionData";
import { useYearPlayback } from "../../hooks/useYearPlayback";
import {
  RADIAL_CENTER,
  RADIAL_INNER_RADIUS,
  RADIAL_LABEL_RADIUS,
  RADIAL_MAX_RADIUS,
  RADIAL_VIEWBOX_SIZE,
  computeMaxMonthCount,
  computeMonthlyBaselines,
  computeSliceAngles,
  computeSliceRadius,
  computeYearTotal,
  describeAnnularSector,
  describeRadialArc,
  formatSliceDetailLine,
  isTooltipAnchorCurrent,
  polarToCartesian,
  resolveActiveYear,
  resolveFocusMonth,
  resolveVisibleYearTicks,
  sliceColorVar,
  stepYear,
} from "../../lib/radialCalendar";
import { useAppStore } from "../../store/useAppStore";
import type { MonthCell } from "../../types";
import styles from "./BurnCalendar.module.css";
import CalendarTooltip, { formatCalendarCellText } from "./CalendarTooltip";

interface SliceCssVars extends CSSProperties {
  "--slice-raw": string;
  "--slice-harm"?: string;
}

interface HistBarCssVars extends CSSProperties {
  "--bar-h": string;
  "--bar-raw": string;
  "--bar-harm"?: string;
}

interface TooltipAnchor {
  year: number;
  month: number;
  position: { x: number; y: number };
  bounds: { width: number; cellHeight: number };
}

const MONTH_NUMBERS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12] as const;

function cellKey(year: number, month: number): string {
  return `${year}-${month}`;
}

function BurnCalendar() {
  const copy = useCopy();
  const { t, fmt } = copy;
  const { geometryHarmonized, revealAnswer } = useHarmonizeTransition((v) => ({
    geometryHarmonized: v.geometryHarmonized,
    revealAnswer: v.revealAnswer,
  }));
  const { status, error, retry, yearly, months, cliff, firstYear, lastYear, defaultMapMonth } =
    useRegionData();
  const selected = useSelectedMonthCell();
  const region = useAppStore((state) => state.region);
  const toggleMonth = useAppStore((state) => state.toggleMonth);

  const tooltipId = useId();
  const containerRef = useRef<HTMLElement | null>(null);
  const wheelSvgRef = useRef<SVGSVGElement | null>(null);
  const histRowRef = useRef<HTMLDivElement | null>(null);

  const [viewYear, setViewYear] = useState<number | null>(() =>
    resolveActiveYear({
      viewYear: null,
      selectedMonth: selected,
      defaultMapMonth,
      firstYear,
      lastYear,
    }),
  );
  const [hoveredMonth, setHoveredMonth] = useState<number | null>(null);
  const [focusedMonth, setFocusedMonth] = useState<number | null>(null);
  const [rovingMonth, setRovingMonth] = useState<number>(() => selected?.month ?? defaultMapMonth?.month ?? 7);
  const [tooltipAnchor, setTooltipAnchor] = useState<TooltipAnchor | null>(null);

  const seriesKey = `${firstYear ?? ""}:${lastYear ?? ""}:${defaultMapMonth ? cellKey(defaultMapMonth.year, defaultMapMonth.month) : ""}`;
  const prevResetRef = useRef({ region, seriesKey });
  const prevSelectedKeyRef = useRef<string | null>(selected ? cellKey(selected.year, selected.month) : null);

  const activeYear = useMemo(
    () =>
      resolveActiveYear({
        viewYear,
        selectedMonth: selected,
        defaultMapMonth,
        firstYear,
        lastYear,
      }),
    [viewYear, selected, defaultMapMonth, firstYear, lastYear],
  );

  // Single entry point for year navigation: drops the hover that belonged to the previous year.
  const handleYearChange = useCallback((nextYear: number) => {
    setViewYear(nextYear);
    setHoveredMonth(null);
  }, []);

  const { isPlaying, stop: stopPlayback, togglePlay } = useYearPlayback({
    activeYear,
    firstYear,
    lastYear,
    enabled: status === "ready" && months.length > 0,
    onYearChange: handleYearChange,
  });

  // A region switch or a (re)loaded series invalidates the viewed year; null re-derives it from
  // the selected month, the default month, or the newest archive year once data is available.
  useEffect(() => {
    const prev = prevResetRef.current;
    if (prev.region === region && prev.seriesKey === seriesKey) return;
    prevResetRef.current = { region, seriesKey };
    stopPlayback();
    setViewYear(null);
    setHoveredMonth(null);
    setFocusedMonth(null);
    setRovingMonth(selected?.month ?? defaultMapMonth?.month ?? 7);
  }, [region, seriesKey, selected, defaultMapMonth, stopPlayback]);

  useEffect(() => {
    const currentSelectedKey = selected ? cellKey(selected.year, selected.month) : null;
    if (prevSelectedKeyRef.current === currentSelectedKey) return;
    prevSelectedKeyRef.current = currentSelectedKey;
    stopPlayback();
    if (selected) {
      setViewYear(selected.year);
      setHoveredMonth(null);
      setRovingMonth(selected.month);
    }
  }, [selected, stopPlayback]);

  const cellMap = useMemo(() => {
    const map = new Map<string, MonthCell>();
    for (const c of months) map.set(cellKey(c.year, c.month), c);
    return map;
  }, [months]);

  const baselines = useMemo(() => computeMonthlyBaselines(months), [months]);

  const maxRawMonthCount = useMemo(
    () => computeMaxMonthCount(months, baselines, false),
    [months, baselines],
  );
  const maxHarmMonthCount = useMemo(
    () => computeMaxMonthCount(months, baselines, true),
    [months, baselines],
  );
  const maxGeomMonthCount = geometryHarmonized ? maxHarmMonthCount : maxRawMonthCount;

  const activeYearCells = useMemo(() => {
    if (activeYear === null) return [];
    return MONTH_NUMBERS.map((m) => {
      const found = cellMap.get(cellKey(activeYear, m));
      if (found) return found;
      return {
        year: activeYear,
        month: m,
        hasData: false,
        rawModis: 0,
        rawViirs: 0,
        rawCount: 0,
        harmonizedCount: 0,
        anomalyRaw: null,
        anomalyHarmonized: null,
      };
    });
  }, [activeYear, cellMap]);

  const focusMonth = useMemo(
    () =>
      resolveFocusMonth({
        yearCells: activeYearCells,
        selectedMonth: selected,
        defaultMapMonth,
        hoveredMonth,
        focusedMonth,
        harmonized: revealAnswer,
      }),
    [activeYearCells, selected, defaultMapMonth, hoveredMonth, focusedMonth, revealAnswer],
  );

  const focusCell = activeYearCells[focusMonth - 1] ?? null;

  const activeYearTotal = useMemo(() => {
    if (activeYear === null) return 0;
    return computeYearTotal(activeYear, yearly, activeYearCells, revealAnswer);
  }, [activeYear, yearly, activeYearCells, revealAnswer]);

  const partialYears = useMemo(() => {
    const set = new Set<number>();
    for (const y of yearly) {
      if (y.partial) set.add(y.year);
    }
    return set;
  }, [yearly]);

  const histYears = useMemo(() => {
    if (yearly.length > 0) return yearly;
    if (firstYear === null || lastYear === null) return [];
    const fallback = [];
    for (let y = firstYear; y <= lastYear; y++) {
      const yCells = MONTH_NUMBERS.map((m) => cellMap.get(cellKey(y, m))).filter(
        (c): c is MonthCell => c !== undefined && c.hasData,
      );
      const raw = yCells.reduce((acc, c) => acc + c.rawCount, 0);
      const harmonized = yCells.reduce((acc, c) => acc + c.harmonizedCount, 0);
      fallback.push({
        year: y,
        raw,
        rawModis: 0,
        rawViirs: 0,
        harmonized,
        matched: 0,
        modisOnly: 0,
        viirsOnly: 0,
        interscanRemoved: 0,
        fleetMerged: 0,
        firstDate: null,
        lastDate: null,
        partial: false,
      });
    }
    return fallback;
  }, [yearly, firstYear, lastYear, cellMap]);

  const histStats = useMemo(() => {
    let maxRaw = 1;
    let maxHarm = 1;
    let sumRaw = 0;
    let sumHarm = 0;
    for (const y of histYears) {
      if (y.raw > maxRaw) maxRaw = y.raw;
      if (y.harmonized > maxHarm) maxHarm = y.harmonized;
      sumRaw += y.raw;
      sumHarm += y.harmonized;
    }
    const count = Math.max(1, histYears.length);
    return {
      maxRaw,
      maxHarm,
      meanRaw: sumRaw / count,
      meanHarm: sumHarm / count,
    };
  }, [histYears]);

  const cliffYear = cliff?.toYear ?? null;

  const visibleTickYears = useMemo(() => {
    if (firstYear === null || lastYear === null) return new Set<number>();
    return resolveVisibleYearTicks(firstYear, lastYear, cliffYear, activeYear);
  }, [firstYear, lastYear, cliffYear, activeYear]);

  const computeAnchorForMonth = useCallback(
    (year: number, month: number, outerRadius: number, midAngle: number): TooltipAnchor | null => {
      const container = containerRef.current;
      const svg = wheelSvgRef.current;
      if (!container || !svg) return null;
      const containerRect = container.getBoundingClientRect();
      const svgRect = svg.getBoundingClientRect();
      if (svgRect.width <= 0 || svgRect.height <= 0) return null;

      const pt = polarToCartesian(RADIAL_CENTER, RADIAL_CENTER, outerRadius, midAngle);
      const scale = Math.min(svgRect.width / RADIAL_VIEWBOX_SIZE, svgRect.height / RADIAL_VIEWBOX_SIZE);
      const offsetX = svgRect.left - containerRect.left + (svgRect.width - RADIAL_VIEWBOX_SIZE * scale) / 2;
      const offsetY = svgRect.top - containerRect.top + (svgRect.height - RADIAL_VIEWBOX_SIZE * scale) / 2;

      return {
        year,
        month,
        position: {
          x: offsetX + pt.x * scale,
          y: offsetY + pt.y * scale,
        },
        bounds: {
          width: containerRect.width,
          cellHeight: 16,
        },
      };
    },
    [],
  );

  // The tooltip follows the interacting month of the *current* year; its anchor is re-measured
  // after every render that could move the slice, so it can never describe a previous year.
  const tooltipMonth = hoveredMonth ?? focusedMonth;
  const tooltipCell = tooltipMonth !== null ? (activeYearCells[tooltipMonth - 1] ?? null) : null;
  const tooltipRadius = tooltipCell
    ? computeSliceRadius(
        tooltipCell.hasData,
        geometryHarmonized ? tooltipCell.harmonizedCount : tooltipCell.rawCount,
        maxGeomMonthCount,
      )
    : null;

  useLayoutEffect(() => {
    if (activeYear === null || tooltipMonth === null || tooltipRadius === null) {
      setTooltipAnchor((prev) => (prev === null ? prev : null));
      return;
    }
    setTooltipAnchor(
      computeAnchorForMonth(activeYear, tooltipMonth, tooltipRadius, computeSliceAngles(tooltipMonth).midAngle),
    );
  }, [activeYear, tooltipMonth, tooltipRadius, computeAnchorForMonth]);

  const tooltipVisible = tooltipCell !== null && isTooltipAnchorCurrent(tooltipAnchor, activeYear, tooltipMonth);

  const handleSliceActivate = useCallback(
    (cell: MonthCell) => {
      stopPlayback();
      setRovingMonth(cell.month);
      if (cell.hasData) {
        toggleMonth({ year: cell.year, month: cell.month });
      }
    },
    [stopPlayback, toggleMonth],
  );

  const handleWheelKeyDown = (e: KeyboardEvent<SVGSVGElement>) => {
    if (activeYear === null || firstYear === null || lastYear === null) return;
    let nextMonth = rovingMonth;
    let nextYear = activeYear;

    switch (e.key) {
      case "ArrowRight":
      case "ArrowDown":
        nextMonth = rovingMonth === 12 ? 1 : rovingMonth + 1;
        break;
      case "ArrowLeft":
      case "ArrowUp":
        nextMonth = rovingMonth === 1 ? 12 : rovingMonth - 1;
        break;
      case "Home":
        nextMonth = 1;
        break;
      case "End":
        nextMonth = 12;
        break;
      case "PageUp":
        nextYear = stepYear(activeYear, -1, firstYear, lastYear);
        break;
      case "PageDown":
        nextYear = stepYear(activeYear, 1, firstYear, lastYear);
        break;
      case "Enter":
      case " ": {
        e.preventDefault();
        const cell = activeYearCells[rovingMonth - 1];
        if (cell) handleSliceActivate(cell);
        return;
      }
      case "Escape":
        setHoveredMonth(null);
        setFocusedMonth(null);
        return;
      default:
        return;
    }

    e.preventDefault();
    stopPlayback();
    setHoveredMonth(null);
    if (nextYear !== activeYear) {
      handleYearChange(nextYear);
    }
    setRovingMonth(nextMonth);
    setFocusedMonth(nextMonth);
    wheelSvgRef.current
      ?.querySelector<SVGGElement>(`g[data-month="${nextMonth}"]`)
      ?.focus();
  };

  const handleHistKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (activeYear === null || firstYear === null || lastYear === null) return;
    let nextYear = activeYear;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") {
      nextYear = stepYear(activeYear, 1, firstYear, lastYear);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
      nextYear = stepYear(activeYear, -1, firstYear, lastYear);
    } else if (e.key === "Home") {
      nextYear = firstYear;
    } else if (e.key === "End") {
      nextYear = lastYear;
    } else {
      return;
    }
    e.preventDefault();
    stopPlayback();
    handleYearChange(nextYear);
    histRowRef.current
      ?.querySelector<HTMLButtonElement>(`button[data-year="${nextYear}"]`)
      ?.focus();
  };

  const clockSubText =
    firstYear !== null && lastYear !== null
      ? t("calendarClockRange", { first: String(firstYear), last: String(lastYear) })
      : t("calendarClockSub");

  const normalLegendText =
    firstYear !== null && lastYear !== null
      ? t("calendarNormalRange", { first: String(firstYear), last: String(lastYear) })
      : t("calendarNormalLabel");

  const showHarmonizedStyle = revealAnswer || geometryHarmonized;

  if (status === "error") {
    return (
      <section ref={containerRef} className={styles.calendar} aria-label={t("calendarLabel")}>
        <div className={styles.header}>
          <h2 className={styles.title}>{t("calendarHeading")}</h2>
          <span className={styles.clockSub}>{clockSubText}</span>
        </div>
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{formatApiError(error, t)}</p>
          <button type="button" className={styles.retry} onClick={retry}>
            {t("retryAction")}
          </button>
        </div>
      </section>
    );
  }

  if (status === "loading" && months.length === 0) {
    return (
      <section ref={containerRef} className={styles.calendar} aria-label={t("calendarLabel")}>
        <div className={styles.header}>
          <h2 className={styles.title}>{t("calendarHeading")}</h2>
          <span className={styles.clockSub}>{clockSubText}</span>
        </div>
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{t("loadingFirms")}</p>
        </div>
      </section>
    );
  }

  if (status === "empty" || months.length === 0 || activeYear === null || firstYear === null || lastYear === null) {
    return (
      <section ref={containerRef} className={styles.calendar} aria-label={t("calendarLabel")}>
        <div className={styles.header}>
          <h2 className={styles.title}>{t("calendarHeading")}</h2>
          <span className={styles.clockSub}>{clockSubText}</span>
        </div>
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{t("emptyState")}</p>
        </div>
      </section>
    );
  }

  const focusBaseline = focusCell ? baselines[focusCell.month - 1] : null;
  const focusMean = focusBaseline
    ? revealAnswer
      ? focusBaseline.meanHarmonized
      : focusBaseline.meanRaw
    : null;
  const detailLineText = focusCell
    ? formatSliceDetailLine(focusCell, focusMean, revealAnswer, copy)
    : "";

  const isActiveYearPartial = partialYears.has(activeYear);

  return (
    <section
      ref={containerRef}
      className={styles.calendar}
      aria-label={t("calendarLabel")}
      data-loading={status === "loading" ? "true" : undefined}
      data-harmonized={geometryHarmonized}
    >
      <div className={styles.header}>
        <h2 className={styles.title}>{t("calendarHeading")}</h2>
        <span className={styles.clockSub}>
          {status === "loading" ? t("loadingFirms") : clockSubText}
        </span>
      </div>

      <div className={styles.body}>
        <div className={styles.wheelWrap}>
          <svg
            ref={wheelSvgRef}
            viewBox={`0 0 ${RADIAL_VIEWBOX_SIZE} ${RADIAL_VIEWBOX_SIZE}`}
            preserveAspectRatio="xMidYMid meet"
            className={styles.wheelSvg}
            role="group"
            aria-label={t("calendarWheelLabel", { year: String(activeYear) })}
            onKeyDown={handleWheelKeyDown}
            onMouseLeave={() => setHoveredMonth(null)}
          >
            <circle
              cx={RADIAL_CENTER}
              cy={RADIAL_CENTER}
              r={RADIAL_MAX_RADIUS}
              className={styles.outerGuideRing}
            />

            {activeYearCells.map((cell) => {
              const month = cell.month;
              const angles = computeSliceAngles(month);
              const baseline = baselines[month - 1];
              const geomCount = geometryHarmonized ? cell.harmonizedCount : cell.rawCount;
              const geomMean = geometryHarmonized ? baseline.meanHarmonized : baseline.meanRaw;
              const outerRadius = computeSliceRadius(cell.hasData, geomCount, maxGeomMonthCount);
              const trackPath = describeAnnularSector(
                RADIAL_CENTER,
                RADIAL_CENTER,
                RADIAL_INNER_RADIUS,
                RADIAL_MAX_RADIUS,
                angles.startAngle,
                angles.endAngle,
              );
              const sectorPath = describeAnnularSector(
                RADIAL_CENTER,
                RADIAL_CENTER,
                RADIAL_INNER_RADIUS,
                outerRadius,
                angles.startAngle,
                angles.endAngle,
              );
              const meanRadius =
                geomMean !== null && geomMean > 0
                  ? computeSliceRadius(true, geomMean, maxGeomMonthCount)
                  : null;
              const meanArcPath =
                meanRadius !== null
                  ? describeRadialArc(
                      RADIAL_CENTER,
                      RADIAL_CENTER,
                      meanRadius,
                      angles.startAngle - 0.008,
                      angles.endAngle + 0.008,
                    )
                  : null;

              const labelPos = polarToCartesian(
                RADIAL_CENTER,
                RADIAL_CENTER,
                RADIAL_LABEL_RADIUS,
                angles.midAngle,
              );

              const isSelectedFilter =
                selected !== null && selected.year === activeYear && selected.month === month;
              const isHighlighted = focusMonth === month;
              const isTooltipCell = tooltipVisible && tooltipMonth === month;

              const sliceStyle: SliceCssVars = {
                "--slice-raw": sliceColorVar(
                  cell.hasData,
                  cell.rawCount,
                  cell.anomalyRaw,
                  baseline.meanRaw,
                ),
                ...(showHarmonizedStyle
                  ? {
                      "--slice-harm": sliceColorVar(
                        cell.hasData,
                        cell.harmonizedCount,
                        cell.anomalyHarmonized,
                        baseline.meanHarmonized,
                      ),
                    }
                  : {}),
              };

              return (
                <g
                  key={month}
                  role="button"
                  tabIndex={rovingMonth === month ? 0 : -1}
                  data-month={month}
                  data-selected={isSelectedFilter ? "true" : undefined}
                  data-highlighted={isHighlighted ? "true" : undefined}
                  data-empty={!cell.hasData ? "true" : undefined}
                  aria-pressed={isSelectedFilter}
                  aria-disabled={!cell.hasData ? true : undefined}
                  aria-label={formatCalendarCellText(cell, revealAnswer, copy)}
                  aria-describedby={isTooltipCell ? tooltipId : undefined}
                  className={styles.sliceGroup}
                  style={sliceStyle}
                  onClick={() => handleSliceActivate(cell)}
                  onMouseEnter={() => setHoveredMonth(month)}
                  onFocus={() => {
                    setRovingMonth(month);
                    setFocusedMonth(month);
                  }}
                  onBlur={() => setFocusedMonth(null)}
                >
                  <path d={trackPath} className={styles.sliceTrack} />
                  <path d={sectorPath} className={styles.sliceWedge} />
                  {meanArcPath && <path d={meanArcPath} className={styles.normalArc} />}
                  <text
                    x={labelPos.x}
                    y={labelPos.y}
                    textAnchor="middle"
                    dominantBaseline="central"
                    className={styles.monthLabel}
                  >
                    {fmt.monthName(month, "short")}
                  </text>
                </g>
              );
            })}

            <circle
              cx={RADIAL_CENTER}
              cy={RADIAL_CENTER}
              r={RADIAL_INNER_RADIUS}
              className={styles.innerHubRing}
            />
            <text
              x={RADIAL_CENTER}
              y={RADIAL_CENTER - 7}
              textAnchor="middle"
              dominantBaseline="central"
              className={styles.centerYear}
            >
              {activeYear}
              {isActiveYearPartial ? "*" : ""}
            </text>
            <text
              x={RADIAL_CENTER}
              y={RADIAL_CENTER + 14}
              textAnchor="middle"
              dominantBaseline="central"
              className={styles.centerCount}
            >
              {t("calendarYearDetections", { count: activeYearTotal })}
            </text>
          </svg>
        </div>

        <div className={styles.side}>
          <div className={styles.controlsRow}>
            <div className={styles.yearStepper}>
              <button
                type="button"
                className={styles.stepBtn}
                aria-label={t("calendarPrevYear")}
                disabled={activeYear <= firstYear}
                onClick={() => {
                  stopPlayback();
                  handleYearChange(stepYear(activeYear, -1, firstYear, lastYear));
                }}
              >
                ◀
              </button>
              <span
                className={styles.activeYearReadout}
                title={isActiveYearPartial ? t("cliffPartialYear", { year: String(activeYear) }) : undefined}
              >
                {activeYear}
                {isActiveYearPartial ? "*" : ""}
              </span>
              <button
                type="button"
                className={styles.stepBtn}
                aria-label={t("calendarNextYear")}
                disabled={activeYear >= lastYear}
                onClick={() => {
                  stopPlayback();
                  handleYearChange(stepYear(activeYear, 1, firstYear, lastYear));
                }}
              >
                ▶
              </button>
            </div>

            <button
              type="button"
              className={styles.playPill}
              aria-pressed={isPlaying}
              disabled={firstYear >= lastYear}
              onClick={togglePlay}
            >
              <span aria-hidden="true">{isPlaying ? "⏸" : "▶"}</span>
              <span>{isPlaying ? t("calendarPauseYears") : t("calendarPlayYears")}</span>
            </button>
          </div>

          <p className={styles.detailLine} role="status" aria-live="polite">
            {detailLineText}
          </p>

          <div
            ref={histRowRef}
            className={styles.histogramWrap}
            role="group"
            aria-label={t("calendarHistogramLabel")}
            onKeyDown={handleHistKeyDown}
          >
            <div className={styles.histogramBars}>
              {histYears.map((y) => {
                const isYearActive = y.year === activeYear;
                const isCliff = cliffYear !== null && y.year === cliffYear;
                const geomVal = geometryHarmonized ? y.harmonized : y.raw;
                const maxGeom = geometryHarmonized ? histStats.maxHarm : histStats.maxRaw;
                const heightPct = Math.max(8, Math.min(100, Math.round((geomVal / Math.max(1, maxGeom)) * 100)));
                const ariaCount = revealAnswer ? y.harmonized : y.raw;
                const rawBarColor = sliceColorVar(true, y.raw, null, histStats.meanRaw);
                const harmBarColor = sliceColorVar(true, y.harmonized, null, histStats.meanHarm);
                const barStyle: HistBarCssVars = {
                  "--bar-h": `${heightPct}%`,
                  "--bar-raw": rawBarColor,
                  ...(showHarmonizedStyle ? { "--bar-harm": harmBarColor } : {}),
                };
                const showTick = visibleTickYears.has(y.year);
                const partialTitle = y.partial
                  ? t("cliffPartialYear", { year: String(y.year) })
                  : undefined;

                return (
                  <div
                    key={y.year}
                    className={styles.histCol}
                    data-cliff={isCliff ? "true" : undefined}
                    data-active={isYearActive ? "true" : undefined}
                  >
                    {isCliff && <span className={styles.histCliffLine} aria-hidden="true" />}
                    <button
                      type="button"
                      data-year={y.year}
                      tabIndex={isYearActive ? 0 : -1}
                      aria-pressed={isYearActive}
                      aria-label={
                        y.partial
                          ? `${t("calendarHistogramBarAria", { year: String(y.year), count: ariaCount })} (${t("cliffPartialYear", { year: String(y.year) })})`
                          : t("calendarHistogramBarAria", { year: String(y.year), count: ariaCount })
                      }
                      title={
                        partialTitle ??
                        t("calendarHistogramBarAria", { year: String(y.year), count: ariaCount })
                      }
                      className={styles.histBarBtn}
                      style={barStyle}
                      data-active={isYearActive ? "true" : undefined}
                      data-partial={y.partial ? "true" : undefined}
                      onClick={() => {
                        stopPlayback();
                        handleYearChange(y.year);
                      }}
                    >
                      <span className={styles.histBarFill} />
                    </button>
                    <span
                      className={styles.histTick}
                      data-visible={showTick ? "true" : undefined}
                      data-cliff={isCliff ? "true" : undefined}
                      data-active={isYearActive ? "true" : undefined}
                      title={partialTitle}
                    >
                      {showTick ? `${y.year}${y.partial ? "*" : ""}` : ""}
                    </span>
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      </div>

      <div className={styles.footerBar}>
        <div className={styles.legendGroup} aria-hidden="true">
          <span className={styles.legendItem}>
            <span className={styles.legendDash} />
            <span>{normalLegendText}</span>
          </span>
          <span className={styles.legendItem}>
            <span className={styles.legendDotBelow} />
            <span>{t("calendarBelowNormalShort")}</span>
          </span>
          <span className={styles.legendItem}>
            <span className={styles.legendDotAbove} />
            <span>{t("calendarAboveNormalShort")}</span>
          </span>
        </div>
        <p className={styles.sliceHint}>{t("calendarSliceHint")}</p>
      </div>

      {tooltipVisible && tooltipCell && tooltipAnchor && (
        <CalendarTooltip
          id={tooltipId}
          cell={tooltipCell}
          harmonized={revealAnswer}
          position={tooltipAnchor.position}
          bounds={tooltipAnchor.bounds}
        />
      )}
    </section>
  );
}

export default memo(BurnCalendar);
