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
import { orderedForKeyboard } from "./engine/pointIndex";
import { buildScenePoints } from "./engine/renderer";
import type { DrawStats, ViewportSize } from "./engine/types";
import { readGlobeColors } from "./globe/colors";
import { buildPickFeatures, pickFeatureAt } from "./globe/featurePick";
import {
  frameForBBox,
  frameForFeatures,
  nearestScreenPoint,
  pointsInView,
  projectToScreen,
  type GeoFrame,
} from "./globe/geoMath";
import { buildGlobeLayer, LONLAT_PROJECTION } from "./globe/pointAttributes";
import { GlobeScene, ZOOM_STEP, type GlobeViewState } from "./globe/scene";
import type { TileId } from "./globe/tileMath";
import MapControls from "./MapControls";
import MapCountryTooltip from "./MapCountryTooltip";
import MapLegend from "./MapLegend";
import MapStateOverlay from "./MapStateOverlay";
import MapStatusBar from "./MapStatusBar";
import PointDetail, { applyPointCardAnchor, pointCardAnchor } from "./PointDetail";
import { useMapModel, useMapSummary, usePaletteVersion, usePointSelection } from "./useMapModel";
import styles from "./HotspotMap.module.css";
import globeStyles from "./GlobeMap.module.css";

const IN_VIEW_MARGIN_PX = 12;
const HOVER_TOLERANCE_PX = 12;
const CLICK_TOLERANCE_PX = 14;
const TAP_MOVE_THRESHOLD_PX = 6;
const DOUBLE_TAP_MS = 300;
const DOUBLE_TAP_PX = 24;
const COUNTRY_CLICK_MAX_MOVE_PX = 4;
const TOOLTIP_OFFSET_X_PX = 14;
const TOOLTIP_OFFSET_Y_PX = 16;
const TOOLTIP_EDGE_PX = 8;

const INITIAL_VIEW: GlobeViewState = {
  canZoomIn: true,
  canZoomOut: true,
  isAtFit: true,
  orbitsVisible: false,
};

function isUiTarget(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest("button, [data-map-ui]") !== null;
}

function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
}

function globeAssetUrl(path: string): string {
  const base = import.meta.env.BASE_URL;
  return `${base.endsWith("/") ? base : `${base}/`}globe/${path}`;
}

function earthTileUrl(tile: TileId): string {
  return globeAssetUrl(`tiles/${tile.row}_${tile.col}.jpg`);
}

/**
 * Merge beat of the running harmonize transition, which the transition clock writes to <html>
 * (0ms while idle or after a snap), so the FRP glow fades in with the morph instead of popping.
 */
function runMergeMs(): number {
  if (typeof document === "undefined") return 0;
  const value = parseFloat(document.documentElement.style.getPropertyValue("--run-merge"));
  return Number.isFinite(value) ? value : 0;
}

function frameKey(frame: GeoFrame | null): string {
  if (!frame) return "none";
  const [x, y, z] = frame.direction;
  return [x, y, z, frame.radius].map((v) => v.toFixed(4)).join(",");
}

export interface GlobeMapProps {
  /** Called when WebGL cannot be started, so the caller can show the flat map instead. */
  readonly onUnsupported: () => void;
}

