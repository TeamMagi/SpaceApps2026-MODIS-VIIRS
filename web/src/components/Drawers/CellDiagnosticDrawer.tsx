/**
 * CellDiagnosticDrawer — what is behind one burn-calendar month (Y9), opened from the calendar's
 * "Month details" button or the D key; selecting the month for the map filter is a separate action.
 *
 * Harmonized (revealAnswer): sensor counts, harmonized and matched / single-sensor records, effort
 *   Ω and the effort-normalised count (or the Terra-only "not comparable" note and the K12 estimate),
 *   whether completeness k_c scales the total FRP, uncertainty bands and the anomaly, and flags.
 * Raw: only raw sensor counts and the quality flags (facts about the instruments) plus an invitation
 *   to harmonize; no harmonized-derived value is rendered (lib/cellDiagnostic withholds them).
 * Missing values show "—". The API exposes neither a per-platform split nor the k_c factor per month.
 */

import { useMemo, type ReactNode } from "react";
import { calendarFrpKey, useCalendarFrp } from "../../hooks/useCalendarFrp";
import { useCellPoints } from "../../hooks/useCellPoints";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useRegionData } from "../../hooks/useRegionData";
import {
  buildCellDiagnostic,
  qualityFlagCopyKey,
  sourceSplit,
  type CellDiagnostic,
  type HarmonizedDiagnostic,
} from "../../lib/cellDiagnostic";
import { methodsHash } from "../../lib/route";
import { useAppStore } from "../../store/useAppStore";
import Drawer from "./Drawer";
import styles from "./CellDiagnosticDrawer.module.css";

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className={styles.row}>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

function HarmonizedSections({ h, onNavigate }: { h: HarmonizedDiagnostic; onNavigate: () => void }) {
  const { t, fmt } = useCopy();
  const dash = t("cellUnavailable");
  const num = (value: number | null, digits = 0) => (value === null ? dash : fmt.number(value, digits));
  const recovered = h.recovered;
  const totalFrp = h.totalFrp;
  const conformal = h.conformal;
  const mw = (value: number) => fmt.number(value, value >= 1000 ? 0 : 1);

  return (
    <>
      <section className={styles.section} aria-labelledby="cell-effort">
        <h3 id="cell-effort" className={styles.heading}>
          {t("cellSectionEffort")}
        </h3>
        <dl className={styles.rows}>
          <Row label={t("cellOmega")}>{num(h.omega, 2)}</Row>
          <Row label={t("cellNormalized")}>{num(h.countNormalized, 1)}</Row>
          {recovered && (
            <Row label={t("cellRecovered")}>
              {recovered.lower !== null && recovered.upper !== null
                ? t("cellRecoveredValue", {
                    value: fmt.number(recovered.value),
                    low: fmt.number(recovered.lower),
                    high: fmt.number(recovered.upper),
                  })
                : fmt.number(recovered.value)}
            </Row>
          )}
        </dl>
        {h.terraOnly && (
          <div className={styles.callout} role="note">
            <strong>{t("cellPreAquaTitle")}</strong>
            <p>{t("cellPreAquaBody")}</p>
            {recovered && <p>{t("cellRecoveredNote")}</p>}
          </div>
        )}
      </section>

      <section className={styles.section} aria-labelledby="cell-completeness">
        <h3 id="cell-completeness" className={styles.heading}>
          {t("cellSectionCompleteness")}
        </h3>
        <p className={styles.note}>
          {h.completenessApplied ? t("cellCompletenessApplies") : t("cellCompletenessNotApplied")}
        </p>
        <a className={styles.link} href={methodsHash("pipeline")} onClick={onNavigate}>
          {t("cellCompletenessLink")}
        </a>
      </section>

      <section className={styles.section} aria-labelledby="cell-uncertainty">
        <h3 id="cell-uncertainty" className={styles.heading}>
          {t("cellSectionUncertainty")}
        </h3>
        <dl className={styles.rows}>
          <Row label={t("cellTotalFrp")}>
            {totalFrp === null
              ? dash
              : totalFrp.lower !== null && totalFrp.upper !== null
                ? t("cellTotalFrpBand", {
                    value: mw(totalFrp.value),
                    low: mw(totalFrp.lower),
                    high: mw(totalFrp.upper),
                  })
                : t("cellTotalFrpPlain", { value: mw(totalFrp.value) })}
          </Row>
          <Row label={t("cellFrpPerDetection")}>{num(h.frpPerDetectionEq, 1)}</Row>
          <Row label={t("cellConformal68")}>
            {conformal
              ? t("cellConformalRange", {
                  low: mw(conformal.p68Lower),
                  high: mw(conformal.p68Upper),
                })
              : dash}
          </Row>
          <Row label={t("cellConformal90")}>
            {conformal
              ? t("cellConformalRange", {
                  low: mw(conformal.p90Lower),
                  high: mw(conformal.p90Upper),
                })
              : dash}
          </Row>
          <Row label={t("cellAnomaly")}>{h.anomaly === null ? dash : fmt.sigma(h.anomaly)}</Row>
        </dl>
        {conformal ? (
          <p className={styles.note}>
            {t("cellConformalNote", {
              era: conformal.era === "viirs" ? t("cellConformalEraViirs") : t("cellConformalEraPre"),
              n: conformal.nCal,
            })}
            {conformal.extrapolated ? ` ${t("cellConformalExtrapolated")}` : ""}
          </p>
        ) : (
          <p className={styles.note}>{t("cellConformalMissing")}</p>
        )}
      </section>
    </>
  );
}

