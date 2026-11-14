import { useCallback } from "react";
import { downloadTextFile, mapPointsToCsv } from "../lib/csv";
import { csvFileName } from "../lib/format";
import type { AsyncStatus } from "../types";
import { useHarmonizeTransition } from "./useHarmonizeTransition";
import { useMapPoints, useRegionData } from "./useRegionData";

export interface ExportView {
  /** File name shown before downloading, e.g. "pyrosync_turkey_2021-08.csv". */
  fileName: string;
  status: AsyncStatus;
  canDownload: boolean;
  download: () => void;
}

/** "Download this view (CSV)": the map's current points in the current (revealed) mode. */
export function useExportView(): ExportView {
  const { region } = useRegionData();
  const points = useMapPoints();
  const revealAnswer = useHarmonizeTransition((v) => v.revealAnswer);
  const fileName = csvFileName(region.slug, points.period);
  const canDownload = points.hasData && (revealAnswer ? points.harmonized.length > 0 : points.raw.length > 0);

  const download = useCallback(() => {
    if (!points.hasData) return;
    downloadTextFile(fileName, mapPointsToCsv(points, revealAnswer));
  }, [fileName, points, revealAnswer]);

  return { fileName, status: points.status, canDownload, download };
}
