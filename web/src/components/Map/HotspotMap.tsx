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
import { formatRegionName, TR_REGION_NAMES } from "../../content/copy";
import { MAP_CONTEXT_NAMES_TR } from "../../content/copyMap";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useMapPoints, useRegionData, useRegions } from "../../hooks/useRegionData";
import { formatYearSpan } from "../../lib/format";
import {
  createProjection,
  padBBox,
  planeBounds,
  regionExtent,
  type GeoBBox,
  type PlaneBounds,
} from "../../lib/geo/projection";
import { matchRegionsToFeatures, type RegionRef } from "../../lib/geo/regionMatch";
import { useAppStore } from "../../store/useAppStore";
import type { HarmonizedPoint, MapState, RawPoint, RegionSummary } from "../../types";
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
import MapCountryTooltip from "./MapCountryTooltip";
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

function HotspotMap() {
  const copy = useCopy();
  const { t, fmt } = copy;
  const { region } = useRegionData();
  const { regions } = useRegions();
  const setRegion = useAppStore((state) => state.setRegion);
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
  const pickHintId = useId();
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const highlightCanvasRef = useRef<HTMLCanvasElement | null>(null);

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

  const regionById = useMemo(() => {
    const map = new Map<string, RegionSummary>();
    for (const summary of regions) map.set(summary.id, summary);
    return map;
  }, [regions]);

  const matchRefs = useMemo<readonly RegionRef[]>(() => {
    const refs: RegionRef[] = regions.map((summary) => ({ id: summary.id, name: summary.name }));
    if (!regionById.has(region.id)) refs.push({ id: region.id, name: region.name });
    return refs;
  }, [regions, regionById, region.id, region.name]);

  const regionMatch = useMemo(
    () => (geo.dataset ? matchRegionsToFeatures(geo.dataset.features, matchRefs) : null),
    [geo.dataset, matchRefs],
  );

  const selectedFeatureSet = useMemo<ReadonlySet<number>>(
    () => new Set(regionMatch?.featuresByRegion.get(region.id) ?? []),
    [regionMatch, region.id],
  );
  const regionFeatures = useMemo(() => {
    const dataset = geo.dataset;
    return dataset ? [...selectedFeatureSet].map((idx) => dataset.features[idx]) : [];
  }, [geo.dataset, selectedFeatureSet]);
  const hasRegionGeo = regionFeatures.length > 0;
  const isUnmappedRegion = geo.status === "ready" && !hasRegionGeo;

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

  const sceneGeometry = useMemo(
    () => buildSceneGeometry(geo.dataset, region.id, projection, selectedFeatureSet),
    [geo.dataset, region.id, projection, selectedFeatureSet],
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

  const [hoveredFeature, setHoveredFeature] = useState<number | null>(null);

  /** Archive region a click on this feature would switch to, or null when it is not selectable. */
  const selectableRegionFor = useCallback(
    (featureIndex: number): string | null => {
      const regionId = regionMatch?.regionByFeature[featureIndex] ?? null;
      if (regionId === null || regionId === region.id) return null;
      const summary = regionById.get(regionId);
      return summary && !summary.tooLarge ? regionId : null;
    },
    [regionMatch, region.id, regionById],
  );

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

  const hoveredCountry = useMemo(() => {
    const dataset = geo.dataset;
    if (hoveredFeature === null || !dataset) return null;
    const feature = dataset.features[hoveredFeature];
    if (!feature) return null;
    const regionId = regionMatch?.regionByFeature[hoveredFeature] ?? null;
    if (regionId === region.id) return null;
    const summary = regionId ? regionById.get(regionId) : undefined;
    const contextName =
      lang === "tr" ? (MAP_CONTEXT_NAMES_TR[feature.code] ?? feature.name) : feature.name;
    if (!summary) {
      return { name: contextName, detail: t("mapCountryNoData"), hint: null, available: false };
    }
    const name =
      lang === "tr"
        ? (TR_REGION_NAMES[summary.id] ?? MAP_CONTEXT_NAMES_TR[feature.code] ?? formatRegionName(summary, lang))
        : formatRegionName(summary, lang);
    if (summary.tooLarge) {
      const detail =
        summary.sizeMb !== null
          ? t("regionTooLarge", {
              size: fmt.number(summary.sizeMb, Number.isInteger(summary.sizeMb) ? 0 : 1),
            })
          : t("regionTooLargeNoSize");
      return { name, detail, hint: null, available: false };
    }
    const coverage: string[] = [];
    if (summary.modisYears) {
      coverage.push(t("regionCoverageModis", { span: formatYearSpan(summary.modisYears) }));
    }
    if (summary.viirsYears) {
      coverage.push(t("regionCoverageViirs", { span: formatYearSpan(summary.viirsYears) }));
    }
    return {
      name,
      detail: coverage.length > 0 ? coverage.join(" · ") : null,
      hint: t("mapCountrySelectHint"),
      available: true,
    };
  }, [geo.dataset, hoveredFeature, regionMatch, region.id, regionById, lang, t, fmt]);

  const highlightedFeature = hoveredCountry?.available ? hoveredFeature : null;

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
      const idx = nearest(activeLayer.spatialIndex, planeX, planeY, maxDistPlane);
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
  }, [viewport.transform, activeLayer, selectionScope, countryAt, clearHoveredFeature, positionTooltip]);

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

  // Zoom (wheel, keys, buttons) moves countries under a resting pointer.
  useEffect(() => {
    if (hoverPosRef.current && pointerDownRef.current === null) scheduleHover();
  }, [viewport.transform, scene, scheduleHover]);

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
        const idx = nearest(activeLayer.spatialIndex, planeX, planeY, maxDistPlane);
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
