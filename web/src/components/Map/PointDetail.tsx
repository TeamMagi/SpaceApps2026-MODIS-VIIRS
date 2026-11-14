import { memo, useLayoutEffect, useRef } from "react";
import { useCopy, type CopyApi } from "../../hooks/useCopy";
import {
  Confidence,
  DayNight,
  HotspotSource,
  type HarmonizedPoint,
  type RawPoint,
} from "../../types";
import type { ViewportSize } from "./engine/types";
import styles from "./PointDetail.module.css";

export interface PointDetailProps {
  readonly rawPoint: RawPoint | null;
  readonly harmPoint: HarmonizedPoint | null;
  readonly isDuplicateInRaw: boolean;
  readonly revealAnswer: boolean;
  readonly screenX: number;
  readonly screenY: number;
  readonly viewportSize: ViewportSize;
  readonly onClose: () => void;
}

export function sourceCopyKey(
  source: HotspotSource,
): "sourceMatched" | "sourceModisOnly" | "sourceViirsOnly" {
  switch (source) {
    case HotspotSource.Matched:
      return "sourceMatched";
    case HotspotSource.ModisOnly:
      return "sourceModisOnly";
    case HotspotSource.ViirsOnly:
      return "sourceViirsOnly";
  }
}

export function confidenceCopyKey(
  confidence: Confidence,
): "confidenceHigh" | "confidenceNominal" | "confidenceLow" {
  switch (confidence) {
    case Confidence.High:
      return "confidenceHigh";
    case Confidence.Nominal:
      return "confidenceNominal";
    case Confidence.Low:
      return "confidenceLow";
  }
}

export function formatPointAnnouncement(
  copy: Pick<CopyApi, "t" | "fmt">,
  rawPoint: RawPoint | null,
  harmPoint: HarmonizedPoint | null,
  isDuplicateInRaw: boolean,
  revealAnswer: boolean,
): string {
  const { t, fmt } = copy;
  if (revealAnswer && harmPoint) {
    const pass = harmPoint.daynight === DayNight.Day ? t("mapDetailDay") : t("mapDetailNight");
    const base = t("mapTooltip", {
      frp: fmt.number(harmPoint.frpCorrected, 1),
      from: fmt.number(harmPoint.frp, 1),
      sources: t(sourceCopyKey(harmPoint.source)),
      confidence: t(confidenceCopyKey(harmPoint.confidence)),
      date: fmt.utcDateTime(harmPoint.acquired),
    });
    return `${base} · ${pass}`;
  }
  if (rawPoint) {
    const pass = rawPoint.daynight === DayNight.Day ? t("mapDetailDay") : t("mapDetailNight");
    const base = t("mapTooltipRaw", {
      frp: fmt.number(rawPoint.frp, 1),
      sensor: rawPoint.sensor,
      confidence: t(confidenceCopyKey(rawPoint.confidence)),
      date: fmt.utcDateTime(rawPoint.acquired),
    });
    const dup = isDuplicateInRaw ? ` · ${t("mapLegendDuplicate")}` : "";
    return `${base} · ${pass}${dup}`;
  }
  return "";
}

