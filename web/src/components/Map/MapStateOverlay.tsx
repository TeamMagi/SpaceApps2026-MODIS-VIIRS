import { memo } from "react";
import { formatApiError, PIPELINE_COPY } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { APPLIED_PIPELINE_ORDER } from "../../hooks/useHarmonizeTransition";
import type { ApiErrorInfo, MapState } from "../../types";
import type { GeoDatasetStatus } from "./useGeoDataset";
import styles from "./MapStateOverlay.module.css";

export interface MapStateOverlayProps {
  readonly state: MapState;
  readonly error?: ApiErrorInfo | null;
  readonly onRetry?: () => void;
  readonly geoStatus?: GeoDatasetStatus;
  readonly onRetryGeo?: () => void;
  readonly unmappedRegionName?: string | null;
}

function MapStateOverlay({
  state,
  error,
  onRetry,
  geoStatus,
  onRetryGeo,
  unmappedRegionName,
}: MapStateOverlayProps) {
  const { t } = useCopy();

  const showGeoErrorBanner = geoStatus === "error" && state !== "loading";
  const showGeoLoadingBanner = geoStatus === "loading" && state === "ready";
  const showUnmappedBanner =
    Boolean(unmappedRegionName) && geoStatus !== "error" && state !== "loading" && state !== "error";

  return (
    <>
      {(showGeoErrorBanner || showGeoLoadingBanner || showUnmappedBanner) && (
        <div className={styles.noticeStack} data-map-ui="true">
          {showGeoErrorBanner && (
            <div className={styles.banner} role="alert" data-state="error">
              <span className={styles.badgePill}>
                <span className={styles.dot} data-tone="error" aria-hidden="true" />
                <span>{t("errorBadge")}</span>
              </span>
              <span className={styles.bannerText}>{t("mapGeoError")}</span>
              {onRetryGeo && (
                <button type="button" className={styles.retry} onClick={onRetryGeo}>
                  {t("retryAction")}
                </button>
              )}
            </div>
          )}

          {showGeoLoadingBanner && (
            <div className={styles.banner} role="status" data-state="loading">
              <span className={styles.bannerText}>{t("mapGeoLoading")}</span>
            </div>
          )}

          {showUnmappedBanner && unmappedRegionName && (
            <div className={styles.banner} role="status" data-state="notice">
              <span className={styles.bannerText}>
                {t("mapUnmappedNotice", { region: unmappedRegionName })}
              </span>
            </div>
          )}
        </div>
      )}

      {state === "error" && (
        <div className={styles.cover} role="alert" data-state="error" data-map-ui="true">
          <div className={styles.errorCard}>
            <div className={styles.errorHeader}>
              <span className={styles.badgePill}>
                <span className={styles.dot} data-tone="error" aria-hidden="true" />
                <span>{t("errorBadge")}</span>
              </span>
            </div>
            <p className={styles.errorBody}>{formatApiError(error, t)}</p>
            {onRetry && (
              <div className={styles.actions}>
                <button type="button" className={styles.retry} onClick={onRetry}>
                  {t("retryAction")}
                </button>
              </div>
            )}
          </div>
        </div>
      )}

      {state === "empty" && (
        <div className={styles.cover} role="status" data-state="empty">
          <div className={styles.emptyCard} data-map-ui="true">
            <p className={styles.emptyText}>{t("emptyState")}</p>
          </div>
        </div>
      )}

      {state === "loading" && (
        <div className={styles.cover} role="status" data-state="loading" data-map-ui="true">
          <div className={styles.loadingCard}>
            <p className={styles.loadingTitle}>{t("loadingFirms")}</p>
            {geoStatus === "loading" && (
              <p className={styles.geoLoadingSub}>{t("mapGeoLoading")}</p>
            )}
            <div className={styles.shimmerBar} aria-hidden="true" />
            <ol className={styles.stepList}>
              {APPLIED_PIPELINE_ORDER.map((key, idx) => (
                <li key={key} className={styles.stepItem}>
                  <span className={styles.stepIndex} aria-hidden="true">
                    {idx + 1}.
                  </span>
                  <span>{t(PIPELINE_COPY[key].progress)}</span>
                </li>
              ))}
            </ol>
          </div>
        </div>
      )}
    </>
  );
}

export default memo(MapStateOverlay);
