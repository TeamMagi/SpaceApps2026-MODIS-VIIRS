import { useCallback, useEffect, useMemo, useState, type RefObject } from "react";
import { formatRegionName, TR_REGION_NAMES } from "../../content/copy";
import { MAP_CONTEXT_NAMES_TR } from "../../content/copyMap";
import { useCopy } from "../../hooks/useCopy";
import { useHarmonizeTransition } from "../../hooks/useHarmonizeTransition";
import { useMapPoints, useRegionData, useRegions } from "../../hooks/useRegionData";
import { formatYearSpan } from "../../lib/format";
import { padBBox, type GeoBBox } from "../../lib/geo/projection";
import { matchRegionsToFeatures, type RegionRef } from "../../lib/geo/regionMatch";
import { computeQuantileBreaks } from "../../lib/scales";
import { useAppStore } from "../../store/useAppStore";
import type { HarmonizedPoint, MapState, RawPoint, RegionSummary } from "../../types";
import { pointMatchesSensorFilter } from "./engine/renderer";
import { PointFlag, type DrawStats, type ScenePointInput, type ScenePointLayer } from "./engine/types";
import type { SensorFilter } from "./MapLegend";
import { formatPointAnnouncement } from "./PointDetail";
import { useGeoDataset } from "./useGeoDataset";

export const DEFAULT_SENSOR_FILTER: SensorFilter = { modis: true, viirs: true };

/** Bumps whenever the theme attributes change so canvas palettes are re-read. */
export function usePaletteVersion(targetRef: RefObject<HTMLElement | null>): number {
  const [paletteVersion, setPaletteVersion] = useState(0);
  useEffect(() => {
    if (typeof MutationObserver === "undefined" || typeof document === "undefined") return;
    const bump = () => setPaletteVersion((v) => v + 1);
    const observer = new MutationObserver(bump);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme", "data-palette", "class", "style"],
    });
    if (targetRef.current && targetRef.current !== document.documentElement) {
      observer.observe(targetRef.current, {
        attributes: true,
        attributeFilter: ["data-theme", "data-palette", "class", "style"],
      });
    }
    return () => observer.disconnect();
  }, [targetRef]);
  return paletteVersion;
}

/**
 * Surface-independent map state shared by the globe and the flat fallback: region matching,
 * sensor filter, hover/selection scopes, the hovered-country card and the point detail pairing.
 */
export function useMapModel() {
  const copy = useCopy();
  const { t, fmt } = copy;
  const { region } = useRegionData();
  const { regions } = useRegions();
  const setRegion = useAppStore((state) => state.setRegion);
  const points = useMapPoints();
  const geo = useGeoDataset();
  const { mode, targetMode, geometryHarmonized, showDuplicateRings, revealAnswer } =
    useHarmonizeTransition((v) => ({
      mode: v.mode,
      targetMode: v.targetMode,
      geometryHarmonized: v.geometryHarmonized,
      showDuplicateRings: v.showDuplicateRings,
      revealAnswer: v.revealAnswer,
    }));
  const selectedMonth = useAppStore((state) => state.selectedMonth);
  const selectMonth = useAppStore((state) => state.selectMonth);
  const theme = useAppStore((state) => state.theme);
  const lang = useAppStore((state) => state.lang);

  const [sensorFilter, setSensorFilter] = useState<SensorFilter>(DEFAULT_SENSOR_FILTER);

  // Sensor filter is Raw-only; reset whenever region or view mode changes so it is never stuck active when hidden.
  useEffect(() => {
    setSensorFilter((prev) =>
      prev.modis && prev.viirs ? prev : DEFAULT_SENSOR_FILTER,
    );
  }, [region.id, mode, targetMode, geometryHarmonized]);

  const effectiveSensorFilter =
    geometryHarmonized || revealAnswer ? DEFAULT_SENSOR_FILTER : sensorFilter;

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
  const selectableFeatureSet = useMemo<ReadonlySet<number>>(() => {
    const set = new Set<number>();
    if (!regionMatch) return set;
    const { regionByFeature } = regionMatch;
    for (let i = 0; i < regionByFeature.length; i++) {
      const rId = regionByFeature[i];
      if (rId === null || rId === region.id) continue;
      const summary = regionById.get(rId);
      if (summary && !summary.tooLarge) {
        set.add(i);
      }
    }
    return set;
  }, [regionMatch, region.id, regionById]);
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

  const pointInput = useMemo<ScenePointInput>(
    () => ({
      raw: points.raw,
      harmonized: points.harmonized,
    }),
    [points.raw, points.harmonized],
  );

  const selectionScope = `${region.id}:${points.period.year ?? "none"}:${points.period.month ?? "all"}:${geometryHarmonized ? "h" : "r"}`;

  const [hoverState, setHoverState] = useState<{ scope: string; index: number | null }>({
    scope: selectionScope,
    index: null,
  });
  const [selectedState, setSelectedState] = useState<{ scope: string; index: number | null }>({
    scope: selectionScope,
    index: null,
  });

  const setHoveredIndex = useCallback(
    (index: number | null) =>
      setHoverState((prev) =>
        prev.scope === selectionScope && prev.index === index
          ? prev
          : { scope: selectionScope, index },
      ),
    [selectionScope],
  );
  const setSelectedIndex = useCallback(
    (index: number | null) => setSelectedState({ scope: selectionScope, index }),
    [selectionScope],
  );

  const rawHoveredIndex = hoverState.scope === selectionScope ? hoverState.index : null;
  const rawSelectedIndex = selectedState.scope === selectionScope ? selectedState.index : null;

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

  const periodText =
    points.period.year === null
      ? ""
      : points.period.month === null
        ? String(points.period.year)
        : fmt.monthYear(
            { year: points.period.year, month: points.period.month },
            "long",
          );

  const rawFrpBreaks = useMemo(
    () => computeQuantileBreaks(points.raw.map((p) => p.frp)),
    [points.raw],
  );
  const harmonizedFrpBreaks = useMemo(
    () => computeQuantileBreaks(points.harmonized.map((p) => p.frpCorrected)),
    [points.harmonized],
  );
  const frpBreaks = geometryHarmonized ? harmonizedFrpBreaks : rawFrpBreaks;

  return {
    copy,
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
    setHoveredIndex,
    setSelectedIndex,
    rawHoveredIndex,
    rawSelectedIndex,
    hoveredFeature,
    setHoveredFeature,
    selectableRegionFor,
    hoveredCountry,
    highlightedFeature,
    harmById,
    rawByHarmId,
    periodText,
  };
}

