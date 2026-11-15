import * as THREE from "three";
import type { GeoFeature, Position } from "../../../lib/geo/types";

const DEG = Math.PI / 180;
const PATCH_MAX_PX = 2048;
const PATCH_MIN_PX = 64;
/** The mask only shades imagery, so a coarse raster with linear filtering is enough. */
const MASK_MAX_PX = 512;
const MASK_FEATHER_DEG = 2.5;

function canvasTexture(
  width: number,
  height: number,
  anisotropy: number,
  paint: (context: CanvasRenderingContext2D) => void,
): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Globe: 2D canvas is unavailable");
  paint(context);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = anisotropy;
  texture.needsUpdate = true;
  return texture;
}

function ringLonRange(ring: readonly Position[]): [min: number, max: number] {
  let min = Infinity;
  let max = -Infinity;
  for (const [lon] of ring) {
    if (lon < min) min = lon;
    if (lon > max) max = lon;
  }
  return [min, max];
}

function traceRing(
  path: Path2D,
  ring: readonly Position[],
  toX: (lon: number) => number,
  toY: (lat: number) => number,
): void {
  if (ring.length < 2) return;
  path.moveTo(toX(ring[0][0]), toY(ring[0][1]));
  for (let i = 1; i < ring.length; i++) path.lineTo(toX(ring[i][0]), toY(ring[i][1]));
  path.closePath();
}

export interface BorderStyle {
  /** Soft halo under the line (from --map-border). */
  readonly halo: string;
  /** Crisp line on top (from --map-border-selected). */
  readonly line: string;
}

/**
 * Equirectangular overlay with every country outline, for a sphere shell just above the photo.
 * Rings are continuous and may run past ±180, so each ring is also drawn one world-width over.
 */
export function makeBordersTexture(
  features: readonly GeoFeature[],
  style: BorderStyle,
  maxTextureSize: number,
  anisotropy: number,
): THREE.CanvasTexture {
  const width = Math.min(4096, maxTextureSize);
  const height = width / 2;
  const path = new Path2D();
  for (const feature of features) {
    for (const polygon of feature.polygons) {
      for (const ring of polygon) {
        const [min, max] = ringLonRange(ring);
        for (const shift of [0, -360, 360]) {
          if (max + shift < -180 || min + shift > 180) continue;
          traceRing(
            path,
            ring,
            (lon) => ((lon + shift + 180) / 360) * width,
            (lat) => ((90 - lat) / 180) * height,
          );
        }
      }
    }
  }
  const scale = width / 4096;
  return canvasTexture(width, height, anisotropy, (context) => {
    context.lineJoin = "round";
    context.lineCap = "round";
    context.globalAlpha = 0.16;
    context.strokeStyle = style.halo;
    context.lineWidth = 3 * scale;
    context.stroke(path);
    context.globalAlpha = 0.4;
    context.shadowColor = style.line;
    context.shadowBlur = 3 * scale;
    context.strokeStyle = style.line;
    context.lineWidth = 1.3 * scale;
    context.stroke(path);
  });
}

export interface PatchStyle {
  readonly color: string;
  readonly fillAlpha: number;
  /** Outline width as a fraction of the longer texture side. */
  readonly lineFraction: number;
  readonly glow: boolean;
}

export interface SurfacePatch {
  readonly geometry: THREE.SphereGeometry;
  readonly texture: THREE.CanvasTexture;
}

/** Lon/lat window of a patch: lon may run past ±180 so the window stays continuous. */
export interface PatchBounds {
  readonly minLon: number;
  readonly minLat: number;
  readonly lonSpan: number;
  readonly latSpan: number;
}

interface PatchLayout extends PatchBounds {
  readonly path: Path2D;
  readonly width: number;
  readonly height: number;
}

