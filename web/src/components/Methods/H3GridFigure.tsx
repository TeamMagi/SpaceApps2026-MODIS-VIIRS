/**
 * H3GridFigure — what the H3 index is and is not. Nineteen pointy-top hexagons (k = 0..2 around the
 * centre cell) with a MODIS detection (hollow circle) and a VIIRS detection (filled square), told apart
 * by shape as in the rest of the app. The frame is dashed and badged because the shared-grid stage is
 * planned; today H3 only prunes match candidates.
 */

import { useCopy } from "../../hooks/useCopy";
import styles from "./Figures.module.css";

const SIZE = 30;
const CENTER = { x: 160, y: 120 };
const RINGS = 2;

/** Mirrors `k_ring_for_radius` in crates/core/src/h3index.rs (cell apothem 0.40 km, 1.25 safety factor). */
function kRingForRadius(radiusKm: number): number {
  return Math.max(1, Math.ceil(((radiusKm * 1.25) / 0.4 - 1) / 2));
}

interface Hex {
  key: string;
  ring: number;
  cx: number;
  cy: number;
}

function buildHexes(): Hex[] {
  const hexes: Hex[] = [];
  for (let q = -RINGS; q <= RINGS; q += 1) {
    for (let r = -RINGS; r <= RINGS; r += 1) {
      const ring = Math.max(Math.abs(q), Math.abs(r), Math.abs(-q - r));
      if (ring > RINGS) continue;
      hexes.push({
        key: `${q},${r}`,
        ring,
        cx: CENTER.x + SIZE * Math.sqrt(3) * (q + r / 2),
        cy: CENTER.y + SIZE * 1.5 * r,
      });
    }
  }
  return hexes;
}

const HEXES = buildHexes();

function hexPoints(cx: number, cy: number): string {
  return Array.from({ length: 6 }, (_, i) => {
    const angle = (Math.PI / 180) * (60 * i - 30);
    return `${(cx + SIZE * Math.cos(angle)).toFixed(1)},${(cy + SIZE * Math.sin(angle)).toFixed(1)}`;
  }).join(" ");
}

export default function H3GridFigure({ matchRadiusKm }: { matchRadiusKm: number }) {
  const { t } = useCopy();
  const modis = HEXES.find((hex) => hex.key === "0,0");
  const viirs = HEXES.find((hex) => hex.key === "1,0");

  return (
    <figure className={styles.figure}>
      <svg className={styles.svg} viewBox="0 0 320 240" role="img" aria-label={t("methodsH3FigureLabel")}>
        {HEXES.map((hex) => (
          <polygon key={hex.key} points={hexPoints(hex.cx, hex.cy)} className={styles.hex} data-ring={hex.ring} />
        ))}
        {modis && <circle cx={modis.cx - 6} cy={modis.cy} r={6} className={styles.modis} />}
        {viirs && <rect x={viirs.cx - 5} y={viirs.cy - 5} width={10} height={10} className={styles.viirs} />}
      </svg>

      <ul className={styles.legend} role="list">
        <li>
          <svg viewBox="0 0 14 14" className={styles.legendMark} aria-hidden="true">
            <circle cx="7" cy="7" r="5" className={styles.modis} />
          </svg>
          {t("methodsH3Modis")}
        </li>
        <li>
          <svg viewBox="0 0 14 14" className={styles.legendMark} aria-hidden="true">
            <rect x="2" y="2" width="10" height="10" className={styles.viirs} />
          </svg>
          {t("methodsH3Viirs")}
        </li>
      </ul>

      <p className={styles.status}>{t("methodsH3Applied")}</p>
      <p className={styles.status} data-planned="true">
        <span className={styles.plannedTag}>{t("pipelinePlannedBadge")}</span> {t("methodsH3Planned")}
      </p>
      <figcaption className={styles.caption}>
        {t("methodsH3Facts", { k: kRingForRadius(matchRadiusKm) })} {t("methodsH3Why")}
      </figcaption>
    </figure>
  );
}
