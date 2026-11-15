/**
 * OrbitalDriftStrip — informational note (role="note") about MODIS Terra/Aqua orbital drift (Y9).
 *
 * Shown when the loaded period's calendar cells carry `modis_orbital_drift` (2021+) or when the
 * overpass-time medians of Terra/Aqua deviate from the nominal crossing times; dismissible for the
 * session. The collapsed line (one line at 1366 px, so the no-scroll dashboard keeps its height) shows
 * the day-pass median local solar time vs nominal with the signed difference per MODIS platform and
 * the short caveat; "Details" opens the full text, every pass, the caveat and the source as an overlay.
 * A regional median is an indicator, never presented as the crossing time. The medians are
 * raw-observation facts, so the strip is also shown in Raw mode. Demo mode shows the flag message
 * without numbers (no real overpass times there).
 */

import { useId, useState } from "react";
import { useCopy } from "../../hooks/useCopy";
import { useOrbitalDrift } from "../../hooks/useOrbitalDrift";
import type { DriftRow } from "../../lib/orbitalDrift";
import { useAppStore } from "../../store/useAppStore";
import { DayNight } from "../../types";
import styles from "./OrbitalDriftStrip.module.css";

export default function OrbitalDriftStrip() {
  const { t, fmt } = useCopy();
  const drift = useOrbitalDrift();
  const dataMode = useAppStore((state) => state.dataMode);
  const dismissDrift = useAppStore((state) => state.dismissDrift);
  const [expanded, setExpanded] = useState(false);
  const detailsId = useId();

  if (!drift.visible) return null;

  const dayRows = drift.rows.filter((row) => row.daynight === DayNight.Day);
  const summaryRows = dayRows.length > 0 ? dayRows : drift.rows.slice(0, 2);
  const rowText = (row: DriftRow) =>
    t("driftRow", {
      platform: row.platform,
      pass: row.daynight === DayNight.Day ? t("driftPassDay") : t("driftPassNight"),
      time: row.medianTime,
      nominal: row.nominalTime,
      diff: fmt.signed(row.diffMinutes),
    });

  return (
    <aside className={styles.strip} role="note" aria-label={t("driftTitle")}>
      <div className={styles.line}>
        <strong className={styles.title}>{t("driftTitle")}</strong>
        <span className={styles.summary}>
          {drift.reason === "flag" ? t("driftSummaryFlag") : t("driftSummaryDeviation")}
        </span>
        {drift.showNumbers ? (
          summaryRows.map((row) => (
            <span key={`${row.platform}-${row.daynight}`} className={styles.chip}>
              {rowText(row)}
            </span>
          ))
        ) : (
          dataMode === "demo" && <span className={styles.summary}>{t("driftNoNumbers")}</span>
        )}
        <span className={styles.summary}>{t("driftShortCaveat")}</span>
      </div>
      <div className={styles.actions}>
        <button
          type="button"
          className={styles.more}
          aria-expanded={expanded}
          aria-controls={detailsId}
          onClick={() => setExpanded((open) => !open)}
        >
          {expanded ? t("driftLess") : t("driftMore")}
        </button>
        <button type="button" className={styles.dismiss} aria-label={t("driftDismiss")} onClick={dismissDrift}>
          ×
        </button>
      </div>
      <div id={detailsId} className={styles.details} hidden={!expanded}>
        <p>{drift.reason === "flag" ? t("driftFlagBody") : t("driftDeviationBody")}</p>
        {drift.showNumbers && (
          <ul className={styles.rows}>
            {drift.rows.map((row) => (
              <li key={`${row.platform}-${row.daynight}`} className={styles.chip}>
                {rowText(row)}
              </li>
            ))}
          </ul>
        )}
        <p>{t("driftCaveat")}</p>
        <p>{t("driftSource")}</p>
      </div>
    </aside>
  );
}
