/**
 * ModeBadge — shows the current data source (live archive vs synthetic demo) with shape + colour
 * tokens and lets the user toggle manually between live and demo data.
 */

import { useEffect, useRef, useState } from "react";
import { useCopy } from "../../hooks/useCopy";
import { requestLiveMode } from "../../hooks/useDataModeProbe";
import { useRegionData, useRegions } from "../../hooks/useRegionData";
import { useAppStore } from "../../store/useAppStore";
import styles from "./ModeBadge.module.css";

const NOTICE_DURATION_MS = 5000;

function StatusGlyph({ tone }: { tone: "live" | "demo" | "error" }) {
  if (tone === "demo") {
    return (
      <svg className={styles.glyph} viewBox="0 0 10 10" width="9" height="9" aria-hidden="true">
        <polygon points="5,0.5 9.5,5 5,9.5 0.5,5" fill="currentColor" />
      </svg>
    );
  }
  if (tone === "error") {
    return (
      <svg className={styles.glyph} viewBox="0 0 10 10" width="9" height="9" aria-hidden="true">
        <polygon points="5,1 9.5,9 0.5,9" fill="currentColor" />
      </svg>
    );
  }
  return (
    <svg className={styles.glyph} viewBox="0 0 10 10" width="9" height="9" aria-hidden="true">
      <circle cx="5" cy="5" r="4" fill="currentColor" />
    </svg>
  );
}

export default function ModeBadge() {
  const { t } = useCopy();
  const dataMode = useAppStore((state) => state.dataMode);
  const setDataMode = useAppStore((state) => state.setDataMode);
  const { status, hasData, firstYear, lastYear, region } = useRegionData();
  const { regions, status: regionsStatus } = useRegions();

  const [probing, setProbing] = useState(false);
  const [stayDemoNotice, setStayDemoNotice] = useState(false);
  const mountedRef = useRef(true);
  const probeAbortRef = useRef<AbortController | null>(null);
  const noticeTimerRef = useRef<number | null>(null);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      probeAbortRef.current?.abort();
      if (noticeTimerRef.current !== null) {
        window.clearTimeout(noticeTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (dataMode === "live") {
      setStayDemoNotice(false);
      if (noticeTimerRef.current !== null) {
        window.clearTimeout(noticeTimerRef.current);
        noticeTimerRef.current = null;
      }
    }
  }, [dataMode]);

  const regionSummary = regions.find((r) => r.id === region.id);
  const first = hasData ? firstYear : (regionSummary?.modisYears?.[0] ?? regionSummary?.viirsYears?.[0] ?? null);
  const last = hasData ? lastYear : (regionSummary?.modisYears?.[1] ?? regionSummary?.viirsYears?.[1] ?? null);

  const liveLabel =
    first !== null && last !== null
      ? t("modeBadgeArchiveYears", { first: String(first), last: String(last) })
      : t("modeBadgeArchive");

  const label = probing
    ? t("modeBadgeChecking")
    : dataMode === "demo"
      ? t("modeBadgeDemo")
      : liveLabel;

  const hasError = dataMode === "live" && (status === "error" || regionsStatus === "error");
  const sourceTone: "live" | "demo" | "error" = hasError ? "error" : dataMode;
  const actionLabel = dataMode === "demo" ? t("modeBadgeToLiveAria") : t("modeBadgeToDemoAria");

  const handleToggle = async () => {
    if (probing) return;
    if (dataMode === "live") {
      setStayDemoNotice(false);
      setDataMode("demo", "manual");
      return;
    }

    setStayDemoNotice(false);
    if (noticeTimerRef.current !== null) {
      window.clearTimeout(noticeTimerRef.current);
      noticeTimerRef.current = null;
    }

    probeAbortRef.current?.abort();
    const controller = new AbortController();
    probeAbortRef.current = controller;
    setProbing(true);

    const switched = await requestLiveMode(controller.signal);
    if (!mountedRef.current || controller.signal.aborted) return;

    setProbing(false);
    if (!switched) {
      setStayDemoNotice(true);
      noticeTimerRef.current = window.setTimeout(() => {
        if (mountedRef.current) {
          setStayDemoNotice(false);
        }
        noticeTimerRef.current = null;
      }, NOTICE_DURATION_MS);
    }
  };

  return (
    <div className={styles.wrap}>
      <button
        type="button"
        className={styles.badge}
        data-source={sourceTone}
        aria-label={actionLabel}
        title={actionLabel}
        disabled={probing}
        onClick={() => {
          void handleToggle();
        }}
      >
        <StatusGlyph tone={sourceTone} />
        <span>{label}</span>
      </button>
      {stayDemoNotice && (
        <p className={styles.notice} role="status" aria-live="polite">
          {t("modeBadgeStayDemoNotice")}
        </p>
      )}
    </div>
  );
}