export type MapModel = ReturnType<typeof useMapModel>;

/** Hover/selection resolved against the active layer and sensor filter, plus the detail pairing. */
export function usePointSelection(model: MapModel, activeLayer: ScenePointLayer) {
  const {
    effectiveSensorFilter,
    rawHoveredIndex,
    rawSelectedIndex,
    geometryHarmonized,
    points,
    rawByHarmId,
    harmById,
    copy,
    revealAnswer,
  } = model;

  const isPointVisible = useCallback(
    (idx: number): boolean =>
      idx >= 0 &&
      idx < activeLayer.count &&
      pointMatchesSensorFilter(activeLayer.flags[idx], activeLayer.shapes[idx], effectiveSensorFilter),
    [activeLayer.count, activeLayer.flags, activeLayer.shapes, effectiveSensorFilter],
  );

  const hoveredIndex =
    rawHoveredIndex !== null && isPointVisible(rawHoveredIndex) ? rawHoveredIndex : null;
  const selectedIndex =
    rawSelectedIndex !== null && isPointVisible(rawSelectedIndex) ? rawSelectedIndex : null;

  const pairFor = useCallback(
    (index: number | null) => {
      if (index === null || !isPointVisible(index)) {
        return {
          rawPoint: null as RawPoint | null,
          harmPoint: null as HarmonizedPoint | null,
          isDuplicateInRaw: false,
        };
      }
      const isDuplicateInRaw = (activeLayer.flags[index] & PointFlag.Duplicate) !== 0;
      if (geometryHarmonized) {
        const harmPoint = points.harmonized[index] ?? null;
        const rawPoint = harmPoint ? (rawByHarmId.get(harmPoint.id) ?? null) : null;
        return { rawPoint, harmPoint, isDuplicateInRaw };
      }
      const rawPoint = points.raw[index] ?? null;
      const harmPoint =
        rawPoint && rawPoint.harmonizedId !== null
          ? (harmById.get(rawPoint.harmonizedId) ?? null)
          : null;
      return { rawPoint, harmPoint, isDuplicateInRaw };
    },
    [activeLayer.flags, isPointVisible, geometryHarmonized, points.harmonized, points.raw, rawByHarmId, harmById],
  );

  const selectedPair = useMemo(() => pairFor(selectedIndex), [pairFor, selectedIndex]);

  const liveAnnouncement = useMemo(
    () =>
      selectedIndex !== null
        ? formatPointAnnouncement(
            copy,
            selectedPair.rawPoint,
            selectedPair.harmPoint,
            selectedPair.isDuplicateInRaw,
            revealAnswer,
          )
        : "",
    [copy, selectedIndex, selectedPair, revealAnswer],
  );

  return { isPointVisible, hoveredIndex, selectedIndex, pairFor, liveAnnouncement };
}

/** Counts, overlay state and the screen-reader summary; Raw totals never use harmonized numbers. */
export function useMapSummary(model: MapModel, rawLayer: ScenePointLayer, drawStats: DrawStats) {
  const { points, effectiveSensorFilter, revealAnswer, t, region, periodText } = model;

  const visibleRawLoaded = useMemo(() => {
    if (effectiveSensorFilter.modis && effectiveSensorFilter.viirs) return points.raw.length;
    if (!effectiveSensorFilter.modis && !effectiveSensorFilter.viirs) return 0;
    const { flags, shapes, count } = rawLayer;
    let n = 0;
    for (let i = 0; i < count; i++) {
      if (pointMatchesSensorFilter(flags[i], shapes[i], effectiveSensorFilter)) n++;
    }
    return n;
  }, [points.raw.length, rawLayer, effectiveSensorFilter]);

  const visibleHarmonizedLoaded = points.harmonized.length;

  const stageRawTotal =
    (effectiveSensorFilter.modis ? points.stages.rawModis : 0) +
    (effectiveSensorFilter.viirs ? points.stages.rawViirs : 0);
  // Never fall back to a capped sample count when the response is truncated.
  const rawTotal =
    !effectiveSensorFilter.modis && !effectiveSensorFilter.viirs
      ? 0
      : stageRawTotal > 0
        ? stageRawTotal
        : points.truncated
          ? 0
          : visibleRawLoaded;
  const harmonizedTotal =
    points.stages.harmonized > 0
      ? points.stages.harmonized
      : points.truncated
        ? 0
        : points.harmonized.length;
  const activeTotal = revealAnswer ? harmonizedTotal : rawTotal;
  const activeLoaded = revealAnswer ? visibleHarmonizedLoaded : visibleRawLoaded;
  const shownCount =
    !effectiveSensorFilter.modis && !effectiveSensorFilter.viirs
      ? 0
      : drawStats.total > 0
        ? drawStats.drawn
        : activeLoaded;

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

  return {
    visibleRawLoaded,
    visibleHarmonizedLoaded,
    rawTotal,
    harmonizedTotal,
    overlayState,
    ariaSummaryText,
  };
}
