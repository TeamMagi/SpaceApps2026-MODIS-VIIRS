/**
 * VzaCurveFigure — R_rel(θ) = max(1, aθ² + bθ + c) against viewing zenith angle, with the ±z·σ_vza band.
 * Hand-written SVG in neutral tokens (the accent is reserved for the Raw/Harmonized toggle). The range
 * input gives keyboard users the same readout that hover would give.
 */

import { useId, useState } from "react";
import { useCopy } from "../../hooks/useCopy";
import type { PipelineConfig } from "../../services/pipelineConfig";
import styles from "./Figures.module.css";

/** Largest MODIS viewing zenith angle, from the 55° scan-angle limit (docs/TASARIM_METHODS.md §4). */
const SCAN_EDGE_VZA = 65.4;
const X_MAX = 66;
const WIDTH = 640;
const HEIGHT = 280;
const PAD = { left: 48, right: 16, top: 16, bottom: 36 };

interface VzaCurveFigureProps {
  vza: PipelineConfig["vza"];
  uncertainty: PipelineConfig["uncertainty"];
}

export default function VzaCurveFigure({ vza, uncertainty }: VzaCurveFigureProps) {
  const { t, fmt } = useCopy();
  const sliderId = useId();
  const bandId = useId();
  const [angle, setAngle] = useState(40);
  const [showBand, setShowBand] = useState(true);

  const ratioAt = (deg: number) => {
    const rad = (deg * Math.PI) / 180;
    return Math.max(1, vza.a * rad * rad + vza.b * rad + vza.c);
  };
  const sigmaAt = (deg: number) => uncertainty.vzaErrorCoeff * Math.max(ratioAt(deg) - 1, 0);

  const degrees = Array.from({ length: Math.ceil(SCAN_EDGE_VZA) + 1 }, (_, i) => Math.min(i, SCAN_EDGE_VZA));
  const yMax = Math.max(1.35, Math.ceil(ratioAt(SCAN_EDGE_VZA) * (1 + uncertainty.z * sigmaAt(SCAN_EDGE_VZA)) * 20) / 20);

  const x = (deg: number) => PAD.left + (deg / X_MAX) * (WIDTH - PAD.left - PAD.right);
  const y = (ratio: number) => HEIGHT - PAD.bottom - ((ratio - 1) / (yMax - 1)) * (HEIGHT - PAD.top - PAD.bottom);
  const point = (deg: number, ratio: number) => `${x(deg).toFixed(1)},${y(Math.max(ratio, 1)).toFixed(1)}`;

  const curve = degrees.map((deg) => point(deg, ratioAt(deg))).join(" ");
  const upper = degrees.map((deg) => point(deg, ratioAt(deg) * (1 + uncertainty.z * sigmaAt(deg))));
  const lower = degrees.map((deg) => point(deg, ratioAt(deg) * (1 - uncertainty.z * sigmaAt(deg)))).reverse();
  const band = `M ${[...upper, ...lower].join(" L ")} Z`;

  const yTicks: number[] = [];
  for (let tick = 1; tick <= yMax + 1e-9; tick += 0.1) yTicks.push(Number(tick.toFixed(2)));
  const xTicks = [0, 10, 20, 30, 40, 50, 60];

  const ratio = ratioAt(angle);
  const sigmaVza = sigmaAt(angle);
  const readout = t("methodsVzaReadout", {
    angle,
    ratio: fmt.number(ratio, 3),
    gain: fmt.signedPercent(ratio - 1, 1),
    sigmaVza: fmt.percent(sigmaVza, 2),
    sigmaRand: fmt.percent(Math.hypot(uncertainty.modisSensorSigma, sigmaVza), 1),
    sigmaScale: fmt.percent(uncertainty.modisScaleSigma, 1),
  });

  return (
    <figure className={styles.figure}>
      <svg
        className={styles.svg}
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role="img"
        aria-label={t("methodsVzaFigureLabel")}
      >
        {yTicks.map((tick) => (
          <g key={tick}>
            <line x1={PAD.left} x2={WIDTH - PAD.right} y1={y(tick)} y2={y(tick)} className={styles.grid} />
            <text x={PAD.left - 6} y={y(tick) + 4} textAnchor="end" className={styles.tick}>
              {fmt.number(tick, 2)}
            </text>
          </g>
        ))}
        {xTicks.map((tick) => (
          <text key={tick} x={x(tick)} y={HEIGHT - PAD.bottom + 16} textAnchor="middle" className={styles.tick}>
            {tick}°
          </text>
        ))}
        <line x1={PAD.left} x2={WIDTH - PAD.right} y1={y(1)} y2={y(1)} className={styles.reference} />
        <line x1={PAD.left} x2={PAD.left} y1={PAD.top} y2={HEIGHT - PAD.bottom} className={styles.axis} />
        <line x1={PAD.left} x2={WIDTH - PAD.right} y1={HEIGHT - PAD.bottom} y2={HEIGHT - PAD.bottom} className={styles.axis} />
        <text x={PAD.left + 6} y={y(1) - 8} className={styles.annotation}>
          {t("methodsVzaNadir")}
        </text>
        <line x1={x(SCAN_EDGE_VZA)} x2={x(SCAN_EDGE_VZA)} y1={PAD.top} y2={HEIGHT - PAD.bottom} className={styles.reference} />
        <text x={x(SCAN_EDGE_VZA) - 6} y={PAD.top + 10} textAnchor="end" className={styles.annotation}>
          {t("methodsVzaEdge")}
        </text>
        {showBand && <path d={band} className={styles.band} />}
        <polyline points={curve} className={styles.curve} />
        <line x1={x(angle)} x2={x(angle)} y1={PAD.top} y2={HEIGHT - PAD.bottom} className={styles.cursor} />
        <circle cx={x(angle)} cy={y(ratio)} r={4} className={styles.dot} />
      </svg>

      <div className={styles.controls}>
        <label htmlFor={sliderId} className={styles.controlLabel}>
          {t("methodsVzaAngleLabel")}
        </label>
        <input
          id={sliderId}
          type="range"
          min={0}
          max={65}
          step={5}
          value={angle}
          className={styles.slider}
          onChange={(event) => setAngle(Number(event.target.value))}
        />
        <label htmlFor={bandId} className={styles.check}>
          <input id={bandId} type="checkbox" checked={showBand} onChange={(event) => setShowBand(event.target.checked)} />
          {t("methodsVzaBandToggle")}
        </label>
      </div>
      <output className={styles.readout} htmlFor={sliderId}>
        {readout}
      </output>
      <figcaption className={styles.caption}>{t("methodsVzaNote")}</figcaption>
    </figure>
  );
}
