import { projectRing, type RegionProjection } from "../../../lib/geo/projection";
import type { GeoDataset, GeoFeature } from "../../../lib/geo/types";
import { frpBucket } from "../../../lib/scales";
import { HotspotSource, type HarmonizedPoint, type RawPoint, type Theme } from "../../../types";
import { LOD_SPAN_DIVISORS, pickLodLevel, simplifyRing } from "./lod";
import { buildIndex } from "./pointIndex";
import {
  PointFlag,
  PointShape,
  type DrawMapView,
  type DrawStats,
  type MapPalette,
  type MapScene,
  type PlaneBounds,
  type SceneFeature,
  type SceneGeometry,
  type ScenePointInput,
  type ScenePoints,
  type ScenePointLayer,
  type SensorFilter,
  type ViewTransform,
} from "./types";

export const DEFAULT_MAX_DRAWN = 30_000;

const TAU = Math.PI * 2;
const VIEWPORT_MARGIN_PX = 12;
const DUP_LINE_DASH: number[] = [2.5, 2];
const SCRATCH_RADII = new Float64Array(8);
const SCRATCH_PATHS: (Path2D | null)[] = [null, null, null, null, null, null, null, null];
const SCRATCH_DIM_PATHS: Path2D[] = [];
const SCRATCH_SELECTABLE_PATHS: Path2D[] = [];

const DARK_DEFAULTS: MapPalette = {
  water: "#0a0c0f",
  land: "#14171d",
  landSelected: "#1f2531",
  landDim: "#101319",
  border: "#262c38",
  borderSelected: "#5a657d",
  ink: "#f1f3f7",
  haloLight: "#0d0f13",
  modis: "#d4d9e2",
  viirs: "#8d96a6",
  duplicateRing: "#f1f3f7",
  focus: "#d0d6e2",
  frp1: "#3d0f5e",
  frp2: "#6a176e",
  frp3: "#932667",
  frp4: "#bc3754",
  frp5: "#dd513a",
  frp6: "#f37819",
  frp7: "#fcb519",
  frp8: "#f6e27a",
  frp: [
    "#3d0f5e",
    "#6a176e",
    "#932667",
    "#bc3754",
    "#dd513a",
    "#f37819",
    "#fcb519",
    "#f6e27a",
  ],
};

const LIGHT_DEFAULTS: MapPalette = {
  water: "#eef0f3",
  land: "#f3f5f8",
  landSelected: "#ffffff",
  landDim: "#e7eaef",
  border: "#cfd4dc",
  borderSelected: "#7c8698",
  ink: "#12161d",
  haloLight: "#ffffff",
  modis: "#3a4252",
  viirs: "#8a93a3",
  duplicateRing: "#12161d",
  focus: "#1b2230",
  frp1: "#fbe7a1",
  frp2: "#fcc85a",
  frp3: "#f89b2c",
  frp4: "#e8682a",
  frp5: "#c8413f",
  frp6: "#9c2a5c",
  frp7: "#6e1c6c",
  frp8: "#3d0f5e",
  frp: [
    "#fbe7a1",
    "#fcc85a",
    "#f89b2c",
    "#e8682a",
    "#c8413f",
    "#9c2a5c",
    "#6e1c6c",
    "#3d0f5e",
  ],
};

function clamp(v: number, lo: number, hi: number): number {
  return v < lo ? lo : v > hi ? hi : v;
}

function readVar(
  style: CSSStyleDeclaration | null,
  primary: string,
  secondary: string | null,
  fallback: string,
): string {
  if (!style) return fallback;
  const v1 = style.getPropertyValue(primary).trim();
  if (v1.length > 0) return v1;
  if (secondary) {
    const v2 = style.getPropertyValue(secondary).trim();
    if (v2.length > 0) return v2;
  }
  return fallback;
}

interface PaletteCacheEntry {
  readonly signature: string;
  readonly palette: MapPalette;
}

const elementPaletteCache = new WeakMap<
  HTMLElement,
  Map<Theme, PaletteCacheEntry>
>();
const detachedPaletteCache = new Map<Theme, PaletteCacheEntry>();

function buildPaletteSignature(target: HTMLElement | null): string {
  const root =
    typeof document !== "undefined" ? document.documentElement : null;
  const rootSig = root
    ? `${root.dataset.theme ?? ""}|${root.className}|${root.getAttribute("style") ?? ""}`
    : "";
  if (!target) {
    return `${rootSig}||disconnected`;
  }
  const targetSig =
    target === root
      ? "root"
      : `${target.dataset.theme ?? ""}|${target.className}|${target.getAttribute("style") ?? ""}`;
  return `${rootSig}||${targetSig}|${target.isConnected ? "1" : "0"}`;
}

