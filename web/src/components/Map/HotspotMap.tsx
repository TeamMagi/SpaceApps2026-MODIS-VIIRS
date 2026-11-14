import {
  memo,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useMapPoints, useRegionData } from "../../hooks/useRegionData";
import {
  createProjection,
  padBBox,
  planeBounds,
  regionExtent,
  type GeoBBox,
  type PlaneBounds,
} from "../../lib/geo/projection";
import { featuresForRegion, isMappedRegion } from "../../lib/geo/regionCodes";
import { useAppStore } from "../../store/useAppStore";
import type { HarmonizedPoint, MapState, RawPoint } from "../../types";
import { HIT_TOLERANCE_PX, nearest, orderedForKeyboard } from "./engine/pointIndex";
import { buildScene, drawMap, readPalette } from "./engine/renderer";
import {
  PointFlag,
  type DrawStats,
  type MapScene,
  type ScenePointInput,
  type ViewportSize,
} from "./engine/types";
import { useMapViewport } from "./engine/useMapViewport";
import { planeToScreen, screenToPlane } from "./engine/viewport";
import MapControls from "./MapControls";
import MapLabels from "./MapLabels";
import MapLegend from "./MapLegend";
import MapStateOverlay from "./MapStateOverlay";
import MapStatusBar from "./MapStatusBar";
import PointDetail, { formatPointAnnouncement } from "./PointDetail";
import { useGeoDataset } from "./useGeoDataset";
import styles from "./HotspotMap.module.css";

const VIEWPORT_FIT_PADDING = 28;
const IN_VIEW_MARGIN_PX = 12;
const TAP_MOVE_THRESHOLD_PX = 6;
const DOUBLE_TAP_MS = 300;
const DOUBLE_TAP_PX = 24;

function readDpr(): number {
  return typeof window !== "undefined" &&
    Number.isFinite(window.devicePixelRatio) &&
    window.devicePixelRatio > 0
    ? window.devicePixelRatio
    : 1;
}

function isUiTarget(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest("button, [data-map-ui]") !== null;
}

