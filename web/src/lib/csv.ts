import type { MapPointSet } from "../types";

type Cell = string | number | null;

function escapeCell(value: Cell): string {
  if (value === null) return "";
  const text = String(value);
  return /[",\n\r]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

export function toCsv(header: readonly string[], rows: readonly (readonly Cell[])[]): string {
  return [header, ...rows].map((row) => row.map(escapeCell).join(",")).join("\r\n");
}

/**
 * CSV of what the map currently shows: raw detections, or harmonized detections once the answer is
 * revealed. The raw file leaves out the merge links, which would give the answer away.
 */
export function mapPointsToCsv(points: MapPointSet, harmonized: boolean): string {
  if (harmonized) {
    return toCsv(
      ["id", "lat", "lon", "frp_mw", "frp_corrected_mw", "source", "confidence", "daynight", "acquired_utc"],
      points.harmonized.map((p) => [
        p.id,
        p.lat,
        p.lon,
        p.frp,
        p.frpCorrected,
        p.source,
        p.confidence,
        p.daynight,
        p.acquired,
      ]),
    );
  }
  return toCsv(
    ["id", "lat", "lon", "sensor", "frp_mw", "confidence", "daynight", "acquired_utc"],
    points.raw.map((p) => [p.id, p.lat, p.lon, p.sensor, p.frp, p.confidence, p.daynight, p.acquired]),
  );
}

export function downloadTextFile(fileName: string, text: string, mimeType = "text/csv;charset=utf-8"): void {
  // BOM so spreadsheet apps read Turkish characters as UTF-8.
  const blob = new Blob(["\uFEFF", text], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = fileName;
  document.body.appendChild(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}