export function readPalette(
  el?: HTMLElement | null,
  theme?: Theme,
): MapPalette {
  const target =
    el ?? (typeof document !== "undefined" ? document.documentElement : null);
  const resolvedTheme: Theme =
    theme ??
    (typeof document !== "undefined" &&
    document.documentElement.dataset.theme === "light"
      ? "light"
      : "dark");
  const signature = buildPaletteSignature(target);
  const themeMap = target
    ? elementPaletteCache.get(target)
    : detachedPaletteCache;
  const cached = themeMap?.get(resolvedTheme);
  if (cached && cached.signature === signature) {
    return cached.palette;
  }

  const base = resolvedTheme === "light" ? LIGHT_DEFAULTS : DARK_DEFAULTS;
  const style =
    target && typeof window !== "undefined" && typeof window.getComputedStyle === "function"
      ? window.getComputedStyle(target)
      : null;

  const frp1 = readVar(style, "--frp-1", null, base.frp1);
  const frp2 = readVar(style, "--frp-2", null, base.frp2);
  const frp3 = readVar(style, "--frp-3", null, base.frp3);
  const frp4 = readVar(style, "--frp-4", null, base.frp4);
  const frp5 = readVar(style, "--frp-5", null, base.frp5);
  const frp6 = readVar(style, "--frp-6", null, base.frp6);
  const frp7 = readVar(style, "--frp-7", null, base.frp7);
  const frp8 = readVar(style, "--frp-8", null, base.frp8);

  const palette: MapPalette = {
    water: readVar(style, "--map-water", "--color-surface-sunken", base.water),
    land: readVar(style, "--map-land", "--color-surface", base.land),
    landSelected: readVar(
      style,
      "--map-land-selected",
      "--color-surface-raised",
      base.landSelected,
    ),
    landDim: readVar(style, "--map-land-dim", "--color-bg", base.landDim),
    border: readVar(style, "--map-border", "--color-border-subtle", base.border),
    borderSelected: readVar(
      style,
      "--map-border-selected",
      "--color-border-strong",
      base.borderSelected,
    ),
    ink: readVar(style, "--map-ink", "--color-text", base.ink),
    haloLight: readVar(style, "--map-halo-light", "--color-bg", base.haloLight),
    modis: readVar(style, "--map-modis", "--color-sensor-modis", base.modis),
    viirs: readVar(style, "--map-viirs", "--color-sensor-viirs", base.viirs),
    duplicateRing: readVar(
      style,
      "--map-duplicate-ring",
      "--color-duplicate-ring",
      base.duplicateRing,
    ),
    focus: readVar(style, "--map-focus", "--color-focus", base.focus),
    frp1,
    frp2,
    frp3,
    frp4,
    frp5,
    frp6,
    frp7,
    frp8,
    frp: [frp1, frp2, frp3, frp4, frp5, frp6, frp7, frp8],
  };

  if (target) {
    let targetCache = elementPaletteCache.get(target);
    if (!targetCache) {
      targetCache = new Map();
      elementPaletteCache.set(target, targetCache);
    }
    targetCache.set(resolvedTheme, { signature, palette });
  } else {
    detachedPaletteCache.set(resolvedTheme, { signature, palette });
  }

  return palette;
}

function extractFeatures(
  input: GeoDataset | readonly GeoFeature[] | null | undefined,
): readonly GeoFeature[] {
  if (!input) return [];
  if (Array.isArray(input)) return input;
  if ("features" in input && Array.isArray(input.features)) return input.features;
  return [];
}

function buildPathFromRings(rings: readonly Float32Array[]): Path2D | null {
  if (rings.length === 0 || typeof Path2D === "undefined") return null;
  const path = new Path2D();
  for (let r = 0; r < rings.length; r++) {
    const coords = rings[r];
    const len = coords.length;
    if (len < 4) continue;
    path.moveTo(coords[0], coords[1]);
    for (let i = 2; i < len; i += 2) {
      path.lineTo(coords[i], coords[i + 1]);
    }
    path.closePath();
  }
  return path;
}

function fillRingsFallback(
  ctx: CanvasRenderingContext2D,
  rings: readonly Float32Array[],
  fillStyle: string,
): void {
  if (rings.length === 0) return;
  ctx.beginPath();
  for (let r = 0; r < rings.length; r++) {
    const coords = rings[r];
    const len = coords.length;
    if (len < 4) continue;
    ctx.moveTo(coords[0], coords[1]);
    for (let i = 2; i < len; i += 2) {
      ctx.lineTo(coords[i], coords[i + 1]);
    }
    ctx.closePath();
  }
  ctx.fillStyle = fillStyle;
  ctx.fill();
}

function strokeRingsFallback(
  ctx: CanvasRenderingContext2D,
  rings: readonly Float32Array[],
  strokeStyle: string,
  lineWidth: number,
): void {
  if (rings.length === 0) return;
  ctx.beginPath();
  for (let r = 0; r < rings.length; r++) {
    const coords = rings[r];
    const len = coords.length;
    if (len < 4) continue;
    ctx.moveTo(coords[0], coords[1]);
    for (let i = 2; i < len; i += 2) {
      ctx.lineTo(coords[i], coords[i + 1]);
    }
    ctx.closePath();
  }
  ctx.strokeStyle = strokeStyle;
  ctx.lineWidth = lineWidth;
  ctx.stroke();
}

export function pointMatchesSensorFilter(
  flag: number,
  shape: number,
  filter?: SensorFilter | null,
): boolean {
  if (!filter || (filter.modis && filter.viirs)) return true;
  if (!filter.modis && !filter.viirs) return false;
  const sensorBits = flag & (PointFlag.Modis | PointFlag.Viirs);
  if (sensorBits !== 0) {
    const mask = (filter.modis ? PointFlag.Modis : 0) | (filter.viirs ? PointFlag.Viirs : 0);
    return (sensorBits & mask) !== 0;
  }
  return shape === PointShape.HollowCircle ? filter.modis : filter.viirs;
}

function emptyPointLayer(): ScenePointLayer {
  const emptyF32 = new Float32Array(0);
  const emptyU8 = new Uint8Array(0);
  return {
    count: 0,
    xs: emptyF32,
    ys: emptyF32,
    frp: emptyF32,
    buckets: emptyU8,
    shapes: emptyU8,
    flags: emptyU8,
    ids: [],
    indexById: new Map(),
    spatialIndex: buildIndex(emptyF32, emptyF32),
  };
}

