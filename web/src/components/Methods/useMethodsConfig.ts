import { useCallback, useEffect, useState } from "react";
import { isAbortError } from "../../services/api";
import { DEFAULT_PIPELINE_CONFIG, fetchPipelineConfig, type PipelineConfig } from "../../services/pipelineConfig";

export type ConfigStatus = "loading" | "live" | "fallback";

export interface MethodsConfig {
  config: PipelineConfig;
  status: ConfigStatus;
  retry: () => void;
}

/** Live server thresholds when reachable; the documented code defaults otherwise, so the page never blocks on the API. */
export function useMethodsConfig(): MethodsConfig {
  const [config, setConfig] = useState<PipelineConfig>(DEFAULT_PIPELINE_CONFIG);
  const [status, setStatus] = useState<ConfigStatus>("loading");
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    const controller = new AbortController();
    setStatus("loading");
    fetchPipelineConfig(controller.signal).then(
      (live) => {
        setConfig(live);
        setStatus("live");
      },
      (error: unknown) => {
        if (isAbortError(error)) return;
        setConfig(DEFAULT_PIPELINE_CONFIG);
        setStatus("fallback");
      },
    );
    return () => controller.abort();
  }, [attempt]);

  const retry = useCallback(() => setAttempt((n) => n + 1), []);
  return { config, status, retry };
}
