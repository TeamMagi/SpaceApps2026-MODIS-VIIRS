import * as THREE from "three";
import type { GeoFeature } from "../../../lib/geo/types";
import { DataFocus, lonLatFromUv } from "./dataFocus";
import type { Vec3 } from "./geoMath";
import { loadEarthPhoto, makeBordersTexture, makeSurfacePatch, type PatchStyle } from "./textures";
import type { TileId } from "./tileMath";
import { TileLayer } from "./tiles";

export interface EarthColors {
  readonly borderHalo: string;
  readonly borderLine: string;
  readonly selection: string;
  readonly hover: string;
  readonly atmosphere: string;
  readonly ground: string;
  readonly dark: boolean;
}

// A light fill: the region outline carries the selection, the fill must not compete with points.
const SELECTION_STYLE = { fillAlpha: 0.12, lineFraction: 1 / 220, glow: true } as const;
const HOVER_STYLE = { fillAlpha: 0.16, lineFraction: 1 / 320, glow: false } as const;

const ATMOSPHERE_VERTEX = /* glsl */ `
varying vec3 vN;
void main() {
  vN = normalize(normalMatrix * normal);
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}`;

const ATMOSPHERE_FRAGMENT = /* glsl */ `
uniform vec3 color;
uniform float alpha;
varying vec3 vN;
void main() {
  float i = pow(clamp(-vN.z, 0.0, 1.0), 1.6);
  i *= smoothstep(0.0, 0.35, -vN.z + 0.02);
  gl_FragColor = vec4(color, i * alpha);
  #include <colorspace_fragment>
}`;

class Overlay {
  readonly mesh: THREE.Mesh<THREE.BufferGeometry, THREE.MeshBasicMaterial>;
  private features: readonly GeoFeature[] = [];

  constructor(
    private readonly radius: number,
    private readonly style: Omit<PatchStyle, "color">,
    private readonly anisotropy: number,
    renderOrder: number,
  ) {
    this.mesh = new THREE.Mesh(
      new THREE.BufferGeometry(),
      new THREE.MeshBasicMaterial({ transparent: true, depthWrite: false }),
    );
    this.mesh.renderOrder = renderOrder;
    this.mesh.visible = false;
  }

  set(features: readonly GeoFeature[], color: string): void {
    this.features = features;
    this.paint(color);
  }

  repaint(color: string): void {
    this.paint(color);
  }

  private paint(color: string): void {
    this.mesh.material.map?.dispose();
    this.mesh.material.map = null;
    this.mesh.geometry.dispose();
    const patch = makeSurfacePatch(this.features, { ...this.style, color }, this.radius, this.anisotropy);
    this.mesh.geometry = patch ? patch.geometry : new THREE.BufferGeometry();
    this.mesh.material.map = patch ? patch.texture : null;
    this.mesh.material.needsUpdate = true;
    this.mesh.visible = patch !== null;
  }

  dispose(): void {
    this.mesh.material.map?.dispose();
    this.mesh.material.dispose();
    this.mesh.geometry.dispose();
  }
}

export class Earth {
  readonly group = new THREE.Group();
  readonly surface: THREE.Mesh<THREE.SphereGeometry, THREE.MeshStandardMaterial>;
  private readonly borderMaterial: THREE.MeshBasicMaterial;
  private readonly borderMesh: THREE.Mesh<THREE.SphereGeometry, THREE.MeshBasicMaterial>;
  private readonly atmosphere: THREE.Mesh<THREE.SphereGeometry, THREE.ShaderMaterial>;
  private readonly selection: Overlay;
  private readonly hover: Overlay;
  private readonly hemi: THREE.HemisphereLight;
  private readonly photo: THREE.Texture;
  private readonly tiles: TileLayer;
  private readonly focus = new DataFocus();
  private borderFeatures: readonly GeoFeature[] = [];
  private colors: EarthColors | null = null;
  private disposed = false;