function buildRawLayer(
  rawPoints: readonly RawPoint[],
  rawCountByHarmId: ReadonlyMap<string, number>,
  matchedHarmIds: ReadonlySet<string>,
  projection: RegionProjection,
): ScenePointLayer {
  const count = rawPoints.length;
  if (count === 0) return emptyPointLayer();

  const xs = new Float32Array(count);
  const ys = new Float32Array(count);
  const frp = new Float32Array(count);
  const buckets = new Uint8Array(count);
  const shapes = new Uint8Array(count);
  const flags = new Uint8Array(count);
  const ids = new Array<string>(count);
  const indexById = new Map<string, number>();

  for (let i = 0; i < count; i++) {
    const p = rawPoints[i];
    const [px, py] = projection.project(p.lon, p.lat);
    xs[i] = px;
    ys[i] = py;
    frp[i] = p.frp;
    buckets[i] = frpBucket(p.frp);
    shapes[i] = p.sensor === "MODIS" ? PointShape.HollowCircle : PointShape.Square;

    let flag = p.sensor === "MODIS" ? PointFlag.Modis : PointFlag.Viirs;
    const hId = p.harmonizedId;
    if (hId === null) {
      flag |= PointFlag.Duplicate | PointFlag.Removed;
    } else {
      if ((rawCountByHarmId.get(hId) ?? 0) > 1) {
        flag |= PointFlag.Duplicate;
      }
      if (matchedHarmIds.has(hId)) {
        flag |= PointFlag.Duplicate | PointFlag.Matched;
      }
    }
    flags[i] = flag;
    ids[i] = p.id;
    indexById.set(p.id, i);
  }

  return {
    count,
    xs,
    ys,
    frp,
    buckets,
    shapes,
    flags,
    ids,
    indexById,
    spatialIndex: buildIndex(xs, ys),
  };
}

function buildHarmonizedLayer(
  harmPoints: readonly HarmonizedPoint[],
  rawCountByHarmId: ReadonlyMap<string, number>,
  projection: RegionProjection,
): ScenePointLayer {
  const count = harmPoints.length;
  if (count === 0) return emptyPointLayer();

  const xs = new Float32Array(count);
  const ys = new Float32Array(count);
  const frp = new Float32Array(count);
  const buckets = new Uint8Array(count);
  const shapes = new Uint8Array(count);
  const flags = new Uint8Array(count);
  const ids = new Array<string>(count);
  const indexById = new Map<string, number>();

  for (let i = 0; i < count; i++) {
    const p = harmPoints[i];
    const [px, py] = projection.project(p.lon, p.lat);
    xs[i] = px;
    ys[i] = py;
    frp[i] = p.frpCorrected;
    buckets[i] = frpBucket(p.frpCorrected);
    shapes[i] = PointShape.Circle;

    let flag =
      p.source === HotspotSource.ModisOnly
        ? PointFlag.Modis
        : p.source === HotspotSource.ViirsOnly
          ? PointFlag.Viirs
          : PointFlag.Modis | PointFlag.Viirs;
    if (p.source === HotspotSource.Matched) {
      flag |= PointFlag.Duplicate | PointFlag.Matched;
    } else if ((rawCountByHarmId.get(p.id) ?? 0) > 1) {
      flag |= PointFlag.Duplicate;
    }
    flags[i] = flag;
    ids[i] = p.id;
    indexById.set(p.id, i);
  }

  return {
    count,
    xs,
    ys,
    frp,
    buckets,
    shapes,
    flags,
    ids,
    indexById,
    spatialIndex: buildIndex(xs, ys),
  };
}

const EMPTY_GEOMETRY: SceneGeometry = {
  features: [],
  contextRings: [],
  selectedRings: [],
  selectedPath: null,
  lodTolerances: [],
  selectedBounds: null,
  allBounds: null,
};

interface ProjectedFeature {
  readonly selected: boolean;
  readonly selectable: boolean;
  readonly rings: Float32Array[];
  readonly ringBounds: number[];
  readonly minX: number;
  readonly minY: number;
  readonly maxX: number;
  readonly maxY: number;
}

function buildLodPath(
  rings: readonly Float32Array[],
  ringBounds: readonly number[],
  tolerance: number,
): Path2D | null {
  const kept: Float32Array[] = [];
  for (let r = 0; r < rings.length; r++) {
    const b = r * 4;
    // Rings smaller than the tolerance are sub-pixel wherever this level is drawn.
    if (ringBounds[b + 2] - ringBounds[b] < tolerance && ringBounds[b + 3] - ringBounds[b + 1] < tolerance) {
      continue;
    }
    const simplified = simplifyRing(rings[r], tolerance);
    if (simplified) kept.push(simplified);
  }
  return buildPathFromRings(kept);
}

