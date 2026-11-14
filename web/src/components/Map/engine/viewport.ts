import { fitTransform, type PlaneBounds } from "../../../lib/geo/projection";
import type { ViewportSize, ViewTransform } from "./types";

export type { PlaneBounds, ViewportSize, ViewTransform };

export const MIN_ZOOM_FACTOR = 0.9;
export const MAX_ZOOM_FACTOR = 40;
/** Zoom-out floor relative to the scale that fits the whole map extent (e.g. the world). */
export const EXTENT_MIN_ZOOM_FACTOR = 0.8;

/** Minimum on-screen overlap (px) between the viewport and the clamp bounds, per axis. */
export interface ClampOverlap {
  readonly x: number;
  readonly y: number;
}
export const DEFAULT_FIT_PADDING = 20;

export const IDENTITY_TRANSFORM: ViewTransform = { k: 1, tx: 0, ty: 0 };

export function fitTo(
  bounds: PlaneBounds,
  size: ViewportSize,
  padding: number = DEFAULT_FIT_PADDING,
): ViewTransform {
  const { width, height } = size;
  if (width <= 0 || height <= 0 || !Number.isFinite(width) || !Number.isFinite(height)) {
    return { k: 1, tx: 0, ty: 0 };
  }
  const spanX = bounds.maxX - bounds.minX;
  const spanY = bounds.maxY - bounds.minY;
  if (!Number.isFinite(spanX) || !Number.isFinite(spanY) || (spanX <= 0 && spanY <= 0)) {
    return { k: 1, tx: width / 2, ty: height / 2 };
  }
  const safePad = Math.max(0, Math.min(padding, Math.min(width, height) * 0.4));
  const fit = fitTransform(bounds, width, height, safePad);
  if (fit.k <= 0 || !Number.isFinite(fit.k)) {
    return { k: 1, tx: width / 2, ty: height / 2 };
  }
  return fit;
}

export function clampTransform(
  t: ViewTransform,
  bounds: PlaneBounds,
  size: ViewportSize,
  minK?: number,
  maxK?: number,
  overlap?: ClampOverlap,
): ViewTransform {
  const { width, height } = size;
  if (width <= 0 || height <= 0 || !Number.isFinite(width) || !Number.isFinite(height)) {
    return { k: t.k, tx: t.tx, ty: t.ty };
  }
  const spanX = bounds.maxX - bounds.minX;
  const spanY = bounds.maxY - bounds.minY;
  if (!Number.isFinite(spanX) || !Number.isFinite(spanY)) {
    return { k: t.k, tx: t.tx, ty: t.ty };
  }

  let loK = minK;
  let hiK = maxK;
  if (loK === undefined || hiK === undefined || loK <= 0 || hiK < loK) {
    const baseK = fitTo(bounds, size, DEFAULT_FIT_PADDING).k;
    if (loK === undefined || loK <= 0) loK = baseK * MIN_ZOOM_FACTOR;
    if (hiK === undefined || hiK < loK) hiK = Math.max(loK, baseK * MAX_ZOOM_FACTOR);
  }

  const k = Math.min(Math.max(Number.isFinite(t.k) && t.k > 0 ? t.k : loK, loK), hiK);

  // By default keep at least 25% of the viewport (capped at 64px) overlapping the bbox so it can never be panned off-screen.
  const marginX = Math.min(overlap?.x ?? Math.min(64, width * 0.25), width / 2);
  const marginY = Math.min(overlap?.y ?? Math.min(64, height * 0.25), height / 2);
  const minTx = marginX - bounds.maxX * k;
  const maxTx = width - marginX - bounds.minX * k;
  const minTy = marginY - bounds.maxY * k;
  const maxTy = height - marginY - bounds.minY * k;

  const tx = Number.isFinite(t.tx) ? Math.min(Math.max(t.tx, minTx), maxTx) : (minTx + maxTx) / 2;
  const ty = Number.isFinite(t.ty) ? Math.min(Math.max(t.ty, minTy), maxTy) : (minTy + maxTy) / 2;

  return { k, tx, ty };
}

export function zoomAt(
  t: ViewTransform,
  factor: number,
  cx: number,
  cy: number,
): ViewTransform {
  if (!Number.isFinite(factor) || factor <= 0) {
    return { k: t.k, tx: t.tx, ty: t.ty };
  }
  return {
    k: t.k * factor,
    tx: cx - (cx - t.tx) * factor,
    ty: cy - (cy - t.ty) * factor,
  };
}

export function panBy(t: ViewTransform, dx: number, dy: number): ViewTransform {
  return {
    k: t.k,
    tx: t.tx + dx,
    ty: t.ty + dy,
  };
}

export function screenToPlane(
  t: ViewTransform,
  screenX: number,
  screenY: number,
): readonly [x: number, y: number] {
  const safeK = t.k > 0 ? t.k : 1;
  return [(screenX - t.tx) / safeK, (screenY - t.ty) / safeK];
}

export function planeToScreen(
  t: ViewTransform,
  planeX: number,
  planeY: number,
): readonly [x: number, y: number] {
  return [planeX * t.k + t.tx, planeY * t.k + t.ty];
}
