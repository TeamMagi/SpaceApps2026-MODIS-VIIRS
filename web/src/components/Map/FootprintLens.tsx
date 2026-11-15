import { memo, useMemo, useState } from "react";
import { useCopy } from "../../hooks/useCopy";
import {
  FLARE_CHRONOS_AREA_RATIO,
  MATCH_WINDOW_SECS,
  MODIS_EDGE_SCAN_ANGLE_DEG,
  MODIS_MID_SCAN_ANGLE_DEG,
  VIIRS_PIXEL_KM,
  computeModisFootprint,
  computeViirsTiling,
  detectionThresholds,
  findModisMember,
  findViirsInFootprint,
  modisFootprintAtScanAngle,
  thresholdFraction,
  vzaDegFromFactor,
} from "../../lib/footprint";
import { HotspotSource, type HarmonizedPoint, type RawPoint } from "../../types";
import styles from "./FootprintLens.module.css";

export interface FootprintLensProps {
  readonly id: string;
  readonly rawPoint: RawPoint | null;
  readonly harmPoint: HarmonizedPoint | null;
  readonly revealAnswer: boolean;
  readonly rawPoints: readonly RawPoint[];
}

type Preset = "point" | "nadir" | "mid" | "edge";

const PRESET_ANGLE: Readonly<Record<Exclude<Preset, "point">, number>> = {
  nadir: 0,
  mid: MODIS_MID_SCAN_ANGLE_DEG,
  edge: MODIS_EDGE_SCAN_ANGLE_DEG,
};

const SVG_W = 240;
const SVG_H = 116;
const CX = SVG_W / 2;
const CY = 52;
/** 44 px per km keeps the 55° edge pixel (~4.8 × 2.0 km) inside the 240 px canvas. */
const PX_PER_KM = 44;
const VIIRS_PX = VIIRS_PIXEL_KM * PX_PER_KM;

const BAR_W = 240;
const BAR_H = 18;
const BAR_X = 4;
const BAR_SPAN = BAR_W - BAR_X * 2;