/** Projected boundaries only; independent of points so period changes never re-project geometry. */
export function buildSceneGeometry(
  datasetOrFeatures: GeoDataset | readonly GeoFeature[] | null | undefined,
  regionId: string,
  projection: RegionProjection | null | undefined,
  selectedFeatures?: ReadonlySet<number> | null,
  selectableFeatures?: ReadonlySet<number> | null,
): SceneGeometry {
  if (!projection) return EMPTY_GEOMETRY;

  const features = extractFeatures(datasetOrFeatures);
  const contextRings: Float32Array[] = [];
  const selectedRings: Float32Array[] = [];
  const projected: ProjectedFeature[] = [];

  let selMinX = Infinity;
  let selMinY = Infinity;
  let selMaxX = -Infinity;
  let selMaxY = -Infinity;

  let allMinX = Infinity;
  let allMinY = Infinity;
  let allMaxX = -Infinity;
  let allMaxY = -Infinity;

  for (let fIdx = 0; fIdx < features.length; fIdx++) {
    const feature = features[fIdx];
    const isSelected = selectedFeatures
      ? selectedFeatures.has(fIdx)
      : feature.regionId === regionId;
    const isSelectable = !isSelected && Boolean(selectableFeatures?.has(fIdx));
    const targetRings = isSelected ? selectedRings : contextRings;
    const polygons = feature.polygons;
    const featureRings: Float32Array[] = [];
    const ringBoxes: number[] = [];
    let fMinX = Infinity;
    let fMinY = Infinity;
    let fMaxX = -Infinity;
    let fMaxY = -Infinity;

    for (let pIdx = 0; pIdx < polygons.length; pIdx++) {
      const rings = polygons[pIdx];
      for (let rIdx = 0; rIdx < rings.length; rIdx++) {
        const ring = rings[rIdx];
        if (ring.length < 3) continue;
        const coords = projectRing(projection, ring);
        let rMinX = Infinity;
        let rMinY = Infinity;
        let rMaxX = -Infinity;
        let rMaxY = -Infinity;
        for (let i = 0; i < coords.length; i += 2) {
          const px = coords[i];
          const py = coords[i + 1];
          if (px < rMinX) rMinX = px;
          if (py < rMinY) rMinY = py;
          if (px > rMaxX) rMaxX = px;
          if (py > rMaxY) rMaxY = py;
        }
        if (rMinX < fMinX) fMinX = rMinX;
        if (rMinY < fMinY) fMinY = rMinY;
        if (rMaxX > fMaxX) fMaxX = rMaxX;
        if (rMaxY > fMaxY) fMaxY = rMaxY;
        ringBoxes.push(rMinX, rMinY, rMaxX, rMaxY);
        targetRings.push(coords);
        featureRings.push(coords);
      }
    }

    if (fMinX < allMinX) allMinX = fMinX;
    if (fMinY < allMinY) allMinY = fMinY;
    if (fMaxX > allMaxX) allMaxX = fMaxX;
    if (fMaxY > allMaxY) allMaxY = fMaxY;
    if (isSelected) {
      if (fMinX < selMinX) selMinX = fMinX;
      if (fMinY < selMinY) selMinY = fMinY;
      if (fMaxX > selMaxX) selMaxX = fMaxX;
      if (fMaxY > selMaxY) selMaxY = fMaxY;
    }

    projected.push({
      selected: isSelected,
      selectable: isSelectable,
      rings: featureRings,
      ringBounds: ringBoxes,
      minX: fMinX,
      minY: fMinY,
      maxX: fMaxX,
      maxY: fMaxY,
    });
  }

  const span = Math.max(allMaxX - allMinX, allMaxY - allMinY);
  const lodTolerances =
    Number.isFinite(span) && span > 0 ? LOD_SPAN_DIVISORS.map((d) => span / d) : [];

  const sceneFeatures: SceneFeature[] = projected.map((f) => ({
    selected: f.selected,
    selectable: f.selectable,
    rings: f.rings,
    ringBounds: Float32Array.from(f.ringBounds),
    minX: f.minX,
    minY: f.minY,
    maxX: f.maxX,
    maxY: f.maxY,
    path: buildPathFromRings(f.rings),
    // Selected features are always drawn from the full-detail selected path.
    lodPaths:
      f.selected || typeof Path2D === "undefined"
        ? []
        : lodTolerances.map((tol) => buildLodPath(f.rings, f.ringBounds, tol)),
  }));

  return {
    features: sceneFeatures,
    contextRings,
    selectedRings,
    selectedPath: buildPathFromRings(selectedRings),
    lodTolerances,
    selectedBounds:
      Number.isFinite(selMinX) && Number.isFinite(selMaxX)
        ? { minX: selMinX, minY: selMinY, maxX: selMaxX, maxY: selMaxY }
        : null,
    allBounds:
      Number.isFinite(allMinX) && Number.isFinite(allMaxX)
        ? { minX: allMinX, minY: allMinY, maxX: allMaxX, maxY: allMaxY }
        : null,
  };
}

export function buildScenePoints(
  points: ScenePointInput | null | undefined,
  projection: RegionProjection | null | undefined,
): ScenePoints {
  if (!projection) {
    const empty = emptyPointLayer();
    return { raw: empty, harmonized: empty };
  }

  const rawPoints = points?.raw ?? [];
  const harmPoints = points?.harmonized ?? [];

  const rawCountByHarmId = new Map<string, number>();
  for (let i = 0; i < rawPoints.length; i++) {
    const hId = rawPoints[i].harmonizedId;
    if (hId !== null) {
      rawCountByHarmId.set(hId, (rawCountByHarmId.get(hId) ?? 0) + 1);
    }
  }

  const matchedHarmIds = new Set<string>();
  for (let i = 0; i < harmPoints.length; i++) {
    if (harmPoints[i].source === HotspotSource.Matched) {
      matchedHarmIds.add(harmPoints[i].id);
    }
  }

  return {
    raw: buildRawLayer(rawPoints, rawCountByHarmId, matchedHarmIds, projection),
    harmonized: buildHarmonizedLayer(harmPoints, rawCountByHarmId, projection),
  };
}

export function composeScene(
  regionId: string,
  geometry: SceneGeometry,
  points: ScenePoints,
): MapScene {
  const rawLayer = points.raw;
  let bounds: PlaneBounds;
  if (geometry.selectedBounds) {
    bounds = geometry.selectedBounds;
  } else if (rawLayer.count > 0) {
    bounds = {
      minX: rawLayer.spatialIndex.minX,
      minY: rawLayer.spatialIndex.minY,
      maxX: rawLayer.spatialIndex.maxX,
      maxY: rawLayer.spatialIndex.maxY,
    };
  } else if (geometry.allBounds) {
    bounds = geometry.allBounds;
  } else {
    bounds = { minX: -1, minY: -1, maxX: 1, maxY: 1 };
  }

  return {
    regionId,
    bounds,
    features: geometry.features,
    contextRings: geometry.contextRings,
    selectedRings: geometry.selectedRings,
    lodTolerances: geometry.lodTolerances,
    selectedPath: geometry.selectedPath,
    raw: rawLayer,
    harmonized: points.harmonized,
  };
}

