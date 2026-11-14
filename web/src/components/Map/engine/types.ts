import type { PlaneBounds } from "../../../lib/geo/projection";
import type { HarmonizedPoint, RawPoint, Theme, ViewMode } from "../../../types";

export type { PlaneBounds, Theme, ViewMode };

export interface ViewTransform {
  k: number;
  tx: number;
  ty: number;
}

export interface ViewportSize {
  width: number;
  height: number;
}

export const PointShape = {
  Circle: 0,
  HollowCircle: 1,
  Square: 2,
} as const;

export type PointShape = (typeof PointShape)[keyof typeof PointShape];

export const PointFlag = {
  None: 0,
  Duplicate: 1,
  Removed: 2,
  Matched: 4,
} as const;

export type PointFlag = (typeof PointFlag)[keyof typeof PointFlag];

export interface PointSpatialIndex {
  readonly count: number;
  readonly xs: Float32Array;
  readonly ys: Float32Array;
  readonly minX: number;
  readonly minY: number;
  readonly maxX: number;
  readonly maxY: number;
  readonly cellSize: number;
  readonly cols: number;
  readonly rows: number;
  readonly heads: Int32Array;
  readonly next: Int32Array;
}

export interface ScenePointLayer {
  readonly count: number;
  readonly xs: Float32Array;
  readonly ys: Float32Array;
  readonly frp: Float32Array;
  readonly buckets: Uint8Array;
  readonly shapes: Uint8Array;
  readonly flags: Uint8Array;
  readonly ids: readonly string[];
  readonly indexById: ReadonlyMap<string, number>;
  readonly spatialIndex: PointSpatialIndex;
}

export interface ScenePointInput {
  readonly raw: readonly RawPoint[];
  readonly harmonized: readonly HarmonizedPoint[];
}

/** One boundary feature projected to the plane; rings are shared with the context/selected lists. */
export interface SceneFeature {
  readonly rings: readonly Float32Array[];
  readonly minX: number;
  readonly minY: number;
  readonly maxX: number;
  readonly maxY: number;
  readonly path: Path2D | null;
}

export interface SceneGeometry {
  readonly features: readonly SceneFeature[];
  readonly contextRings: readonly Float32Array[];
  readonly selectedRings: readonly Float32Array[];
  readonly contextPath: Path2D | null;
  readonly selectedPath: Path2D | null;
  readonly selectedBounds: PlaneBounds | null;
  readonly allBounds: PlaneBounds | null;
}

export interface ScenePoints {
  readonly raw: ScenePointLayer;
  readonly harmonized: ScenePointLayer;
}

export interface MapScene {
  readonly regionId: string;
  readonly bounds: PlaneBounds;
  /** Index-aligned with the geo feature list passed to buildScene. */
  readonly features: readonly SceneFeature[];
  readonly contextRings: readonly Float32Array[];
  readonly selectedRings: readonly Float32Array[];
  readonly contextPath: Path2D | null;
  readonly selectedPath: Path2D | null;
  readonly raw: ScenePointLayer;
  readonly harmonized: ScenePointLayer;
}

export interface MapPalette {
  readonly water: string;
  readonly land: string;
  readonly landSelected: string;
  readonly landDim: string;
  readonly border: string;
  readonly borderSelected: string;
  readonly ink: string;
  readonly haloLight: string;
  readonly modis: string;
  readonly viirs: string;
  readonly duplicateRing: string;
  readonly focus: string;
  readonly frp1: string;
  readonly frp2: string;
  readonly frp3: string;
  readonly frp4: string;
  readonly frp5: string;
  readonly frp6: string;
  readonly frp7: string;
  readonly frp8: string;
  readonly frp: readonly [string, string, string, string, string, string, string, string];
}

export interface DrawMapView {
  readonly transform?: ViewTransform;
  readonly k?: number;
  readonly tx?: number;
  readonly ty?: number;
  readonly width?: number;
  readonly height?: number;
  readonly dpr?: number;
  readonly harmonized?: boolean;
  readonly mode?: ViewMode;
  readonly showDuplicateRings?: boolean;
  readonly hoveredIndex?: number | null;
  readonly selectedIndex?: number | null;
  readonly hoveredId?: string | null;
  readonly selectedId?: string | null;
  readonly maxDrawn?: number;
}

export interface DrawStats {
  drawn: number;
  inView: number;
  total: number;
}
