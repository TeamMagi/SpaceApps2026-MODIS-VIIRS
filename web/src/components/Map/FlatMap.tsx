import {
  memo,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import {
  createProjection,
  padBBox,
  planeBounds,
  regionExtent,
  type PlaneBounds,
} from "../../lib/geo/projection";
import { hitTestFeatures } from "./engine/featureHit";
import { HIT_TOLERANCE_PX, nearest, orderedForKeyboard } from "./engine/pointIndex";
import {
  buildScenePoints,
  buildSceneGeometry,
  composeScene,
  drawFeatureHighlight,
  drawMap,
  readPalette,
} from "./engine/renderer";
import { type DrawStats, type MapScene, type ViewportSize } from "./engine/types";
import { useMapViewport } from "./engine/useMapViewport";
import { planeToScreen, screenToPlane } from "./engine/viewport";
import MapControls from "./MapControls";
import MapCountryTooltip from "./MapCountryTooltip";
import MapLabels from "./MapLabels";
import MapLegend from "./MapLegend";
import MapStateOverlay from "./MapStateOverlay";
import MapStatusBar from "./MapStatusBar";
import PointDetail from "./PointDetail";
import { useMapModel, useMapSummary, usePaletteVersion, usePointSelection } from "./useMapModel";
import styles from "./HotspotMap.module.css";

const VIEWPORT_FIT_PADDING = 28;
const IN_VIEW_MARGIN_PX = 12;
const TAP_MOVE_THRESHOLD_PX = 6;
const DOUBLE_TAP_MS = 300;
const DOUBLE_TAP_PX = 24;
const COUNTRY_CLICK_MAX_MOVE_PX = 4;
const TOOLTIP_OFFSET_X_PX = 14;
const TOOLTIP_OFFSET_Y_PX = 16;
const TOOLTIP_EDGE_PX = 8;


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

/** The 2D canvas map, kept as the fallback when WebGL is unavailable. */
function FlatMap() {
  const model = useMapModel();
  const {
    t,
    region,
    setRegion,
    points,
    geo,
    geometryHarmonized,
    showDuplicateRings,
    revealAnswer,
    selectedMonth,
    selectMonth,
    theme,
    lang,
    setSensorFilter,
    effectiveSensorFilter,
    regionMatch,
    selectedFeatureSet,
    selectableFeatureSet,
    regionFeatures,
    hasRegionGeo,
    isUnmappedRegion,
    fallbackGeoBBox,
    pointInput,
    selectionScope,
    setHoverState,
    setSelectedState,
    setHoveredFeature,
    selectableRegionFor,
    hoveredCountry,
    highlightedFeature,
    periodText,
  } = model;

  const summaryId = useId();
  const pickHintId = useId();
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const highlightCanvasRef = useRef<HTMLCanvasElement | null>(null);

  const [canvasSize, setCanvasSize] = useState<ViewportSize>({ width: 0, height: 0 });
  const [dpr, setDpr] = useState<number>(readDpr);
  const paletteVersion = usePaletteVersion(viewportRef);

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

  const projection = useMemo(() => {
    if (hasRegionGeo) {
      return createProjection(padBBox(regionExtent(regionFeatures), 0.08));
    }
    if (fallbackGeoBBox) {
      return createProjection(fallbackGeoBBox);
    }
    return null;
  }, [hasRegionGeo, regionFeatures, fallbackGeoBBox]);

  const sceneGeometry = useMemo(
    () =>
      buildSceneGeometry(
        geo.dataset,
        region.id,
        projection,
        selectedFeatureSet,
        selectableFeatureSet,
      ),
    [geo.dataset, region.id, projection, selectedFeatureSet, selectableFeatureSet],
  );
  const scenePoints = useMemo(
    () => buildScenePoints(pointInput, projection),
    [pointInput, projection],
  );
  const scene = useMemo<MapScene>(
    () => composeScene(region.id, sceneGeometry, scenePoints),
    [region.id, sceneGeometry, scenePoints],
  );

  const viewportBounds = useMemo<PlaneBounds | null>(() => {
    if (!projection) return null;
    if (hasRegionGeo) return scene.bounds;
    if (fallbackGeoBBox) return planeBounds(projection, fallbackGeoBBox);
    return null;
  }, [projection, hasRegionGeo, scene.bounds, fallbackGeoBBox]);

  const viewport = useMapViewport({
    bounds: viewportBounds,
    extent: sceneGeometry.allBounds,
    size: canvasSize,
    resetKey: region.id,
    padding: VIEWPORT_FIT_PADDING,
  });

  const activeLayer = geometryHarmonized ? scene.harmonized : scene.raw;

  const { isPointVisible, hoveredIndex, selectedIndex, pairFor, liveAnnouncement } =
    usePointSelection(model, activeLayer);

  const countryAt = useCallback(
    (planeX: number, planeY: number): number =>
      hitTestFeatures(
        scene.features,
        planeX,
        planeY,
        (idx) => (regionMatch?.regionByFeature[idx] ?? null) !== region.id,
      ),
    [scene.features, regionMatch, region.id],
  );

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
          sensorFilter: effectiveSensorFilter,
          selectableFeatures: selectableFeatureSet,
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
    effectiveSensorFilter,
    selectableFeatureSet,
    hoveredIndex,
    selectedIndex,
    theme,
    paletteVersion,
  ]);

  useEffect(() => {
    if (typeof requestAnimationFrame === "undefined") return;
    const rafId = requestAnimationFrame(() => {
      const canvas = highlightCanvasRef.current;
      const ctx = canvas?.getContext("2d");
      if (!canvas || !ctx) return;
      drawFeatureHighlight(
        ctx,
        highlightedFeature !== null ? scene.features[highlightedFeature] : null,
        {
          transform: viewport.transform,
          width: canvasSize.width,
          height: canvasSize.height,
          dpr,
          lodTolerances: scene.lodTolerances,
        },
        readPalette(viewportRef.current ?? canvas, theme),
      );
    });
    return () => cancelAnimationFrame(rafId);
  }, [highlightedFeature, scene.features, scene.lodTolerances, viewport.transform, canvasSize, dpr, theme, paletteVersion]);

  const resolvePointPair = useCallback(
    (index: number | null) => {
      const pair = pairFor(index);
      if (index === null || (!pair.rawPoint && !pair.harmPoint)) {
        return { ...pair, screenX: 0, screenY: 0 };
      }
      const [screenX, screenY] = planeToScreen(
        viewport.transform,
        activeLayer.xs[index],
        activeLayer.ys[index],
      );
      return { ...pair, screenX, screenY };
    },
    [pairFor, viewport.transform, activeLayer],
  );

  const detailIndex = selectedIndex !== null ? selectedIndex : hoveredIndex;
  const activeDetail = useMemo(
    () => resolvePointPair(detailIndex),
    [resolvePointPair, detailIndex],
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
  const tooltipRef = useRef<HTMLDivElement | null>(null);
  const pendingCountryTapRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const canvasSizeRef = useRef(canvasSize);
  const tooltipSizeRef = useRef({ width: 0, height: 0 });

  useLayoutEffect(() => {
    canvasSizeRef.current = canvasSize;
  }, [canvasSize]);

  /** Writes only; the tooltip size is measured once per content change, never per pointer move. */
  const positionTooltip = useCallback(() => {
    const node = tooltipRef.current;
    const pos = hoverPosRef.current;
    if (!node || !pos) return;
    const { width, height } = canvasSizeRef.current;
    const { width: w, height: h } = tooltipSizeRef.current;
    // Prefer below-right of the pointer, flip when that overflows, then clamp inside the viewport.
    let x = pos.x + TOOLTIP_OFFSET_X_PX;
    if (x + w > width - TOOLTIP_EDGE_PX) x = pos.x - TOOLTIP_OFFSET_X_PX - w;
    let y = pos.y + TOOLTIP_OFFSET_Y_PX;
    if (y + h > height - TOOLTIP_EDGE_PX) y = pos.y - TOOLTIP_OFFSET_Y_PX - h;
    x = Math.max(TOOLTIP_EDGE_PX, Math.min(x, width - w - TOOLTIP_EDGE_PX));
    y = Math.max(TOOLTIP_EDGE_PX, Math.min(y, height - h - TOOLTIP_EDGE_PX));
    node.style.setProperty("--x", `${Math.round(x)}px`);
    node.style.setProperty("--y", `${Math.round(y)}px`);
  }, []);

  const clearHoveredFeature = useCallback(() => {
    setHoveredFeature((prev) => (prev === null ? prev : null));
  }, []);

  const cancelPendingCountryTap = useCallback(() => {
    if (pendingCountryTapRef.current !== null) {
      clearTimeout(pendingCountryTapRef.current);
      pendingCountryTapRef.current = null;
    }
  }, []);

  // A region picked elsewhere (header, URL) must not be overwritten by a tap still in its double-tap wait.
  useEffect(() => cancelPendingCountryTap, [region.id, cancelPendingCountryTap]);

  const runHover = useCallback(() => {
    const pos = hoverPosRef.current;
    if (!pos || viewport.transform.k <= 0) {
      setHoverState((prev) =>
        prev.scope === selectionScope && prev.index === null
          ? prev
          : { scope: selectionScope, index: null },
      );
      clearHoveredFeature();
      return;
    }
    const [planeX, planeY] = screenToPlane(viewport.transform, pos.x, pos.y);
    let resolved: number | null = null;
    if (activeLayer.count > 0) {
      const maxDistPlane = Math.max(HIT_TOLERANCE_PX, 12) / viewport.transform.k;
      const idx = nearest(activeLayer.spatialIndex, planeX, planeY, maxDistPlane, isPointVisible);
      resolved = idx >= 0 ? idx : null;
    }
    setHoverState((prev) =>
      prev.scope === selectionScope && prev.index === resolved
        ? prev
        : { scope: selectionScope, index: resolved },
    );
    // A hotspot under the pointer wins over the country beneath it.
    const feature = resolved === null ? countryAt(planeX, planeY) : -1;
    const nextFeature = feature >= 0 ? feature : null;
    setHoveredFeature((prev) => (prev === nextFeature ? prev : nextFeature));
    positionTooltip();
  }, [
    viewport.transform,
    activeLayer,
    isPointVisible,
    selectionScope,
    countryAt,
    clearHoveredFeature,
    positionTooltip,
  ]);

  const runHoverRef = useRef(runHover);
  useLayoutEffect(() => {
    runHoverRef.current = runHover;
  }, [runHover]);

  const scheduleHover = useCallback(() => {
    if (hoverRafRef.current !== null || typeof requestAnimationFrame === "undefined") return;
    hoverRafRef.current = requestAnimationFrame(() => {
      hoverRafRef.current = null;
      runHoverRef.current();
    });
  }, []);

  useLayoutEffect(() => {
    const node = tooltipRef.current;
    if (!hoveredCountry || !node) return;
    tooltipSizeRef.current = { width: node.offsetWidth, height: node.offsetHeight };
    positionTooltip();
  }, [hoveredCountry, positionTooltip]);

  // Zoom (wheel, keys, buttons) or filter changes update what lies under a resting pointer.
  useEffect(() => {
    if (hoverPosRef.current && pointerDownRef.current === null) scheduleHover();
  }, [viewport.transform, scene, effectiveSensorFilter, scheduleHover]);

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
      if (pendingCountryTapRef.current !== null) {
        clearTimeout(pendingCountryTapRef.current);
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
      cancelPendingCountryTap();
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
    [viewport.bind, cancelPendingCountryTap],
  );

  const handlePointerMove = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      viewport.bind.onPointerMove(e);
      const rect = e.currentTarget.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;

      const down = pointerDownRef.current;
      if (down && down.id === e.pointerId) {
        if (!down.moved && Math.hypot(x - down.x, y - down.y) > TAP_MOVE_THRESHOLD_PX) {
          down.moved = true;
          lastTouchTapRef.current = { time: 0, x: 0, y: 0 };
          hoverPosRef.current = null;
          clearHoveredFeature();
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
        clearHoveredFeature();
        return;
      }

      hoverPosRef.current = { x, y };
      scheduleHover();
    },
    [viewport.bind, selectionScope, clearHoveredFeature, scheduleHover],
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

      const rect = e.currentTarget.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      // Hover tracking pauses while a button is held; resume it where the pointer was released.
      if (e.pointerType === "mouse" && !isUiTarget(e.target)) {
        hoverPosRef.current = { x, y };
        scheduleHover();
      }

      if (e.button !== 0 || wasSuppressed || down.moved || isUiTarget(e.target)) {
        return;
      }
      if (viewport.transform.k <= 0) return;

      const [planeX, planeY] = screenToPlane(viewport.transform, x, y);

      if (activeLayer.count > 0) {
        const maxDistPlane = Math.max(HIT_TOLERANCE_PX, 14) / viewport.transform.k;
        const idx = nearest(activeLayer.spatialIndex, planeX, planeY, maxDistPlane, isPointVisible);
        const nextIdx = idx >= 0 ? idx : null;
        setSelectedState({ scope: selectionScope, index: nextIdx });
        if (nextIdx !== null) {
          viewportRef.current?.focus({ preventScroll: true });
          return;
        }
      }

      if (Math.hypot(x - down.x, y - down.y) >= COUNTRY_CLICK_MAX_MOVE_PX) return;
      const feature = countryAt(planeX, planeY);
      const targetRegion = feature >= 0 ? selectableRegionFor(feature) : null;
      if (targetRegion === null) return;

      // Wait out the double-tap/double-click window so a zoom gesture never switches region first;
      // the next pointerdown or dblclick cancels it.
      pendingCountryTapRef.current = setTimeout(() => {
        pendingCountryTapRef.current = null;
        clearHoveredFeature();
        setRegion(targetRegion);
      }, DOUBLE_TAP_MS);
    },
    [
      viewport.bind,
      viewport.transform,
      activeLayer,
      isPointVisible,
      selectionScope,
      countryAt,
      selectableRegionFor,
      setRegion,
      clearHoveredFeature,
      scheduleHover,
    ],
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
    clearHoveredFeature();
  }, [selectionScope, clearHoveredFeature]);

  const handleDoubleClick = useCallback(
    (e: ReactMouseEvent<HTMLElement>) => {
      cancelPendingCountryTap();
      if (isUiTarget(e.target)) return;
      viewport.bind.onDoubleClick(e);
    },
    [viewport.bind, cancelPendingCountryTap],
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
          if (!isPointVisible(i)) continue;
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
      isPointVisible,
      canvasSize,
      viewport.transform,
      viewport.bind,
      selectionScope,
    ],
  );

  const { visibleRawLoaded, visibleHarmonizedLoaded, rawTotal, harmonizedTotal, overlayState, ariaSummaryText } =
    useMapSummary(model, scene.raw, drawStats);

  return (
    <section className={styles.map} aria-label={t("mapLabel")}>
      <p id={summaryId} className={styles.srOnly}>
        {ariaSummaryText}
      </p>
      <p id={pickHintId} className={styles.srOnly}>
        {t("mapCountryPickHint")}
      </p>
      <div className={styles.srOnly} aria-live="polite" aria-atomic="true">
        {liveAnnouncement}
      </div>

      <MapStatusBar
        regionName={region.displayName}
        periodText={periodText}
        revealAnswer={revealAnswer}
        sensorFilter={effectiveSensorFilter}
        status={overlayState}
        error={points.error}
        onRetry={points.retry}
        rawTotal={rawTotal}
        harmonizedTotal={harmonizedTotal}
        rawLoaded={visibleRawLoaded}
        harmonizedLoaded={visibleHarmonizedLoaded}
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
        aria-describedby={`${summaryId} ${pickHintId}`}
        data-country-hover={highlightedFeature !== null ? "available" : undefined}
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
        <canvas ref={highlightCanvasRef} className={styles.canvas} aria-hidden="true" />

        {hasRegionGeo && (
          <MapLabels
            dataset={geo.dataset}
            regionId={region.id}
            featureRegionIds={regionMatch?.regionByFeature}
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

        {hoveredCountry && !activeDetail.rawPoint && !activeDetail.harmPoint && (
          <MapCountryTooltip
            ref={tooltipRef}
            name={hoveredCountry.name}
            detail={hoveredCountry.detail}
            hint={hoveredCountry.hint}
            available={hoveredCountry.available}
          />
        )}

        {(activeDetail.rawPoint || activeDetail.harmPoint) && (
          <PointDetail
            key={`${activeDetail.rawPoint?.id ?? ""}:${activeDetail.harmPoint?.id ?? ""}`}
            rawPoint={activeDetail.rawPoint}
            harmPoint={activeDetail.harmPoint}
            isDuplicateInRaw={activeDetail.isDuplicateInRaw}
            revealAnswer={revealAnswer}
            screenX={activeDetail.screenX}
            screenY={activeDetail.screenY}
            viewportSize={canvasSize}
            onClose={handleCloseDetail}
            rawPoints={points.raw}
          />
        )}

        <div className={styles.legendDock} data-map-ui="true">
          <MapLegend
            sensorFilter={effectiveSensorFilter}
            onSensorFilterChange={setSensorFilter}
          />
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

export default memo(FlatMap);
