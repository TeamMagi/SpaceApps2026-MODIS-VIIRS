/**
 * BurnCalendar — monthly anomaly heat map built from the archive API's returned months.
 *
 * Each cell carries both its raw and harmonized colour as CSS variables; the grid's
 * data-harmonized attribute picks one, and null anomalies render with the hatched no-data pattern.
 */

import {
  memo,
  useCallback,
  useId,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type FocusEvent,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import { formatApiError } from "../../content/copy";
import { useCopy, type CopyApi } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData, useSelectedMonthCell } from "../../hooks/useRegionData";
import { anomalyColorVar } from "../../lib/scales";
import { yearTicks } from "../../lib/yearTicks";
import { useAppStore } from "../../store/useAppStore";
import type { MonthCell } from "../../types";
import styles from "./BurnCalendar.module.css";
import CalendarTooltip, { formatCalendarCellText } from "./CalendarTooltip";

interface GridCssVars extends CSSProperties {
  "--years": number;
}

interface HeaderYearVars extends CSSProperties {
  "--y": number;
}

interface HeaderMonthVars extends CSSProperties {
  "--m": number;
}

interface CellCssVars extends CSSProperties {
  "--y": number;
  "--m": number;
  "--bg-raw": string;
  "--bg-harm"?: string;
}

function cellColorVar(hasData: boolean, count: number, z: number | null): string {
  if (!hasData) return "var(--anom-none)";
  if (count === 0) return "var(--anom-neg-3)";
  if (z === null) return "var(--anom-0)";
  return anomalyColorVar(z);
}

interface TooltipAnchor {
  cell: MonthCell;
  position: { x: number; y: number };
  bounds: { width: number; cellHeight: number };
}

const ANOMALY_LEGEND_VARS = [
  "var(--anom-neg-3)",
  "var(--anom-neg-2)",
  "var(--anom-neg-1)",
  "var(--anom-0)",
  "var(--anom-pos-1)",
  "var(--anom-pos-2)",
  "var(--anom-pos-3)",
] as const;

function cellKey(year: number, month: number): string {
  return `${year}-${month}`;
}

function cellElementOf(target: EventTarget | null): HTMLButtonElement | null {
  if (!(target instanceof Element)) return null;
  return target.closest<HTMLButtonElement>("button[data-key]");
}

interface CalendarCellProps {
  cell: MonthCell;
  yIdx: number;
  mIdx: number;
  revealAnswer: boolean;
  showHarmonizedStyle: boolean;
  copy: Pick<CopyApi, "t" | "fmt">;
  selected: boolean;
  roving: boolean;
  describedBy: string | undefined;
}

const CalendarCell = memo(function CalendarCell({
  cell,
  yIdx,
  mIdx,
  revealAnswer,
  showHarmonizedStyle,
  copy,
  selected,
  roving,
  describedBy,
}: CalendarCellProps) {
  const style: CellCssVars = {
    "--y": yIdx,
    "--m": mIdx,
    "--bg-raw": cellColorVar(cell.hasData, cell.rawCount, cell.anomalyRaw),
    ...(showHarmonizedStyle
      ? { "--bg-harm": cellColorVar(cell.hasData, cell.harmonizedCount, cell.anomalyHarmonized) }
      : {}),
  };
  return (
    <button
      type="button"
      role="gridcell"
      data-key={cellKey(cell.year, cell.month)}
      className={styles.cell}
      style={style}
      tabIndex={roving ? 0 : -1}
      aria-selected={selected}
      aria-disabled={!cell.hasData ? true : undefined}
      aria-label={formatCalendarCellText(cell, revealAnswer, copy)}
      aria-describedby={describedBy}
      data-selected={selected ? "true" : undefined}
      data-empty={!cell.hasData ? "true" : undefined}
    />
  );
});

