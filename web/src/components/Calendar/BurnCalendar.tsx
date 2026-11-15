/**
 * BurnCalendar — annular radial "year clock" of monthly fire detections, paired with
 * year navigation and playback.
 *
 * Visual geometry (slice radii, dashed normal arcs, colours) follows
 * `geometryHarmonized`; numerical text and ARIA labels only flip when `revealAnswer` is true
 * so Raw mode never leaks the harmonized answer. In tropical regions a short swath-gap note
 * (sensor limitation only, from `/api/points` effort metadata) sits under the detail line.
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
import { calendarFrpKey, useCalendarFrp } from "../../hooks/useCalendarFrp";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useMapPoints, useRegionData, useSelectedMonthCell } from "../../hooks/useRegionData";
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
  sliceColorVar,
  stepYear,
} from "../../lib/radialCalendar";
import { useAppStore } from "../../store/useAppStore";
import type { MonthCell } from "../../types";
import styles from "./BurnCalendar.module.css";
import CalendarTooltip, { formatCalendarCellText } from "./CalendarTooltip";
import { formatSwathGapNote } from "./swathGapNote";

interface SliceCssVars extends CSSProperties {
  "--slice-raw": string;
  "--slice-harm"?: string;
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
  const { status, error, retry, yearly, months, firstYear, lastYear, defaultMapMonth } =
    useRegionData();
  const { tropicalSwathGap } = useMapPoints();
  const swathGapNote = useMemo(() => formatSwathGapNote(tropicalSwathGap, t), [tropicalSwathGap, t]);
  const selected = useSelectedMonthCell();
  const region = useAppStore((state) => state.region);
  const calendarFrp = useCalendarFrp(region);
  const toggleMonth = useAppStore((state) => state.toggleMonth);
  const calendarYear = useAppStore((state) => state.calendarYear);
  const setCalendarYear = useAppStore((state) => state.setCalendarYear);

  const tooltipId = useId();
  const containerRef = useRef<HTMLElement | null>(null);
  const wheelSvgRef = useRef<SVGSVGElement | null>(null);

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
        viewYear: calendarYear,
        selectedMonth: selected,
        defaultMapMonth,
        firstYear,
        lastYear,
      }),
    [calendarYear, selected, defaultMapMonth, firstYear, lastYear],
  );

  // Single entry point for year navigation: drops the hover that belonged to the previous year.
  const handleYearChange = useCallback(
    (nextYear: number) => {
      setCalendarYear(nextYear);
      setHoveredMonth(null);
    },
    [setCalendarYear],
  );

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
    setCalendarYear(null);
    setHoveredMonth(null);
    setFocusedMonth(null);
    setRovingMonth(selected?.month ?? defaultMapMonth?.month ?? 7);
  }, [region, seriesKey, selected, defaultMapMonth, stopPlayback, setCalendarYear]);

  useEffect(() => {
    const currentSelectedKey = selected ? cellKey(selected.year, selected.month) : null;
    if (prevSelectedKeyRef.current === currentSelectedKey) return;
    prevSelectedKeyRef.current = currentSelectedKey;
    stopPlayback();
    if (selected) {
      setCalendarYear(selected.year);
      setHoveredMonth(null);
      setRovingMonth(selected.month);
    }
  }, [selected, stopPlayback, setCalendarYear]);

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
          <div className={styles.titleGroup}>
            <h2 className={styles.title}>{t("calendarHeading")}</h2>
            <p className={styles.sliceHint}>{t("calendarSliceHint")}</p>
          </div>
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
          <div className={styles.titleGroup}>
            <h2 className={styles.title}>{t("calendarHeading")}</h2>
            <p className={styles.sliceHint}>{t("calendarSliceHint")}</p>
          </div>
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
          <div className={styles.titleGroup}>
            <h2 className={styles.title}>{t("calendarHeading")}</h2>
            <p className={styles.sliceHint}>{t("calendarSliceHint")}</p>
          </div>
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
  const formattedYearTotal = fmt.number(activeYearTotal);
  const isLongYearTotal = formattedYearTotal.length >= 8;

  return (
    <section
      ref={containerRef}
      className={styles.calendar}
      aria-label={t("calendarLabel")}
      data-loading={status === "loading" ? "true" : undefined}
      data-harmonized={geometryHarmonized}
    >
      <div className={styles.header}>
        <div className={styles.titleGroup}>
          <h2 className={styles.title}>{t("calendarHeading")}</h2>
          <p className={styles.sliceHint}>{t("calendarSliceHint")}</p>
        </div>
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
                  aria-label={formatCalendarCellText(cell, revealAnswer, copy, calendarFrp.get(calendarFrpKey(cell.year, cell.month)))}
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
              y={RADIAL_CENTER - 9}
              textAnchor="middle"
              dominantBaseline="central"
              className={styles.centerYear}
              data-long={isLongYearTotal ? "true" : undefined}
            >
              {formattedYearTotal}
            </text>
            <text
              x={RADIAL_CENTER}
              y={RADIAL_CENTER + 13}
              textAnchor="middle"
              dominantBaseline="central"
              className={styles.centerCount}
            >
              {t("calendarYearScope", { year: String(activeYear) })}
              {isActiveYearPartial ? "*" : ""}
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
          {swathGapNote && <p className={styles.swathGapNote}>{swathGapNote}</p>}
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
      </div>

      {tooltipVisible && tooltipCell && tooltipAnchor && (
        <CalendarTooltip
          id={tooltipId}
          cell={tooltipCell}
          harmonized={revealAnswer}
          frp={calendarFrp.get(calendarFrpKey(tooltipCell.year, tooltipCell.month))}
          position={tooltipAnchor.position}
          bounds={tooltipAnchor.bounds}
        />
      )}
    </section>
  );
}

export default memo(BurnCalendar);
