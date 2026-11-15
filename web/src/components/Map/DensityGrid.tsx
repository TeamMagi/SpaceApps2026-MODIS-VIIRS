/**
 * DensityGrid — monthly fire density on an equal-angle grid, canvas-drawn (cells can number in the thousands).
 *
 * One class scale per (year, metric) from lib/density: months and Raw/Harmonized never renormalize, so
 * differences between them are visible. Raw <-> Harmonized cross-fades on the shared clock's merge beat
 * (geometryHarmonized); harmonized numbers in the tooltip wait for revealAnswer. Empty cells are not drawn.
 */

import { animate, useMotionValue, useReducedMotion } from "motion/react";
import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import {
  busiestMonth, cellAreaKm2, cellBounds, cellCol, cellRow, cellValue, CLASS_COUNT, densityScale, neighbourCell,
  type DensityBBox, type DensityCell, type DensityMetric, type DensityMode, type DensityYear, type Direction,
} from "../../lib/density";
import { clampTooltipLeft } from "../../lib/frpSeries";
import { useAppStore } from "../../store/useAppStore";
import { useGeoDataset } from "./useGeoDataset";
import styles from "./DensityGrid.module.css";

interface DensityGridProps {
  data: DensityYear;
  sourceLabel: string;
  sampledNote?: string;
}

const PADDING = 12;
const MIN_HEIGHT = 200;
const MAX_HEIGHT = 480;
const DEFAULT_WIDTH = 640;
const MIN_CELL_PX_FOR_STROKE = 4;
const KEY_DIRECTIONS: Record<string, Direction> = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" };

function viewBox(data: DensityYear): DensityBBox | null {
  let box = data.bbox;
  if (box === null) {
    for (const month of data.months) {
      for (const c of month.cells) {
        const b = cellBounds(c.row, c.col, data.cellDeg);
        box = box === null ? b : { west: Math.min(box.west, b.west), south: Math.min(box.south, b.south), east: Math.max(box.east, b.east), north: Math.max(box.north, b.north) };
      }
    }
  }
  if (box === null) return null;
  const padLon = Math.max((box.east - box.west) * 0.06, data.cellDeg);
  const padLat = Math.max((box.north - box.south) * 0.06, data.cellDeg);
  return { west: box.west - padLon, east: box.east + padLon, south: Math.max(-90, box.south - padLat), north: Math.min(90, box.north + padLat) };
}

function niceDegreeStep(span: number): number {
  const steps = [0.25, 0.5, 1, 2, 5, 10, 20, 30];
  return steps.find(step => span / step <= 8) ?? 30;
}

const cellKey = (row: number, col: number): number => row * 8192 + col;

