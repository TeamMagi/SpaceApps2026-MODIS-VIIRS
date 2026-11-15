import { useCallback, useMemo, useState } from "react";
import { downloadBlob, downloadTextFile, mapPointsToCsv } from "../lib/csv";
import { EXPORT_FORMATS, EXPORT_MIME, type ExportFormat } from "../lib/export";
import { exportFileName } from "../lib/format";
import { mapPointsToGeoJson } from "../lib/geojson";
import { fetchExport } from "../services/api";
import { useAppStore } from "../store/useAppStore";
import { useHarmonizeTransition } from "./useHarmonizeTransition";
import { useMapPoints, useRegionData } from "./useRegionData";

export type ExportStatus = "idle" | "loading" | "error";

export interface ExportController {
  /** File name shown before downloading, per format, e.g. "pyrosync_turkey_2021-08.csv". */
  fileNames: Readonly<Record<ExportFormat, string>>;
  statuses: Readonly<Record<ExportFormat, ExportStatus>>;
  canDownload: boolean;
  /** True when the last export failed; the message itself comes from copy (`exportFailed`). */
  failed: boolean;
  download: (format: ExportFormat) => void;
}

const IDLE: Record<ExportFormat, ExportStatus> = { csv: "idle", geojson: "idle" };

/**
 * "Download this view" for the map's current period, in the current (revealed) mode. Live mode asks
 * the backend for the full archive export (not the map's downsampled points); demo mode builds the
 * file from the synthetic fixtures in the browser.
 */
export function useExportView(): ExportController {
  const dataMode = useAppStore((state) => state.dataMode);
  const overrides = useAppStore((state) => state.pipelineOverrides);
  const serverConfig = useAppStore((state) => state.serverConfig);
  const minConfidence = useAppStore((state) => state.minConfidence);
  const fusionModel = useAppStore((state) => state.fusionModel);
  const { region } = useRegionData();
  const points = useMapPoints();
  const revealAnswer = useHarmonizeTransition((v) => v.revealAnswer);
  const [statuses, setStatuses] = useState<Record<ExportFormat, ExportStatus>>(IDLE);
  const isDemo = dataMode === "demo";
  const canDownload = points.hasData && (revealAnswer ? points.harmonized.length > 0 : points.raw.length > 0);

  const fileNames = useMemo(
    () => ({
      csv: exportFileName(region.slug, points.period, "csv", isDemo, !revealAnswer),
      geojson: exportFileName(region.slug, points.period, "geojson", isDemo, !revealAnswer),
    }),
    [region.slug, points.period, isDemo, revealAnswer],
  );

  const setStatus = useCallback((format: ExportFormat, status: ExportStatus) => {
    setStatuses((prev) => ({ ...prev, [format]: status }));
  }, []);

  const download = useCallback(
    (format: ExportFormat) => {
      if (!points.hasData || points.period.year === null || statuses[format] === "loading") return;
      const fileName = fileNames[format];
      if (isDemo) {
        if (format === "csv") {
          downloadTextFile(fileName, mapPointsToCsv(points, revealAnswer, true), EXPORT_MIME.csv);
        } else {
          downloadTextFile(fileName, JSON.stringify(mapPointsToGeoJson(points, revealAnswer, true)), EXPORT_MIME.geojson);
        }
        return;
      }
      setStatus(format, "loading");
      fetchExport(
        {
          region: region.id,
          year: points.period.year,
          month: points.period.month,
          overrides,
          defaults: serverConfig ?? undefined,
          minConfidence,
          fusionModel,
        },
        format,
        revealAnswer ? "harmonized" : "raw",
        fileName,
      )
        .then((file) => {
          downloadBlob(file.fileName, file.blob);
          setStatus(format, "idle");
        })
        .catch(() => setStatus(format, "error"));
    },
    [points, statuses, fileNames, isDemo, revealAnswer, setStatus, region.id, overrides, serverConfig, minConfidence, fusionModel],
  );

  const failed = EXPORT_FORMATS.some((format) => statuses[format] === "error");
  return { fileNames, statuses, canDownload, failed, download };
}
