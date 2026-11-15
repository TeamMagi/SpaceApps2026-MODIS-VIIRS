import * as THREE from "three";
import type { MapPalette, SensorFilter } from "../engine/types";
import type { GlobePointLayer } from "./pointAttributes";

/*
 * Sprite shader for the hotspots over satellite imagery. Points are the data and the photo is
 * context, so every mark carries a dark casing that separates it from any background:
 * Raw = MODIS ring / VIIRS square in a bright "on imagery" fill with a soft dark drop-halo,
 * Harmonized = FRP-bucket discs with a thin casing and an outer glow in their own colour that
 * grows with the bucket, plus dashed duplicate rings. Sizes are CSS pixels times the DPR; each
 * sprite is enlarged to hold its halo or glow, all in one draw per layer.
 */

const MARK_GLSL = /* glsl */ `
uniform float uZoom;
uniform float uGlow;
const float CASING = 1.35;
const float FRP_CASING = 1.0;
const float HALO_ALPHA = 0.35;
const float CASING_ALPHA = 0.9;
const float GLOW_PEAK = 0.45;
const float GLOW_REACH = 2.2;
const float MAX_SPRITE_PX = 64.0;

float ringCenter() { return clamp(3.6 * uZoom, 2.6, 7.2); }
float ringHalf() { return clamp(1.1 * uZoom, 0.9, 1.4); }
float squareHalf() { return clamp(2.7 * uZoom, 2.25, 5.6); }
float frpBucket(float kind) { return kind - 1.0; }
/** Bucket 1 = 1.0x, bucket 8 = 1.6x. */
float discRadius(float kind) {
  return clamp(2.7 * uZoom, 2.4, 5.6) * (1.0 + 0.6 * (frpBucket(kind) - 1.0) / 7.0);
}
float markOuter(float kind) {
  if (kind < 0.5) return ringCenter() + ringHalf();
  if (kind < 1.5) return squareHalf();
  return discRadius(kind);
}
/** Half the sprite: raw halos reach twice the mark radius, FRP glows GLOW_REACH times. */
float markReach(float kind, float scale) {
  float r = markOuter(kind) * scale;
  return kind < 1.5 ? 2.0 * r + CASING : GLOW_REACH * r + FRP_CASING + 1.0;
}

vec4 over(vec4 dst, vec3 color, float alpha) {
  return vec4(color * alpha + dst.rgb * (1.0 - alpha), alpha + dst.a * (1.0 - alpha));
}

/**
 * Premultiplied mark at sprite offset p (CSS px): under-layer (dark halo, or FRP glow), dark
 * casing, then the fill. Signed distance sd is to the fill shape, positive outside it.
 */
vec4 shadeMark(vec2 p, float kind, float scale, vec3 fill, float fillAlpha, vec3 casing) {
  float sd;
  float r;
  if (kind < 0.5) {
    sd = abs(length(p) - ringCenter() * scale) - ringHalf() * scale;
    r = markOuter(kind) * scale;
  } else if (kind < 1.5) {
    vec2 q = abs(p) - vec2(squareHalf() * scale);
    sd = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0);
    r = squareHalf() * scale;
  } else {
    r = discRadius(kind) * scale;
    sd = length(p) - r;
  }
  float fillCov = 1.0 - smoothstep(-0.5, 0.5, sd);
  vec4 c = vec4(0.0);
  if (kind < 1.5) {
    float out_ = max(sd, 0.0) / r;
    float halo = HALO_ALPHA * exp(-3.0 * out_ * out_) * (1.0 - smoothstep(0.6, 1.0, out_));
    float cas = (1.0 - smoothstep(CASING - 0.5, CASING + 0.5, sd)) * CASING_ALPHA;
    c = over(c, casing, max(halo, cas));
  } else {
    float b = frpBucket(kind);
    float strength = mix(0.25, 1.0, smoothstep(2.5, 6.5, b));
    float t = max(sd - FRP_CASING, 0.0) / ((GLOW_REACH - 1.0) * r);
    float glow = GLOW_PEAK * strength * uGlow * exp(-2.5 * t * t) * (1.0 - smoothstep(0.55, 1.0, t));
    c = over(c, fill, glow);
    float cas = (1.0 - smoothstep(FRP_CASING - 0.5, FRP_CASING + 0.5, sd)) * CASING_ALPHA;
    c = over(c, casing, cas);
  }
  return over(c, fill, fillCov * fillAlpha);
}

vec4 unpremultiply(vec4 c) {
  return c.a > 0.0 ? vec4(c.rgb / c.a, c.a) : vec4(0.0);
}
`;

