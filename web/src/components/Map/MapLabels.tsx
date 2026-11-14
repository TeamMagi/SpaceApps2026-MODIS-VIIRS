import { memo, useLayoutEffect, useMemo, useRef } from "react";
import { formatRegionName } from "../../content/copy";
import { MAP_CONTEXT_NAMES_TR } from "../../content/copyMap";
import type { RegionProjection } from "../../lib/geo/projection";
import type { GeoDataset, Position } from "../../lib/geo/types";
import type { Lang } from "../../types";
import type { ViewportSize, ViewTransform } from "./engine/types";
import { planeToScreen } from "./engine/viewport";
import styles from "./MapLabels.module.css";

export interface MapLabelsProps {
  readonly dataset: GeoDataset | null;
  readonly regionId: string;
  readonly selectedDisplayName: string;
  readonly projection: RegionProjection | null;
  readonly transform: ViewTransform;
  readonly size: ViewportSize;
  readonly lang: Lang;
}

interface CountryAnchor {
  readonly key: string;
  readonly code: string;
  readonly isSelected: boolean;
  readonly regionId: string | null;
  readonly rawName: string;
  readonly anchorX: number;
  readonly anchorY: number;
  readonly minX: number;
  readonly minY: number;
  readonly maxX: number;
  readonly maxY: number;
  readonly area: number;
}

interface VisibleLabel {
  readonly key: string;
  readonly name: string;
  readonly x: number;
  readonly y: number;
  readonly isSelected: boolean;
}

interface ScreenRect {
  readonly x0: number;
  readonly y0: number;
  readonly x1: number;
  readonly y1: number;
}

function computeRingInteriorPoint(
  ring: readonly Position[],
  projection: RegionProjection,
): {
  anchorX: number;
  anchorY: number;
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
  area: number;
} {
  const n = ring.length;
  const xs = new Float32Array(n);
  const ys = new Float32Array(n);
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (let i = 0; i < n; i++) {
    const [px, py] = projection.project(ring[i][0], ring[i][1]);
    xs[i] = px;
    ys[i] = py;
    if (px < minX) minX = px;
    if (py < minY) minY = py;
    if (px > maxX) maxX = px;
    if (py > maxY) maxY = py;
  }

  const cy = (minY + maxY) / 2;
  const intersections: number[] = [];
  for (let i = 0, j = n - 1; i < n; j = i++) {
    const y0 = ys[j];
    const y1 = ys[i];
    if ((y0 <= cy && y1 > cy) || (y1 <= cy && y0 > cy)) {
      const t = (cy - y0) / (y1 - y0);
      intersections.push(xs[j] + t * (xs[i] - xs[j]));
    }
  }

  let anchorX = (minX + maxX) / 2;
  if (intersections.length >= 2) {
    intersections.sort((a, b) => a - b);
    let bestSpan = -1;
    for (let k = 0; k + 1 < intersections.length; k += 2) {
      const span = intersections[k + 1] - intersections[k];
      if (span > bestSpan) {
        bestSpan = span;
        anchorX = (intersections[k] + intersections[k + 1]) / 2;
      }
    }
  }

  const area = Math.max(0, maxX - minX) * Math.max(0, maxY - minY);
  return { anchorX, anchorY: cy, minX, minY, maxX, maxY, area };
}

function rectsOverlap(a: ScreenRect, b: ScreenRect): boolean {
  return a.x0 < b.x1 && a.x1 > b.x0 && a.y0 < b.y1 && a.y1 > b.y0;
}