function PointDetail({
  rawPoint,
  harmPoint,
  isDuplicateInRaw,
  revealAnswer,
  screenX,
  screenY,
  viewportSize,
  onClose,
}: PointDetailProps) {
  const copy = useCopy();
  const { t, fmt } = copy;
  const cardRef = useRef<HTMLDivElement | null>(null);

  const showHarmonized = revealAnswer && harmPoint !== null;
  const activePoint = showHarmonized ? harmPoint : rawPoint;

  const { width, height } = viewportSize;
  const halfW = 150;
  const cardH = 180;
  const controlsLeft = Math.max(0, width - 184);
  const controlsBottom = 60;

  const wouldHitTopOrControls =
    screenY - 14 - cardH < 10 ||
    (screenX + halfW > controlsLeft && screenY - 14 - cardH < controlsBottom);
  const placement: "above" | "below" = wouldHitTopOrControls ? "below" : "above";

  let anchorY: number;
  if (placement === "above") {
    anchorY = Math.min(
      Math.max(screenY - 14, cardH + 10),
      Math.max(cardH + 10, height - 10),
    );
  } else {
    anchorY = Math.min(
      Math.max(screenY + 14, 10),
      Math.max(10, height - cardH - 10),
    );
  }

  const cardTop = placement === "above" ? anchorY - cardH : anchorY;
  const maxAllowedX =
    cardTop < controlsBottom
      ? Math.max(halfW + 10, controlsLeft - halfW - 8)
      : Math.max(halfW + 10, width - halfW - 10);
  const anchorX = Math.min(Math.max(screenX, halfW + 10), maxAllowedX);

  useLayoutEffect(() => {
    const el = cardRef.current;
    if (!el) return;
    el.style.setProperty("--card-x", `${Math.round(anchorX)}px`);
    el.style.setProperty("--card-y", `${Math.round(anchorY)}px`);
  }, [anchorX, anchorY]);

  if (!activePoint) return null;

  const passLabel =
    activePoint.daynight === DayNight.Day ? t("mapDetailDay") : t("mapDetailNight");
  const confidenceLabel = t(confidenceCopyKey(activePoint.confidence));
  const timeLabel = fmt.utcDateTime(activePoint.acquired);

  return (
    <div
      ref={cardRef}
      className={styles.card}
      data-placement={placement}
      data-map-ui="true"
      role="region"
      aria-label={showHarmonized ? t("mapDetailTitleHarmonized") : t("mapDetailTitleRaw")}
    >
      <div className={styles.header}>
        <div className={styles.titleGroup}>
          <span className={styles.title}>
            {showHarmonized ? t("mapDetailTitleHarmonized") : t("mapDetailTitleRaw")}
          </span>
          {!showHarmonized && isDuplicateInRaw && (
            <span className={styles.dupBadge}>{t("mapLegendDuplicate")}</span>
          )}
        </div>
        <button
          type="button"
          className={styles.closeButton}
          aria-label={t("closeAction")}
          title={t("closeAction")}
          onClick={onClose}
        >
          <span aria-hidden="true">×</span>
        </button>
      </div>

      {showHarmonized && harmPoint ? (
        <>
          <p className={styles.srOnly}>
            {t("mapTooltip", {
              frp: fmt.number(harmPoint.frpCorrected, 1),
              from: fmt.number(harmPoint.frp, 1),
              sources: t(sourceCopyKey(harmPoint.source)),
              confidence: confidenceLabel,
              date: timeLabel,
            })}
          </p>
          <dl className={styles.grid}>
            <div className={styles.row}>
              <dt>{t("mapDetailFrp")}</dt>
              <dd>
                {fmt.mw(harmPoint.frpCorrected)} (
                {t("mapDetailFrpCorrectedFrom", { from: fmt.number(harmPoint.frp, 1) })})
              </dd>
            </div>
            <div className={styles.row}>
              <dt>{t("mapDetailSources")}</dt>
              <dd>{t(sourceCopyKey(harmPoint.source))}</dd>
            </div>
            <div className={styles.row}>
              <dt>{t("mapDetailConfidence")}</dt>
              <dd>{confidenceLabel}</dd>
            </div>
            <div className={styles.row}>
              <dt>{t("mapDetailPass")}</dt>
              <dd>{passLabel}</dd>
            </div>
            <div className={styles.row}>
              <dt>{t("mapDetailTime")}</dt>
              <dd>{timeLabel}</dd>
            </div>
          </dl>
        </>
      ) : (
        rawPoint && (
          <>
            <p className={styles.srOnly}>
              {t("mapTooltipRaw", {
                frp: fmt.number(rawPoint.frp, 1),
                sensor: rawPoint.sensor,
                confidence: confidenceLabel,
                date: timeLabel,
              })}
            </p>
            {isDuplicateInRaw && (
              <p className={styles.dupNote}>{t("mapDetailDuplicateNote")}</p>
            )}
            <dl className={styles.grid}>
              <div className={styles.row}>
                <dt>{t("mapDetailFrp")}</dt>
                <dd>{fmt.mw(rawPoint.frp)}</dd>
              </div>
              <div className={styles.row}>
                <dt>{t("mapDetailSensor")}</dt>
                <dd>{rawPoint.sensor}</dd>
              </div>
              <div className={styles.row}>
                <dt>{t("mapDetailConfidence")}</dt>
                <dd>{confidenceLabel}</dd>
              </div>
              <div className={styles.row}>
                <dt>{t("mapDetailPass")}</dt>
                <dd>{passLabel}</dd>
              </div>
              <div className={styles.row}>
                <dt>{t("mapDetailTime")}</dt>
                <dd>{timeLabel}</dd>
              </div>
            </dl>
          </>
        )
      )}
    </div>
  );
}

export default memo(PointDetail);