  constructor(
    photoUrl: string,
    tileUrl: (tile: TileId) => string,
    private readonly anisotropy: number,
    private readonly maxTextureSize: number,
    reducedMotion: boolean,
    onPhoto: () => void,
  ) {
    this.group.name = "earth";
    this.surface = new THREE.Mesh(
      new THREE.SphereGeometry(1, 128, 64),
      new THREE.MeshStandardMaterial({ roughness: 0.95, metalness: 0, color: "white" }),
    );
    this.surface.name = "earth-surface";
    this.focus.apply(this.surface.material, lonLatFromUv(0, 2 * Math.PI, 0, Math.PI));

    this.borderMaterial = new THREE.MeshBasicMaterial({
      transparent: true,
      depthWrite: false,
    });
    this.borderMesh = new THREE.Mesh(new THREE.SphereGeometry(1.002, 128, 64), this.borderMaterial);
    this.borderMesh.renderOrder = 1;
    this.borderMesh.visible = false;

    this.selection = new Overlay(1.004, SELECTION_STYLE, anisotropy, 3);
    this.hover = new Overlay(1.003, HOVER_STYLE, anisotropy, 2);

    this.atmosphere = new THREE.Mesh(
      new THREE.SphereGeometry(1.1, 64, 32),
      new THREE.ShaderMaterial({
        transparent: true,
        side: THREE.BackSide,
        depthWrite: false,
        uniforms: {
          color: { value: new THREE.Color() },
          alpha: { value: 0.5 },
        },
        vertexShader: ATMOSPHERE_VERTEX,
        fragmentShader: ATMOSPHERE_FRAGMENT,
      }),
    );
    this.atmosphere.name = "atmosphere";

    this.tiles = new TileLayer(tileUrl, anisotropy, reducedMotion, onPhoto, this.focus.apply);

    this.hemi = new THREE.HemisphereLight("white", "black", 1.0);
    this.group.add(
      this.surface,
      this.tiles.group,
      this.borderMesh,
      this.hover.mesh,
      this.selection.mesh,
      this.atmosphere,
      this.hemi,
    );

    this.photo = loadEarthPhoto(photoUrl, anisotropy, (texture) => {
      if (this.disposed) return;
      this.surface.material.map = texture;
      this.surface.material.needsUpdate = true;
      onPhoto();
    });
  }

  /** Area needing high-resolution tiles, or `null` when the base photo is sharp enough. */
  setDetailView(direction: Vec3 | null, capRadius: number): void {
    this.tiles.setView(direction, capRadius);
  }

  /** Advances tile uploads and fades; true while another frame is needed. */
  stepTiles(now: number, renderer: THREE.WebGLRenderer): boolean {
    return this.tiles.step(now, renderer);
  }

  /** `zoomRatio` is the fitted altitude over the current one: 1 at the region view. */
  setZoom(zoomRatio: number): void {
    this.focus.setZoom(zoomRatio);
  }

  setColors(colors: EarthColors): void {
    const prev = this.colors;
    this.colors = colors;
    if (!prev || prev.borderHalo !== colors.borderHalo || prev.borderLine !== colors.borderLine) {
      this.paintBorders();
    }
    if (!prev || prev.selection !== colors.selection) this.selection.repaint(colors.selection);
    if (!prev || prev.hover !== colors.hover) this.hover.repaint(colors.hover);
    this.hemi.intensity = colors.dark ? 1.0 : 1.3;
    this.hemi.groundColor.set(colors.ground);
    this.atmosphere.material.uniforms.color.value.set(colors.atmosphere);
    this.atmosphere.material.uniforms.alpha.value = colors.dark ? 0.6 : 0.4;
  }

  setBorders(features: readonly GeoFeature[]): void {
    if (this.borderFeatures === features) return;
    this.borderFeatures = features;
    this.paintBorders();
  }

  setSelection(features: readonly GeoFeature[]): void {
    this.selection.set(features, this.colors?.selection ?? "white");
    this.focus.setSelection(features);
  }

  setHover(feature: GeoFeature | null): void {
    this.hover.set(feature ? [feature] : [], this.colors?.hover ?? "white");
  }

  private paintBorders(): void {
    this.borderMaterial.map?.dispose();
    this.borderMaterial.map = null;
    const colors = this.colors;
    if (colors && this.borderFeatures.length > 0) {
      this.borderMaterial.map = makeBordersTexture(
        this.borderFeatures,
        { halo: colors.borderHalo, line: colors.borderLine },
        this.maxTextureSize,
        this.anisotropy,
      );
    }
    this.borderMaterial.needsUpdate = true;
    this.borderMesh.visible = this.borderMaterial.map !== null;
  }

  dispose(): void {
    this.disposed = true;
    this.photo.dispose();
    this.tiles.dispose();
    this.focus.dispose();
    this.surface.geometry.dispose();
    this.surface.material.dispose();
    this.borderMaterial.map?.dispose();
    this.borderMaterial.dispose();
    this.borderMesh.geometry.dispose();
    this.atmosphere.geometry.dispose();
    this.atmosphere.material.dispose();
    this.selection.dispose();
    this.hover.dispose();
    this.hemi.dispose();
  }
}