function FootprintLens({ id, rawPoint, harmPoint, revealAnswer, rawPoints }: FootprintLensProps) {
  const { t, fmt } = useCopy();

  // Raw mode never reads the harmonized point: no VZA factor, no fusion links, no source.
  const harm = revealAnswer ? harmPoint : null;
  const modisMember = useMemo(
    () => (harm ? findModisMember(harm.id, rawPoints) : null),
    [harm, rawPoints],
  );
  const center = harm ? (modisMember ?? harm) : rawPoint;
  const pointVzaFactor =
    harm && typeof harm.vzaFactor === "number" && Number.isFinite(harm.vzaFactor) && harm.vzaFactor > 0
      ? harm.vzaFactor
      : null;

  const [preset, setPreset] = useState<Preset | null>(null);
  const effectivePreset: Preset =
    preset === null || (preset === "point" && pointVzaFactor === null)
      ? pointVzaFactor !== null
        ? "point"
        : "edge"
      : preset;

  const footprint = useMemo(
    () =>
      effectivePreset === "point"
        ? computeModisFootprint({ vzaFactor: pointVzaFactor })
        : modisFootprintAtScanAngle(PRESET_ANGLE[effectivePreset]),
    [effectivePreset, pointVzaFactor],
  );
  const tiling = useMemo(() => computeViirsTiling(footprint), [footprint]);

  const found = useMemo(
    () =>
      center
        ? findViirsInFootprint(center, footprint, rawPoints, { linkedHarmonizedId: harm?.id ?? null })
        : [],
    [center, footprint, rawPoints, harm],
  );

  if (!center) return null;

  const thresholds = detectionThresholds(footprint.areaKm2, center.daynight);
  const insideCount = found.reduce((n, f) => (f.inside ? n + 1 : n), 0);
  const linkedCount = found.reduce((n, f) => (f.linked ? n + 1 : n), 0);

  const scan = fmt.number(footprint.scanKm, 1);
  const track = fmt.number(footprint.trackKm, 1);
  const area = fmt.number(footprint.areaKm2, footprint.areaKm2 < 2 ? 2 : 1);
  const vza = fmt.number(footprint.vzaDeg, 0);
  const pixels = fmt.number(tiling.pixelCount, 0);
  const ratio = fmt.number(tiling.areaRatio, 0);
  const inside = fmt.number(insideCount, 0);
  const windowMin = fmt.number(MATCH_WINDOW_SECS / 60, 0);
  const modisMw = fmt.number(thresholds.modisMw, 1);

  const boxW = footprint.scanKm * PX_PER_KM;
  const boxH = footprint.trackKm * PX_PER_KM;
  const boxX = CX - boxW / 2;
  const boxY = CY - boxH / 2;
  const gridXs: number[] = [];
  for (let c = 1; c < tiling.cols; c++) gridXs.push(boxX + c * VIIRS_PX);
  const gridYs: number[] = [];
  for (let r = 1; r < tiling.rows; r++) gridYs.push(boxY + r * VIIRS_PX);

  const viirsBandX = BAR_X + thresholdFraction(thresholds.viirsMinMw) * BAR_SPAN;
  const viirsBandW = Math.max(
    3,
    (thresholdFraction(thresholds.viirsMaxMw) - thresholdFraction(thresholds.viirsMinMw)) * BAR_SPAN,
  );
  const modisNadirX = BAR_X + thresholdFraction(thresholds.modisNadirMw) * BAR_SPAN;
  const modisEdgeX = BAR_X + thresholdFraction(thresholds.modisEdgeMw) * BAR_SPAN;
  const modisHereX = BAR_X + thresholdFraction(thresholds.modisMw) * BAR_SPAN;
  const viirsHereX = BAR_X + thresholdFraction(thresholds.viirsMw) * BAR_SPAN;

  const badge =
    footprint.source === "measured"
      ? t("mapFootprintBadgeMeasured")
      : footprint.source === "vza"
        ? t("mapFootprintBadgeVza")
        : t("mapFootprintBadgeModel");

  let explanation: string;
  if (harm?.source === HotspotSource.Matched) {
    explanation = t("mapFootprintExplainMatched", { n: pixels, area, linked: fmt.number(linkedCount, 0) });
  } else if (harm?.source === HotspotSource.ModisOnly) {
    explanation = t("mapFootprintExplainModisOnly", { n: pixels, area, window: windowMin });
  } else if (insideCount > 0) {
    explanation = t("mapFootprintExplainRaw", { n: pixels, area, inside, window: windowMin });
  } else {
    explanation = t("mapFootprintExplainRawNone", { n: pixels, area, window: windowMin });
  }

  const presets: readonly { key: Preset; label: string }[] = [
    ...(pointVzaFactor !== null
      ? [{ key: "point" as const, label: t("mapFootprintPresetPoint", { vza: fmt.number(vzaDegFromFactor(pointVzaFactor), 0) }) }]
      : []),
    { key: "nadir", label: t("mapFootprintPresetNadir") },
    { key: "mid", label: t("mapFootprintPresetMid") },
    { key: "edge", label: t("mapFootprintPresetEdge") },
  ];

  return (
    <section id={id} className={styles.lens} aria-label={t("mapFootprintTitle")} data-map-ui="true">
      <div className={styles.header}>
        <span className={styles.title}>{t("mapFootprintTitle")}</span>
        <span className={styles.badge}>{badge}</span>
      </div>

      <div className={styles.presetGroup} role="group" aria-label={t("mapFootprintPresetGroupLabel")}>
        {presets.map((p) => (
          <button
            key={p.key}
            type="button"
            className={styles.presetBtn}
            aria-pressed={effectivePreset === p.key}
            onClick={() => setPreset(p.key)}
          >
            {p.label}
          </button>
        ))}
      </div>

      <div className={styles.diagramWrap}>
        <svg
          className={styles.diagramSvg}
          viewBox={`0 0 ${SVG_W} ${SVG_H}`}
          role="img"
          aria-label={t("mapFootprintSvgAria", { scan, track, n: pixels, inside })}
        >
          <rect x={boxX} y={boxY} width={boxW} height={boxH} className={styles.modisBox} />
          {gridXs.map((x) => (
            <line key={`x${x}`} x1={x} y1={boxY} x2={x} y2={boxY + boxH} className={styles.gridLine} />
          ))}
          {gridYs.map((y) => (
            <line key={`y${y}`} x1={boxX} y1={y} x2={boxX + boxW} y2={y} className={styles.gridLine} />
          ))}
          {footprint.scanAngleDeg > 1 && (
            <rect
              x={CX - PX_PER_KM / 2}
              y={CY - PX_PER_KM / 2}
              width={PX_PER_KM}
              height={PX_PER_KM}
              className={styles.nadirRef}
            />
          )}
          {found.map((f) => (
            <rect
              key={f.point.id}
              x={CX + f.dxKm * PX_PER_KM - VIIRS_PX / 2}
              y={CY - f.dyKm * PX_PER_KM - VIIRS_PX / 2}
              width={VIIRS_PX}
              height={VIIRS_PX}
              className={`${f.inside ? styles.viirsDetection : styles.viirsOutside} ${f.linked ? styles.viirsLinked : ""}`}
            />
          ))}
          <circle cx={CX} cy={CY} r={2} className={styles.modisCenter} />
          <line x1={6} y1={SVG_H - 6} x2={6 + PX_PER_KM} y2={SVG_H - 6} className={styles.scaleBar} />
          <text x={6 + PX_PER_KM + 4} y={SVG_H - 3} className={styles.svgLabel}>
            {t("mapFootprintScaleKm")}
          </text>
        </svg>
      </div>
      <p className={styles.ratioCaption}>{t("mapFootprintSvgRatio", { ratio })}</p>

      <ul className={styles.legend}>
        <li>
          <span className={`${styles.swatch} ${styles.swatchModis}`} aria-hidden="true" />
          {t("mapFootprintLegendModis")}
        </li>
        <li>
          <span className={`${styles.swatch} ${styles.swatchGrid}`} aria-hidden="true" />
          {t("mapFootprintLegendGrid")}
        </li>
        <li>
          <span className={`${styles.swatch} ${styles.swatchViirs}`} aria-hidden="true" />
          {t("mapFootprintLegendViirs")}
        </li>
        {harm && (
          <li>
            <span className={`${styles.swatch} ${styles.swatchLinked}`} aria-hidden="true" />
            {t("mapFootprintLegendLinked")}
          </li>
        )}
        {footprint.scanAngleDeg > 1 && (
          <li>
            <span className={`${styles.swatch} ${styles.swatchNadir}`} aria-hidden="true" />
            {t("mapFootprintLegendNadir")}
          </li>
        )}
      </ul>

      <dl className={styles.metrics}>
        <div className={styles.metricRow}>
          <dt>{t("mapFootprintModisLabel")}</dt>
          <dd>{t("mapFootprintModisDims", { scan, track, area, vza })}</dd>
        </div>
        <div className={styles.metricRow}>
          <dt>{t("mapFootprintTilingLabel")}</dt>
          <dd>{t("mapFootprintTilingValue", { n: pixels })}</dd>
        </div>
        <div className={styles.metricRow}>
          <dt>{t("mapFootprintInsideLabel", { window: windowMin })}</dt>
          <dd>{inside}</dd>
        </div>
        {harm && (
          <div className={styles.metricRow}>
            <dt>{t("mapFootprintLinkedLabel")}</dt>
            <dd>{fmt.number(linkedCount, 0)}</dd>
          </div>
        )}
      </dl>

      <div className={styles.thresholdBlock}>
        <span className={styles.thresholdHeader}>{t("mapFootprintThresholdTitle")}</span>
        <svg className={styles.thresholdSvg} viewBox={`0 0 ${BAR_W} ${BAR_H}`} aria-hidden="true">
          <rect x={BAR_X} y={6} width={BAR_SPAN} height={6} rx={3} className={styles.thresholdTrack} />
          <rect x={viirsBandX} y={6} width={viirsBandW} height={6} rx={2} className={styles.thresholdViirs} />
          <line x1={modisNadirX} y1={9} x2={modisEdgeX} y2={9} className={styles.thresholdModis} />
          <circle cx={viirsHereX} cy={9} r={3.5} className={styles.thresholdViirsHere} />
          <circle cx={modisHereX} cy={9} r={4} className={styles.thresholdModisHere} />
        </svg>
        <div className={styles.thresholdLabels}>
          <span>
            {t("mapFootprintThresholdViirs", {
              min: fmt.number(thresholds.viirsMinMw, 1),
              max: fmt.number(thresholds.viirsMaxMw, 1),
            })}
          </span>
          <span className={styles.thresholdModisValue}>
            {t("mapFootprintThresholdModis", {
              here: modisMw,
              nadir: fmt.number(thresholds.modisNadirMw, 1),
              edge: fmt.number(thresholds.modisEdgeMw, 0),
            })}
          </span>
        </div>
        <span className={styles.thresholdRatio}>
          {t("mapFootprintThresholdRatio", {
            modis: modisMw,
            viirs: fmt.number(thresholds.viirsMw, 1),
            ratio: fmt.number(thresholds.ratio, 0),
          })}
        </span>
      </div>

      <p className={styles.explanation}>{explanation}</p>
      <p className={styles.refs}>
        {t("mapFootprintRefs", { flare: fmt.number(FLARE_CHRONOS_AREA_RATIO, 2) })}
      </p>
    </section>
  );
}

export default memo(FootprintLens);
