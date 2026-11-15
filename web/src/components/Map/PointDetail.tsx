import {
  memo,
  useCallback,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type RefObject,
} from "react";
import { useCopy, type CopyApi } from "../../hooks/useCopy";
import {
  formatFrpInterval,
  relativeUncertaintyWidth,
} from "../../lib/format";
import { uncertaintyTierFor } from "./engine/renderer";
import {
  Confidence,
  DayNight,
  HotspotSource,
  type HarmonizedPoint,
  type RawPoint,
} from "../../types";
import type { ViewportSize } from "./engine/types";
import FootprintLens from "./FootprintLens";
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
  /** Loaded raw detections, so the footprint lens can find the VIIRS pixels inside a MODIS pixel. */
  readonly rawPoints: readonly RawPoint[];
  /** Lets a caller re-anchor the card every frame (see `applyPointCardAnchor`) without re-rendering it. */
  readonly cardRef?: RefObject<HTMLDivElement | null>;
}

export interface PointCardAnchor {
  readonly placement: "above" | "below";
  readonly x: number;
  readonly y: number;
}

/** Where the card sits for a point at (screenX, screenY), clear of the panel edges and zoom controls. */
export function pointCardAnchor(screenX: number, screenY: number, viewportSize: ViewportSize): PointCardAnchor {
  const { width, height } = viewportSize;
  const halfW = 150;
  const cardH = 180;
  const controlsLeft = Math.max(0, width - 184);
  const controlsBottom = 60;

  const wouldHitTopOrControls =
    screenY - 14 - cardH < 10 ||
    (screenX + halfW > controlsLeft && screenY - 14 - cardH < controlsBottom);
  const placement: "above" | "below" = wouldHitTopOrControls ? "below" : "above";

  let y: number;
  if (placement === "above") {
    y = Math.min(Math.max(screenY - 14, cardH + 10), Math.max(cardH + 10, height - 10));
  } else {
    y = Math.min(Math.max(screenY + 14, 10), Math.max(10, height - cardH - 10));
  }

  const cardTop = placement === "above" ? y - cardH : y;
  const maxAllowedX =
    cardTop < controlsBottom
      ? Math.max(halfW + 10, controlsLeft - halfW - 8)
      : Math.max(halfW + 10, width - halfW - 10);
  const x = Math.min(Math.max(screenX, halfW + 10), maxAllowedX);
  return { placement, x, y };
}

export function applyPointCardAnchor(el: HTMLElement, anchor: PointCardAnchor): void {
  el.style.setProperty("--card-x", `${Math.round(anchor.x)}px`);
  el.style.setProperty("--card-y", `${Math.round(anchor.y)}px`);
  if (el.dataset.placement !== anchor.placement) el.dataset.placement = anchor.placement;
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
  rawPoints,
  cardRef: externalCardRef,
}: PointDetailProps) {
  const copy = useCopy();
  const { lang, t, fmt } = copy;
  const localCardRef = useRef<HTMLDivElement | null>(null);
  const cardRef = externalCardRef ?? localCardRef;
  const lensToggleRef = useRef<HTMLButtonElement | null>(null);
  const [lensOpen, setLensOpen] = useState(false);
  const lensId = useId();

  const showHarmonized = revealAnswer && harmPoint !== null;
  const activePoint = showHarmonized ? harmPoint : rawPoint;

  // Raw mode decides from the raw detection alone, so the button itself cannot hint at the harmonized source.
  const canShowFootprintLens = showHarmonized
    ? harmPoint.source === HotspotSource.Matched ||
      harmPoint.source === HotspotSource.ModisOnly
    : rawPoint !== null && rawPoint.sensor === "MODIS";

  const showFootprintLens = canShowFootprintLens && lensOpen;

  const handleCloseLens = useCallback(() => {
    setLensOpen(false);
    lensToggleRef.current?.focus();
  }, []);

  // Esc closes the lens first and stops there, so the map does not also drop the selection.
  const handleLensKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLDivElement>) => {
      if (e.key !== "Escape" || !lensOpen) return;
      e.preventDefault();
      e.stopPropagation();
      handleCloseLens();
    },
    [lensOpen, handleCloseLens],
  );

  const anchor = pointCardAnchor(screenX, screenY, viewportSize);
  const { placement } = anchor;

  useLayoutEffect(() => {
    const el = cardRef.current;
    if (el) applyPointCardAnchor(el, { placement, x: anchor.x, y: anchor.y });
  }, [cardRef, placement, anchor.x, anchor.y]);

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
      data-lens-open={showFootprintLens ? "true" : undefined}
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
        (() => {
          const hasInterval = harmPoint.frpLower !== null && harmPoint.frpUpper !== null;
          const relWidth = relativeUncertaintyWidth(
            harmPoint.frpLower,
            harmPoint.frpUpper,
            harmPoint.frpCorrected,
          );
          const isWideInterval = uncertaintyTierFor(relWidth) === 2;
          return (
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
                    {formatFrpInterval(
                      harmPoint.frpCorrected,
                      harmPoint.frpLower,
                      harmPoint.frpUpper,
                      lang,
                    )}{" "}
                    ({t("mapDetailFrpCorrectedFrom", { from: fmt.number(harmPoint.frp, 1) })})
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
              {hasInterval && (
                <p className={styles.uncertCaption}>{t("uncertaintyConfidenceLabel")}</p>
              )}
              {isWideInterval && (
                <p className={styles.uncertHint}>
                  <strong>{t("uncertaintyWideTitle")}:</strong> {t("uncertaintyWideBody")}
                </p>
              )}
            </>
          );
        })()
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
            <p className={styles.uncertCaption}>{t("uncertaintyRawNote")}</p>
          </>
        )
      )}

      {canShowFootprintLens && (
        <div className={styles.lensFooter} onKeyDown={handleLensKeyDown}>
          <button
            ref={lensToggleRef}
            type="button"
            className={styles.lensToggle}
            aria-expanded={showFootprintLens}
            aria-controls={showFootprintLens ? lensId : undefined}
            onClick={() => setLensOpen((prev) => !prev)}
          >
            <span>{showFootprintLens ? t("mapFootprintHide") : t("mapFootprintOpen")}</span>
            <span className={styles.lensChevron} aria-hidden="true">
              {showFootprintLens ? "▴" : "▾"}
            </span>
          </button>
          {showFootprintLens && (
            <FootprintLens
              id={lensId}
              rawPoint={rawPoint}
              harmPoint={harmPoint}
              revealAnswer={revealAnswer}
              rawPoints={rawPoints}
            />
          )}
        </div>
      )}
    </div>
  );
}

export default memo(PointDetail);