export default function DensityGrid({ data, sourceLabel, sampledNote }: DensityGridProps) {
  const { t, fmt } = useCopy();
  const { geometryHarmonized, revealAnswer, durations } = useHarmonizeTransition(v => ({
    geometryHarmonized: v.geometryHarmonized,
    revealAnswer: v.revealAnswer,
    durations: v.durations,
  }));
  const selectedMonth = useAppStore(s => s.selectedMonth);
  const selectMonth = useAppStore(s => s.selectMonth);
  const theme = useAppStore(s => s.theme);
  const { dataset } = useGeoDataset();
  const reducedMotion = useReducedMotion() ?? false;
  const hintId = useId();
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const tooltipRef = useRef<HTMLDivElement | null>(null);
  const [tooltipWidth, setTooltipWidth] = useState(0);
  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const [metric, setMetric] = useState<DensityMetric>("count");
  const [active, setActive] = useState<{ row: number; col: number } | null>(null);
  const [pointerPx, setPointerPx] = useState<{ x: number; y: number } | null>(null);

  const month = selectedMonth !== null && selectedMonth.year === data.year ? selectedMonth.month : busiestMonth(data);
  const monthData = data.months.find(m => m.month === month);
  const cells = useMemo(() => monthData?.cells ?? [], [monthData]);
  const covered = useMemo(() => new Set(data.coveredMonths), [data.coveredMonths]);
  const monthLabel = fmt.monthYear({ year: data.year, month });
  const scale = useMemo(() => densityScale(data, metric), [data, metric]);
  const cellMap = useMemo(() => new Map(cells.map(c => [cellKey(c.row, c.col), c])), [cells]);
  const view = useMemo(() => viewBox(data), [data]);

  const geometry = useMemo(() => {
    if (view === null) return null;
    const kx = Math.max(Math.cos(((view.north + view.south) / 2) * (Math.PI / 180)), 0.05);
    const worldW = (view.east - view.west) * kx;
    const worldH = view.north - view.south;
    const height = Math.round(Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, (width - 2 * PADDING) * (worldH / worldW) + 2 * PADDING)));
    const px = Math.min((width - 2 * PADDING) / worldW, (height - 2 * PADDING) / worldH);
    const ox = (width - worldW * px) / 2;
    const oy = (height - worldH * px) / 2;
    return {
      height,
      px,
      x: (lon: number) => ox + (lon - view.west) * kx * px,
      y: (lat: number) => oy + (view.north - lat) * px,
      lon: (x: number) => view.west + (x - ox) / (kx * px),
      lat: (y: number) => view.north - (y - oy) / px,
    };
  }, [view, width]);

  useEffect(() => { setActive(null); }, [data, month, metric]);
  useLayoutEffect(() => {
    const w = tooltipRef.current?.offsetWidth ?? 0;
    setTooltipWidth(prev => (prev === w ? prev : w));
  });

  useEffect(() => {
    const el = wrapRef.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const update = () => {
      const w = Math.round(el.getBoundingClientRect().width);
      if (w > 0) setWidth(prev => (prev === w ? prev : w));
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, [geometry === null]);

  const morph = useMotionValue(geometryHarmonized ? 1 : 0);
  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (canvas === null || geometry === null || view === null) return;
    const dpr = window.devicePixelRatio || 1;
    const pixelW = Math.round(width * dpr);
    const pixelH = Math.round(geometry.height * dpr);
    if (canvas.width !== pixelW || canvas.height !== pixelH) { canvas.width = pixelW; canvas.height = pixelH; }
    const ctx = canvas.getContext("2d");
    if (ctx === null) return;
    const css = getComputedStyle(canvas);
    const token = (name: string) => css.getPropertyValue(name).trim();
    const ramp = Array.from({ length: CLASS_COUNT }, (_, i) => token(`--frp-${i + 1}`));
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, geometry.height);
    ctx.fillStyle = token("--color-surface-sunken");
    ctx.fillRect(0, 0, width, geometry.height);

    ctx.strokeStyle = token("--color-border-subtle");
    ctx.lineWidth = 1;
    ctx.fillStyle = token("--color-text-muted");
    ctx.font = "10px sans-serif";
    const step = niceDegreeStep(Math.max(view.east - view.west, view.north - view.south));
    ctx.beginPath();
    for (let lon = Math.ceil(view.west / step) * step; lon <= view.east; lon += step) { ctx.moveTo(geometry.x(lon), 0); ctx.lineTo(geometry.x(lon), geometry.height); }
    for (let lat = Math.ceil(view.south / step) * step; lat <= view.north; lat += step) { ctx.moveTo(0, geometry.y(lat)); ctx.lineTo(width, geometry.y(lat)); }
    ctx.stroke();
    for (let lon = Math.ceil(view.west / step) * step; lon <= view.east; lon += step) ctx.fillText(`${fmt.number(lon, 2)}°`, geometry.x(lon) + 2, geometry.height - 3);
    for (let lat = Math.ceil(view.south / step) * step; lat <= view.north; lat += step) ctx.fillText(`${fmt.number(lat, 2)}°`, 2, geometry.y(lat) - 2);

    const m = morph.get();
    const layer = (mode: DensityMode, alpha: number) => {
      if (alpha <= 0) return;
      ctx.globalAlpha = alpha;
      const sizeX = geometry.x(data.cellDeg) - geometry.x(0);
      const stroke = sizeX >= MIN_CELL_PX_FOR_STROKE;
      ctx.strokeStyle = token("--color-border-strong");
      ctx.lineWidth = 0.5;
      for (const c of cells) {
        const cls = scale.classOf(cellValue(c, metric, mode));
        if (cls === 0) continue;
        const b = cellBounds(c.row, c.col, data.cellDeg);
        const x = geometry.x(b.west);
        const y = geometry.y(b.north);
        const w = geometry.x(b.east) - x;
        const h = geometry.y(b.south) - y;
        ctx.fillStyle = ramp[cls - 1];
        ctx.fillRect(x, y, w, h);
        if (stroke) ctx.strokeRect(x, y, w, h);
      }
      ctx.globalAlpha = 1;
    };
    layer("raw", 1 - m);
    layer("harmonized", m);

    if (dataset !== null) {
      ctx.strokeStyle = token("--color-text-secondary");
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (const feature of dataset.features) {
        const [w, s, e, n] = feature.bbox;
        if (e < view.west || w > view.east || n < view.south || s > view.north) continue;
        for (const polygon of feature.polygons) {
          for (const ring of polygon) {
            ring.forEach(([lon, lat], i) => (i === 0 ? ctx.moveTo(geometry.x(lon), geometry.y(lat)) : ctx.lineTo(geometry.x(lon), geometry.y(lat))));
          }
        }
      }
      ctx.stroke();
    }

    if (active !== null) {
      const b = cellBounds(active.row, active.col, data.cellDeg);
      ctx.strokeStyle = token("--color-focus");
      ctx.lineWidth = 2;
      ctx.strokeRect(geometry.x(b.west), geometry.y(b.north), geometry.x(b.east) - geometry.x(b.west), geometry.y(b.south) - geometry.y(b.north));
    }
  }, [geometry, view, width, cells, scale, metric, data.cellDeg, dataset, active, morph, fmt]);

  const drawRef = useRef(draw);
  drawRef.current = draw;
  useEffect(() => { draw(); }, [draw, theme]);
  useEffect(() => morph.on("change", () => drawRef.current()), [morph]);
  useEffect(() => {
    const target = geometryHarmonized ? 1 : 0;
    if (reducedMotion || durations.merge <= 0) {
      morph.jump(target);
      return;
    }
    const controls = animate(morph, target, { duration: durations.merge / 1000, ease: [0.2, 0, 0, 1] });
    return () => controls.stop();
  }, [geometryHarmonized, reducedMotion, durations.merge, morph]);

  const changeMonth = (next: number) => selectMonth({ year: data.year, month: next });
  const mode: DensityMode = geometryHarmonized ? "harmonized" : "raw";

  const onPointer = (event: PointerEvent<HTMLDivElement>) => {
    if (geometry === null) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    const cell = cellMap.get(cellKey(cellRow(geometry.lat(y), data.cellDeg), cellCol(geometry.lon(x), data.cellDeg)));
    setActive(cell === undefined ? null : { row: cell.row, col: cell.col });
    setPointerPx(cell === undefined ? null : { x, y });
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const direction = KEY_DIRECTIONS[event.key];
    if (direction === undefined || cells.length === 0) return;
    event.preventDefault();
    setPointerPx(null);
    const from = active ?? { row: cells[0].row, col: cells[0].col };
    const next = active === null ? cells.reduce((a, b) => (cellValue(b, metric, mode) > cellValue(a, metric, mode) ? b : a)) : neighbourCell(cells, from, direction);
    if (next !== null) setActive({ row: next.row, col: next.col });
  };

  const activeCell: DensityCell | null = active === null ? null : cellMap.get(cellKey(active.row, active.col)) ?? null;
  const metricTitle = metric === "count" ? t("densLegendTitleCount") : t("densLegendTitleFrp");

  if (view === null || geometry === null) {
    return <figure className={styles.grid} aria-label={t("densTitle")}>
      <figcaption className={styles.title}>{t("densTitle")}</figcaption>
      <p className={styles.empty} role="status">{t("densEmpty")}</p>
    </figure>;
  }

  const anchor = activeCell === null ? null : (() => {
    if (pointerPx !== null) return pointerPx;
    const b = cellBounds(activeCell.row, activeCell.col, data.cellDeg);
    return { x: geometry.x((b.west + b.east) / 2), y: geometry.y((b.south + b.north) / 2) };
  })();
  const bounds = activeCell === null ? null : cellBounds(activeCell.row, activeCell.col, data.cellDeg);
  const mw = (value: number) => t("densTooltipFrp", { value: fmt.number(value, 1) });
  const edges = scale.breaks.map(b => fmt.number(b, metric === "count" ? 0 : 1)).join(" · ");
  const emptyMessage = !covered.has(month)
    ? t("densNotCovered", { month: monthLabel })
    : cells.length === 0 ? t("densEmptyMonth", { month: monthLabel }) : null;

  return <figure className={styles.grid} aria-label={t("densTitle")}>
    <div className={styles.header}>
      <figcaption className={styles.title}>{t("densTitle")}</figcaption>
      <div className={styles.segmented} role="group" aria-label={t("densMetricGroup")}>
        {(["count", "frp"] as const).map(option => (
          <button key={option} type="button" aria-pressed={metric === option} onClick={() => setMetric(option)}>
            {option === "count" ? t("densMetricCount") : t("densMetricFrp")}
          </button>
        ))}
      </div>
    </div>

    <div className={styles.monthRow}>
      <label className={styles.monthLabel}>
        <span>{t("densMonthSlider")}: <strong>{monthLabel}</strong></span>
        <input
          type="range" min={1} max={12} step={1} value={month}
          aria-valuetext={covered.has(month) ? monthLabel : t("densMonthNotCovered", { month: monthLabel })}
          onChange={e => changeMonth(Number(e.target.value))}
        />
      </label>
      <div className={styles.monthButtons} role="group" aria-label={t("densMonthGroup")}>
        {Array.from({ length: 12 }, (_, i) => i + 1).map(m => (
          <button
            key={m} type="button" aria-pressed={m === month} disabled={!covered.has(m)}
            aria-label={covered.has(m) ? fmt.monthYear({ year: data.year, month: m }) : t("densMonthNotCovered", { month: fmt.monthYear({ year: data.year, month: m }) })}
            onClick={() => changeMonth(m)}
          >{fmt.monthName(m, "short")}</button>
        ))}
      </div>
    </div>

    <div
      ref={wrapRef}
      className={styles.canvasWrap}
      tabIndex={0}
      role="group"
      aria-label={t("densCanvasLabel", { month: monthLabel })}
      aria-describedby={hintId}
      onPointerMove={onPointer}
      onPointerDown={onPointer}
      onPointerLeave={() => { setActive(null); setPointerPx(null); }}
      onBlur={() => { setActive(null); setPointerPx(null); }}
      onKeyDown={onKeyDown}
    >
      <canvas ref={canvasRef} className={styles.canvas} width={width} height={geometry.height} aria-hidden="true" />
      {emptyMessage !== null && <p className={styles.emptyMonth} role="status">{emptyMessage}</p>}
      {activeCell !== null && bounds !== null && anchor !== null && (
        <div
          ref={tooltipRef}
          className={anchor.y > geometry.height / 2 ? styles.tooltipTop : styles.tooltipBottom}
          style={{ left: clampTooltipLeft(anchor.x, tooltipWidth, width) }}
          role="status"
        >
          <strong>{t("densTooltipCell", { south: fmt.number(bounds.south, 2), north: fmt.number(bounds.north, 2), west: fmt.number(bounds.west, 2), east: fmt.number(bounds.east, 2) })}</strong>
          <div className={styles.tooltipRow}>
            <span>{t("densTooltipRaw")}</span>
            <span>{t("densTooltipDetections", { count: fmt.number(activeCell.rawCount) })}</span>
            <span>{mw(activeCell.rawFrp)}</span>
          </div>
          {revealAnswer && (
            <div className={styles.tooltipRow}>
              <span>{t("densTooltipHarmonized")}</span>
              <span>{t("densTooltipRecords", { count: fmt.number(activeCell.harmCount) })}</span>
              <span>{mw(activeCell.harmFrp)}</span>
            </div>
          )}
          <span className={styles.tooltipMeta}>{t("densTooltipArea", { area: fmt.number(cellAreaKm2(activeCell.row, data.cellDeg), 0) })}</span>
        </div>
      )}
    </div>
    <span id={hintId} className="sr-only">{t("densHint")}</span>

    <div className={styles.legend}>
      <p className={styles.legendHead}>
        <strong>{metricTitle}</strong> · {t("densLegendCell", { size: fmt.number(data.cellDeg, 2) })} · {t("densLegendMonth", { month: monthLabel })} · {geometryHarmonized ? t("densTooltipHarmonized") : t("densTooltipRaw")}
      </p>
      <div className={styles.swatches} aria-hidden="true">
        {Array.from({ length: CLASS_COUNT }, (_, i) => <span key={i} className={styles[`class${i + 1}`]} />)}
      </div>
      <p className={styles.note}>
        {scale.method === "quantile" ? t("densLegendQuantile") : t("densLegendFixed")} · {t("densLegendRange", { breaks: edges, max: fmt.number(scale.max, metric === "count" ? 0 : 1) })}
      </p>
      <p className={styles.note}>{t("densLegendNoData")} · {t("densLegendScaleFixed")}</p>
    </div>
    <p className={styles.note}>{sampledNote ?? t("densFullData")}</p>
    <p className={styles.note}>{t("densEqualAngle")}</p>
    <p className={styles.note}>{t("densSource", { source: sourceLabel })}</p>
  </figure>;
}