const MARKER_VERTEX = /* glsl */ `
${MARK_GLSL}
uniform float uDpr;
uniform vec2 uFilter;
uniform float uRing;
uniform float uShowDup;
uniform vec3 uFill;
uniform vec3 uFillViirs;
uniform vec3 uFrp[8];
uniform float uModisAlpha;
uniform float uViirsAlpha;
attribute float aKind;
attribute float aSensor;
attribute float aDup;
varying float vKind;
varying float vExtent;
varying vec3 vColor;
varying float vAlpha;
float dupRingRadius(float kind) { return markOuter(kind) + CASING + 2.2; }
void main() {
  float hasModis = mod(aSensor, 2.0);
  float hasViirs = step(1.5, aSensor);
  float visible = max(hasModis * uFilter.x, hasViirs * uFilter.y);
  if (uRing > 0.5) visible *= aDup * uShowDup;
  vec4 world = modelMatrix * vec4(position, 1.0);
  if (dot(world.xyz, cameraPosition - world.xyz) <= 0.0) visible = 0.0;
  vKind = aKind;
  float half_ = uRing > 0.5 ? dupRingRadius(aKind) + 2.5 : markReach(aKind, 1.0);
  if (aKind < 0.5) { vColor = uFill; vAlpha = uModisAlpha; }
  else if (aKind < 1.5) { vColor = uFillViirs; vAlpha = uViirsAlpha; }
  else { vColor = uFrp[int(aKind - 2.0 + 0.5)]; vAlpha = 1.0; }
  gl_Position = projectionMatrix * viewMatrix * world;
  gl_PointSize = min(2.0 * half_ * uDpr, MAX_SPRITE_PX);
  vExtent = gl_PointSize / uDpr;
  if (visible < 0.5) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    gl_PointSize = 0.0;
  }
}`;

const MARKER_FRAGMENT = /* glsl */ `
${MARK_GLSL}
uniform float uRing;
uniform vec3 uCasing;
uniform vec3 uDupColor;
varying float vKind;
varying float vExtent;
varying vec3 vColor;
varying float vAlpha;
float dupRingRadius(float kind) { return markOuter(kind) + CASING + 2.2; }
void main() {
  vec2 p = (gl_PointCoord - 0.5) * vExtent;
  // Every mark, halo and glow is round: skip the sprite's corners before any shading.
  if (dot(p, p) > 0.25 * vExtent * vExtent) discard;
  vec4 c;
  if (uRing > 0.5) {
    float rr = dupRingRadius(vKind);
    float off = abs(length(p) - rr);
    float arc = (atan(p.y, p.x) + 3.14159265) * rr;
    float dash = step(mod(arc, 4.5), 2.5);
    float line = (1.0 - smoothstep(0.3, 0.9, off)) * dash;
    // Lighter than the mark casing: in dense clusters the rings overlap and would pile up into a blot.
    float cas = (1.0 - smoothstep(1.1, 1.8, off)) * 0.55;
    c = over(over(vec4(0.0), uCasing, cas), uDupColor, line * 0.95);
  } else {
    c = shadeMark(p, vKind, 1.0, vColor, vAlpha, uCasing);
  }
  if (c.a < 0.01) discard;
  gl_FragColor = unpremultiply(c);
  #include <colorspace_fragment>
}`;

/** Hovered/selected point: the mark again at 1.4x with a bright ring between dark casings. */
const HIGHLIGHT_SCALE = 1.4;

