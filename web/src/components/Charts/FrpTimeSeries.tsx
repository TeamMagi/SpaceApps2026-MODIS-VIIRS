/**
 * FrpTimeSeries — monthly FRP per sensor and harmonized, hand-written SVG (no chart library).
 *
 * Raw: MODIS dotted + hollow circles, VIIRS solid + filled squares. Harmonized fades in the shared
 * clock's merge beat (geometryHarmonized) with its uncertainty band while the raw lines stay as a
 * faint reference. Gaps never bridge and unobservable months are never drawn as zero; harmonized
 * numbers in the tooltip wait for revealAnswer.
 */

import { animate, useMotionValue, useReducedMotion } from "motion/react";
import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import {
  clampRange, clampTooltipLeft, densify, fullRange, isNotComparable, MIN_ZOOM_MONTHS, pathSegments, seriesValues,
  type FrpMetric, type FrpSeriesPoint, type SeriesValues,
} from "../../lib/frpSeries";
import styles from "./FrpTimeSeries.module.css";

interface FrpTimeSeriesProps {
  points: readonly FrpSeriesPoint[];
  title?: string;
  sourceLabel: string;
}

const PAD_LEFT = 44;
const PAD_RIGHT = 10;
const PAD_TOP = 10;
const PAD_BOTTOM = 22;
const MIN_TICK_GAP_PX = 40;
const MAX_MARKER_MONTHS = 48;
const DEFAULT_SIZE = { width: 640, height: 260 };
const RAW_REFERENCE_OPACITY = 0.35;

function niceStep(max: number, target: number): number {
  const raw = Math.max(max, 1e-9) / target;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const norm = raw / mag;
  return (norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 5 ? 5 : 10) * mag;
}

function yearStep(pxPerMonth: number): number {
  const steps = [1, 2, 5, 10, 20];
  return steps.find(step => step * 12 * pxPerMonth >= MIN_TICK_GAP_PX) ?? 20;
}

const line = (segment: readonly { x: number; y: number }[]): string =>
  segment.map((p, i) => `${i === 0 ? "M" : "L"}${p.x.toFixed(1)} ${p.y.toFixed(1)}`).join(" ");

