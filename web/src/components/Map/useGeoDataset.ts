import { useCallback, useEffect, useState } from "react";
import { GeoLoadError, loadGeoDataset } from "../../lib/geo/loadGeo";
import type { GeoDataset } from "../../lib/geo/types";

export type GeoDatasetStatus = "loading" | "ready" | "error";

export interface UseGeoDatasetResult {
  readonly status: GeoDatasetStatus;
  readonly dataset: GeoDataset | null;
  readonly error: GeoLoadError | null;
  readonly retry: () => void;
}

let resolvedDataset: GeoDataset | null = null;

export function useGeoDataset(): UseGeoDatasetResult {
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<{
    status: GeoDatasetStatus;
    dataset: GeoDataset | null;
    error: GeoLoadError | null;
  }>(() =>
    resolvedDataset
      ? { status: "ready", dataset: resolvedDataset, error: null }
      : { status: "loading", dataset: null, error: null },
  );

  useEffect(() => {
    if (resolvedDataset && attempt === 0) {
      return;
    }

    const controller = new AbortController();
    setState((prev) =>
      prev.status === "loading" && prev.error === null
        ? prev
        : { status: "loading", dataset: null, error: null },
    );

    loadGeoDataset(controller.signal)
      .then((dataset) => {
        if (controller.signal.aborted) return;
        resolvedDataset = dataset;
        setState({ status: "ready", dataset, error: null });
      })
      .catch((err: unknown) => {
        if (controller.signal.aborted) return;
        const geoErr =
          err instanceof GeoLoadError
            ? err
            : new GeoLoadError("network", "Failed to load geodata.", err);
        setState({ status: "error", dataset: null, error: geoErr });
      });

    return () => {
      controller.abort();
    };
  }, [attempt]);

  const retry = useCallback(() => {
    resolvedDataset = null;
    setAttempt((n) => n + 1);
  }, []);

  return {
    status: state.status,
    dataset: state.dataset,
    error: state.error,
    retry,
  };
}
