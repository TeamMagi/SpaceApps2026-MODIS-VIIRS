/**
 * CliffChart — detections per year from the archive API. Hand-written SVG
 * (no chart library), sized to its 220 px panel via a ResizeObserver.
 *
 * Raw also draws MODIS alone (dotted, continuous across the cliff year) under the raw line, so the
 * step is visibly the VIIRS layer stacked on top. It stays mounted and fades out across the merge beat.
 */

import { animate, motion, useMotionValue, useReducedMotion } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { formatApiError } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import { yearTicks } from "../../lib/yearTicks";
import type { YearlyPoint } from "../../types";
import styles from "./CliffChart.module.css";

const PAD_LEFT = 44;
const PAD_RIGHT = 8;
const PAD_TOP = 20;
const PAD_BOTTOM = 20;
const DEFAULT_SIZE = { width: 460, height: 170 };

function niceCeil(value: number): number {
  if (value <= 0) return 10;
  const exp = Math.floor(Math.log10(value));
  const mag = 10 ** exp;
  const norm = value / mag;
  const steps = [1.2, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10];
  const step = steps.find((s) => norm <= s) ?? 10;
  return step * mag;
}

function buildMorphPaths(
  yearly: readonly YearlyPoint[],
  t: number,
  xAt: (index: number) => number,
  yAt: (value: number) => number,
): { solidD: string; partialD: string; partialDots: { x: number; y: number }[] } {
  const n = yearly.length;
  if (n === 0) return { solidD: "", partialD: "", partialDots: [] };

  const val = (p: YearlyPoint) => p.raw + (p.harmonized - p.raw) * t;

  const solidParts: string[] = [];
  const partialParts: string[] = [];
  if (n === 1 && !yearly[0].partial) {
    solidParts.push(`M${xAt(0).toFixed(1)} ${yAt(val(yearly[0])).toFixed(1)}`);
  }
  for (let i = 1; i < n; i++) {
    const prev = yearly[i - 1];
    const curr = yearly[i];
    const seg = `M${xAt(i - 1).toFixed(1)} ${yAt(val(prev)).toFixed(1)} L${xAt(i).toFixed(1)} ${yAt(val(curr)).toFixed(1)}`;
    if (prev.partial || curr.partial) {
      partialParts.push(seg);
    } else {
      solidParts.push(seg);
    }
  }

  const partialDots: { x: number; y: number }[] = [];
  for (let i = 0; i < n; i++) {
    if (yearly[i].partial) {
      partialDots.push({ x: xAt(i), y: yAt(val(yearly[i])) });
    }
  }

  return {
    solidD: solidParts.join(" "),
    partialD: partialParts.join(" "),
    partialDots,
  };
}

function buildModisPaths(
  yearly: readonly YearlyPoint[],
  xAt: (index: number) => number,
  yAt: (value: number) => number,
): { lineD: string; areaD: string } {
  const n = yearly.length;
  if (n === 0) return { lineD: "", areaD: "" };
  const lineParts: string[] = [];
  for (let i = 0; i < n; i++) {
    const x = xAt(i).toFixed(1);
    const y = yAt(yearly[i].rawModis).toFixed(1);
    lineParts.push(`${i === 0 ? "M" : "L"}${x} ${y}`);
  }
  const lineD = lineParts.join(" ");
  const baseline = yAt(0).toFixed(1);
  const areaD = `${lineD} L${xAt(n - 1).toFixed(1)} ${baseline} L${xAt(0).toFixed(1)} ${baseline} Z`;
  return { lineD, areaD };
}