export function buildScene(
  datasetOrFeatures: GeoDataset | readonly GeoFeature[] | null | undefined,
  regionId: string,
  points: ScenePointInput | null | undefined,
  projection: RegionProjection | null | undefined,
  selectedFeatures?: ReadonlySet<number> | null,
  selectableFeatures?: ReadonlySet<number> | null,
): MapScene {
  return composeScene(
    regionId,
    buildSceneGeometry(
      datasetOrFeatures,
      regionId,
      projection,
      selectedFeatures,
      selectableFeatures,
    ),
    buildScenePoints(points, projection),
  );
}

function isPaletteObject(val: MapPalette | Theme | undefined): val is MapPalette {
  return typeof val === "object" && val !== null && "landSelected" in val;
}

function pointRadiusFor(
  bucket: number,
  shape: number,
  isHarmonized: boolean,
  zoomScale: number,
): number {
  if (!isHarmonized) {
    const base = shape === PointShape.HollowCircle ? 3.5 : 2.4;
    return shape === PointShape.HollowCircle
      ? clamp(base * zoomScale, 1.75, 7.5)
      : clamp(base * zoomScale, 1.25, 5.8);
  }
  const b = clamp(bucket, 1, 8);
  const base = 2.2 + (b - 1) * 0.48;
  return clamp(base * zoomScale, 1.4, 8.5);
}

function drawHighlightedPoint(
  ctx: CanvasRenderingContext2D,
  layer: ScenePointLayer,
  idx: number,
  k: number,
  tx: number,
  ty: number,
  isHarmonized: boolean,
  zoomScale: number,
  palette: MapPalette,
  isSelected: boolean,
): void {
  if (idx < 0 || idx >= layer.count) return;
  const sx = layer.xs[idx] * k + tx;
  const sy = layer.ys[idx] * k + ty;
  if (!Number.isFinite(sx) || !Number.isFinite(sy)) return;

  const bucket = layer.buckets[idx] || 1;
  const shape = layer.shapes[idx];
  const r = pointRadiusFor(bucket, shape, isHarmonized, zoomScale);
  const ringR = r + 4.5;

  // Contrast halo underneath the highlight ring so it stands out over dense fire clusters.
  ctx.beginPath();
  ctx.arc(sx, sy, ringR, 0, TAU);
  ctx.strokeStyle = palette.haloLight;
  ctx.lineWidth = 4.5;
  ctx.stroke();

  if (shape === PointShape.Square && !isHarmonized) {
    ctx.fillStyle = palette.viirs;
    ctx.fillRect(sx - r, sy - r, r * 2, r * 2);
  } else if (shape === PointShape.HollowCircle && !isHarmonized) {
    ctx.beginPath();
    ctx.arc(sx, sy, r, 0, TAU);
    ctx.strokeStyle = palette.modis;
    ctx.lineWidth = clamp(1.6 * zoomScale, 1.4, 2.4);
    ctx.stroke();
  } else {
    const fill = palette.frp[clamp(bucket - 1, 0, 7)];
    ctx.beginPath();
    ctx.arc(sx, sy, r, 0, TAU);
    ctx.fillStyle = fill;
    ctx.fill();
    ctx.strokeStyle = palette.haloLight;
    ctx.lineWidth = 1;
    ctx.stroke();
  }

  ctx.beginPath();
  ctx.arc(sx, sy, ringR, 0, TAU);
  ctx.strokeStyle = isSelected ? palette.focus : palette.ink;
  ctx.lineWidth = isSelected ? 2.2 : 1.75;
  ctx.stroke();
}

export interface HighlightView {
  readonly transform: ViewTransform;
  readonly width: number;
  readonly height: number;
  readonly dpr: number;
  /** The scene's tolerances; when given, non-selected features are highlighted from their LOD path. */
  readonly lodTolerances?: readonly number[];
}

const HIGHLIGHT_FILL_ALPHA = 0.55;

/**
 * Draws one country highlight on its own overlay canvas so hover changes never repaint the base
 * map and its points. The fill is translucent because the overlay sits above the points.
 */
export function drawFeatureHighlight(
  ctx: CanvasRenderingContext2D,
  feature: SceneFeature | null | undefined,
  view: HighlightView,
  palette: MapPalette,
): void {
  const dpr = clamp(view.dpr, 1, 3);
  const { width, height } = view;
  if (width <= 0 || height <= 0) return;
  const targetW = Math.round(width * dpr);
  const targetH = Math.round(height * dpr);
  if (ctx.canvas.width !== targetW) ctx.canvas.width = targetW;
  if (ctx.canvas.height !== targetH) ctx.canvas.height = targetH;

  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);

  const { k, tx, ty } = view.transform;
  if (!feature?.path || !(k > 0) || !Number.isFinite(tx) || !Number.isFinite(ty)) return;

  const level = view.lodTolerances ? pickLodLevel(view.lodTolerances, k) : -1;
  const path = (level >= 0 ? feature.lodPaths[level] : null) ?? feature.path;

  ctx.save();
  ctx.setTransform(dpr * k, 0, 0, dpr * k, dpr * tx, dpr * ty);
  ctx.lineJoin = "round";
  ctx.globalAlpha = HIGHLIGHT_FILL_ALPHA;
  ctx.fillStyle = palette.landSelected;
  ctx.fill(path, "evenodd");
  ctx.globalAlpha = 1;
  ctx.strokeStyle = palette.borderSelected;
  ctx.lineWidth = 1.2 / k;
  ctx.stroke(path);
  ctx.restore();
}