const HIGHLIGHT_VERTEX = /* glsl */ `
${MARK_GLSL}
uniform float uDpr;
uniform vec3 uFill;
uniform vec3 uFillViirs;
uniform vec3 uFrp[8];
attribute float aKind;
attribute float aSelected;
varying float vKind;
varying float vRing;
varying float vExtent;
varying float vSelected;
varying vec3 vColor;
void main() {
  vec4 world = modelMatrix * vec4(position, 1.0);
  vKind = aKind;
  vRing = markOuter(aKind) * ${HIGHLIGHT_SCALE.toFixed(2)} + CASING + 2.6;
  vSelected = aSelected;
  if (aKind < 0.5) vColor = uFill;
  else if (aKind < 1.5) vColor = uFillViirs;
  else vColor = uFrp[int(aKind - 2.0 + 0.5)];
  float half_ = max(vRing + 4.0, markReach(aKind, ${HIGHLIGHT_SCALE.toFixed(2)}));
  gl_Position = projectionMatrix * viewMatrix * world;
  gl_PointSize = min(2.0 * half_ * uDpr, MAX_SPRITE_PX * 1.5);
  vExtent = gl_PointSize / uDpr;
  if (dot(world.xyz, cameraPosition - world.xyz) <= 0.0) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    gl_PointSize = 0.0;
  }
}`;

const HIGHLIGHT_FRAGMENT = /* glsl */ `
${MARK_GLSL}
uniform vec3 uCasing;
uniform vec3 uInk;
uniform vec3 uFocus;
varying float vKind;
varying float vRing;
varying float vExtent;
varying float vSelected;
varying vec3 vColor;
void main() {
  vec2 p = (gl_PointCoord - 0.5) * vExtent;
  vec4 c = shadeMark(p, vKind, ${HIGHLIGHT_SCALE.toFixed(2)}, vColor, 1.0, uCasing);
  float off = abs(length(p) - vRing);
  float lineHalf = vSelected > 0.5 ? 1.1 : 0.85;
  float line = 1.0 - smoothstep(lineHalf - 0.4, lineHalf + 0.4, off);
  float cas = 1.0 - smoothstep(lineHalf + 1.1 - 0.4, lineHalf + 1.1 + 0.4, off);
  c = over(c, uCasing, cas * CASING_ALPHA);
  c = over(c, vSelected > 0.5 ? uFocus : uInk, line);
  if (c.a < 0.01) discard;
  gl_FragColor = unpremultiply(c);
  #include <colorspace_fragment>
}`;

function colorUniform(): { value: THREE.Color } {
  return { value: new THREE.Color() };
}

function luminance(color: THREE.Color): number {
  return 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b;
}

/** Orders `[a, b]` as `[lighter, darker]`. */
function byLightness(a: string, b: string): [lighter: THREE.Color, darker: THREE.Color] {
  const ca = new THREE.Color(a);
  const cb = new THREE.Color(b);
  return luminance(ca) >= luminance(cb) ? [ca, cb] : [cb, ca];
}

/** Share of the MODIS fill mixed toward the casing for VIIRS, so the sensors stay apart. */
const VIIRS_DIM = 0.15;

interface LayerMeshes {
  readonly geometry: THREE.BufferGeometry;
  readonly markers: THREE.Points;
  readonly rings: THREE.Points;
}

export interface PointStyle {
  readonly harmonized: boolean;
  readonly showDuplicateRings: boolean;
  readonly filter: SensorFilter;
  /** How long the FRP glow takes to fade in when the harmonized layer appears (0 = at once). */
  readonly glowFadeMs: number;
}

export interface HighlightTarget {
  readonly layer: GlobePointLayer;
  readonly index: number;
}