export default function CliffChart() {
  const { t, fmt } = useCopy();
  const { geometryHarmonized, revealAnswer, durations } = useHarmonizeTransition((v) => ({
    geometryHarmonized: v.geometryHarmonized,
    revealAnswer: v.revealAnswer,
    durations: v.durations,
  }));
  const { status, error, retry, yearly, cliff, firstYear, lastYear } = useRegionData();
  const reducedMotion = useReducedMotion() ?? false;

  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState(DEFAULT_SIZE);
  const [activeIndex, setActiveIndex] = useState<number | null>(null);

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
  }, [yearly.length]);

  const plotW = Math.max(40, size.width - PAD_LEFT - PAD_RIGHT);
  const plotH = Math.max(40, size.height - PAD_TOP - PAD_BOTTOM);
  const n = yearly.length;

  const niceMax = useMemo(() => {
    const peak = yearly.reduce((acc, p) => Math.max(acc, p.raw, p.harmonized), 1);
    return niceCeil(peak * 1.06);
  }, [yearly]);

  const xAt = useCallback(
    (index: number) => PAD_LEFT + ((index + 0.5) / Math.max(n, 1)) * plotW,
    [n, plotW],
  );
  const yAt = useCallback(
    (value: number) => PAD_TOP + plotH - (value / Math.max(niceMax, 1)) * plotH,
    [niceMax, plotH],
  );

  const solidPathRef = useRef<SVGPathElement | null>(null);
  const partialPathRef = useRef<SVGPathElement | null>(null);
  const partialDotsGroupRef = useRef<SVGGElement | null>(null);
  const morph = useMotionValue(geometryHarmonized ? 1 : 0);
  const reducedOpacity = useMotionValue(1);

  const applyMorphToDom = useCallback(
    (tValue: number) => {
      const paths = buildMorphPaths(yearly, tValue, xAt, yAt);
      if (solidPathRef.current) solidPathRef.current.setAttribute("d", paths.solidD);
      if (partialPathRef.current) partialPathRef.current.setAttribute("d", paths.partialD);
      if (partialDotsGroupRef.current) {
        const circles = partialDotsGroupRef.current.querySelectorAll("circle");
        paths.partialDots.forEach((dot, idx) => {
          const c = circles[idx];
          if (c) {
            c.setAttribute("cx", dot.x.toFixed(1));
            c.setAttribute("cy", dot.y.toFixed(1));
          }
        });
      }
    },
    [yearly, xAt, yAt],
  );

  useEffect(() => {
    applyMorphToDom(morph.get());
    const unsub = morph.on("change", applyMorphToDom);
    return unsub;
  }, [morph, applyMorphToDom]);

  useEffect(() => {
    const target = geometryHarmonized ? 1 : 0;
    if (reducedMotion) {
      morph.jump(target);
      applyMorphToDom(target);
      if (durations.merge > 0) {
        reducedOpacity.set(0.15);
        const controls = animate(reducedOpacity, 1, {
          duration: durations.merge / 1000,
          ease: [0.2, 0, 0, 1],
        });
        return () => controls.stop();
      }
      reducedOpacity.jump(1);
      return;
    }
    reducedOpacity.jump(1);
    if (durations.merge <= 0) {
      morph.jump(target);
      applyMorphToDom(target);
      return;
    }
    const controls = animate(morph, target, {
      duration: durations.merge / 1000,
      ease: [0.2, 0, 0, 1],
    });
    return () => controls.stop();
  }, [geometryHarmonized, reducedMotion, durations.merge, morph, reducedOpacity, applyMorphToDom]);

  const initialPaths = useMemo(
    () => buildMorphPaths(yearly, geometryHarmonized ? 1 : 0, xAt, yAt),
    [yearly, geometryHarmonized, xAt, yAt],
  );
  const modisPaths = useMemo(() => buildModisPaths(yearly, xAt, yAt), [yearly, xAt, yAt]);

  const cliffYear = cliff?.toYear ?? null;
  const cliffIndex = cliffYear !== null ? yearly.findIndex((p) => p.year === cliffYear) : -1;
  const cliffX = cliffIndex >= 0 ? xAt(cliffIndex) : null;
  const hasAnyPartial = useMemo(() => yearly.some((p) => p.partial), [yearly]);

  const yTicks = [0, niceMax / 2, niceMax];
  const xTickYears = useMemo(() => {
    if (n === 0) return [];
    const ticks = yearTicks(yearly[0].year, yearly[n - 1].year, cliffYear);
    return yearly.map((p, index) => ({ year: p.year, index })).filter((item) => ticks.has(item.year));
  }, [yearly, n, cliffYear]);

  const handlePointerMove = (e: PointerEvent<SVGSVGElement>) => {
    if (n === 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const relX = e.clientX - rect.left - PAD_LEFT;
    const clamped = Math.max(0, Math.min(plotW - 0.001, relX));
    const idx = Math.min(n - 1, Math.max(0, Math.floor((clamped / plotW) * n)));
    setActiveIndex(idx);
  };

  const handleKeyDown = (e: KeyboardEvent<SVGSVGElement>) => {
    if (n === 0) return;
    if (e.key === "ArrowRight") {
      e.preventDefault();
      setActiveIndex((prev) => (prev === null ? 0 : Math.min(n - 1, prev + 1)));
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      setActiveIndex((prev) => (prev === null ? n - 1 : Math.max(0, prev - 1)));
    } else if (e.key === "Escape") {
      setActiveIndex(null);
    }
  };

  const activePoint = activeIndex !== null ? yearly[activeIndex] ?? null : null;
  const activeValue = activePoint ? (revealAnswer ? activePoint.harmonized : activePoint.raw) : 0;
  const activeX = activeIndex !== null ? xAt(activeIndex) : 0;
  const activeY = activePoint ? yAt(activeValue) : 0;

  const tooltipLeft = Math.max(85, Math.min(size.width - 85, activeX));
  const tooltipTop = Math.max(44, activeY);
  const chartTitle =
    firstYear !== null && lastYear !== null
      ? t("cliffChartTitle", { first: String(firstYear), last: String(lastYear) })
      : t("cliffChartLabel");

  if (status === "error") {
    return (
      <figure className={styles.chart} aria-label={t("cliffChartLabel")}>
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
      <figure className={styles.chart} aria-label={t("cliffChartLabel")}>
        <div className={styles.header}>
          <figcaption className={styles.title}>{chartTitle}</figcaption>
        </div>
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{t("loadingFirms")}</p>
        </div>
      </figure>
    );
  }

  if (status === "empty" || n === 0) {
    return (
      <figure className={styles.chart} aria-label={t("cliffChartLabel")}>
        <div className={styles.header}>
          <figcaption className={styles.title}>{chartTitle}</figcaption>
        </div>
        <div className={styles.stateBox} role="status">
          <p className={styles.stateText}>{t("emptyState")}</p>
        </div>
      </figure>
    );
  }

  return (
    <figure className={styles.chart} aria-label={t("cliffChartLabel")} data-loading={status === "loading" ? "true" : undefined}>
      <div className={styles.header}>
        <figcaption className={styles.title}>{chartTitle}</figcaption>
        <div className={styles.legend}>
          {status === "loading" && <span className={styles.loadingTag}>{t("loadingFirms")}</span>}
          {revealAnswer ? (
            <span className={styles.legendItem}>
              <span className={styles.swatchLine} aria-hidden="true" />
              {t("cliffLegendHarmonized")}
            </span>
          ) : (
            <>
              <span className={styles.legendItem}>
                <span className={styles.swatchLine} aria-hidden="true" />
                {t("cliffLegendRaw")}
              </span>
              <span className={styles.legendItem}>
                <span className={styles.swatchModis} aria-hidden="true" />
                {t("cliffLegendModis")}
              </span>
            </>
          )}
          {hasAnyPartial && (
            <span className={styles.legendItem}>
              <span className={styles.swatchPartial} aria-hidden="true" />
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
          aria-label={t("cliffChartLabel")}
          tabIndex={0}
          onPointerMove={handlePointerMove}
          onPointerLeave={() => setActiveIndex(null)}
          onBlur={() => setActiveIndex(null)}
          onKeyDown={handleKeyDown}
        >
          {yTicks.map((tick) => {
            const y = yAt(tick);
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
                  {fmt.number(tick)}
                </text>
              </g>
            );
          })}

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

          {cliffYear !== null && cliffX !== null && (
            <g>
              <line x1={cliffX} y1={PAD_TOP - 4} x2={cliffX} y2={PAD_TOP + plotH} className={styles.cliffLine} />
              <text x={cliffX + 5} y={PAD_TOP - 6} className={styles.cliffLabel}>
                {t("cliffMarker2012", { year: String(cliffYear) })}
              </text>
            </g>
          )}

          {n > 0 && (
            <g className={styles.modisGroup} data-hidden={geometryHarmonized} aria-hidden="true">
              <path className={styles.modisArea} d={modisPaths.areaD} />
              <path className={styles.modisLine} d={modisPaths.lineD} />
              {yearly.map((p, i) =>
                p.partial ? null : (
                  <circle
                    key={`m-${p.year}`}
                    cx={xAt(i)}
                    cy={yAt(p.rawModis)}
                    r={2.1}
                    className={styles.modisDot}
                  />
                ),
              )}
              {yearly.map((p, i) =>
                (cliffYear !== null ? p.year >= cliffYear : p.rawViirs > 0) && !p.partial ? (
                  <rect
                    key={`v-${p.year}`}
                    x={xAt(i) - 1.75}
                    y={yAt(p.raw) - 1.75}
                    width={3.5}
                    height={3.5}
                    className={styles.viirsDot}
                  />
                ) : null,
              )}
            </g>
          )}

          <motion.g key="series-group" style={{ opacity: reducedOpacity }}>
            <path ref={solidPathRef} className={styles.seriesLine} d={initialPaths.solidD} />
            {hasAnyPartial && <path ref={partialPathRef} className={styles.partialLine} d={initialPaths.partialD} />}
            <g ref={partialDotsGroupRef}>
              {initialPaths.partialDots.map((dot, idx) => (
                <circle
                  key={`pdot-${idx}`}
                  cx={dot.x}
                  cy={dot.y}
                  r={3.5}
                  className={styles.partialDot}
                />
              ))}
            </g>
          </motion.g>

          {activePoint && (
            <g aria-hidden="true">
              <line x1={activeX} y1={PAD_TOP} x2={activeX} y2={PAD_TOP + plotH} className={styles.hoverGuide} />
              <circle cx={activeX} cy={activeY} r={3.5} className={styles.hoverDot} />
            </g>
          )}
        </svg>

        {activePoint && (
          <div className={styles.tooltip} style={{ left: tooltipLeft, top: tooltipTop }} role="status">
            <span className={styles.tooltipYear}>
              {activePoint.partial ? t("cliffPartialYear", { year: String(activePoint.year) }) : activePoint.year}
            </span>
            <span>{fmt.number(activeValue)}</span>
            {!revealAnswer && (
              <span className={styles.tooltipMeta}>
                {" "}
                ·{" "}
                {t("countRawSubline", {
                  modis: fmt.number(activePoint.rawModis),
                  viirs: fmt.number(activePoint.rawViirs),
                })}
              </span>
            )}
          </div>
        )}
      </div>

      <table className={styles.srOnly}>
        <caption>{chartTitle}</caption>
        <thead>
          <tr>
            <th scope="col">{t("cliffTableYear")}</th>
            <th scope="col">{revealAnswer ? t("cliffLegendHarmonized") : t("cliffLegendRaw")}</th>
          </tr>
        </thead>
        <tbody>
          {yearly.map((p) => (
            <tr key={p.year}>
              <th scope="row">{p.partial ? t("cliffPartialYear", { year: String(p.year) }) : p.year}</th>
              <td>{fmt.number(revealAnswer ? p.harmonized : p.raw)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  );
}