function GlobeMap({ onUnsupported }: GlobeMapProps) {
  const model = useMapModel();
  const {
    t,
    region,
    setRegion,
    points,
    rawFrpBreaks,
    harmonizedFrpBreaks,
    frpBreaks,
    geo,
    geometryHarmonized,
    showDuplicateRings,
    revealAnswer,
    selectedMonth,
    selectMonth,
    theme,
    setSensorFilter,
    effectiveSensorFilter,
    regionMatch,
    regionFeatures,
    hasRegionGeo,
    isUnmappedRegion,
    fallbackGeoBBox,
    pointInput,
    setHoveredIndex,
    setSelectedIndex,
    setHoveredFeature,
    selectableRegionFor,
    hoveredCountry,
    highlightedFeature,
    periodText,
  } = model;

  const summaryId = useId();
  const pickHintId = useId();
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const hostRef = useRef<HTMLDivElement | null>(null);
  const sceneRef = useRef<GlobeScene | null>(null);
  const [scene, setScene] = useState<GlobeScene | null>(null);
  const [size, setSize] = useState<ViewportSize>({ width: 0, height: 0 });
  const [viewState, setViewState] = useState<GlobeViewState>(INITIAL_VIEW);
  /** Bumps when the camera settles (or resizes): view-dependent counts are recomputed then. */
  const [settleTick, setSettleTick] = useState(0);
  const paletteVersion = usePaletteVersion(viewportRef);

  const settleRef = useRef<() => void>(() => {});
  /** Re-anchors the open point card to its point; runs inside the render loop, never via React state. */
  const followDetailRef = useRef<() => void>(() => {});

  useEffect(() => {
    const host = hostRef.current;
    const viewportEl = viewportRef.current;
    if (!host || !viewportEl) return;
    let created: GlobeScene;
    try {
      created = new GlobeScene({
        host,
        canvasClassName: globeStyles.canvas,
        photoUrl: globeAssetUrl("earth.jpg"),
        tileUrl: earthTileUrl,
        reducedMotion: prefersReducedMotion(),
        onViewState: setViewState,
        onCameraMove: () => followDetailRef.current(),
        onSettle: () => settleRef.current(),
      });
    } catch {
      onUnsupported();
      return;
    }
    sceneRef.current = created;
    setScene(created);

    const measure = () => {
      const rect = viewportEl.getBoundingClientRect();
      const width = Math.round(rect.width);
      const height = Math.round(rect.height);
      setSize((prev) => (prev.width === width && prev.height === height ? prev : { width, height }));
      created.resize();
      setSettleTick((n) => n + 1);
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(viewportEl);

    return () => {
      observer?.disconnect();
      created.dispose();
      sceneRef.current = null;
      setScene(null);
    };
  }, [onUnsupported]);

  // Scene inputs ------------------------------------------------------------------------------

  useEffect(() => {
    const el = viewportRef.current;
    if (!scene || !el) return;
    scene.setColors(readGlobeColors(el, theme));
  }, [scene, theme, paletteVersion]);

  useEffect(() => {
    scene?.setBorders(geo.dataset?.features ?? []);
  }, [scene, geo.dataset]);

  useEffect(() => {
    scene?.setSelection(regionFeatures);
  }, [scene, regionFeatures]);

  useEffect(() => {
    const feature = highlightedFeature !== null ? (geo.dataset?.features[highlightedFeature] ?? null) : null;
    scene?.setHoverFeature(feature);
  }, [scene, highlightedFeature, geo.dataset]);

  const scenePoints = useMemo(
    () =>
      buildScenePoints(pointInput, LONLAT_PROJECTION, {
        raw: rawFrpBreaks,
        harmonized: harmonizedFrpBreaks,
      }),
    [pointInput, rawFrpBreaks, harmonizedFrpBreaks],
  );
  const globeLayers = useMemo(
    () => ({
      raw: buildGlobeLayer(scenePoints.raw, false),
      harmonized: buildGlobeLayer(scenePoints.harmonized, true),
    }),
    [scenePoints],
  );

  useEffect(() => {
    scene?.setPointLayers(globeLayers.raw, globeLayers.harmonized);
  }, [scene, globeLayers]);

  useEffect(() => {
    scene?.setPointStyle({
      harmonized: geometryHarmonized,
      showDuplicateRings,
      filter: effectiveSensorFilter,
      glowFadeMs: runMergeMs(),
    });
  }, [scene, geometryHarmonized, showDuplicateRings, effectiveSensorFilter]);

  const frame = useMemo<GeoFrame | null>(() => {
    if (hasRegionGeo) return frameForFeatures(regionFeatures);
    if (fallbackGeoBBox) return frameForBBox(fallbackGeoBBox, []);
    return null;
  }, [hasRegionGeo, regionFeatures, fallbackGeoBBox]);
  const currentFrameKey = frameKey(frame);
  const hasFramedRef = useRef(false);

  // The first framing jumps; every later region change flies there.
  useEffect(() => {
    if (!scene) return;
    scene.setFrame(frame, hasFramedRef.current);
    if (frame) hasFramedRef.current = true;
    // Keyed on the frame's value: a new array identity for the same region must not re-fly.
  }, [scene, currentFrameKey]);

  // Points ------------------------------------------------------------------------------------

  const activeLayer = geometryHarmonized ? scenePoints.harmonized : scenePoints.raw;
  const activeGlobeLayer = geometryHarmonized ? globeLayers.harmonized : globeLayers.raw;
  const { isPointVisible, hoveredIndex, selectedIndex, pairFor, liveAnnouncement } =
    usePointSelection(model, activeLayer);

  useEffect(() => {
    if (!scene) return;
    const hovered =
      hoveredIndex !== null && hoveredIndex !== selectedIndex
        ? { layer: activeGlobeLayer, index: hoveredIndex }
        : null;
    const selected = selectedIndex !== null ? { layer: activeGlobeLayer, index: selectedIndex } : null;
    scene.setHighlight(hovered, selected);
  }, [scene, activeGlobeLayer, hoveredIndex, selectedIndex]);

  const nearestPointAt = useCallback(
    (x: number, y: number, maxPx: number): number | null => {
      const current = sceneRef.current;
      if (!current || activeGlobeLayer.count === 0) return null;
      const idx = nearestScreenPoint(
        activeGlobeLayer.positions,
        activeGlobeLayer.count,
        current.projector(),
        x,
        y,
        maxPx,
        isPointVisible,
      );
      return idx >= 0 ? idx : null;
    },
    [activeGlobeLayer, isPointVisible],
  );

  const pickFeatures = useMemo(
    () => (geo.dataset ? buildPickFeatures(geo.dataset.features) : []),
    [geo.dataset],
  );

  const countryAt = useCallback(
    (x: number, y: number): number => {
      const lonLat = sceneRef.current?.surfaceAt(x, y);
      if (!lonLat) return -1;
      return pickFeatureAt(
        pickFeatures,
        lonLat[0],
        lonLat[1],
        (idx) => (regionMatch?.regionByFeature[idx] ?? null) !== region.id,
      );
    },
    [pickFeatures, regionMatch, region.id],
  );

  const [drawStats, setDrawStats] = useState<DrawStats>({ drawn: 0, inView: 0, total: 0 });

  useEffect(() => {
    if (!scene) return;
    let total = 0;
    for (let i = 0; i < activeGlobeLayer.count; i++) if (isPointVisible(i)) total++;
    const inView = pointsInView(
      activeGlobeLayer.positions,
      activeGlobeLayer.count,
      scene.projector(),
      IN_VIEW_MARGIN_PX,
      isPointVisible,
    ).length;
    setDrawStats((prev) =>
      prev.drawn === inView && prev.inView === inView && prev.total === total
        ? prev
        : { drawn: inView, inView, total },
    );
  }, [scene, activeGlobeLayer, isPointVisible, settleTick]);

  const detailIndex = selectedIndex !== null ? selectedIndex : hoveredIndex;
  const detailCardRef = useRef<HTMLDivElement | null>(null);

  const detailScreenPosition = useCallback(
    (index: number | null): [x: number, y: number] | null => {
      const current = sceneRef.current;
      if (index === null || !current || index >= activeGlobeLayer.count) return null;
      const p = activeGlobeLayer.positions;
      return projectToScreen(current.projector(), p[index * 3], p[index * 3 + 1], p[index * 3 + 2]);
    },
    [activeGlobeLayer],
  );

  const activeDetail = useMemo(() => {
    const pair = pairFor(detailIndex);
    const projected = pair.rawPoint || pair.harmPoint ? detailScreenPosition(detailIndex) : null;
    return { ...pair, screenX: projected?.[0] ?? 0, screenY: projected?.[1] ?? 0 };
    // settleTick re-projects after the camera rests; while it moves the render loop re-anchors the card.
  }, [pairFor, detailIndex, detailScreenPosition, settleTick, scene]);

  // Pointer & keyboard ------------------------------------------------------------------------

  const pointerDownRef = useRef<{ id: number; x: number; y: number; moved: boolean } | null>(null);
  const activePointersRef = useRef<Set<number>>(new Set());
  const suppressTapRef = useRef(false);
  const lastTouchTapRef = useRef<{ time: number; x: number; y: number }>({ time: 0, x: 0, y: 0 });
  const hoverRafRef = useRef<number | null>(null);
  const hoverPosRef = useRef<{ x: number; y: number } | null>(null);
  const tooltipRef = useRef<HTMLDivElement | null>(null);
  const pendingCountryTapRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const sizeRef = useRef(size);
  const tooltipSizeRef = useRef({ width: 0, height: 0 });

  useLayoutEffect(() => {
    sizeRef.current = size;
  }, [size]);

  useLayoutEffect(() => {
    followDetailRef.current = () => {
      const el = detailCardRef.current;
      if (!el || detailIndex === null) return;
      const projected = detailScreenPosition(detailIndex);
      if (projected) applyPointCardAnchor(el, pointCardAnchor(projected[0], projected[1], sizeRef.current));
    };
  }, [detailIndex, detailScreenPosition]);

  const positionTooltip = useCallback(() => {
    const node = tooltipRef.current;
    const pos = hoverPosRef.current;
    if (!node || !pos) return;
    const { width, height } = sizeRef.current;
    const { width: w, height: h } = tooltipSizeRef.current;
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
  }, [setHoveredFeature]);

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
    if (!pos) {
      setHoveredIndex(null);
      clearHoveredFeature();
      return;
    }
    const resolved = nearestPointAt(pos.x, pos.y, HOVER_TOLERANCE_PX);
    setHoveredIndex(resolved);
    // A hotspot under the pointer wins over the country beneath it.
    const feature = resolved === null ? countryAt(pos.x, pos.y) : -1;
    const nextFeature = feature >= 0 ? feature : null;
    setHoveredFeature((prev) => (prev === nextFeature ? prev : nextFeature));
    positionTooltip();
  }, [nearestPointAt, countryAt, setHoveredIndex, setHoveredFeature, clearHoveredFeature, positionTooltip]);

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
    settleRef.current = () => {
      setSettleTick((n) => n + 1);
      if (hoverPosRef.current && pointerDownRef.current === null) scheduleHover();
    };
  }, [scheduleHover]);

  useLayoutEffect(() => {
    const node = tooltipRef.current;
    if (!hoveredCountry || !node) return;
    tooltipSizeRef.current = { width: node.offsetWidth, height: node.offsetHeight };
    positionTooltip();
  }, [hoveredCountry, positionTooltip]);

  // Layer or filter changes update what lies under a resting pointer.
  useEffect(() => {
    if (hoverPosRef.current && pointerDownRef.current === null) scheduleHover();
  }, [activeGlobeLayer, effectiveSensorFilter, scheduleHover]);

  useEffect(() => {
    return () => {
      if (hoverRafRef.current !== null) cancelAnimationFrame(hoverRafRef.current);
      if (pendingCountryTapRef.current !== null) clearTimeout(pendingCountryTapRef.current);
    };
  }, []);

  const handleCloseDetail = useCallback(() => {
    setSelectedIndex(null);
    setHoveredIndex(null);
    viewportRef.current?.focus({ preventScroll: true });
  }, [setSelectedIndex, setHoveredIndex]);

  const zoomIn = useCallback(() => sceneRef.current?.zoomBy(1 / ZOOM_STEP), []);
  const zoomOut = useCallback(() => sceneRef.current?.zoomBy(ZOOM_STEP), []);
  const fit = useCallback(() => sceneRef.current?.fit(true), []);

  const localPoint = (e: ReactPointerEvent<HTMLElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };

  const handlePointerDown = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      cancelPendingCountryTap();
      if (isUiTarget(e.target) || e.button !== 0) return;
      activePointersRef.current.add(e.pointerId);
      const { x, y } = localPoint(e);

      if (activePointersRef.current.size > 1) {
        suppressTapRef.current = true;
        pointerDownRef.current = null;
        lastTouchTapRef.current = { time: 0, x: 0, y: 0 };
        return;
      }
      if (e.pointerType === "touch") {
        const now = Date.now();
        const prevTap = lastTouchTapRef.current;
        if (now - prevTap.time <= DOUBLE_TAP_MS && Math.hypot(x - prevTap.x, y - prevTap.y) <= DOUBLE_TAP_PX) {
          suppressTapRef.current = true;
          pointerDownRef.current = null;
          lastTouchTapRef.current = { time: 0, x: 0, y: 0 };
          zoomIn();
          return;
        }
        lastTouchTapRef.current = { time: now, x, y };
      }
      pointerDownRef.current = { id: e.pointerId, x, y, moved: false };
    },
    [cancelPendingCountryTap, zoomIn],
  );

  const handlePointerMove = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      const { x, y } = localPoint(e);
      const down = pointerDownRef.current;
      if (down && down.id === e.pointerId) {
        if (!down.moved && Math.hypot(x - down.x, y - down.y) > TAP_MOVE_THRESHOLD_PX) {
          down.moved = true;
          lastTouchTapRef.current = { time: 0, x: 0, y: 0 };
          hoverPosRef.current = null;
          setHoveredIndex(null);
          clearHoveredFeature();
        }
        return;
      }
      if (e.pointerType !== "mouse" || e.buttons !== 0) return;
      if (isUiTarget(e.target)) {
        hoverPosRef.current = null;
        setHoveredIndex(null);
        clearHoveredFeature();
        return;
      }
      hoverPosRef.current = { x, y };
      scheduleHover();
    },
    [setHoveredIndex, clearHoveredFeature, scheduleHover],
  );

  const clearPointer = useCallback((pointerId: number) => {
    activePointersRef.current.delete(pointerId);
    if (activePointersRef.current.size === 0) suppressTapRef.current = false;
    if (pointerDownRef.current?.id === pointerId) pointerDownRef.current = null;
  }, []);

  const handlePointerUp = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      activePointersRef.current.delete(e.pointerId);
      const wasSuppressed = suppressTapRef.current;
      if (activePointersRef.current.size === 0) suppressTapRef.current = false;

      const down = pointerDownRef.current;
      if (!down || down.id !== e.pointerId) return;
      pointerDownRef.current = null;

      const { x, y } = localPoint(e);
      // Hover tracking pauses while a button is held; resume it where the pointer was released.
      if (e.pointerType === "mouse" && !isUiTarget(e.target)) {
        hoverPosRef.current = { x, y };
        scheduleHover();
      }
      if (e.button !== 0 || wasSuppressed || down.moved || isUiTarget(e.target)) return;

      if (activeGlobeLayer.count > 0) {
        const idx = nearestPointAt(x, y, CLICK_TOLERANCE_PX);
        setSelectedIndex(idx);
        if (idx !== null) {
          viewportRef.current?.focus({ preventScroll: true });
          return;
        }
      }

      if (Math.hypot(x - down.x, y - down.y) >= COUNTRY_CLICK_MAX_MOVE_PX) return;
      const feature = countryAt(x, y);
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
      activeGlobeLayer.count,
      nearestPointAt,
      setSelectedIndex,
      countryAt,
      selectableRegionFor,
      setRegion,
      clearHoveredFeature,
      scheduleHover,
    ],
  );

  const handlePointerCancel = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => clearPointer(e.pointerId),
    [clearPointer],
  );

  const handlePointerLeave = useCallback(() => {
    hoverPosRef.current = null;
    setHoveredIndex(null);
    clearHoveredFeature();
  }, [setHoveredIndex, clearHoveredFeature]);

  const handleDoubleClick = useCallback(
    (e: ReactMouseEvent<HTMLElement>) => {
      cancelPendingCountryTap();
      if (isUiTarget(e.target)) return;
      zoomIn();
    },
    [cancelPendingCountryTap, zoomIn],
  );

  const handleKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLElement>) => {
      if (e.key === "Escape") {
        if (selectedIndex !== null || hoveredIndex !== null) {
          e.preventDefault();
          e.stopPropagation();
          handleCloseDetail();
        }
        return;
      }
      if (isUiTarget(e.target) || e.altKey || e.ctrlKey || e.metaKey) return;
      const current = sceneRef.current;
      if (!current) return;

      switch (e.key) {
        case "n":
        case "N":
        case "p":
        case "P": {
          e.preventDefault();
          if (activeGlobeLayer.count === 0) return;
          const inView = pointsInView(
            activeGlobeLayer.positions,
            activeGlobeLayer.count,
            current.projector(),
            IN_VIEW_MARGIN_PX,
            isPointVisible,
          );
          const ordered = orderedForKeyboard(inView, activeLayer.frp, activeLayer.ids);
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
          if (nextIndex !== undefined) setSelectedIndex(nextIndex);
          return;
        }
        case "ArrowLeft":
        case "ArrowRight":
        case "ArrowUp":
        case "ArrowDown":
          e.preventDefault();
          current.rotateBy(
            e.key === "ArrowLeft" ? -1 : e.key === "ArrowRight" ? 1 : 0,
            e.key === "ArrowUp" ? 1 : e.key === "ArrowDown" ? -1 : 0,
          );
          return;
        case "+":
        case "=":
          e.preventDefault();
          zoomIn();
          return;
        case "-":
        case "_":
          e.preventDefault();
          zoomOut();
          return;
        case "0":
          e.preventDefault();
          fit();
          return;
      }
    },
    [
      selectedIndex,
      hoveredIndex,
      handleCloseDetail,
      activeGlobeLayer,
      activeLayer.frp,
      activeLayer.ids,
      isPointVisible,
      setSelectedIndex,
      zoomIn,
      zoomOut,
      fit,
    ],
  );

  const { visibleRawLoaded, visibleHarmonizedLoaded, rawTotal, harmonizedTotal, overlayState, ariaSummaryText } =
    useMapSummary(model, scenePoints.raw, drawStats);

  return (
    <section className={styles.map} data-surface="globe" aria-label={t("mapLabel")}>
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
        ref={viewportRef}
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
        onPointerLeave={handlePointerLeave}
        onDoubleClick={handleDoubleClick}
        onKeyDown={handleKeyDown}
      >
        <div ref={hostRef} className={globeStyles.host} aria-hidden="true" />

        <MapControls
          canZoomIn={viewState.canZoomIn}
          canZoomOut={viewState.canZoomOut}
          isAtFit={viewState.isAtFit}
          onZoomIn={zoomIn}
          onZoomOut={zoomOut}
          onReset={fit}
          rotates
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
            viewportSize={size}
            onClose={handleCloseDetail}
            rawPoints={points.raw}
            cardRef={detailCardRef}
          />
        )}

        <div className={styles.legendDock} data-map-ui="true">
          <MapLegend
            sensorFilter={effectiveSensorFilter}
            onSensorFilterChange={setSensorFilter}
            frpBreaks={frpBreaks}
          />
        </div>

        <div className={globeStyles.credits}>
          {viewState.orbitsVisible && (
            <p className={globeStyles.orbitNote}>
              <span className={globeStyles.orbitGlyph} aria-hidden="true" />
              <span className={globeStyles.orbitLabel}>{t("mapGlobeOrbitsLabel")}</span>
              <span>{t("mapGlobeOrbitsNote")}</span>
            </p>
          )}
          <p className={globeStyles.credit}>{t("mapGlobeCredit")}</p>
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

export default memo(GlobeMap);