/**
 * Populates scratch lists of visible non-selected feature paths split into non-selectable (dim)
 * and selectable (data-bearing) buckets at the coarsest LOD that stays within error budget.
 */
function collectVisibleContextPaths(
  scene: MapScene,
  k: number,
  tx: number,
  ty: number,
  width: number,
  height: number,
  selectableFeatures: ReadonlySet<number> | null | undefined,
): void {
  SCRATCH_DIM_PATHS.length = 0;
  SCRATCH_SELECTABLE_PATHS.length = 0;
  const level = pickLodLevel(scene.lodTolerances, k);
  const minX = -tx / k;
  const minY = -ty / k;
  const maxX = (width - tx) / k;
  const maxY = (height - ty) / k;
  const { features } = scene;
  for (let i = 0; i < features.length; i++) {
    const f = features[i];
    if (f.selected || f.maxX < minX || f.minX > maxX || f.maxY < minY || f.minY > maxY) continue;
    const path = (level >= 0 ? f.lodPaths[level] : null) ?? f.path;
    if (!path) continue;
    const isSelectable = selectableFeatures ? selectableFeatures.has(i) : Boolean(f.selectable);
    if (isSelectable) {
      SCRATCH_SELECTABLE_PATHS.push(path);
    } else {
      SCRATCH_DIM_PATHS.push(path);
    }
  }
}

function strokeContextAndSelectedBorders(
  ctx: CanvasRenderingContext2D,
  scene: MapScene,
  dpr: number,
  k: number,
  tx: number,
  ty: number,
  palette: MapPalette,
): void {
  ctx.save();
  ctx.setTransform(dpr * k, 0, 0, dpr * k, dpr * tx, dpr * ty);
  ctx.lineJoin = "round";
  ctx.lineCap = "round";

  const hasContextPaths = SCRATCH_DIM_PATHS.length > 0 || SCRATCH_SELECTABLE_PATHS.length > 0;
  if (hasContextPaths) {
    ctx.strokeStyle = palette.border;
    ctx.lineWidth = 0.85 / k;
    for (let i = 0; i < SCRATCH_DIM_PATHS.length; i++) ctx.stroke(SCRATCH_DIM_PATHS[i]);
    for (let i = 0; i < SCRATCH_SELECTABLE_PATHS.length; i++) ctx.stroke(SCRATCH_SELECTABLE_PATHS[i]);
  } else if (typeof Path2D === "undefined" && scene.contextRings.length > 0) {
    strokeRingsFallback(ctx, scene.contextRings, palette.border, 0.85 / k);
  }

  if (scene.selectedPath) {
    // Subtle dark/light casing beneath the primary border keeps coastlines legible over dense coastal hotspots.
    ctx.globalAlpha = 0.55;
    ctx.strokeStyle = palette.haloLight;
    ctx.lineWidth = 2.4 / k;
    ctx.stroke(scene.selectedPath);
    ctx.globalAlpha = 1;
    ctx.strokeStyle = palette.borderSelected;
    ctx.lineWidth = 1.4 / k;
    ctx.stroke(scene.selectedPath);
  } else if (scene.selectedRings.length > 0) {
    strokeRingsFallback(ctx, scene.selectedRings, palette.borderSelected, 1.4 / k);
  }

  ctx.restore();
}

