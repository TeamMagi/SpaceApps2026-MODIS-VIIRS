import type { GeoDataset, GeoFeature, GeoMeta, Position } from "./types.ts";

export type GeoLoadErrorReason = "network" | "invalid";

export class GeoLoadError extends Error {
  readonly reason: GeoLoadErrorReason;
  readonly cause?: unknown;

  constructor(reason: GeoLoadErrorReason, message: string, cause?: unknown) {
    super(message);
    this.name = "GeoLoadError";
    this.reason = reason;
    this.cause = cause;
  }
}

let cachedPromise: Promise<GeoDataset> | null = null;

function getGeoUrl(): string {
  const base = import.meta.env.BASE_URL;
  const prefix = base.endsWith("/") ? base : `${base}/`;
  return `${prefix}geo/regions.json`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isValidMeta(value: unknown): value is GeoMeta {
  if (!isRecord(value)) return false;
  return (
    typeof value["package"] === "string" &&
    typeof value["packageVersion"] === "string" &&
    typeof value["source"] === "string" &&
    typeof value["sourceVersion"] === "string" &&
    typeof value["license"] === "string" &&
    typeof value["resolution"] === "string" &&
    typeof value["simplification"] === "string"
  );
}

function isValidPosition(value: unknown): value is Position {
  return (
    Array.isArray(value) &&
    value.length >= 2 &&
    typeof value[0] === "number" &&
    Number.isFinite(value[0]) &&
    typeof value[1] === "number" &&
    Number.isFinite(value[1])
  );
}

function isValidFeature(value: unknown): value is GeoFeature {
  if (!isRecord(value)) return false;
  if (typeof value["code"] !== "string" || value["code"].length === 0) return false;
  if (typeof value["name"] !== "string") return false;
  const regionId = value["regionId"];
  if (regionId !== null && typeof regionId !== "string") return false;

  const bbox = value["bbox"];
  if (
    !Array.isArray(bbox) ||
    bbox.length !== 4 ||
    !bbox.every((coord) => typeof coord === "number" && Number.isFinite(coord))
  ) {
    return false;
  }

  const polygons = value["polygons"];
  if (!Array.isArray(polygons)) return false;
  return polygons.every(
    (poly) =>
      Array.isArray(poly) &&
      poly.every((ring) => Array.isArray(ring) && ring.every(isValidPosition)),
  );
}

function isValidDataset(value: unknown): value is GeoDataset {
  if (!isRecord(value)) return false;
  if (!isValidMeta(value["meta"])) return false;
  const features = value["features"];
  return Array.isArray(features) && features.every(isValidFeature);
}

async function fetchAndValidateGeoDataset(): Promise<GeoDataset> {
  let response: Response;
  try {
    response = await fetch(getGeoUrl());
  } catch (err) {
    throw new GeoLoadError("network", "Failed to fetch geodata.", err);
  }

  if (!response.ok) {
    throw new GeoLoadError(
      "network",
      `Failed to fetch geodata: HTTP ${response.status} ${response.statusText}`.trim(),
    );
  }

  let payload: unknown;
  try {
    payload = await response.json();
  } catch (err) {
    throw new GeoLoadError("invalid", "Geodata response is not valid JSON.", err);
  }

  if (!isValidDataset(payload)) {
    throw new GeoLoadError("invalid", "Geodata payload failed schema validation.");
  }

  return payload;
}

function createAbortError(signal: AbortSignal): unknown {
  return signal.reason !== undefined
    ? signal.reason
    : new DOMException("The operation was aborted.", "AbortError");
}

export function loadGeoDataset(signal?: AbortSignal): Promise<GeoDataset> {
  if (signal?.aborted) {
    return Promise.reject(createAbortError(signal));
  }

  if (cachedPromise === null) {
    cachedPromise = fetchAndValidateGeoDataset().catch((err) => {
      // Clear cache on failure so subsequent calls can retry.
      cachedPromise = null;
      throw err;
    });
  }

  if (!signal) {
    return cachedPromise;
  }

  // Decouple caller abort from the shared fetch promise so one aborted caller never poisons the cache.
  const activePromise = cachedPromise;
  return new Promise<GeoDataset>((resolve, reject) => {
    const cleanup = () => {
      signal.removeEventListener("abort", onAbort);
    };
    const onAbort = () => {
      cleanup();
      reject(createAbortError(signal));
    };

    signal.addEventListener("abort", onAbort, { once: true });

    activePromise.then(
      (dataset) => {
        cleanup();
        if (!signal.aborted) {
          resolve(dataset);
        }
      },
      (err) => {
        cleanup();
        if (!signal.aborted) {
          reject(err);
        }
      },
    );
  });
}
