import { useMemo } from "react";
import { useCopy } from "../../../hooks/useCopy";
import { useHarmonizeTransition } from "../../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../../hooks/useRegionData";
import { linePath, niceTicks, scaleLinear } from "../../../lib/dockCharts";
import styles from "./charts.module.css";
import { ChartNote, formatTick, VIEW_H, VIEW_W, Y_BASE, Y_TOP } from "./shared";

const RAW_COLOR = "#f37819";
const X0 = 44;
const X1 = 428;
const LABEL_GAP = 13;

export default function DetectionsChart() {
  const { t, fmt } = useCopy();
  const { geometryHarmonized } = useHarmonizeTransition((v) => ({ geometryHarmonized: v.geometryHarmonized }));
  const { status, hasData, yearly, cliff } = useRegionData();

  const years = useMemo(() => (yearly[0]?.partial ? yearly.slice(1) : yearly), [yearly]);

  if (status === "loading") return <ChartNote text={t("loadingFirms")} />;
  if (status === "error") return <ChartNote text={t("errorLoading")} />;
  if (!hasData || years.length < 2) return <ChartNote text={t("dockNoData")} />;

  const first = years[0].year;
  const last = years[years.length - 1].year;
  const maxVal = Math.max(...years.map((y) => Math.max(y.raw, y.harmonized)));
  const { ticks, top } = niceTicks(maxVal);
  const sx = scaleLinear(first, last, X0, X1);
  const sy = scaleLinear(0, top, Y_BASE, Y_TOP);

  const rawPts = years.map((y) => ({ x: sx(y.year), y: sy(y.raw) }));
  const harmPts = years.map((y) => ({ x: sx(y.year), y: sy(y.harmonized) }));
  let rawEnd = rawPts[rawPts.length - 1].y;
  let harmEnd = harmPts[harmPts.length - 1].y;
  if (Math.abs(rawEnd - harmEnd) < LABEL_GAP) {
    const mid = (rawEnd + harmEnd) / 2;
    const rawAbove = rawEnd <= harmEnd;
    rawEnd = rawAbove ? mid - LABEL_GAP / 2 : mid + LABEL_GAP / 2;
    harmEnd = rawAbove ? mid + LABEL_GAP / 2 : mid - LABEL_GAP / 2;
  }

  const cliffYear = cliff?.toYear ?? null;
  const markerX = cliffYear !== null && cliffYear >= first && cliffYear <= last ? sx(cliffYear) : null;
  const lead = cliffYear !== null ? t("chartLeadDetections", { year: String(cliffYear) }) : "";
  const rawLabel = t("chartRaw");
  const harmLabel = t("chartHarmonized");

  return (
    <div>
      {lead && <p className={styles.lead}>{lead}</p>}
      <svg
        className={styles.svg}
        viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
        role="img"
        aria-label={t("chartAriaDetections", { first: String(first), last: String(last) })}
      >
        {ticks.map((v, i) => (
          <g key={v}>
            <line x1={X0} x2={X1 + 4} y1={sy(v)} y2={sy(v)} className={i === 0 ? styles.base : styles.grid} />
            <text x={X0 - 6} y={sy(v) + 4} textAnchor="end" className={styles.axis}>
              {formatTick(t, v)}
            </text>
          </g>
        ))}
        {markerX !== null && cliffYear !== null && (
          <g>
            <line x1={markerX} x2={markerX} y1={14} y2={Y_BASE} className={styles.marker} />
            <text x={markerX} y={10} textAnchor="middle" className={styles.markerLabel}>
              {t("chartViirsAdded", { year: String(cliffYear) })}
            </text>
          </g>
        )}
        <path
          d={linePath(harmPts)}
          className={styles.line}
          stroke="var(--color-accent)"
          opacity={geometryHarmonized ? 1 : 0.5}
        />
        <path d={linePath(rawPts)} className={styles.line} stroke={RAW_COLOR} opacity={geometryHarmonized ? 0.5 : 1} />
        {years.map((y) => (
          <g key={y.year}>
            <circle cx={sx(y.year)} cy={sy(y.raw)} r={5} className={styles.hit}>
              <title>{`${y.year} · ${rawLabel}: ${fmt.number(y.raw)}`}</title>
            </circle>
            <circle cx={sx(y.year)} cy={sy(y.harmonized)} r={5} className={styles.hit}>
              <title>{`${y.year} · ${harmLabel}: ${fmt.number(y.harmonized)}`}</title>
            </circle>
          </g>
        ))}
        <text x={X1 + 8} y={rawEnd + 4} fill={RAW_COLOR} className={styles.endLabel}>
          {rawLabel}
        </text>
        <text x={X1 + 8} y={harmEnd + 4} fill="var(--color-accent)" className={styles.endLabel}>
          {harmLabel}
        </text>
        <text x={X0} y={232} className={styles.axis}>
          {first}
        </text>
        <text x={X1} y={232} textAnchor="end" className={styles.axis}>
          {last}
        </text>
      </svg>
    </div>
  );
}