export default function CellDiagnosticDrawer() {
  const { t, fmt } = useCopy();
  const open = useAppStore((state) => state.openDrawer === "cell");
  const month = useAppStore((state) => state.diagnosticMonth);
  const closeDrawer = useAppStore((state) => state.closeDrawer);
  const revealAnswer = useHarmonizeTransition((v) => v.revealAnswer);
  const { status: seriesStatus, months, region } = useRegionData();
  const calendar = useCalendarFrp(region.id);
  const pointsState = useCellPoints(month, open && revealAnswer);

  const cell = useMemo(
    () => (month ? (months.find((c) => c.year === month.year && c.month === month.month) ?? null) : null),
    [month, months],
  );
  // An empty period (demo months outside the fixture) answers with zero stages that must not read as data.
  const points = pointsState.points !== null && pointsState.points.bbox !== null ? pointsState.points : null;

  const diagnostic: CellDiagnostic | null = useMemo(() => {
    if (!cell) return null;
    return buildCellDiagnostic(
      {
        cell,
        calendar: calendar.get(calendarFrpKey(cell.year, cell.month)),
        stages: points?.stages ?? null,
        sources: points ? sourceSplit(points.harmonized, points.stages, points.truncated) : null,
      },
      revealAnswer,
    );
  }, [cell, calendar, points, revealAnswer]);

  const title = month ? t("cellDrawerTitle", { month: fmt.monthYear(month, "long") }) : t("cellOpenDetails");
  const dash = t("cellUnavailable");
  const harmonized = diagnostic?.harmonized ?? null;
  const hasCalendarCell = cell !== null && calendar.has(calendarFrpKey(cell.year, cell.month));

  let body: ReactNode;
  if (seriesStatus === "loading" && !cell) {
    body = <p role="status">{t("cellLoading")}</p>;
  } else if (seriesStatus === "error") {
    body = <p role="status">{t("cellError")}</p>;
  } else if (!diagnostic || !diagnostic.hasData) {
    body = <p role="status">{t("cellEmpty")}</p>;
  } else {
    body = (
      <>
        <section className={styles.section} aria-labelledby="cell-sensors">
          <h3 id="cell-sensors" className={styles.heading}>
            {t("cellSectionSensors")}
          </h3>
          <dl className={styles.rows}>
            <Row label={t("cellRawModis")}>{fmt.number(diagnostic.raw.modis)}</Row>
            <Row label={t("cellRawViirs")}>{fmt.number(diagnostic.raw.viirs)}</Row>
            <Row label={t("cellRawTotal")}>{fmt.number(diagnostic.raw.total)}</Row>
            {harmonized && (
              <>
                <Row label={t("cellHarmonized")}>{fmt.number(harmonized.count)}</Row>
                <Row label={t("cellMatched")}>
                  {pointsState.status === "loading"
                    ? t("cellLoading")
                    : harmonized.matched === null
                      ? dash
                      : fmt.number(harmonized.matched)}
                </Row>
                <Row label={t("cellModisOnly")}>
                  {harmonized.modisOnly === null ? dash : fmt.number(harmonized.modisOnly)}
                </Row>
                <Row label={t("cellViirsOnly")}>
                  {harmonized.viirsOnly === null ? dash : fmt.number(harmonized.viirsOnly)}
                </Row>
              </>
            )}
          </dl>
          <p className={styles.note}>{t("cellPlatformNote")}</p>
          {harmonized && pointsState.status === "ready" && harmonized.modisOnly === null && (
            <p className={styles.note}>{t("cellSourcesSampled")}</p>
          )}
          {pointsState.status === "error" && (
            <p className={styles.note} role="status">
              {t("cellError")}
            </p>
          )}
        </section>

        {harmonized ? (
          <HarmonizedSections h={harmonized} onNavigate={closeDrawer} />
        ) : (
          <p className={styles.invite}>{t("cellRawInvite")}</p>
        )}

        <section className={styles.section} aria-labelledby="cell-flags">
          <h3 id="cell-flags" className={styles.heading}>
            {t("cellSectionFlags")}
          </h3>
          {diagnostic.flags.length > 0 ? (
            <ul className={styles.flags}>
              {diagnostic.flags.map((flag) => {
                const key = qualityFlagCopyKey(flag);
                return <li key={flag}>{key ? t(key) : t("cellFlagUnknown", { flag })}</li>;
              })}
            </ul>
          ) : (
            <p className={styles.note}>{hasCalendarCell ? t("cellFlagsNone") : t("cellFlagsUnavailable")}</p>
          )}
        </section>
      </>
    );
  }

  return (
    <Drawer id="cell" title={title} description={t("cellDrawerLead")}>
      {body}
    </Drawer>
  );
}
