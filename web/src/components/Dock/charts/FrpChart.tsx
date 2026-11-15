import { useMemo } from "react";
import { useCopy } from "../../../hooks/useCopy";
import { useRegionData } from "../../../hooks/useRegionData";
import { annualMeanFrp } from "../../../lib/annualFrp";
import { bandPath, linePath, niceTicks, scaleLinear } from "../../../lib/dockCharts";
import styles from "./charts.module.css";
import { ChartNote, formatTick, VIEW_H, VIEW_W, Y_BASE, Y_TOP } from "./shared";

const FRP_COLOR = "#fcb519";
const X0 = 44;
const X1 = 480;

export default function FrpChart() {
  const { t, fmt } = useCopy();
  const { status, hasData, months, cliff, yearly } = useRegionData();

  const points = useMemo(() => {
    const partialFirst = yearly[0]?.partial ? yearly[0].year : null;
    return annualMeanFrp(months)
      .filter((p) => p.year !== partialFirst)
      .sort((a, b) => a.year - b.year);
  }, [months, yearly]);

  if (status === "loading") return <ChartNote text={t("loadingFirms")} />;
  if (status === "error") return <ChartNote text={t("errorLoading")} />;
  if (!hasData || points.length < 2 || points.every((p) => p.mean === null)) {
    return <ChartNote text={t("dockNoData")} />;
  }

  const first = points[0].year;
  const last = points[points.length - 1].year;
  const maxVal = Math.max(...points.map((p) => p.upper ?? p.mean ?? 0));
  const { ticks, top } = niceTicks(maxVal);
  const sx = scaleLinear(first, last, X0, X1);
  const sy = scaleLinear(0, top, Y_BASE, Y_TOP);

  const linePts = points.map((p) => ({ x: sx(p.year), y: p.mean === null ? null : sy(p.mean) }));
  const bandPts = points.map((p) => ({
    x: sx(p.year),
    lo: p.lower === null ? null : sy(p.lower),
    hi: p.upper === null ? null : sy(p.upper),
  }));

  const cliffYear = cliff?.toYear ?? null;
  const markerX = cliffYear !== null && cliffYear >= first && cliffYear <= last ? sx(cliffYear) : null;
  const lead = cliffYear !== null ? t("chartLeadFrp", { year: String(cliffYear) }) : "";

  const tip = (p: (typeof points)[number]): string => {
    if (p.mean === null) return `${p.year}`;
    const base = t("dockFrpValue", { value: fmt.number(p.mean, 1), year: String(p.year) });
    return p.lower !== null && p.upper !== null
      ? `${base} (${fmt.number(p.lower, 1)}–${fmt.number(p.upper, 1)})`
      : base;
  };

  return (
    <div>
      {lead && <p className={styles.lead}>{lead}</p>}
      <svg
        className={styles.svg}
        viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
        role="img"
        aria-label={t("chartAriaFrp", { first: String(first), last: String(last) })}
      >
        {ticks.map((v, i) => (
          <g key={v}>
            <line x1={X0} x2={X1 + 4} y1={sy(v)} y2={sy(v)} className={i === 0 ? styles.base : styles.grid} />
            <text x={X0 - 6} y={sy(v) + 4} textAnchor="end" className={styles.axis}>
              {i === 2 ? `${formatTick(t, v)} MW` : formatTick(t, v)}
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
        <path d={bandPath(bandPts)} fill={FRP_COLOR} fillOpacity={0.18} stroke="none" />
        <path d={linePath(linePts)} className={styles.line} stroke={FRP_COLOR} />
        {points.map((p) =>
          p.mean === null ? null : (
            <circle key={p.year} cx={sx(p.year)} cy={sy(p.mean)} r={5} className={styles.hit}>
              <title>{tip(p)}</title>
            </circle>
          ),
        )}
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