function BurnCalendar() {
  const copy = useCopy();
  const { t, fmt } = copy;
  const { geometryHarmonized, revealAnswer } = useHarmonizeTransition((v) => ({
    geometryHarmonized: v.geometryHarmonized,
    revealAnswer: v.revealAnswer,
  }));
  const { status, error, retry, yearly, months, cliff, firstYear, lastYear } = useRegionData();
  const selected = useSelectedMonthCell();
  const toggleMonth = useAppStore((state) => state.toggleMonth);

  const tooltipId = useId();
  const containerRef = useRef<HTMLElement | null>(null);
  const gridRef = useRef<HTMLDivElement | null>(null);

  const [hovered, setHovered] = useState<TooltipAnchor | null>(null);
  const [focused, setFocused] = useState<TooltipAnchor | null>(null);
  const [rovingKey, setRovingKey] = useState<string>(() =>
    selected ? cellKey(selected.year, selected.month) : cellKey(firstYear ?? 0, 1),
  );

  const years = useMemo(() => {
    if (firstYear === null || lastYear === null) return [];
    const list: number[] = [];
    for (let y = firstYear; y <= lastYear; y++) list.push(y);
    return list;
  }, [firstYear, lastYear]);

  const partialYears = useMemo(() => {
    const set = new Set<number>();
    for (const y of yearly) {
      if (y.partial) set.add(y.year);
    }
    return set;
  }, [yearly]);

  const monthNumbers = useMemo(() => Array.from({ length: 12 }, (_, i) => i + 1), []);

  const cellMap = useMemo(() => {
    const map = new Map<string, MonthCell>();
    for (const c of months) map.set(cellKey(c.year, c.month), c);
    return map;
  }, [months]);

  const activeRovingKey = cellMap.has(rovingKey)
    ? rovingKey
    : selected
      ? cellKey(selected.year, selected.month)
      : cellKey(firstYear ?? 0, 1);

  const anchorFor = useCallback(
    (el: HTMLElement): TooltipAnchor | null => {
      const cell = el.dataset.key ? cellMap.get(el.dataset.key) : undefined;
      const container = containerRef.current;
      if (!cell || !container) return null;
      const containerRect = container.getBoundingClientRect();
      const cellRect = el.getBoundingClientRect();
      return {
        cell,
        position: {
          x: cellRect.left - containerRect.left + cellRect.width / 2,
          y: cellRect.top - containerRect.top,
        },
        bounds: { width: containerRect.width, cellHeight: cellRect.height },
      };
    },
    [cellMap],
  );

  const handleMouseOver = (e: MouseEvent<HTMLDivElement>) => {
    const el = cellElementOf(e.target);
    if (!el || hovered?.cell === cellMap.get(el.dataset.key ?? "")) return;
    setHovered(anchorFor(el));
  };

  const handleFocus = (e: FocusEvent<HTMLDivElement>) => {
    const el = cellElementOf(e.target);
    if (!el?.dataset.key) return;
    setRovingKey(el.dataset.key);
    setFocused(anchorFor(el));
  };

  const handleBlur = (e: FocusEvent<HTMLDivElement>) => {
    if (!(e.relatedTarget instanceof Node) || !e.currentTarget.contains(e.relatedTarget)) {
      setFocused(null);
    }
  };

  const handleClick = (e: MouseEvent<HTMLDivElement>) => {
    const cell = cellMap.get(cellElementOf(e.target)?.dataset.key ?? "");
    if (cell?.hasData) toggleMonth({ year: cell.year, month: cell.month });
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (firstYear === null || lastYear === null) return;
    const cell = cellMap.get(cellElementOf(e.target)?.dataset.key ?? "");
    if (!cell) return;
    const rotated = window.matchMedia("(max-width: 1023px)").matches;
    let nextYear = cell.year;
    let nextMonth = cell.month;

    switch (e.key) {
      case "ArrowRight":
        if (rotated) nextMonth += 1;
        else nextYear += 1;
        break;
      case "ArrowLeft":
        if (rotated) nextMonth -= 1;
        else nextYear -= 1;
        break;
      case "ArrowDown":
        if (rotated) nextYear += 1;
        else nextMonth += 1;
        break;
      case "ArrowUp":
        if (rotated) nextYear -= 1;
        else nextMonth -= 1;
        break;
      case "Home":
        if (rotated) nextMonth = 1;
        else nextYear = firstYear;
        break;
      case "End":
        if (rotated) nextMonth = 12;
        else nextYear = lastYear;
        break;
      case "Escape":
        setHovered(null);
        setFocused(null);
        return;
      default:
        return;
    }

    e.preventDefault();
    const nextKey = cellKey(Math.max(firstYear, Math.min(lastYear, nextYear)), Math.max(1, Math.min(12, nextMonth)));
    setRovingKey(nextKey);
    gridRef.current?.querySelector<HTMLButtonElement>(`button[data-key="${nextKey}"]`)?.focus();
  };

  const activeTooltip = hovered ?? focused;
  const gridStyle: GridCssVars = { "--years": Math.max(1, years.length) };
  const cliffYear = cliff?.toYear ?? null;
  const desktopTicks = useMemo(
    () => (firstYear !== null && lastYear !== null ? yearTicks(firstYear, lastYear, cliffYear) : new Set<number>()),
    [firstYear, lastYear, cliffYear],
  );
  const titleText = years.length > 0 ? t("calendarTitle", { years: years.length }) : t("calendarLabel");
  const showHarmonizedStyle = revealAnswer || geometryHarmonized;

  if (status === "error") {
    return (
      <section ref={containerRef} className={styles.calendar} aria-label={t("calendarLabel")}>
        <div className={styles.header}>
          <h2 className={styles.title}>{titleText}</h2>
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
          <h2 className={styles.title}>{titleText}</h2>
        </div>
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{t("loadingFirms")}</p>
        </div>
      </section>
    );
  }

  if (status === "empty" || months.length === 0) {
    return (
      <section ref={containerRef} className={styles.calendar} aria-label={t("calendarLabel")}>
        <div className={styles.header}>
          <h2 className={styles.title}>{titleText}</h2>
        </div>
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{t("emptyState")}</p>
        </div>
      </section>
    );
  }

  return (
    <section
      ref={containerRef}
      className={styles.calendar}
      aria-label={t("calendarLabel")}
      data-loading={status === "loading" ? "true" : undefined}
    >
      <div className={styles.header}>
        <h2 className={styles.title}>{titleText}</h2>
        <div className={styles.legend} aria-hidden="true">
          <span>{t("calendarLegendBelow")}</span>
          <span className={styles.legendSwatches}>
            {ANOMALY_LEGEND_VARS.map((colorVar) => (
              <span key={colorVar} className={styles.swatch} style={{ backgroundColor: colorVar }} />
            ))}
          </span>
          <span>{t("calendarLegendAbove")}</span>
        </div>
      </div>

      <div className={styles.subbar}>
        <p className={styles.hint}>{status === "loading" ? t("loadingFirms") : t("calendarSelectHint")}</p>
      </div>

      <div
        ref={gridRef}
        role="grid"
        aria-label={t("calendarLabel")}
        className={styles.grid}
        style={gridStyle}
        data-harmonized={geometryHarmonized}
        onMouseOver={handleMouseOver}
        onMouseLeave={() => setHovered(null)}
        onFocus={handleFocus}
        onBlur={handleBlur}
        onClick={handleClick}
        onKeyDown={handleKeyDown}
      >
        <div role="row" className={styles.rowContents}>
          <div role="columnheader" className={styles.corner} />
          {years.map((year, yIdx) => {
            const yearStyle: HeaderYearVars = { "--y": yIdx };
            const isPartial = partialYears.has(year);
            return (
              <div
                key={`y-${year}`}
                role="columnheader"
                className={styles.yearHeader}
                data-cliff={cliffYear !== null && year === cliffYear ? "true" : undefined}
                data-partial={isPartial ? "true" : undefined}
                title={isPartial ? t("cliffPartialYear", { year: String(year) }) : undefined}
                style={yearStyle}
              >
                <span className={desktopTicks.has(year) ? undefined : styles.yearTickDesktopHidden}>
                  {year}
                  {isPartial ? "*" : ""}
                </span>
              </div>
            );
          })}
        </div>

        {monthNumbers.map((month, mIdx) => {
          const monthStyle: HeaderMonthVars = { "--m": mIdx };
          return (
            <div key={`m-${month}`} role="row" className={styles.rowContents}>
              <div role="rowheader" className={styles.monthHeader} style={monthStyle}>
                {fmt.monthName(month, "short")}
              </div>
              {years.map((year, yIdx) => {
                const key = cellKey(year, month);
                const cell = cellMap.get(key);
                if (!cell) return null;
                const isTooltipCell = activeTooltip !== null && activeTooltip.cell === cell;
                return (
                  <CalendarCell
                    key={key}
                    cell={cell}
                    yIdx={yIdx}
                    mIdx={mIdx}
                    revealAnswer={revealAnswer}
                    showHarmonizedStyle={showHarmonizedStyle}
                    copy={copy}
                    selected={selected !== null && selected.year === year && selected.month === month}
                    roving={activeRovingKey === key}
                    describedBy={isTooltipCell ? tooltipId : undefined}
                  />
                );
              })}
            </div>
          );
        })}
      </div>

      {activeTooltip && (
        <CalendarTooltip
          id={tooltipId}
          cell={activeTooltip.cell}
          harmonized={revealAnswer}
          position={activeTooltip.position}
          bounds={activeTooltip.bounds}
        />
      )}
    </section>
  );
}

export default memo(BurnCalendar);
