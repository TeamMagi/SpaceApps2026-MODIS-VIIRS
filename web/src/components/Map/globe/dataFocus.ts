import * as THREE from "three";
import type { GeoFeature } from "../../../lib/geo/types";
import { makeSelectionMask } from "./textures";

/*
 * "Imagery yields to data": once the camera is near the region, the photo is dimmed and
 * desaturated — lightly inside the selected region, more outside it — so hotspots carry the
 * contrast. One set of uniforms is shared by the base photo and every tile material.
 */

const INSIDE_DARKEN = 0.18;
const INSIDE_DESATURATE = 0.35;
const OUTSIDE_DARKEN = 0.4;
const OUTSIDE_DESATURATE = 0.6;

const FRAGMENT_DECL = /* glsl */ `
uniform sampler2D uYieldMask;
uniform vec4 uYieldBounds;
uniform float uYieldStrength;
uniform vec4 uLonLatFromUv;
`;

// Imagery UVs are linear in lon/lat on every sphere patch, so no trigonometry per fragment.
const FRAGMENT_APPLY = /* glsl */ `
#ifdef USE_MAP
if (uYieldStrength > 0.0) {
  float lon = uLonLatFromUv.x + vMapUv.x * uLonLatFromUv.y;
  float lat = uLonLatFromUv.z + vMapUv.y * uLonLatFromUv.w;
  vec2 maskUv = vec2(mod(lon - uYieldBounds.x + 720.0, 360.0) / uYieldBounds.z, (lat - uYieldBounds.y) / uYieldBounds.w);
  // The mask's padded border is black and clamps to the edge, so outside the window reads 0.
  float inside = texture2D(uYieldMask, maskUv).r;
  float darken = mix(${OUTSIDE_DARKEN.toFixed(3)}, ${INSIDE_DARKEN.toFixed(3)}, inside) * uYieldStrength;
  float desaturate = mix(${OUTSIDE_DESATURATE.toFixed(3)}, ${INSIDE_DESATURATE.toFixed(3)}, inside) * uYieldStrength;
  float luma = dot(diffuseColor.rgb, vec3(0.2126, 0.7152, 0.0722));
  diffuseColor.rgb = mix(diffuseColor.rgb, vec3(luma), desaturate) * (1.0 - darken);
}
#endif
`;

/** `[lon at u=0, lon span, lat at v=0, lat span]` in degrees for a SphereGeometry patch's UVs. */
export function lonLatFromUv(
  phiStart: number,
  phiLength: number,
  thetaStart: number,
  thetaLength: number,
): THREE.Vector4 {
  const deg = 180 / Math.PI;
  return new THREE.Vector4(
    phiStart * deg - 180,
    phiLength * deg,
    90 - (thetaStart + thetaLength) * deg,
    thetaLength * deg,
  );
}

export class DataFocus {
  private readonly uniforms = {
    uYieldMask: { value: null as THREE.Texture | null },
    uYieldBounds: { value: new THREE.Vector4(0, 0, 1, 1) },
    uYieldStrength: { value: 0 },
  };
  private zoomStrength = 0;

  /**
   * Patches an imagery material whose map UVs span `uvToLonLat` (see `lonLatFromUv`); all patched
   * materials share this instance's selection and strength uniforms.
   */
  readonly apply = (material: THREE.MeshStandardMaterial, uvToLonLat: THREE.Vector4): void => {
    material.onBeforeCompile = (shader) => {
      Object.assign(shader.uniforms, this.uniforms, { uLonLatFromUv: { value: uvToLonLat } });
      shader.fragmentShader = shader.fragmentShader
        .replace("#include <common>", `#include <common>\n${FRAGMENT_DECL}`)
        .replace("#include <map_fragment>", `#include <map_fragment>\n${FRAGMENT_APPLY}`);
    };
  };

  setSelection(features: readonly GeoFeature[]): void {
    this.uniforms.uYieldMask.value?.dispose();
    const mask = makeSelectionMask(features);
    this.uniforms.uYieldMask.value = mask?.texture ?? null;
    if (mask) {
      const { minLon, minLat, lonSpan, latSpan } = mask.bounds;
      this.uniforms.uYieldBounds.value.set(minLon, minLat, lonSpan, latSpan);
    }
    this.sync();
  }

  /** 0 at the global view, 1 at the region's fitted view; `zoomRatio` as for the point sizes. */
  setZoom(zoomRatio: number): void {
    const t = Math.min(1, Math.max(0, (zoomRatio - 0.3) / 0.6));
    this.zoomStrength = t * t * (3 - 2 * t);
    this.sync();
  }

  private sync(): void {
    this.uniforms.uYieldStrength.value = this.uniforms.uYieldMask.value ? this.zoomStrength : 0;
  }

  dispose(): void {
    this.uniforms.uYieldMask.value?.dispose();
    this.uniforms.uYieldMask.value = null;
  }
}
