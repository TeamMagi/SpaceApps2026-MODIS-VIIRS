import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import type { PlaneBounds, ViewportSize, ViewTransform } from "./types";
import {
  clampTransform,
  DEFAULT_FIT_PADDING,
  EXTENT_MIN_ZOOM_FACTOR,
  fitTo,
  IDENTITY_TRANSFORM,
  MAX_ZOOM_FACTOR,
  MIN_ZOOM_FACTOR,
  panBy,
  zoomAt,
} from "./viewport";

const ZOOM_BUTTON_STEP = 1.5;
const DOUBLE_ZOOM_STEP = 1.8;
const PAN_KEY_STEP = 48;
const DOUBLE_TAP_MS = 300;
const DOUBLE_TAP_PX = 24;

export interface UseMapViewportOptions {
  /** What fit/reset frames. */
  readonly bounds: PlaneBounds | null;
  /** Everything drawable (e.g. the whole world): zoom-out floor and pan limit. Defaults to `bounds`. */
  readonly extent?: PlaneBounds | null;
  readonly size: ViewportSize;
  readonly resetKey?: string | number;
  readonly padding?: number;
}

export interface MapViewportBind {
  readonly onPointerDown: (e: ReactPointerEvent<HTMLElement>) => void;
  readonly onPointerMove: (e: ReactPointerEvent<HTMLElement>) => void;
  readonly onPointerUp: (e: ReactPointerEvent<HTMLElement>) => void;
  readonly onPointerCancel: (e: ReactPointerEvent<HTMLElement>) => void;
  readonly onLostPointerCapture: (e: ReactPointerEvent<HTMLElement>) => void;
  readonly onDoubleClick: (e: ReactMouseEvent<HTMLElement>) => void;
  readonly onKeyDown: (e: ReactKeyboardEvent<HTMLElement>) => void;
}

export interface UseMapViewportResult {
  readonly transform: ViewTransform;
  readonly fit: () => void;
  readonly zoomIn: () => void;
  readonly zoomOut: () => void;
  readonly reset: () => void;
  readonly canZoomIn: boolean;
  readonly canZoomOut: boolean;
  readonly isAtFit: boolean;
  readonly bind: MapViewportBind;
  readonly attachWheel: (el: HTMLElement | null) => void;
}

interface PointerPoint {
  x: number;
  y: number;
}

function sameTransform(a: ViewTransform, b: ViewTransform): boolean {
  return (
    Math.abs(a.k - b.k) < 1e-6 &&
    Math.abs(a.tx - b.tx) < 0.05 &&
    Math.abs(a.ty - b.ty) < 0.05
  );
}