function layoutPatch(features: readonly GeoFeature[], maxPx: number): PatchLayout | null {
  if (features.length === 0) return null;
  let west = Infinity;
  let east = -Infinity;
  for (const f of features) {
    if (f.bbox[0] < west) west = f.bbox[0];
    if (f.bbox[2] > east) east = f.bbox[2];
  }
  const centerLon = (west + east) / 2;

  // Shift each ring by whole turns so it sits within ±180° of the shared centre.
  const rings: { ring: readonly Position[]; shift: number }[] = [];
  let minLon = Infinity;
  let maxLon = -Infinity;
  let minLat = Infinity;
  let maxLat = -Infinity;
  for (const f of features) {
    for (const polygon of f.polygons) {
      for (const ring of polygon) {
        if (ring.length < 3) continue;
        const [lo, hi] = ringLonRange(ring);
        const shift = Math.round((centerLon - (lo + hi) / 2) / 360) * 360;
        rings.push({ ring, shift });
        if (lo + shift < minLon) minLon = lo + shift;
        if (hi + shift > maxLon) maxLon = hi + shift;
        for (const [, lat] of ring) {
          if (lat < minLat) minLat = lat;
          if (lat > maxLat) maxLat = lat;
        }
      }
    }
  }
  if (rings.length === 0) return null;

  const pad = Math.max(0.15, 0.03 * Math.max(maxLon - minLon, maxLat - minLat));
  minLon -= pad;
  maxLon += pad;
  minLat = Math.max(-89.5, minLat - pad);
  maxLat = Math.min(89.5, maxLat + pad);
  const lonSpan = Math.min(360, maxLon - minLon);
  const latSpan = maxLat - minLat;

  // Roughly square texels on the sphere: longitude shrinks with the cosine of the middle latitude.
  const lonExtent = lonSpan * Math.max(0.2, Math.cos(((minLat + maxLat) / 2) * DEG));
  const ppd = maxPx / Math.max(lonExtent, latSpan);
  const width = Math.max(PATCH_MIN_PX, Math.min(maxPx, Math.round(lonExtent * ppd)));
  const height = Math.max(PATCH_MIN_PX, Math.min(maxPx, Math.round(latSpan * ppd)));

  const path = new Path2D();
  for (const { ring, shift } of rings) {
    traceRing(
      path,
      ring,
      (lon) => ((lon + shift - minLon) / lonSpan) * width,
      (lat) => ((maxLat - lat) / latSpan) * height,
    );
  }
  return { path, width, height, minLon, minLat, lonSpan, latSpan };
}

/**
 * A sphere patch covering just the features' extent, painted with a translucent fill and a glowing
 * outline. Patches keep small countries crisp where a whole-world texture would blur them.
 */
export function makeSurfacePatch(
  features: readonly GeoFeature[],
  style: PatchStyle,
  radius: number,
  anisotropy: number,
): SurfacePatch | null {
  const layout = layoutPatch(features, PATCH_MAX_PX);
  if (!layout) return null;
  const { path, width, height, minLon, minLat, lonSpan, latSpan } = layout;
  const maxLat = minLat + latSpan;

  const line = Math.max(1.5, Math.max(width, height) * style.lineFraction);
  const texture = canvasTexture(width, height, anisotropy, (context) => {
    context.lineJoin = "round";
    context.globalAlpha = style.fillAlpha;
    context.fillStyle = style.color;
    context.fill(path, "evenodd");
    context.globalAlpha = 1;
    context.strokeStyle = style.color;
    if (style.glow) {
      context.shadowColor = style.color;
      context.shadowBlur = line * 1.6;
    }
    context.lineWidth = line;
    context.stroke(path);
  });

  const widthSegments = Math.min(128, Math.max(8, Math.ceil(lonSpan / 1.5)));
  const heightSegments = Math.min(96, Math.max(6, Math.ceil(latSpan / 1.5)));
  const geometry = new THREE.SphereGeometry(
    radius,
    widthSegments,
    heightSegments,
    (minLon + 180) * DEG,
    lonSpan * DEG,
    (90 - maxLat) * DEG,
    latSpan * DEG,
  );
  return { geometry, texture };
}

export interface SelectionMask {
  readonly texture: THREE.CanvasTexture;
  readonly bounds: PatchBounds;
}

/** Coverage of the features (1 inside, 0 outside) over their lon/lat window, for imagery shaders. */
export function makeSelectionMask(features: readonly GeoFeature[]): SelectionMask | null {
  const layout = layoutPatch(features, MASK_MAX_PX);
  if (!layout) return null;
  const { path, width, height, minLon, minLat, lonSpan, latSpan } = layout;
  const texture = canvasTexture(width, height, 1, (context) => {
    context.fillStyle = "#000";
    context.fillRect(0, 0, width, height);
    context.fillStyle = "#fff";
    // A few degrees of blur is the feather between the selected region and the dimmed rest.
    context.filter = `blur(${((width / lonSpan) * MASK_FEATHER_DEG).toFixed(1)}px)`;
    context.fill(path, "evenodd");
  });
  texture.colorSpace = THREE.NoColorSpace;
  // Low-frequency coverage: bilinear without mips is enough and keeps the per-pixel fetch cheap.
  texture.generateMipmaps = false;
  texture.minFilter = THREE.LinearFilter;
  return { texture, bounds: { minLon, minLat, lonSpan, latSpan } };
}

export function loadEarthPhoto(
  url: string,
  anisotropy: number,
  onLoad: (texture: THREE.Texture) => void,
): THREE.Texture {
  return new THREE.TextureLoader().load(url, (loaded) => {
    loaded.colorSpace = THREE.SRGBColorSpace;
    loaded.generateMipmaps = true;
    loaded.minFilter = THREE.LinearMipmapLinearFilter;
    loaded.magFilter = THREE.LinearFilter;
    loaded.anisotropy = anisotropy;
    onLoad(loaded);
  });
}
