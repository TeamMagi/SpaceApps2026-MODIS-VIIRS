export type ExportFormat = "csv" | "geojson";
export type ExportViewKind = "harmonized" | "raw";

export const EXPORT_FORMATS: readonly ExportFormat[] = ["csv", "geojson"];

export const EXPORT_MIME: Readonly<Record<ExportFormat, string>> = {
  csv: "text/csv;charset=utf-8",
  geojson: "application/geo+json;charset=utf-8",
};

/** File name from a Content-Disposition header (RFC 6266 `filename*` first); null when absent or unusable. */
export function parseContentDispositionFileName(header: string | null | undefined): string | null {
  if (!header) return null;
  const extended = /filename\*\s*=\s*([^']*)'[^']*'([^;]+)/i.exec(header);
  let raw: string | null = null;
  if (extended) {
    try {
      raw = decodeURIComponent(extended[2].trim());
    } catch {
      raw = null;
    }
  }
  if (raw === null) {
    const plain = /filename\s*=\s*(?:"((?:[^"\\]|\\.)*)"|([^;]+))/i.exec(header);
    raw = plain ? (plain[1] !== undefined ? plain[1].replace(/\\(.)/g, "$1") : plain[2].trim()) : null;
  }
  if (raw === null) return null;
  const name = raw.replace(/[\\/]/g, "_").trim();
  return name === "" || name === "." || name === ".." ? null : name;
}