function HotspotMap() {
  const copy = useCopy();
  const { t, fmt } = copy;
  const { region } = useRegionData();
  const points = useMapPoints();
  const geo = useGeoDataset();
  const { geometryHarmonized, showDuplicateRings, revealAnswer } = useHarmonizeTransition((v) => ({
    geometryHarmonized: v.geometryHarmonized,
    showDuplicateRings: v.showDuplicateRings,
    revealAnswer: v.revealAnswer,
  }));
  const selectedMonth = useAppStore((state) => state.selectedMonth);
  const selectMonth = useAppStore((state) => state.selectMonth);
  const theme = useAppStore((state) => state.theme);
  const lang = useAppStore((state) => state.lang);

  const summaryId = useId();
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  const [canvasSize, setCanvasSize] = useState<ViewportSize>({ width: 0, height: 0 });
  const [dpr, setDpr] = useState<number>(readDpr);
  const [paletteVersion, setPaletteVersion] = useState(0);

  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;

    const measure = () => {
      const rect = el.getBoundingClientRect();
      const width = Math.round(rect.width);
      const height = Math.round(rect.height);
      setCanvasSize((prev) =>
        prev.width === width && prev.height === height ? prev : { width, height },
      );
    };

    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => measure());
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const syncDpr = () => {
      const next = readDpr();
      setDpr((prev) => (prev === next ? prev : next));
    };
    syncDpr();
    if (typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia(`(resolution: ${dpr}dppx)`);
    mq.addEventListener("change", syncDpr);
    return () => mq.removeEventListener("change", syncDpr);
  }, [dpr]);

  useEffect(() => {
    if (typeof MutationObserver === "undefined" || typeof document === "undefined") return;
    const bump = () => setPaletteVersion((v) => v + 1);
    const observer = new MutationObserver(bump);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme", "class", "style"],
    });
    if (viewportRef.current && viewportRef.current !== document.documentElement) {
      observer.observe(viewportRef.current, {
        attributes: true,
        attributeFilter: ["data-theme", "class", "style"],
      });
    }
    return () => observer.disconnect();
  }, []);

  const mapped = isMappedRegion(region.id);
  const regionFeatures = useMemo(
    () => (geo.dataset && mapped ? featuresForRegion(geo.dataset, region.id) : []),
    [geo.dataset, mapped, region.id],
  );
  const hasRegionGeo = mapped && regionFeatures.length > 0;
  const isUnmappedRegion = !mapped || (geo.status === "ready" && regionFeatures.length === 0);

  const fallbackGeoBBox = useMemo<GeoBBox | null>(() => {
    if (!points.bbox) return null;
    const west = points.bbox.west;
    const south = points.bbox.south;
    // Unwrap antimeridian-crossing bboxes into continuous eastward extent.
    const rawEast =
      points.bbox.east < west ? points.bbox.east + 360 : points.bbox.east;
    const east = rawEast > west ? rawEast : west + 0.5;
    const north = points.bbox.north > south ? points.bbox.north : south + 0.5;
    return padBBox([west, south, east, north], 0.08);
  }, [points.bbox]);

  const projection = useMemo(() => {
    if (hasRegionGeo) {
      return createProjection(padBBox(regionExtent(regionFeatures), 0.08));
    }
    if (fallbackGeoBBox) {
      return createProjection(fallbackGeoBBox);
    }
    return null;
  }, [hasRegionGeo, regionFeatures, fallbackGeoBBox]);

  const pointInput = useMemo<ScenePointInput>(
    () => ({
      raw: points.raw,
      harmonized: points.harmonized,
    }),
    [points.raw, points.harmonized],
  );

  const scene = useMemo<MapScene>(
    () => buildScene(hasRegionGeo ? geo.dataset : null, region.id, pointInput, projection),
    [geo.dataset, hasRegionGeo, region.id, pointInput, projection],
  );

  const viewportBounds = useMemo<PlaneBounds | null>(() => {
    if (!projection) return null;
    if (hasRegionGeo) return scene.bounds;
    if (fallbackGeoBBox) return planeBounds(projection, fallbackGeoBBox);
    return null;
  }, [projection, hasRegionGeo, scene.bounds, fallbackGeoBBox]);

  const viewport = useMapViewport({
    bounds: viewportBounds,
    size: canvasSize,
    resetKey: region.id,
    padding: VIEWPORT_FIT_PADDING,
  });

  const selectionScope = `${region.id}:${points.period.year ?? "none"}:${points.period.month ?? "all"}:${geometryHarmonized ? "h" : "r"}`;

  const [hoverState, setHoverState] = useState<{ scope: string; index: number | null }>({
    scope: selectionScope,
    index: null,
  });
  const [selectedState, setSelectedState] = useState<{ scope: string; index: number | null }>({
    scope: selectionScope,
    index: null,
  });

  const hoveredIndex = hoverState.scope === selectionScope ? hoverState.index : null;
  const selectedIndex = selectedState.scope === selectionScope ? selectedState.index : null;

  const activeLayer = geometryHarmonized ? scene.harmonized : scene.raw;

  const [drawStats, setDrawStats] = useState<DrawStats>({
    drawn: 0,
    inView: 0,
    total: 0,
  });

  useEffect(() => {
    if (typeof requestAnimationFrame === "undefined") return;
    const rafId = requestAnimationFrame(() => {
      const canvas = canvasRef.current;
      if (!canvas || canvasSize.width <= 0 || canvasSize.height <= 0) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      const palette = readPalette(viewportRef.current ?? canvas, theme);
      const stats = drawMap(
        ctx,
        scene,
        {
          transform: viewport.transform,
          width: canvasSize.width,
          height: canvasSize.height,
          dpr,
          harmonized: geometryHarmonized,
          showDuplicateRings,
          hoveredIndex,
          selectedIndex,
        },
        palette,
      );
      setDrawStats((prev) =>
        prev.drawn === stats.drawn &&
        prev.inView === stats.inView &&
        prev.total === stats.total
          ? prev
          : stats,
      );
    });
    return () => cancelAnimationFrame(rafId);
  }, [
    scene,
    viewport.transform,
    canvasSize,
    dpr,
    geometryHarmonized,
    showDuplicateRings,
    hoveredIndex,
    selectedIndex,
    theme,
    paletteVersion,
  ]);

  const harmById = useMemo(() => {
    const map = new Map<string, HarmonizedPoint>();
    for (const h of points.harmonized) {
      map.set(h.id, h);
    }
    return map;
  }, [points.harmonized]);

  const rawByHarmId = useMemo(() => {
    const map = new Map<string, RawPoint>();
    for (const r of points.raw) {
      if (r.harmonizedId !== null && !map.has(r.harmonizedId)) {
        map.set(r.harmonizedId, r);
      }
    }
    return map;
  }, [points.raw]);

  const resolvePointPair = useCallback(
    (index: number | null) => {
      if (index === null || index < 0 || index >= activeLayer.count) {
        return {
          rawPoint: null as RawPoint | null,
          harmPoint: null as HarmonizedPoint | null,
          isDuplicateInRaw: false,
          screenX: 0,
          screenY: 0,
        };
      }
      const [screenX, screenY] = planeToScreen(
        viewport.transform,
        activeLayer.xs[index],
        activeLayer.ys[index],
      );
      if (geometryHarmonized) {
        const harmPoint = points.harmonized[index] ?? null;
        const rawPoint = harmPoint ? (rawByHarmId.get(harmPoint.id) ?? null) : null;
        const isDuplicateInRaw = (scene.harmonized.flags[index] & PointFlag.Duplicate) !== 0;
        return { rawPoint, harmPoint, isDuplicateInRaw, screenX, screenY };
      }
      const rawPoint = points.raw[index] ?? null;
      const harmPoint =
        rawPoint && rawPoint.harmonizedId !== null
          ? (harmById.get(rawPoint.harmonizedId) ?? null)
          : null;
      const isDuplicateInRaw = (scene.raw.flags[index] & PointFlag.Duplicate) !== 0;
      return { rawPoint, harmPoint, isDuplicateInRaw, screenX, screenY };
    },
    [
      activeLayer,
      viewport.transform,
      geometryHarmonized,
      points.harmonized,
      points.raw,
      rawByHarmId,
      harmById,
      scene.harmonized.flags,
      scene.raw.flags,
    ],
  );

  const detailIndex = selectedIndex !== null ? selectedIndex : hoveredIndex;
  const activeDetail = useMemo(
    () => resolvePointPair(detailIndex),
    [resolvePointPair, detailIndex],
  );

  const selectedDetail = useMemo(
    () => resolvePointPair(selectedIndex),
    [resolvePointPair, selectedIndex],
  );

  const liveAnnouncement = useMemo(
    () =>
      selectedIndex !== null
        ? formatPointAnnouncement(
            copy,
            selectedDetail.rawPoint,
            selectedDetail.harmPoint,
            selectedDetail.isDuplicateInRaw,
            revealAnswer,
          )
        : "",
    [copy, selectedIndex, selectedDetail, revealAnswer],
  );

  const pointerDownRef = useRef<{
    id: number;
    x: number;
    y: number;
    moved: boolean;
  } | null>(null);
  const activePointersRef = useRef<Set<number>>(new Set());
  const suppressTapRef = useRef(false);
  const lastTouchTapRef = useRef<{ time: number; x: number; y: number }>({
    time: 0,
    x: 0,
    y: 0,
  });
  const hoverRafRef = useRef<number | null>(null);
  const hoverPosRef = useRef<{ x: number; y: number } | null>(null);

  const setViewportEl = useCallback(
    (el: HTMLDivElement | null) => {
      viewportRef.current = el;
      viewport.attachWheel(el);
    },
    [viewport.attachWheel],
  );

  useEffect(() => {
    return () => {
      if (hoverRafRef.current !== null && typeof cancelAnimationFrame !== "undefined") {
        cancelAnimationFrame(hoverRafRef.current);
      }
    };
  }, []);

  const handleCloseDetail = useCallback(() => {
    setSelectedState({ scope: selectionScope, index: null });
    setHoverState({ scope: selectionScope, index: null });
    viewportRef.current?.focus({ preventScroll: true });
  }, [selectionScope]);

  const handlePointerDown = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      if (isUiTarget(e.target)) return;
      if (e.button !== 0) return;

      activePointersRef.current.add(e.pointerId);
      const rect = e.currentTarget.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;

      if (activePointersRef.current.size > 1) {
        suppressTapRef.current = true;
        pointerDownRef.current = null;
        lastTouchTapRef.current = { time: 0, x: 0, y: 0 };
      } else if (e.pointerType === "touch") {
        const now = Date.now();
        const prevTap = lastTouchTapRef.current;
        const isDoubleTap =
          now - prevTap.time <= DOUBLE_TAP_MS &&
          Math.hypot(x - prevTap.x, y - prevTap.y) <= DOUBLE_TAP_PX;
        if (isDoubleTap) {
          suppressTapRef.current = true;
          pointerDownRef.current = null;
          lastTouchTapRef.current = { time: 0, x: 0, y: 0 };
        } else {
          lastTouchTapRef.current = { time: now, x, y };
          pointerDownRef.current = { id: e.pointerId, x, y, moved: false };
        }
      } else {
        pointerDownRef.current = { id: e.pointerId, x, y, moved: false };
      }

      viewport.bind.onPointerDown(e);
    },
    [viewport.bind],
  );

  const handlePointerMove = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      viewport.bind.onPointerMove(e);
      const rect = e.currentTarget.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;

      const down = pointerDownRef.current;
      if (down && down.id === e.pointerId) {
        if (Math.hypot(x - down.x, y - down.y) > TAP_MOVE_THRESHOLD_PX) {
          down.moved = true;
          lastTouchTapRef.current = { time: 0, x: 0, y: 0 };
        }
        return;
      }

      if (e.pointerType !== "mouse" || e.buttons !== 0) return;

      if (isUiTarget(e.target)) {
        hoverPosRef.current = null;
        setHoverState((prev) =>
          prev.scope === selectionScope && prev.index === null
            ? prev
            : { scope: selectionScope, index: null },
        );
        return;
      }

      hoverPosRef.current = { x, y };
      if (hoverRafRef.current === null && typeof requestAnimationFrame !== "undefined") {
        hoverRafRef.current = requestAnimationFrame(() => {
          hoverRafRef.current = null;
          const pos = hoverPosRef.current;
          if (!pos || activeLayer.count === 0 || viewport.transform.k <= 0) {
            setHoverState((prev) =>
              prev.scope === selectionScope && prev.index === null
                ? prev
                : { scope: selectionScope, index: null },
            );
            return;
          }
          const [planeX, planeY] = screenToPlane(viewport.transform, pos.x, pos.y);
          const maxDistPlane = Math.max(HIT_TOLERANCE_PX, 12) / viewport.transform.k;
          const idx = nearest(activeLayer.spatialIndex, planeX, planeY, maxDistPlane);
          const resolved = idx >= 0 ? idx : null;
          setHoverState((prev) =>
            prev.scope === selectionScope && prev.index === resolved
              ? prev
              : { scope: selectionScope, index: resolved },
          );
        });
      }
    },
    [viewport.bind, viewport.transform, activeLayer, selectionScope],
  );

  const clearPointerTracking = useCallback((pointerId: number) => {
    activePointersRef.current.delete(pointerId);
    if (activePointersRef.current.size === 0) {
      suppressTapRef.current = false;
    }
    if (pointerDownRef.current?.id === pointerId) {
      pointerDownRef.current = null;
    }
  }, []);

  const handlePointerUp = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      viewport.bind.onPointerUp(e);
      activePointersRef.current.delete(e.pointerId);
      const wasSuppressed = suppressTapRef.current;
      if (activePointersRef.current.size === 0) {
        suppressTapRef.current = false;
      }

      const down = pointerDownRef.current;
      if (!down || down.id !== e.pointerId) return;
      pointerDownRef.current = null;

      if (e.button !== 0 || wasSuppressed || down.moved || isUiTarget(e.target)) {
        return;
      }
      if (activeLayer.count === 0 || viewport.transform.k <= 0) return;

      const rect = e.currentTarget.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      const [planeX, planeY] = screenToPlane(viewport.transform, x, y);
      const maxDistPlane = Math.max(HIT_TOLERANCE_PX, 14) / viewport.transform.k;
      const idx = nearest(activeLayer.spatialIndex, planeX, planeY, maxDistPlane);
      const nextIdx = idx >= 0 ? idx : null;
      setSelectedState({ scope: selectionScope, index: nextIdx });
      if (nextIdx !== null) {
        viewportRef.current?.focus({ preventScroll: true });
      }
    },
    [viewport.bind, viewport.transform, activeLayer, selectionScope],
  );

  const handlePointerCancel = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      clearPointerTracking(e.pointerId);
      viewport.bind.onPointerCancel(e);
    },
    [clearPointerTracking, viewport.bind],
  );

  const handleLostPointerCapture = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      clearPointerTracking(e.pointerId);
      viewport.bind.onLostPointerCapture(e);
    },
    [clearPointerTracking, viewport.bind],
  );

  const handlePointerLeave = useCallback(() => {
    hoverPosRef.current = null;
    setHoverState((prev) =>
      prev.scope === selectionScope && prev.index === null
        ? prev
        : { scope: selectionScope, index: null },
    );
  }, [selectionScope]);

  const handleDoubleClick = useCallback(
    (e: ReactMouseEvent<HTMLElement>) => {
      if (isUiTarget(e.target)) return;
      viewport.bind.onDoubleClick(e);
    },
    [viewport.bind],
  );

  const handleKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLElement>) => {
      if (e.key === "Escape") {
        if (selectedIndex !== null || hoveredIndex !== null) {
          e.preventDefault();
          e.stopPropagation();
          handleCloseDetail();
          viewportRef.current?.focus({ preventScroll: true });
        }
        return;
      }

      if (isUiTarget(e.target)) return;

      if (
        !e.ctrlKey &&
        !e.metaKey &&
        !e.altKey &&
        (e.key === "n" || e.key === "N" || e.key === "p" || e.key === "P")
      ) {
        e.preventDefault();
        if (
          activeLayer.count === 0 ||
          canvasSize.width <= 0 ||
          canvasSize.height <= 0 ||
          viewport.transform.k <= 0
        ) {
          return;
        }
        const { k, tx, ty } = viewport.transform;
        const minPlaneX = (-IN_VIEW_MARGIN_PX - tx) / k;
        const maxPlaneX = (canvasSize.width + IN_VIEW_MARGIN_PX - tx) / k;
        const minPlaneY = (-IN_VIEW_MARGIN_PX - ty) / k;
        const maxPlaneY = (canvasSize.height + IN_VIEW_MARGIN_PX - ty) / k;

        const inViewIndices: number[] = [];
        const { xs, ys, count } = activeLayer;
        for (let i = 0; i < count; i++) {
          const px = xs[i];
          const py = ys[i];
          if (px >= minPlaneX && px <= maxPlaneX && py >= minPlaneY && py <= maxPlaneY) {
            inViewIndices.push(i);
          }
        }
        if (inViewIndices.length === 0) return;

        const ordered = orderedForKeyboard(inViewIndices, activeLayer.frp, activeLayer.ids);
        if (ordered.length === 0) return;

        const isNext = e.key === "n" || e.key === "N";
        const currentPos = selectedIndex !== null ? ordered.indexOf(selectedIndex) : -1;
        const nextPos =
          currentPos === -1
            ? isNext
              ? 0
              : ordered.length - 1
            : (currentPos + (isNext ? 1 : -1) + ordered.length) % ordered.length;
        const nextIndex = ordered[nextPos];
        if (nextIndex !== undefined) {
          setSelectedState({ scope: selectionScope, index: nextIndex });
        }
        return;
      }

      viewport.bind.onKeyDown(e);
    },
    [
      selectedIndex,
      hoveredIndex,
      handleCloseDetail,
      activeLayer,
      canvasSize,
      viewport.transform,
      viewport.bind,
      selectionScope,
    ],
  );

  const periodText =
    points.period.year === null
      ? ""
      : points.period.month === null
        ? String(points.period.year)
        : fmt.monthYear(
            { year: points.period.year, month: points.period.month },
            "long",
          );

  const rawTotal = points.stages.rawModis + points.stages.rawViirs || points.raw.length;
  const harmonizedTotal = points.stages.harmonized || points.harmonized.length;
  const activeTotal = revealAnswer ? harmonizedTotal : rawTotal;
  const activeLoaded = revealAnswer ? points.harmonized.length : points.raw.length;
  const shownCount = drawStats.total > 0 ? drawStats.drawn : activeLoaded;

  const overlayState: MapState =
    points.status === "ready" && points.raw.length === 0 && points.harmonized.length === 0
      ? "empty"
      : points.status;

  const ariaSummaryText = useMemo(() => {
    if (overlayState === "loading") {
      return t("mapAriaSummaryLoading", { region: region.displayName });
    }
    if (overlayState === "error") {
      return t("mapAriaSummaryError", { region: region.displayName });
    }
    if (overlayState === "empty") {
      return t("mapAriaSummaryEmpty", {
        region: region.displayName,
        period: periodText || "—",
      });
    }
    if (drawStats.total > 0 && drawStats.drawn === 0) {
      if (periodText) {
        return t("mapAriaSummaryNoneInView", {
          region: region.displayName,
          period: periodText,
          total: activeTotal,
        });
      }
      return t("mapAriaSummaryNoneInViewNoPeriod", {
        region: region.displayName,
        total: activeTotal,
      });
    }
    if (periodText) {
      return t("mapAriaSummary", {
        region: region.displayName,
        period: periodText,
        total: activeTotal,
        shown: shownCount,
      });
    }
    return t("mapAriaSummaryNoPeriod", {
      region: region.displayName,
      total: activeTotal,
      shown: shownCount,
    });
  }, [
    overlayState,
    t,
    region.displayName,
    periodText,
    activeTotal,
    shownCount,
    drawStats.total,
    drawStats.drawn,
  ]);

  return (
    <section className={styles.map} aria-label={t("mapLabel")}>
      <p id={summaryId} className={styles.srOnly}>
        {ariaSummaryText}
      </p>
      <div className={styles.srOnly} aria-live="polite" aria-atomic="true">
        {liveAnnouncement}
      </div>

      <MapStatusBar
        regionName={region.displayName}
        periodText={periodText}
        revealAnswer={revealAnswer}
        status={overlayState}
        error={points.error}
        onRetry={points.retry}
        rawTotal={rawTotal}
        harmonizedTotal={harmonizedTotal}
        rawLoaded={points.raw.length}
        harmonizedLoaded={points.harmonized.length}
        truncated={points.truncated}
        drawStats={drawStats}
        selectedMonth={selectedMonth}
        onClearMonth={() => selectMonth(null)}
      />

      <div
        ref={setViewportEl}
        className={styles.viewport}
        role="group"
        aria-label={t("mapLabel")}
        aria-describedby={summaryId}
        tabIndex={0}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerCancel}
        onLostPointerCapture={handleLostPointerCapture}
        onPointerLeave={handlePointerLeave}
        onDoubleClick={handleDoubleClick}
        onKeyDown={handleKeyDown}
      >
        <canvas ref={canvasRef} className={styles.canvas} />

        {hasRegionGeo && (
          <MapLabels
            dataset={geo.dataset}
            regionId={region.id}
            selectedDisplayName={region.displayName}
            projection={projection}
            transform={viewport.transform}
            size={canvasSize}
            lang={lang}
          />
        )}

        <MapControls
          canZoomIn={viewport.canZoomIn}
          canZoomOut={viewport.canZoomOut}
          isAtFit={viewport.isAtFit}
          onZoomIn={viewport.zoomIn}
          onZoomOut={viewport.zoomOut}
          onReset={viewport.reset}
        />

        {(activeDetail.rawPoint || activeDetail.harmPoint) && (
          <PointDetail
            rawPoint={activeDetail.rawPoint}
            harmPoint={activeDetail.harmPoint}
            isDuplicateInRaw={activeDetail.isDuplicateInRaw}
            revealAnswer={revealAnswer}
            screenX={activeDetail.screenX}
            screenY={activeDetail.screenY}
            viewportSize={canvasSize}
            onClose={handleCloseDetail}
          />
        )}

        <div className={styles.legendDock} data-map-ui="true">
          <MapLegend />
        </div>

        <MapStateOverlay
          state={overlayState}
          error={points.error}
          onRetry={points.retry}
          geoStatus={geo.status}
          onRetryGeo={geo.retry}
          unmappedRegionName={isUnmappedRegion ? region.displayName : null}
        />
      </div>
    </section>
  );
}

export default memo(HotspotMap);