export class PointLayers {
  readonly group = new THREE.Group();
  private readonly shared = {
    uZoom: { value: 1 },
    uDpr: { value: 1 },
    uGlow: { value: 1 },
    uFilter: { value: new THREE.Vector2(1, 1) },
    uShowDup: { value: 0 },
    uFill: colorUniform(),
    uFillViirs: colorUniform(),
    uCasing: colorUniform(),
    uFrp: { value: Array.from({ length: 8 }, () => new THREE.Color()) },
    uModisAlpha: { value: 0.85 },
    uViirsAlpha: { value: 0.8 },
    uDupColor: colorUniform(),
  };
  private readonly markerMaterial: THREE.ShaderMaterial;
  private readonly ringMaterial: THREE.ShaderMaterial;
  private readonly highlightMaterial: THREE.ShaderMaterial;
  private readonly highlightGeometry = new THREE.BufferGeometry();
  private readonly highlight: THREE.Points;
  private raw: LayerMeshes | null = null;
  private harmonized: LayerMeshes | null = null;
  private style: PointStyle = {
    harmonized: false,
    showDuplicateRings: false,
    filter: { modis: true, viirs: true },
    glowFadeMs: 0,
  };
  private glowFade: { from: number; ms: number; start: number | null } | null = null;

  constructor() {
    const base = {
      transparent: true,
      depthWrite: false,
      depthTest: false,
      vertexShader: MARKER_VERTEX,
      fragmentShader: MARKER_FRAGMENT,
    };
    this.markerMaterial = new THREE.ShaderMaterial({
      ...base,
      uniforms: { ...this.shared, uRing: { value: 0 } },
    });
    this.ringMaterial = new THREE.ShaderMaterial({
      ...base,
      uniforms: { ...this.shared, uRing: { value: 1 } },
    });
    this.highlightMaterial = new THREE.ShaderMaterial({
      transparent: true,
      depthWrite: false,
      depthTest: false,
      vertexShader: HIGHLIGHT_VERTEX,
      fragmentShader: HIGHLIGHT_FRAGMENT,
      uniforms: {
        ...this.shared,
        uInk: colorUniform(),
        uFocus: colorUniform(),
      },
    });
    this.highlight = new THREE.Points(this.highlightGeometry, this.highlightMaterial);
    this.highlight.renderOrder = 7;
    this.highlight.frustumCulled = false;
    this.highlight.visible = false;
    this.group.add(this.highlight);
  }