function MapLabels({
  dataset,
  regionId,
  selectedDisplayName,
  projection,
  transform,
  size,
  lang,
}: MapLabelsProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);

  const countryAnchors = useMemo<readonly CountryAnchor[]>(() => {
    if (!dataset || !projection) return [];

    const byKey = new Map<
      string,
      {
        key: string;
        code: string;
        isSelected: boolean;
        regionId: string | null;
        rawName: string;
        anchorX: number;
        anchorY: number;
        minX: number;
        minY: number;
        maxX: number;
        maxY: number;
        bestRingArea: number;
      }
    >();

    for (const feature of dataset.features) {
      const key = feature.regionId ?? feature.code;
      const isSelected = feature.regionId === regionId;

      for (const poly of feature.polygons) {
        const outerRing = poly[0];
        if (!outerRing || outerRing.length < 3) continue;
        const info = computeRingInteriorPoint(outerRing, projection);
        if (!Number.isFinite(info.minX) || !Number.isFinite(info.maxX)) continue;

        const existing = byKey.get(key);
        if (!existing) {
          byKey.set(key, {
            key,
            code: feature.code,
            isSelected,
            regionId: feature.regionId,
            rawName: feature.name,
            anchorX: info.anchorX,
            anchorY: info.anchorY,
            minX: info.minX,
            minY: info.minY,
            maxX: info.maxX,
            maxY: info.maxY,
            bestRingArea: info.area,
          });
        } else {
          if (info.minX < existing.minX) existing.minX = info.minX;
          if (info.minY < existing.minY) existing.minY = info.minY;
          if (info.maxX > existing.maxX) existing.maxX = info.maxX;
          if (info.maxY > existing.maxY) existing.maxY = info.maxY;
          if (info.area > existing.bestRingArea) {
            existing.bestRingArea = info.area;
            existing.anchorX = info.anchorX;
            existing.anchorY = info.anchorY;
          }
        }
      }
    }

    const list: CountryAnchor[] = [];
    for (const item of byKey.values()) {
      list.push({
        key: item.key,
        code: item.code,
        isSelected: item.isSelected,
        regionId: item.regionId,
        rawName: item.rawName,
        anchorX: item.anchorX,
        anchorY: item.anchorY,
        minX: item.minX,
        minY: item.minY,
        maxX: item.maxX,
        maxY: item.maxY,
        area: Math.max(0, item.maxX - item.minX) * Math.max(0, item.maxY - item.minY),
      });
    }

    list.sort((a, b) => {
      if (a.isSelected !== b.isSelected) return a.isSelected ? -1 : 1;
      return b.area - a.area;
    });

    return list;
  }, [dataset, regionId, projection]);

  const visibleLabels = useMemo<readonly VisibleLabel[]>(() => {
    const { width, height } = size;
    if (
      width <= 0 ||
      height <= 0 ||
      countryAnchors.length === 0 ||
      !Number.isFinite(transform.k) ||
      transform.k <= 0
    ) {
      return [];
    }

    const placed: ScreenRect[] = [
      // Reserve top-right zoom controls area so labels never collide with buttons.
      { x0: Math.max(0, width - 176), y0: 0, x1: width, y1: 60 },
    ];
    const out: VisibleLabel[] = [];

    for (const c of countryAnchors) {
      const sMinX = c.minX * transform.k + transform.tx;
      const sMaxX = c.maxX * transform.k + transform.tx;
      const sMinY = c.minY * transform.k + transform.ty;
      const sMaxY = c.maxY * transform.k + transform.ty;

      if (sMaxX < 8 || sMinX > width - 8 || sMaxY < 8 || sMinY > height - 8) {
        continue;
      }

      const name = c.isSelected
        ? selectedDisplayName
        : c.regionId
          ? formatRegionName({ id: c.regionId, name: c.rawName }, lang)
          : lang === "tr"
            ? (MAP_CONTEXT_NAMES_TR[c.code] ?? c.rawName)
            : c.rawName;
      if (!name) continue;

      const labelW = c.isSelected
        ? Math.ceil(name.length * 7.8 + 18)
        : Math.ceil(name.length * 6.8 + 14);
      const labelH = c.isSelected ? 24 : 20;

      const spanW = sMaxX - sMinX;
      const spanH = sMaxY - sMinY;
      if (c.isSelected) {
        if (spanW < labelW * 0.35 || spanH < labelH * 0.5) continue;
      } else {
        if (spanW < labelW * 0.65 || spanH < labelH * 0.75) continue;
      }

      const [rawSx, rawSy] = planeToScreen(transform, c.anchorX, c.anchorY);
      let sx = rawSx;
      let sy = rawSy;

      if (c.isSelected) {
        const minValidX = Math.max(sMinX + labelW * 0.3, labelW / 2 + 8);
        const maxValidX = Math.min(sMaxX - labelW * 0.3, width - labelW / 2 - 8);
        const minValidY = Math.max(sMinY + labelH * 0.3, labelH / 2 + 8);
        const maxValidY = Math.min(sMaxY - labelH * 0.3, height - labelH / 2 - 8);
        if (minValidX <= maxValidX) {
          sx = Math.min(Math.max(sx, minValidX), maxValidX);
        }
        if (minValidY <= maxValidY) {
          sy = Math.min(Math.max(sy, minValidY), maxValidY);
        }
      }

      const x0 = sx - labelW / 2;
      const y0 = sy - labelH / 2;
      const x1 = sx + labelW / 2;
      const y1 = sy + labelH / 2;

      if (x0 < 6 || x1 > width - 6 || y0 < 6 || y1 > height - 6) {
        continue;
      }

      const candidateRect: ScreenRect = { x0, y0, x1, y1 };
      let collides = false;
      for (let i = 0; i < placed.length; i++) {
        if (rectsOverlap(candidateRect, placed[i])) {
          collides = true;
          break;
        }
      }
      if (collides) continue;

      placed.push({ x0: x0 - 4, y0: y0 - 4, x1: x1 + 4, y1: y1 + 4 });
      out.push({
        key: c.key,
        name,
        x: Math.round(sx),
        y: Math.round(sy),
        isSelected: c.isSelected,
      });
    }

    return out;
  }, [countryAnchors, transform, size, lang, selectedDisplayName]);

  useLayoutEffect(() => {
    const root = containerRef.current;
    if (!root) return;
    const byKey = new Map<string, VisibleLabel>();
    for (const item of visibleLabels) {
      byKey.set(item.key, item);
    }
    const nodes = root.querySelectorAll<HTMLElement>("[data-label-key]");
    nodes.forEach((node) => {
      const k = node.dataset.labelKey;
      if (!k) return;
      const item = byKey.get(k);
      if (!item) return;
      node.style.setProperty("--x", `${item.x}px`);
      node.style.setProperty("--y", `${item.y}px`);
    });
  }, [visibleLabels]);

  if (visibleLabels.length === 0) return null;

  return (
    <div ref={containerRef} className={styles.labelsLayer} aria-hidden="true">
      {visibleLabels.map((label) => (
        <span
          key={label.key}
          data-label-key={label.key}
          data-selected={label.isSelected}
          className={label.isSelected ? styles.labelSelected : styles.labelNeighbour}
        >
          {label.name}
        </span>
      ))}
    </div>
  );
}

export default memo(MapLabels);
