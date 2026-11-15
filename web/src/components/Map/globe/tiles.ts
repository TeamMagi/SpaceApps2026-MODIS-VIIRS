import * as THREE from "three";
import { lonLatFromUv } from "./dataFocus";
import type { Vec3 } from "./geoMath";
import { tileKey, tileSphereParams, tilesInCap, type TileId } from "./tileMath";

/** Just above the base photo, below the borders shell (1.002) once its chords sag. */
const TILE_RADIUS = 1.0005;
const TILE_SEGMENTS = 16;
/** Decoded tiles kept on the GPU (1024² RGBA + mips ≈ 5.6 MB each). */
const MAX_CACHED = 24;
/** At most this many tiles are shown at once; the rest of the view keeps the base photo. */
const MAX_VISIBLE = 18;
const MAX_IN_FLIGHT = 4;
const FADE_MS = 220;
/** Extra cap margin so tiles are already loading as they rotate into view. */
const CAP_MARGIN = 3 * (Math.PI / 180);

interface Tile {
  readonly key: string;
  readonly mesh: THREE.Mesh<THREE.SphereGeometry, THREE.MeshStandardMaterial>;
  readonly texture: THREE.Texture;
  uploaded: boolean;
  opacity: number;
  lastUsed: number;
}

/**
 * High-resolution imagery as sphere patches over the base photo, loaded only for the part of the
 * globe in view once the camera is close enough that the base photo would be magnified.
 */
export class TileLayer {
  readonly group = new THREE.Group();
  private readonly loader = new THREE.ImageBitmapLoader();
  private readonly tiles = new Map<string, Tile>();
  private readonly inFlight = new Set<string>();
  private readonly failed = new Set<string>();
  private wanted: TileId[] = [];
  private wantedKeys = new Set<string>();
  private lastStep = 0;
  private disposed = false;

  constructor(
    private readonly urlFor: (tile: TileId) => string,
    private readonly anisotropy: number,
    private readonly reducedMotion: boolean,
    private readonly onChange: () => void,
    /** Applies the same imagery treatment as the base photo to each tile material. */
    private readonly patchMaterial: (material: THREE.MeshStandardMaterial, uvToLonLat: THREE.Vector4) => void,
  ) {
    this.group.name = "earth-tiles";
    this.loader.setOptions({ imageOrientation: "flipY", premultiplyAlpha: "none" });
  }

  /** Sets which part of the globe needs detail; `null` when the base photo is sharp enough. */
  setView(direction: Vec3 | null, capRadius: number): void {
    this.wanted = direction ? tilesInCap(direction, capRadius + CAP_MARGIN).slice(0, MAX_VISIBLE) : [];
    const keys = new Set(this.wanted.map(tileKey));
    let changed = keys.size !== this.wantedKeys.size;
    if (!changed) for (const key of keys) if (!this.wantedKeys.has(key)) changed = true;
    if (!changed) return;
    this.wantedKeys = keys;
    this.request();
    this.onChange();
  }

  private request(): void {
    for (const tile of this.wanted) {
      if (this.inFlight.size >= MAX_IN_FLIGHT) return;
      const key = tileKey(tile);
      if (this.tiles.has(key) || this.inFlight.has(key) || this.failed.has(key)) continue;
      this.load(tile, key);
    }
  }

  private load(id: TileId, key: string): void {
    this.inFlight.add(key);
    this.loader.load(
      this.urlFor(id),
      (bitmap) => {
        this.inFlight.delete(key);
        if (this.disposed) {
          bitmap.close();
          return;
        }
        this.add(id, key, bitmap);
        this.request();
        this.onChange();
      },
      undefined,
      () => {
        this.inFlight.delete(key);
        if (this.disposed) return;
        this.failed.add(key);
        this.request();
      },
    );
  }

  private add(id: TileId, key: string, bitmap: ImageBitmap): void {
    const texture = new THREE.Texture(bitmap);
    texture.flipY = false;
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.generateMipmaps = true;
    texture.minFilter = THREE.LinearMipmapLinearFilter;
    texture.magFilter = THREE.LinearFilter;
    texture.anisotropy = this.anisotropy;
    texture.needsUpdate = true;

    const p = tileSphereParams(id);
    const mesh = new THREE.Mesh(
      new THREE.SphereGeometry(
        TILE_RADIUS,
        TILE_SEGMENTS,
        TILE_SEGMENTS,
        p.phiStart,
        p.phiLength,
        p.thetaStart,
        p.thetaLength,
      ),
      new THREE.MeshStandardMaterial({
        map: texture,
        roughness: 0.95,
        metalness: 0,
        transparent: true,
        opacity: 0,
      }),
    );
    this.patchMaterial(mesh.material, lonLatFromUv(p.phiStart, p.phiLength, p.thetaStart, p.thetaLength));
    mesh.name = `earth-tile-${key}`;
    // After the base photo and the atmosphere, before the borders (1) and region patches.
    mesh.renderOrder = 0.5;
    mesh.visible = false;
    this.group.add(mesh);
    this.tiles.set(key, { key, mesh, texture, uploaded: false, opacity: 0, lastUsed: performance.now() });
    this.evict();
  }

  private evict(): void {
    if (this.tiles.size <= MAX_CACHED) return;
    const idle = [...this.tiles.values()]
      .filter((t) => !this.wantedKeys.has(t.key))
      .sort((a, b) => a.lastUsed - b.lastUsed);
    for (const tile of idle) {
      if (this.tiles.size <= MAX_CACHED) break;
      this.drop(tile);
    }
  }

  private drop(tile: Tile): void {
    this.group.remove(tile.mesh);
    tile.mesh.geometry.dispose();
    tile.mesh.material.dispose();
    tile.texture.dispose();
    (tile.texture.image as ImageBitmap | null)?.close?.();
    this.tiles.delete(tile.key);
  }

  /**
   * Uploads at most one new texture and advances fades. Returns true while anything still changes,
   * so the caller keeps rendering.
   */
  step(now: number, renderer: THREE.WebGLRenderer): boolean {
    const dt = this.lastStep > 0 ? Math.min(100, now - this.lastStep) : 16;
    this.lastStep = now;
    let uploaded = false;
    let animating = false;
    for (const tile of this.tiles.values()) {
      const wanted = this.wantedKeys.has(tile.key);
      if (wanted) tile.lastUsed = now;
      if (wanted && !tile.uploaded) {
        // One upload per frame keeps the texImage2D + mipmap cost from stacking into a hitch.
        if (uploaded) {
          animating = true;
          continue;
        }
        renderer.initTexture(tile.texture);
        tile.uploaded = true;
        uploaded = true;
      }
      const target = wanted && tile.uploaded ? 1 : 0;
      if (tile.opacity !== target) {
        const delta = this.reducedMotion ? 1 : dt / FADE_MS;
        tile.opacity =
          target > tile.opacity ? Math.min(target, tile.opacity + delta) : Math.max(target, tile.opacity - delta);
        animating = animating || tile.opacity !== target;
      }
      tile.mesh.material.opacity = tile.opacity;
      tile.mesh.visible = tile.opacity > 0;
    }
    return animating || uploaded;
  }

  dispose(): void {
    this.disposed = true;
    for (const tile of [...this.tiles.values()]) this.drop(tile);
  }
}