  private buildMeshes(layer: GlobePointLayer): LayerMeshes {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(layer.positions, 3));
    geometry.setAttribute("aKind", new THREE.BufferAttribute(layer.kinds, 1));
    geometry.setAttribute("aSensor", new THREE.BufferAttribute(layer.sensors, 1));
    geometry.setAttribute("aDup", new THREE.BufferAttribute(layer.duplicates, 1));
    // Draw order only: attribute indices stay those of the layer, so picking is unaffected.
    if (layer.drawOrder) geometry.setIndex(new THREE.BufferAttribute(layer.drawOrder, 1));
    const markers = new THREE.Points(geometry, this.markerMaterial);
    const rings = new THREE.Points(geometry, this.ringMaterial);
    markers.renderOrder = 5;
    rings.renderOrder = 6;
    markers.frustumCulled = false;
    rings.frustumCulled = false;
    this.group.add(markers, rings);
    return { geometry, markers, rings };
  }

  private dropMeshes(meshes: LayerMeshes | null): void {
    if (!meshes) return;
    this.group.remove(meshes.markers, meshes.rings);
    meshes.geometry.dispose();
  }

  setLayers(raw: GlobePointLayer, harmonized: GlobePointLayer): void {
    this.dropMeshes(this.raw);
    this.dropMeshes(this.harmonized);
    this.raw = this.buildMeshes(raw);
    this.harmonized = this.buildMeshes(harmonized);
    this.applyStyle();
  }

  setStyle(style: PointStyle): void {
    const appearing = style.harmonized && !this.style.harmonized;
    this.style = style;
    if (!style.harmonized) {
      this.glowFade = null;
      this.shared.uGlow.value = 0;
    } else if (appearing) {
      if (style.glowFadeMs > 0) {
        this.glowFade = { from: this.shared.uGlow.value, ms: style.glowFadeMs, start: null };
      } else {
        this.glowFade = null;
        this.shared.uGlow.value = 1;
      }
    }
    this.applyStyle();
  }

  /** Advances the glow fade from the render loop; true while another frame is needed. */
  step(now: number): boolean {
    const fade = this.glowFade;
    if (!fade) return false;
    if (fade.start === null) fade.start = now;
    const t = Math.min(1, (now - fade.start) / fade.ms);
    const eased = t * t * (3 - 2 * t);
    this.shared.uGlow.value = fade.from + (1 - fade.from) * eased;
    if (t >= 1) this.glowFade = null;
    return true;
  }

  private applyStyle(): void {
    const { harmonized, showDuplicateRings, filter } = this.style;
    this.shared.uFilter.value.set(filter.modis ? 1 : 0, filter.viirs ? 1 : 0);
    this.shared.uShowDup.value = showDuplicateRings ? 1 : 0;
    for (const [meshes, active] of [
      [this.raw, !harmonized],
      [this.harmonized, harmonized],
    ] as const) {
      if (!meshes) continue;
      meshes.markers.visible = active;
      meshes.rings.visible = active && showDuplicateRings;
    }
  }

  /**
   * The photo is the same in both themes, so raw marks use a theme-independent "on imagery" pair:
   * the lighter of ink / halo as the fill and the darker as the casing.
   */
  setPalette(palette: MapPalette): void {
    const [fill, casing] = byLightness(palette.ink, palette.haloLight);
    this.shared.uFill.value.copy(fill);
    this.shared.uCasing.value.copy(casing);
    this.shared.uFillViirs.value.copy(fill).lerp(casing, VIIRS_DIM);
    palette.frp.forEach((c, i) => this.shared.uFrp.value[i].set(c));
    const [dupColor] = byLightness(palette.duplicateRing, palette.haloLight);
    this.shared.uDupColor.value.copy(dupColor);
    const uniforms = this.highlightMaterial.uniforms;
    (uniforms.uInk.value as THREE.Color).copy(byLightness(palette.ink, palette.haloLight)[0]);
    (uniforms.uFocus.value as THREE.Color).copy(byLightness(palette.focus, palette.haloLight)[0]);
  }

  /** `zoomRatio` is the camera altitude at fit over the current altitude, as the flat map's k / fitK. */
  setZoom(zoomRatio: number, dpr: number): void {
    const ratio = Math.max(0.4, zoomRatio);
    this.shared.uZoom.value = Math.min(2.1, Math.max(0.48, 0.62 * Math.pow(ratio, 0.38)));
    const rawZoomT = Math.min(1, Math.max(0, Math.log2(Math.max(1, ratio)) / 4));
    this.shared.uModisAlpha.value = 0.85 + 0.15 * rawZoomT;
    this.shared.uViirsAlpha.value = 0.8 + 0.2 * rawZoomT;
    this.shared.uDpr.value = dpr;
  }

  setHighlight(hovered: HighlightTarget | null, selected: HighlightTarget | null): void {
    const targets = [hovered, selected].filter(
      (t): t is HighlightTarget => t !== null && t.index >= 0 && t.index < t.layer.count,
    );
    const positions = new Float32Array(targets.length * 3);
    const kinds = new Float32Array(targets.length);
    const flags = new Float32Array(targets.length);
    targets.forEach((t, i) => {
      positions.set(t.layer.positions.subarray(t.index * 3, t.index * 3 + 3), i * 3);
      kinds[i] = t.layer.kinds[t.index];
      flags[i] = t === selected ? 1 : 0;
    });
    // Releases the previous attribute buffers on the GPU before the new ones replace them.
    this.highlightGeometry.dispose();
    this.highlightGeometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    this.highlightGeometry.setAttribute("aKind", new THREE.BufferAttribute(kinds, 1));
    this.highlightGeometry.setAttribute("aSelected", new THREE.BufferAttribute(flags, 1));
    this.highlight.visible = targets.length > 0;
  }

  dispose(): void {
    this.dropMeshes(this.raw);
    this.dropMeshes(this.harmonized);
    this.highlightGeometry.dispose();
    this.markerMaterial.dispose();
    this.ringMaterial.dispose();
    this.highlightMaterial.dispose();
  }
}