export default function FrpTimeSeries({ points, title, sourceLabel }: FrpTimeSeriesProps) {
  const { t, fmt } = useCopy();
  const { geometryHarmonized, revealAnswer, durations } = useHarmonizeTransition(v => ({
    geometryHarmonized: v.geometryHarmonized,
    revealAnswer: v.revealAnswer,
    durations: v.durations,
  }));
  const reducedMotion = useReducedMotion() ?? false;
  const hintId = useId();
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const tooltipRef = useRef<HTMLDivElement | null>(null);
  const [tooltipWidth, setTooltipWidth] = useState(0);
  const rawGroupRef = useRef<SVGGElement | null>(null);
  const harmGroupRef = useRef<SVGGElement | null>(null);
  const [size, setSize] = useState(DEFAULT_SIZE);
  const [metric, setMetric] = useState<FrpMetric>("perDetection");
  const [zoom, setZoom] = useState<{ start: number; end: number } | null>(null);
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  useLayoutEffect(() => {
    const width = tooltipRef.current?.offsetWidth ?? 0;
    setTooltipWidth(prev => (prev === width ? prev : width));
  });

  const dense = useMemo(() => densify(points), [points]);
  const length = dense.length;
  const range = useMemo(() => clampRange(zoom ?? fullRange(length), length), [zoom, length]);
  const visible = useMemo(() => dense.slice(range.start, range.end + 1), [dense, range]);
  const values = useMemo(() => visible.map(p => seriesValues(p, metric)), [visible, metric]);
  const n = visible.length;

  useEffect(() => { setZoom(null); setActiveIndex(null); }, [dense]);

  useEffect(() => {
    const el = wrapRef.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const update = () => {
      const rect = el.getBoundingClientRect();
      const width = Math.round(rect.width);
      const height = Math.round(rect.height);
      if (width > 0 && height > 0) setSize(prev => (prev.width === width && prev.height === height ? prev : { width, height }));
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, [length]);

  const morph = useMotionValue(geometryHarmonized ? 1 : 0);
  const applyMorph = useCallback((value: number) => {
    rawGroupRef.current?.setAttribute("opacity", String(1 - (1 - RAW_REFERENCE_OPACITY) * value));
    harmGroupRef.current?.setAttribute("opacity", String(value));
  }, []);
  useEffect(() => {
    applyMorph(morph.get());
    return morph.on("change", applyMorph);
  }, [morph, applyMorph, length]);
  useEffect(() => {
    const target = geometryHarmonized ? 1 : 0;
    if (reducedMotion || durations.merge <= 0) {
      morph.jump(target);
      applyMorph(target);
      return;
    }
    const controls = animate(morph, target, { duration: durations.merge / 1000, ease: [0.2, 0, 0, 1] });
    return () => controls.stop();
  }, [geometryHarmonized, reducedMotion, durations.merge, morph, applyMorph]);

  const plotW = Math.max(40, size.width - PAD_LEFT - PAD_RIGHT);
  const plotH = Math.max(40, size.height - PAD_TOP - PAD_BOTTOM);
  const colW = plotW / Math.max(n, 1);
  const xAt = useCallback((i: number) => PAD_LEFT + (i + 0.5) * colW, [colW]);

  const { yMax, yTicks } = useMemo(() => {
    let peak = 0;
    for (const v of values) {
      for (const value of [v.modis, v.viirs]) if (value !== null && value > peak) peak = value;
      if (geometryHarmonized) for (const value of [v.harmonized, v.harmonizedUpper]) if (value !== null && value > peak) peak = value;
    }
    const step = niceStep(peak * 1.05, 4);
    const top = Math.max(step, Math.ceil((peak * 1.05) / step) * step);
    const ticks: number[] = [];
    for (let tick = 0; tick <= top + step / 2; tick += step) ticks.push(tick);
    return { yMax: top, yTicks: ticks };
  }, [values, geometryHarmonized]);
  const yAt = useCallback((value: number) => PAD_TOP + plotH - (value / yMax) * plotH, [plotH, yMax]);

  const showMarkers = n <= MAX_MARKER_MONTHS;
  const series = useMemo(() => {
    const pick = (get: (v: SeriesValues) => number | null) =>
      pathSegments(values.map((v, i) => ({ x: xAt(i), y: get(v) === null ? null : yAt(get(v) as number) })));
    const bothBounds = (v: SeriesValues) => v.harmonizedLower !== null && v.harmonizedUpper !== null;
    const upper = pathSegments(values.map((v, i) => ({ x: xAt(i), y: bothBounds(v) ? yAt(v.harmonizedUpper as number) : null })));
    const lower = pathSegments(values.map((v, i) => ({ x: xAt(i), y: bothBounds(v) ? yAt(v.harmonizedLower as number) : null })));
    const bandFill = upper.map((seg, k) => `${line(seg)} ${[...lower[k]].reverse().map(p => `L${p.x.toFixed(1)} ${p.y.toFixed(1)}`).join(" ")} Z`);
    return {
      modis: pick(v => v.modis), viirs: pick(v => v.viirs), harmonized: pick(v => v.harmonized),
      bandFill, bandEdges: [...upper, ...lower].map(line),
    };
  }, [values, xAt, yAt]);

  const xTicks = useMemo(() => {
    const step = yearStep(colW);
    return visible
      .map((p, i) => ({ p, i }))
      .filter(({ p }) => p.month === 1 && p.year % step === 0)
      .map(({ p, i }) => ({ year: p.year, x: xAt(i) }));
  }, [visible, colW, xAt]);

  const moveActive = (index: number) => setActiveIndex(Math.max(0, Math.min(n - 1, index)));
  const onPointer = (event: PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    moveActive(Math.floor((event.clientX - rect.left - PAD_LEFT) / colW));
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const keys: Record<string, number> = { ArrowLeft: (activeIndex ?? n) - 1, ArrowRight: (activeIndex ?? -1) + 1, Home: 0, End: n - 1 };
    if (!(event.key in keys)) return;
    event.preventDefault();
    moveActive(keys[event.key]);
  };

  if (length === 0) {
    return <figure className={styles.chart} aria-label={t("frpChartLabel")}>
      <figcaption className={styles.title}>{title ?? t("frpTitle")}</figcaption>
      <p className={styles.empty} role="status">{t("frpEmpty")}</p>
    </figure>;
  }

  const active = activeIndex === null ? null : visible[activeIndex];
  const activeValues = activeIndex === null ? null : values[activeIndex];
  const mw = (value: number) => t("frpValueMw", { value: fmt.number(value, 1) });
  const tooltipLeft = activeIndex === null ? 0 : clampTooltipLeft(xAt(activeIndex), tooltipWidth, size.width);
  const bandNote = activeValues !== null && activeValues.harmonizedLower !== null && activeValues.harmonizedUpper !== null
    ? t("frpBandRange", { lower: fmt.number(activeValues.harmonizedLower, 1), upper: fmt.number(activeValues.harmonizedUpper, 1) })
    : null;

  const sensorRow = (label: string, symbol: string, sensor: FrpSeriesPoint["modis"], value: number | null) => (
    <div className={styles.tooltipRow}>
      <span aria-hidden="true">{symbol}</span>
      <span>{label}</span>
      <span>{sensor === null ? t("frpNotObservable") : value === null ? t("frpNoData") : mw(value)}</span>
      {sensor !== null && <span className={styles.tooltipMeta}>{t("frpDetections", { count: fmt.number(sensor.count) })}</span>}
    </div>
  );

  const rangeLabel = `${fmt.monthYear(visible[0])} – ${fmt.monthYear(visible[n - 1])}`;
  const axisTitle = metric === "perDetection" ? t("frpAxisPerDetection") : t("frpAxisTotal");

  return <figure className={styles.chart} aria-label={t("frpChartLabel")}>
    <div className={styles.header}>
      <figcaption className={styles.title}>{title ?? t("frpTitle")}</figcaption>
      <div className={styles.segmented} role="group" aria-label={t("frpMetricGroup")}>
        {(["perDetection", "total"] as const).map(option => (
          <button key={option} type="button" aria-pressed={metric === option} onClick={() => setMetric(option)}>
            {option === "perDetection" ? t("frpMetricPerDetection") : t("frpMetricTotal")}
          </button>
        ))}
      </div>
    </div>

    <div className={styles.zoom} role="group" aria-label={t("frpZoomGroup")}>
      <label>{t("frpZoomStart")}
        <select value={range.start} onChange={e => setZoom(clampRange({ start: Number(e.target.value), end: range.end }, length))}>
          {dense.map((p, i) => <option key={i} value={i}>{fmt.monthYear(p)}</option>)}
        </select>
      </label>
      <label>{t("frpZoomEnd")}
        <select value={range.end} onChange={e => {
          const end = Number(e.target.value);
          setZoom(clampRange({ start: Math.min(range.start, end - (MIN_ZOOM_MONTHS - 1)), end }, length));
        }}>
          {dense.map((p, i) => <option key={i} value={i}>{fmt.monthYear(p)}</option>)}
        </select>
      </label>
      <button type="button" onClick={() => setZoom(null)} disabled={zoom === null}>{t("frpZoomReset")}</button>
    </div>

    <p className={styles.axisTitle}>{axisTitle}</p>
    <div
      ref={wrapRef}
      className={styles.canvasWrap}
      tabIndex={0}
      role="group"
      aria-label={`${t("frpChartLabel")} · ${rangeLabel}`}
      aria-describedby={hintId}
      onPointerMove={onPointer}
      onPointerDown={onPointer}
      onPointerLeave={() => setActiveIndex(null)}
      onFocus={() => setActiveIndex(prev => prev ?? n - 1)}
      onBlur={() => setActiveIndex(null)}
      onKeyDown={onKeyDown}
    >
      <svg className={styles.svg} width={size.width} height={size.height} aria-hidden="true">
        {yTicks.map(tick => (
          <g key={tick}>
            <line x1={PAD_LEFT} x2={PAD_LEFT + plotW} y1={yAt(tick)} y2={yAt(tick)} className={tick === 0 ? styles.axisLine : styles.gridLine} />
            <text x={PAD_LEFT - 6} y={yAt(tick) + 3} textAnchor="end" className={styles.axisLabel}>{fmt.number(tick)}</text>
          </g>
        ))}
        {xTicks.map(tick => (
          <g key={tick.year}>
            <line x1={tick.x} x2={tick.x} y1={PAD_TOP + plotH} y2={PAD_TOP + plotH + 4} className={styles.axisLine} />
            <text x={tick.x} y={PAD_TOP + plotH + 16} textAnchor="middle" className={styles.axisLabel}>{tick.year}</text>
          </g>
        ))}

        <g ref={harmGroupRef}>
          {series.bandFill.map((d, i) => <path key={`bf${i}`} d={d} className={styles.bandFill} />)}
          {series.bandEdges.map((d, i) => <path key={`be${i}`} d={d} className={styles.bandEdge} />)}
          {series.harmonized.map((seg, i) => seg.length === 1
            ? <circle key={`h${i}`} cx={seg[0].x} cy={seg[0].y} r={2} className={styles.harmDot} />
            : <path key={`h${i}`} d={line(seg)} className={styles.harmLine} />)}
          {showMarkers && series.harmonized.flat().map((p, i) => <circle key={`hm${i}`} cx={p.x} cy={p.y} r={2} className={styles.harmDot} />)}
        </g>

        <g ref={rawGroupRef}>
          {series.modis.map((seg, i) => seg.length === 1
            ? <circle key={`m${i}`} cx={seg[0].x} cy={seg[0].y} r={2.2} className={styles.modisMarker} />
            : <path key={`m${i}`} d={line(seg)} className={styles.modisLine} />)}
          {series.viirs.map((seg, i) => seg.length === 1
            ? <rect key={`v${i}`} x={seg[0].x - 2} y={seg[0].y - 2} width={4} height={4} className={styles.viirsMarker} />
            : <path key={`v${i}`} d={line(seg)} className={styles.viirsLine} />)}
          {showMarkers && series.modis.flat().map((p, i) => <circle key={`mm${i}`} cx={p.x} cy={p.y} r={2.2} className={styles.modisMarker} />)}
          {showMarkers && series.viirs.flat().map((p, i) => <rect key={`vm${i}`} x={p.x - 2} y={p.y - 2} width={4} height={4} className={styles.viirsMarker} />)}
        </g>

        {activeIndex !== null && <line x1={xAt(activeIndex)} x2={xAt(activeIndex)} y1={PAD_TOP} y2={PAD_TOP + plotH} className={styles.hoverGuide} />}
      </svg>

      {active !== null && activeValues !== null && (
        <div ref={tooltipRef} className={styles.tooltip} style={{ left: tooltipLeft }} role="status">
          <strong>{fmt.monthYear(active)}</strong>
          {sensorRow(t("frpLegendModis"), "○", active.modis, activeValues.modis)}
          {sensorRow(t("frpLegendViirs"), "■", active.viirs, activeValues.viirs)}
          {revealAnswer && <>
            <div className={styles.tooltipRow}>
              <span aria-hidden="true">●</span>
              <span>{t("frpLegendHarmonized")}</span>
              <span>{activeValues.harmonized !== null ? mw(activeValues.harmonized) : isNotComparable(active, metric) ? t("frpNotComparable") : t("frpNoData")}</span>
              {active.harmonized !== null && (
                <span className={styles.tooltipMeta}>
                  {active.harmonized.countNormalized === null
                    ? t("frpDetections", { count: fmt.number(active.harmonized.count) })
                    : t("frpHarmonizedDetections", { count: fmt.number(active.harmonized.count), normalized: fmt.number(active.harmonized.countNormalized, 1) })}
                </span>
              )}
            </div>
            {bandNote !== null && <div className={styles.tooltipMeta}>{bandNote}</div>}
          </>}
        </div>
      )}
    </div>
    <span id={hintId} className="sr-only">{t("frpTooltipHint")}</span>

    <ul className={styles.legend}>
      <li><svg width="28" height="10" aria-hidden="true"><line x1="0" x2="28" y1="5" y2="5" className={styles.modisLine} /><circle cx="14" cy="5" r="2.5" className={styles.modisMarker} /></svg>○ {t("frpLegendModis")}</li>
      <li><svg width="28" height="10" aria-hidden="true"><line x1="0" x2="28" y1="5" y2="5" className={styles.viirsLine} /><rect x="11" y="2" width="6" height="6" className={styles.viirsMarker} /></svg>■ {t("frpLegendViirs")}</li>
      {geometryHarmonized && <>
        <li><svg width="28" height="10" aria-hidden="true"><line x1="0" x2="28" y1="5" y2="5" className={styles.harmLine} /></svg>{t("frpLegendHarmonized")}</li>
        <li><svg width="28" height="10" aria-hidden="true"><rect x="0" y="1" width="28" height="8" className={styles.bandFill} /><line x1="0" x2="28" y1="1" y2="1" className={styles.bandEdge} /><line x1="0" x2="28" y1="9" y2="9" className={styles.bandEdge} /></svg>{t("frpLegendBand")}</li>
      </>}
    </ul>
    <p className={styles.note}>{metric === "perDetection" ? t("frpDefPerDetection") : t("frpDefTotal")}</p>
    {metric === "perDetection" && <p className={styles.note}>{t("frpNotePerDetection")}</p>}
    {geometryHarmonized && <p className={styles.note}>{t("frpRawReference")}</p>}
    <p className={styles.note}>{t("frpSource", { source: sourceLabel })}</p>
  </figure>;
}
