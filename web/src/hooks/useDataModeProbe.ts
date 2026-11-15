import { useEffect } from "react";
import { decideNextDataMode } from "../lib/dataMode";
import { isAbortError, probeHealth } from "../services/api";
import { useAppStore } from "../store/useAppStore";

export const AUTO_DEMO_REPROBE_INTERVAL_MS = 30_000;

function applyProbeOutcome(
  outcome: Awaited<ReturnType<typeof probeHealth>>,
  forceLive = false,
): boolean {
  const state = useAppStore.getState();
  const next = decideNextDataMode(
    { dataMode: state.dataMode, dataModeSource: state.dataModeSource },
    { ...outcome, forceLive },
  );
  if (next.dataMode !== state.dataMode || next.dataModeSource !== state.dataModeSource) {
    state.setDataMode(next.dataMode, next.dataModeSource);
  }
  return next.dataMode === "live";
}

/** User-triggered probe when asking to leave demo mode for the live backend. */
export async function requestLiveMode(signal?: AbortSignal): Promise<boolean> {
  const outcome = await probeHealth(signal);
  return applyProbeOutcome(outcome, true);
}

/**
 * Probes `/api/health` once at startup (unless the URL opened in manual demo) and polls every 30 s
 * while in auto-demo so the app returns to live data once the backend answers. Mount once (App does).
 */
export function useDataModeProbe(): void {
  const dataMode = useAppStore((state) => state.dataMode);
  const dataModeSource = useAppStore((state) => state.dataModeSource);

  useEffect(() => {
    const initial = useAppStore.getState();
    if (initial.dataMode === "demo" && initial.dataModeSource === "manual") {
      return;
    }

    const controller = new AbortController();
    probeHealth(controller.signal)
      .then((outcome) => {
        if (!controller.signal.aborted) {
          applyProbeOutcome(outcome);
        }
      })
      .catch((err: unknown) => {
        if (!isAbortError(err) && !controller.signal.aborted) {
          applyProbeOutcome({ source: "probe", ok: false, status: null });
        }
      });

    return () => {
      controller.abort();
    };
  }, []);

  useEffect(() => {
    if (dataMode !== "demo" || dataModeSource !== "auto") {
      return;
    }

    let inFlight: AbortController | null = null;
    const runProbe = () => {
      if (inFlight) return;
      const controller = new AbortController();
      inFlight = controller;
      probeHealth(controller.signal)
        .then((outcome) => {
          if (!controller.signal.aborted) {
            applyProbeOutcome(outcome);
          }
        })
        .catch((err: unknown) => {
          if (!isAbortError(err) && !controller.signal.aborted) {
            applyProbeOutcome({ source: "probe", ok: false, status: null });
          }
        })
        .finally(() => {
          if (inFlight === controller) {
            inFlight = null;
          }
        });
    };

    const intervalId = window.setInterval(runProbe, AUTO_DEMO_REPROBE_INTERVAL_MS);
    return () => {
      window.clearInterval(intervalId);
      inFlight?.abort();
      inFlight = null;
    };
  }, [dataMode, dataModeSource]);
}