export function useMapViewport({
  bounds,
  extent = null,
  size,
  resetKey = "",
  padding = DEFAULT_FIT_PADDING,
}: UseMapViewportOptions): UseMapViewportResult {
  const { width, height } = size;
  const minX = bounds?.minX ?? 0;
  const minY = bounds?.minY ?? 0;
  const maxX = bounds?.maxX ?? 0;
  const maxY = bounds?.maxY ?? 0;
  const hasValidBounds = bounds !== null && Number.isFinite(minX) && Number.isFinite(maxX);

  const scopeKey = `${String(resetKey)}:${hasValidBounds ? `${minX},${minY},${maxX},${maxY}` : "none"}:${padding}`;

  const fitView = useMemo<ViewTransform>(() => {
    if (!bounds || !hasValidBounds || width <= 0 || height <= 0) {
      return IDENTITY_TRANSFORM;
    }
    return fitTo(bounds, { width, height }, padding);
  }, [bounds, hasValidBounds, width, height, padding]);

  const extentValid =
    extent !== null &&
    [extent.minX, extent.minY, extent.maxX, extent.maxY].every(Number.isFinite);
  const extMinX = extentValid ? Math.min(extent.minX, minX) : minX;
  const extMinY = extentValid ? Math.min(extent.minY, minY) : minY;
  const extMaxX = extentValid ? Math.max(extent.maxX, maxX) : maxX;
  const extMaxY = extentValid ? Math.max(extent.maxY, maxY) : maxY;
  const panBounds = useMemo<PlaneBounds>(
    () => ({ minX: extMinX, minY: extMinY, maxX: extMaxX, maxY: extMaxY }),
    [extMinX, extMinY, extMaxX, extMaxY],
  );

  const extentFitK =
    hasValidBounds && width > 0 && height > 0 ? fitTo(panBounds, { width, height }, padding).k : 0;
  const minK =
    extentFitK > 0
      ? Math.min(fitView.k * MIN_ZOOM_FACTOR, extentFitK * EXTENT_MIN_ZOOM_FACTOR)
      : fitView.k * MIN_ZOOM_FACTOR;
  const maxK = Math.max(minK, fitView.k * MAX_ZOOM_FACTOR);

  // With an extent the view centre must stay over it, so zooming far out or panning never loses the map.
  const clampView = useCallback(
    (t: ViewTransform): ViewTransform =>
      clampTransform(
        t,
        panBounds,
        { width, height },
        minK,
        maxK,
        extentValid ? { x: width / 2, y: height / 2 } : undefined,
      ),
    [panBounds, width, height, minK, maxK, extentValid],
  );

  const [viewState, setViewState] = useState<{
    scope: string;
    width: number;
    height: number;
    transform: ViewTransform;
  }>({
    scope: scopeKey,
    width,
    height,
    transform: fitView,
  });

  let activeTransform: ViewTransform;
  if (viewState.scope !== scopeKey) {
    activeTransform = fitView;
  } else if (width <= 0 || height <= 0 || !bounds || !hasValidBounds) {
    activeTransform =
      viewState.width > 0 && viewState.height > 0 ? viewState.transform : fitView;
  } else if (viewState.width !== width || viewState.height !== height) {
    if (viewState.width <= 0 || viewState.height <= 0) {
      activeTransform = fitView;
    } else {
      const prevFit = fitTo(
        bounds,
        { width: viewState.width, height: viewState.height },
        padding,
      );
      activeTransform = sameTransform(viewState.transform, prevFit)
        ? fitView
        : clampView(viewState.transform);
    }
  } else {
    activeTransform = viewState.transform;
  }

  const transformRef = useRef<ViewTransform>(fitView);
  const scopeRef = useRef<string>(scopeKey);
  const sizeRef = useRef<ViewportSize>({ width, height });
  const rafRef = useRef<number | null>(null);
  const pointersRef = useRef<Map<number, PointerPoint>>(new Map());
  const lastTapRef = useRef<{ time: number; x: number; y: number }>({
    time: 0,
    x: 0,
    y: 0,
  });

  const ensureCurrent = useCallback((): ViewTransform => {
    if (scopeRef.current !== scopeKey) {
      scopeRef.current = scopeKey;
      sizeRef.current = { width, height };
      transformRef.current = fitView;
    } else if (
      (sizeRef.current.width !== width || sizeRef.current.height !== height) &&
      width > 0 &&
      height > 0
    ) {
      const prevSize = sizeRef.current;
      sizeRef.current = { width, height };
      if (prevSize.width <= 0 || prevSize.height <= 0 || !bounds || !hasValidBounds) {
        transformRef.current = fitView;
      } else {
        const prevFit = fitTo(bounds, prevSize, padding);
        transformRef.current = sameTransform(transformRef.current, prevFit)
          ? fitView
          : clampView(transformRef.current);
      }
    }
    return transformRef.current;
  }, [scopeKey, width, height, bounds, hasValidBounds, padding, fitView, clampView]);

  const cancelPendingRaf = useCallback(() => {
    if (rafRef.current !== null && typeof cancelAnimationFrame !== "undefined") {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
  }, []);

  useEffect(() => {
    const scopeChanged = scopeRef.current !== scopeKey;
    const sizeChanged =
      sizeRef.current.width !== width || sizeRef.current.height !== height;
    if (scopeChanged) {
      cancelPendingRaf();
      pointersRef.current.clear();
      scopeRef.current = scopeKey;
      sizeRef.current = { width, height };
      transformRef.current = fitView;
      setViewState({ scope: scopeKey, width, height, transform: fitView });
      return;
    }
    if (sizeChanged && width > 0 && height > 0) {
      cancelPendingRaf();
      sizeRef.current = { width, height };
      transformRef.current = activeTransform;
      setViewState({ scope: scopeKey, width, height, transform: activeTransform });
    }
  }, [scopeKey, width, height, fitView, activeTransform, cancelPendingRaf]);

  useEffect(() => cancelPendingRaf, [cancelPendingRaf]);

  // Coalesce pointer/wheel updates to at most one React state update per animation frame.
  const publishTransform = useCallback(
    (next: ViewTransform, immediate: boolean = false) => {
      scopeRef.current = scopeKey;
      sizeRef.current = { width, height };
      transformRef.current = next;
      if (immediate || typeof requestAnimationFrame === "undefined") {
        cancelPendingRaf();
        setViewState({ scope: scopeKey, width, height, transform: next });
        return;
      }
      if (rafRef.current === null) {
        rafRef.current = requestAnimationFrame(() => {
          rafRef.current = null;
          setViewState({
            scope: scopeRef.current,
            width: sizeRef.current.width,
            height: sizeRef.current.height,
            transform: transformRef.current,
          });
        });
      }
    },
    [scopeKey, width, height, cancelPendingRaf],
  );

  const zoomAround = useCallback(
    (factor: number, cx: number, cy: number, immediate: boolean = false): boolean => {
      if (!bounds || width <= 0 || height <= 0) return false;
      const cur = ensureCurrent();
      const targetK = Math.min(Math.max(cur.k * factor, minK), maxK);
      const effectiveFactor = cur.k > 0 ? targetK / cur.k : 1;
      if (Math.abs(effectiveFactor - 1) < 1e-5) return false;
      const next = clampView(zoomAt(cur, effectiveFactor, cx, cy));
      if (sameTransform(cur, next)) return false;
      publishTransform(next, immediate);
      return true;
    },
    [bounds, width, height, minK, maxK, clampView, ensureCurrent, publishTransform],
  );

  const fit = useCallback(() => {
    pointersRef.current.clear();
    publishTransform(fitView, true);
  }, [fitView, publishTransform]);

  const zoomIn = useCallback(() => {
    zoomAround(ZOOM_BUTTON_STEP, width / 2, height / 2, true);
  }, [zoomAround, width, height]);

  const zoomOut = useCallback(() => {
    zoomAround(1 / ZOOM_BUTTON_STEP, width / 2, height / 2, true);
  }, [zoomAround, width, height]);

  const onPointerDown = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      if (!bounds || width <= 0 || height <= 0) return;
      if (e.pointerType === "mouse" && e.button !== 0) return;

      const rect = e.currentTarget.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;

      if (e.pointerType === "touch" && pointersRef.current.size === 0) {
        const now = performance.now();
        const prevTap = lastTapRef.current;
        if (
          now - prevTap.time <= DOUBLE_TAP_MS &&
          Math.hypot(x - prevTap.x, y - prevTap.y) <= DOUBLE_TAP_PX
        ) {
          lastTapRef.current = { time: 0, x: 0, y: 0 };
          zoomAround(DOUBLE_ZOOM_STEP, x, y, true);
          return;
        }
        lastTapRef.current = { time: now, x, y };
      }

      try {
        e.currentTarget.setPointerCapture(e.pointerId);
      } catch {
        // Synthetic or detached targets in tests may not support pointer capture.
      }
      pointersRef.current.set(e.pointerId, { x, y });
    },
    [bounds, width, height, zoomAround],
  );

  const onPointerMove = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      if (!bounds || width <= 0 || height <= 0) return;
      const pointers = pointersRef.current;
      if (!pointers.has(e.pointerId)) return;

      const rect = e.currentTarget.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;

      if (pointers.size === 1) {
        const prev = pointers.get(e.pointerId);
        if (!prev) return;
        const dx = x - prev.x;
        const dy = y - prev.y;
        pointers.set(e.pointerId, { x, y });
        if (dx === 0 && dy === 0) return;
        const cur = ensureCurrent();
        publishTransform(clampView(panBy(cur, dx, dy)));
        return;
      }

      const entries = Array.from(pointers.entries());
      const [id1, p1Prev] = entries[0];
      const [id2, p2Prev] = entries[1];
      pointers.set(e.pointerId, { x, y });
      const p1Next = pointers.get(id1) ?? p1Prev;
      const p2Next = pointers.get(id2) ?? p2Prev;

      const prevCx = (p1Prev.x + p2Prev.x) / 2;
      const prevCy = (p1Prev.y + p2Prev.y) / 2;
      const nextCx = (p1Next.x + p2Next.x) / 2;
      const nextCy = (p1Next.y + p2Next.y) / 2;

      const prevDist = Math.hypot(p2Prev.x - p1Prev.x, p2Prev.y - p1Prev.y);
      const nextDist = Math.hypot(p2Next.x - p1Next.x, p2Next.y - p1Next.y);

      const cur = ensureCurrent();
      let stepped = panBy(cur, nextCx - prevCx, nextCy - prevCy);

      if (prevDist > 4 && nextDist > 4) {
        const rawFactor = nextDist / prevDist;
        const targetK = Math.min(Math.max(stepped.k * rawFactor, minK), maxK);
        const effectiveFactor = stepped.k > 0 ? targetK / stepped.k : 1;
        stepped = zoomAt(stepped, effectiveFactor, nextCx, nextCy);
      }

      publishTransform(clampView(stepped));
    },
    [bounds, width, height, minK, maxK, clampView, ensureCurrent, publishTransform],
  );

  const releasePointer = useCallback((e: ReactPointerEvent<HTMLElement>) => {
    pointersRef.current.delete(e.pointerId);
  }, []);

  const [wheelEl, setWheelEl] = useState<HTMLElement | null>(null);
  const attachWheel = useCallback((el: HTMLElement | null) => {
    setWheelEl(el);
  }, []);

  useEffect(() => {
    if (!wheelEl) return;
    const handleWheel = (e: WheelEvent) => {
      const targetEl =
        e.target instanceof Element
          ? e.target
          : e.target instanceof Node
            ? e.target.parentElement
            : null;
      if (targetEl && targetEl.closest("[data-map-ui]") !== null) return;
      if (!bounds || width <= 0 || height <= 0) return;

      const modeScale = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 100 : 1;
      const dy = e.deltaY * modeScale;
      if (dy === 0 || !Number.isFinite(dy)) return;

      const sensitivity = e.ctrlKey || e.metaKey ? 0.008 : 0.0022;
      const factor = Math.exp(-dy * sensitivity);
      const rect = wheelEl.getBoundingClientRect();
      const cx = e.clientX - rect.left;
      const cy = e.clientY - rect.top;

      // Only prevent page scroll when the map actually consumes the wheel delta.
      const consumed = zoomAround(factor, cx, cy, false);
      if (consumed) {
        e.preventDefault();
      }
    };
    wheelEl.addEventListener("wheel", handleWheel, { passive: false });
    return () => {
      wheelEl.removeEventListener("wheel", handleWheel);
    };
  }, [wheelEl, bounds, width, height, zoomAround]);

  const onDoubleClick = useCallback(
    (e: ReactMouseEvent<HTMLElement>) => {
      if (!bounds || width <= 0 || height <= 0) return;
      const rect = e.currentTarget.getBoundingClientRect();
      const cx = e.clientX - rect.left;
      const cy = e.clientY - rect.top;
      zoomAround(DOUBLE_ZOOM_STEP, cx, cy, true);
    },
    [bounds, width, height, zoomAround],
  );

  const onKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLElement>) => {
      if (e.altKey || e.ctrlKey || e.metaKey) return;
      if (!bounds || width <= 0 || height <= 0) return;

      switch (e.key) {
        case "ArrowLeft":
        case "ArrowRight":
        case "ArrowUp":
        case "ArrowDown": {
          e.preventDefault();
          const dx = e.key === "ArrowLeft" ? PAN_KEY_STEP : e.key === "ArrowRight" ? -PAN_KEY_STEP : 0;
          const dy = e.key === "ArrowUp" ? PAN_KEY_STEP : e.key === "ArrowDown" ? -PAN_KEY_STEP : 0;
          const cur = ensureCurrent();
          publishTransform(clampView(panBy(cur, dx, dy)), true);
          break;
        }
        case "+":
        case "=": {
          e.preventDefault();
          zoomIn();
          break;
        }
        case "-":
        case "_": {
          e.preventDefault();
          zoomOut();
          break;
        }
        case "0": {
          e.preventDefault();
          fit();
          break;
        }
      }
    },
    [bounds, width, height, clampView, ensureCurrent, publishTransform, zoomIn, zoomOut, fit],
  );

  const bind = useMemo<MapViewportBind>(
    () => ({
      onPointerDown,
      onPointerMove,
      onPointerUp: releasePointer,
      onPointerCancel: releasePointer,
      onLostPointerCapture: releasePointer,
      onDoubleClick,
      onKeyDown,
    }),
    [onPointerDown, onPointerMove, releasePointer, onDoubleClick, onKeyDown],
  );

  const canZoomIn = hasValidBounds && width > 0 && height > 0 && activeTransform.k < maxK - 1e-4;
  const canZoomOut = hasValidBounds && width > 0 && height > 0 && activeTransform.k > minK + 1e-4;
  const isAtFit = sameTransform(activeTransform, fitView);

  return {
    transform: activeTransform,
    fit,
    zoomIn,
    zoomOut,
    reset: fit,
    canZoomIn,
    canZoomOut,
    isAtFit,
    bind,
    attachWheel,
  };
}