export function drawMap(
  ctx: CanvasRenderingContext2D,
  scene: MapScene,
  view: DrawMapView,
  theme?: MapPalette | Theme,
): DrawStats {
  const isHarmonized = view.harmonized ?? view.mode === "harmonized";
  const layer = isHarmonized ? scene.harmonized : scene.raw;
  const layerTotal = layer.count;
  const sensorFilter = view.sensorFilter;
  const filterAll = !sensorFilter || (sensorFilter.modis && sensorFilter.viirs);
  const filterNone = Boolean(sensorFilter && !sensorFilter.modis && !sensorFilter.viirs);

  const dpr = clamp(
    view.dpr ?? (typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1),
    1,
    3,
  );
  const width = view.width ?? ctx.canvas.clientWidth;
  const height = view.height ?? ctx.canvas.clientHeight;

  if (width <= 0 || height <= 0 || !Number.isFinite(width) || !Number.isFinite(height)) {
    return { drawn: 0, inView: 0, total: filterNone ? 0 : layerTotal };
  }

  const targetW = Math.round(width * dpr);
  const targetH = Math.round(height * dpr);
  if (ctx.canvas.width !== targetW) ctx.canvas.width = targetW;
  if (ctx.canvas.height !== targetH) ctx.canvas.height = targetH;

  const k = view.transform ? view.transform.k : (view.k ?? 1);
  const tx = view.transform ? view.transform.tx : (view.tx ?? 0);
  const ty = view.transform ? view.transform.ty : (view.ty ?? 0);
  const hasValidTransform = k > 0 && Number.isFinite(k) && Number.isFinite(tx) && Number.isFinite(ty);

  const palette = isPaletteObject(theme) ? theme : readPalette(ctx.canvas, theme);

  ctx.save();
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  ctx.beginPath();
  ctx.rect(0, 0, width, height);
  ctx.clip();

  if (hasValidTransform) {
    ctx.save();
    ctx.setTransform(dpr * k, 0, 0, dpr * k, dpr * tx, dpr * ty);
    ctx.lineJoin = "round";
    ctx.lineCap = "round";

    collectVisibleContextPaths(scene, k, tx, ty, width, height, view.selectableFeatures);
    if (SCRATCH_DIM_PATHS.length > 0) {
      ctx.fillStyle = palette.landDim;
      for (let i = 0; i < SCRATCH_DIM_PATHS.length; i++) ctx.fill(SCRATCH_DIM_PATHS[i]);
    }
    if (SCRATCH_SELECTABLE_PATHS.length > 0) {
      ctx.fillStyle = palette.land;
      for (let i = 0; i < SCRATCH_SELECTABLE_PATHS.length; i++) ctx.fill(SCRATCH_SELECTABLE_PATHS[i]);
      ctx.globalAlpha = 0.25;
      ctx.fillStyle = palette.landSelected;
      for (let i = 0; i < SCRATCH_SELECTABLE_PATHS.length; i++) ctx.fill(SCRATCH_SELECTABLE_PATHS[i]);
      ctx.globalAlpha = 1;
    }
    if (
      SCRATCH_DIM_PATHS.length === 0 &&
      SCRATCH_SELECTABLE_PATHS.length === 0 &&
      typeof Path2D === "undefined" &&
      scene.contextRings.length > 0
    ) {
      fillRingsFallback(ctx, scene.contextRings, palette.landDim);
    }

    if (scene.selectedPath) {
      ctx.fillStyle = palette.landSelected;
      ctx.fill(scene.selectedPath);
    } else if (scene.selectedRings.length > 0) {
      fillRingsFallback(ctx, scene.selectedRings, palette.landSelected);
    }

    ctx.restore();
  }

  if (layerTotal === 0 || !hasValidTransform || filterNone) {
    if (hasValidTransform) {
      strokeContextAndSelectedBorders(ctx, scene, dpr, k, tx, ty, palette);
    }
    SCRATCH_DIM_PATHS.length = 0;
    SCRATCH_SELECTABLE_PATHS.length = 0;
    ctx.restore();
    return { drawn: 0, inView: 0, total: 0 };
  }

  const spanX = Math.max(scene.bounds.maxX - scene.bounds.minX, 1e-6);
  const spanY = Math.max(scene.bounds.maxY - scene.bounds.minY, 1e-6);
  const refFitK = Math.min(width / spanX, height / spanY);
  const zoomRatio = refFitK > 0 ? Math.max(0.4, k / refFitK) : 1;
  // Smaller markers at country-level zoom (zoomRatio ~ 1) that grow smoothly as the user zooms in.
  const zoomScale = clamp(0.62 * Math.pow(zoomRatio, 0.38), 0.48, 2.1);
  const rawZoomT = clamp(Math.log2(Math.max(1, zoomRatio)) / 4, 0, 1);

  const minPlaneX = (-VIEWPORT_MARGIN_PX - tx) / k;
  const maxPlaneX = (width + VIEWPORT_MARGIN_PX - tx) / k;
  const minPlaneY = (-VIEWPORT_MARGIN_PX - ty) / k;
  const maxPlaneY = (height + VIEWPORT_MARGIN_PX - ty) / k;

  const { xs, ys, buckets, shapes, flags } = layer;
  let visibleTotal = 0;
  let inView = 0;
  for (let i = 0; i < layerTotal; i++) {
    if (!filterAll && !pointMatchesSensorFilter(flags[i], shapes[i], sensorFilter)) {
      continue;
    }
    visibleTotal++;
    const px = xs[i];
    const py = ys[i];
    if (px >= minPlaneX && px <= maxPlaneX && py >= minPlaneY && py <= maxPlaneY) {
      inView++;
    }
  }

  const maxDrawn = Math.max(1, view.maxDrawn ?? DEFAULT_MAX_DRAWN);
  const needsStride = inView > maxDrawn;
  const hasPath2D = typeof Path2D !== "undefined";
  const showDupRings = view.showDuplicateRings === true;

  let drawn = 0;
  let visIdx = 0;

  if (inView > 0) {
    if (!isHarmonized) {
      const rModis = pointRadiusFor(1, PointShape.HollowCircle, false, zoomScale);
      const rViirs = pointRadiusFor(1, PointShape.Square, false, zoomScale);
      const sideViirs = rViirs * 2;
      const rDup = clamp(rModis + 2.4, 3.6, 8);
      const viirsAlpha = 0.28 + 0.5 * rawZoomT;
      const modisAlpha = 0.36 + 0.48 * rawZoomT;
      const dupPath = showDupRings && hasPath2D ? new Path2D() : null;
      let hasDup = false;

      ctx.fillStyle = palette.viirs;
      ctx.strokeStyle = palette.modis;
      ctx.lineWidth = clamp(0.95 * zoomScale, 0.85, 1.5);

      // Draw each Raw marker individually with low alpha so overlapping detections accumulate density.
      for (let i = 0; i < layerTotal; i++) {
        if (!filterAll && !pointMatchesSensorFilter(flags[i], shapes[i], sensorFilter)) {
          continue;
        }
        const px = xs[i];
        const py = ys[i];
        if (px < minPlaneX || px > maxPlaneX || py < minPlaneY || py > maxPlaneY) {
          continue;
        }
        const curVis = visIdx++;
        if (needsStride) {
          const prevSlot = Math.floor((curVis * maxDrawn) / inView);
          const nextSlot = Math.floor(((curVis + 1) * maxDrawn) / inView);
          if (nextSlot === prevSlot) continue;
        }
        drawn++;

        const sx = px * k + tx;
        const sy = py * k + ty;

        if (showDupRings && (flags[i] & PointFlag.Duplicate) !== 0) {
          hasDup = true;
          if (dupPath) {
            dupPath.moveTo(sx + rDup, sy);
            dupPath.arc(sx, sy, rDup, 0, TAU);
          }
        }

        if (shapes[i] === PointShape.HollowCircle) {
          ctx.globalAlpha = modisAlpha;
          ctx.beginPath();
          ctx.arc(sx, sy, rModis, 0, TAU);
          ctx.stroke();
        } else {
          ctx.globalAlpha = viirsAlpha;
          ctx.fillRect(sx - rViirs, sy - rViirs, sideViirs, sideViirs);
        }
      }

      ctx.globalAlpha = 1;
      if (hasDup && dupPath) {
        ctx.save();
        ctx.globalAlpha = 0.7;
        ctx.setLineDash(DUP_LINE_DASH);
        ctx.strokeStyle = palette.duplicateRing;
        ctx.lineWidth = 1.05;
        ctx.stroke(dupPath);
        ctx.restore();
      }
    } else {
      for (let bIdx = 0; bIdx < 8; bIdx++) {
        SCRATCH_PATHS[bIdx] = null;
        SCRATCH_RADII[bIdx] = pointRadiusFor(bIdx + 1, PointShape.Circle, true, zoomScale);
      }
      const dupPath = showDupRings && hasPath2D ? new Path2D() : null;
      let hasDup = false;

      for (let i = 0; i < layerTotal; i++) {
        if (!filterAll && !pointMatchesSensorFilter(flags[i], shapes[i], sensorFilter)) {
          continue;
        }
        const px = xs[i];
        const py = ys[i];
        if (px < minPlaneX || px > maxPlaneX || py < minPlaneY || py > maxPlaneY) {
          continue;
        }
        const curVis = visIdx++;
        if (needsStride) {
          const prevSlot = Math.floor((curVis * maxDrawn) / inView);
          const nextSlot = Math.floor(((curVis + 1) * maxDrawn) / inView);
          if (nextSlot === prevSlot) continue;
        }
        drawn++;

        const sx = px * k + tx;
        const sy = py * k + ty;
        const bIdx = clamp((buckets[i] || 1) - 1, 0, 7);
        const r = SCRATCH_RADII[bIdx];

        if (hasPath2D) {
          let path = SCRATCH_PATHS[bIdx];
          if (!path) {
            path = new Path2D();
            SCRATCH_PATHS[bIdx] = path;
          }
          if (shapes[i] === PointShape.Square) {
            path.rect(sx - r, sy - r, r * 2, r * 2);
          } else {
            path.moveTo(sx + r, sy);
            path.arc(sx, sy, r, 0, TAU);
          }
        }

        if (showDupRings && (flags[i] & PointFlag.Duplicate) !== 0) {
          hasDup = true;
          const rDup = clamp(r + 2.4, 3.6, 8);
          if (dupPath) {
            dupPath.moveTo(sx + rDup, sy);
            dupPath.arc(sx, sy, rDup, 0, TAU);
          }
        }
      }

      ctx.strokeStyle = palette.haloLight;
      ctx.lineWidth = clamp(0.6 * zoomScale, 0.45, 0.85);
      for (let bIdx = 0; bIdx < 8; bIdx++) {
        const path = SCRATCH_PATHS[bIdx];
        SCRATCH_PATHS[bIdx] = null;
        if (!path) continue;
        ctx.fillStyle = palette.frp[bIdx];
        ctx.fill(path);
        ctx.stroke(path);
      }

      if (hasDup && dupPath) {
        ctx.save();
        ctx.globalAlpha = 0.7;
        ctx.setLineDash(DUP_LINE_DASH);
        ctx.strokeStyle = palette.duplicateRing;
        ctx.lineWidth = 1.05;
        ctx.stroke(dupPath);
        ctx.restore();
      }
    }
  }

  // Draw coastlines and country borders ABOVE the point layer so geography stays legible.
  strokeContextAndSelectedBorders(ctx, scene, dpr, k, tx, ty, palette);
  SCRATCH_DIM_PATHS.length = 0;
  SCRATCH_SELECTABLE_PATHS.length = 0;

  const resolvedHoveredIdx =
    view.hoveredIndex !== undefined && view.hoveredIndex !== null
      ? view.hoveredIndex
      : view.hoveredId
        ? (layer.indexById.get(view.hoveredId) ?? -1)
        : -1;

  const resolvedSelectedIdx =
    view.selectedIndex !== undefined && view.selectedIndex !== null
      ? view.selectedIndex
      : view.selectedId
        ? (layer.indexById.get(view.selectedId) ?? -1)
        : -1;

  if (
    resolvedHoveredIdx >= 0 &&
    resolvedHoveredIdx < layerTotal &&
    resolvedHoveredIdx !== resolvedSelectedIdx &&
    (filterAll || pointMatchesSensorFilter(flags[resolvedHoveredIdx], shapes[resolvedHoveredIdx], sensorFilter))
  ) {
    drawHighlightedPoint(
      ctx,
      layer,
      resolvedHoveredIdx,
      k,
      tx,
      ty,
      isHarmonized,
      zoomScale,
      palette,
      false,
    );
  }

  if (
    resolvedSelectedIdx >= 0 &&
    resolvedSelectedIdx < layerTotal &&
    (filterAll || pointMatchesSensorFilter(flags[resolvedSelectedIdx], shapes[resolvedSelectedIdx], sensorFilter))
  ) {
    drawHighlightedPoint(
      ctx,
      layer,
      resolvedSelectedIdx,
      k,
      tx,
      ty,
      isHarmonized,
      zoomScale,
      palette,
      true,
    );
  }

  ctx.restore();
  return { drawn, inView, total: visibleTotal };
}
