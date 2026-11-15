import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import type { GeoFeature } from "../../../lib/geo/types";
import type { MapPalette } from "../engine/types";
import { Earth, type EarthColors } from "./earth";
import { CameraFlight } from "./flight";
import {
  fitDistance,
  lonLatToVec,
  vecToLonLat,
  viewHalfAngle,
  type GeoFrame,
  type ScreenProjector,
} from "./geoMath";
import type { GlobePointLayer } from "./pointAttributes";
import { PointLayers, type HighlightTarget, type PointStyle } from "./points";
import { Orbits, type OrbitColors } from "./orbits";
import { surfacePxPerDegree, visibleCapRadius, type TileId } from "./tileMath";

const DEG = Math.PI / 180;
const FOV_DEG = 30;
export const MIN_DISTANCE = 1.04;
export const MAX_DISTANCE = 6;
const MIN_FIT_DISTANCE = 1.08;
/** Share of the narrower view half-angle a fitted region may fill. */
const FIT_PADDING = 0.86;
/** Altitude multiplier per zoom-button or key step. */
export const ZOOM_STEP = 1.6;
const ORBIT_FRAME_MS = 33;
const ROTATE_MS = 200;
/** Time constant of the eased wheel/button zoom; ~95 % of a step lands within 3τ. */
const ZOOM_TAU_MS = 70;
/** Altitude multiplier per 100 px of wheel delta. */
const WHEEL_ZOOM_BASE = 1.2;
const WHEEL_MAX_DELTA = 400;
/** Quiet time after the last camera change before the view counts as settled. */
const SETTLE_MS = 140;
/** Width of public/globe/earth.jpg. */
const BASE_PHOTO_WIDTH = 4096;
/** How far the base photo may be magnified before high-resolution tiles load. */
const DETAIL_MAGNIFICATION = 1.25;

export interface GlobeColors {
  readonly palette: MapPalette;
  readonly earth: EarthColors;
  readonly orbits: OrbitColors;
}

export interface GlobeViewState {
  readonly canZoomIn: boolean;
  readonly canZoomOut: boolean;
  readonly isAtFit: boolean;
  readonly orbitsVisible: boolean;
}

export interface GlobeSceneOptions {
  readonly host: HTMLElement;
  readonly canvasClassName: string;
  readonly photoUrl: string;
  readonly tileUrl: (tile: TileId) => string;
  readonly reducedMotion: boolean;
  readonly onViewState: (state: GlobeViewState) => void;
  /** After every rendered frame in which the camera moved; keep it cheap (no React state). */
  readonly onCameraMove: () => void;
  /** Once the camera has rested for a moment. */
  readonly onSettle: () => void;
}

function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

export class GlobeScene {
  private readonly options: GlobeSceneOptions;
  private readonly canvas: HTMLCanvasElement;
  private readonly renderer: THREE.WebGLRenderer;
  private readonly scene = new THREE.Scene();
  private readonly camera: THREE.PerspectiveCamera;
  private readonly controls: OrbitControls;
  private readonly flight: CameraFlight;
  private readonly earth: Earth;
  private readonly orbits = new Orbits();
  private readonly points = new PointLayers();
  private readonly raycaster = new THREE.Raycaster();
  private readonly unitSphere = new THREE.Sphere(new THREE.Vector3(), 1);
  private readonly viewProjection = new THREE.Matrix4();
  private readonly intersection: IntersectionObserver | null;
  private frame: GeoFrame | null = null;
  private viewState: GlobeViewState | null = null;
  private width = 1;
  private height = 1;
  private dpr = 1;
  private dirty = true;
  private controlsChanged = false;
  /** Target altitude of the eased zoom, or null when no zoom is running. */
  private zoomGoal: number | null = null;
  private lastTick = 0;
  private lastMove = 0;
  private settlePending = false;
  private lastOrbit = 0;
  private readonly baseTexelsPerDegree: number;
  private running = false;
  private disposed = false;

  constructor(options: GlobeSceneOptions) {
    this.options = options;
    this.canvas = document.createElement("canvas");
    this.canvas.className = options.canvasClassName;
    options.host.appendChild(this.canvas);

    try {
      this.renderer = new THREE.WebGLRenderer({
        canvas: this.canvas,
        antialias: true,
        alpha: true,
        powerPreference: "high-performance",
      });
    } catch (error) {
      this.canvas.remove();
      throw error;
    }
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;

    this.camera = new THREE.PerspectiveCamera(FOV_DEG, 1, 0.005, 100);
    this.camera.position.set(...lonLatToVec(0, 20, 4));
    this.camera.lookAt(0, 0, 0);
    // The light rides with the camera so whichever side faces the viewer is lit.
    const sun = new THREE.DirectionalLight("white", 2.0);
    sun.position.set(-1.2, 1.4, 2);
    this.camera.add(sun);
    this.scene.add(this.camera);

    this.controls = new OrbitControls(this.camera, this.canvas);
    this.controls.enablePan = false;
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.08;
    this.controls.minDistance = MIN_DISTANCE;
    this.controls.maxDistance = MAX_DISTANCE;
    this.controls.minPolarAngle = 0.08;
    this.controls.maxPolarAngle = Math.PI - 0.08;
    this.controls.addEventListener("start", this.handleControlStart);
    // Drag and pinch move the camera inside OrbitControls' own event handlers, so the next
    // update() in the loop sees no change; this flag still gets those frames drawn.
    this.controls.addEventListener("change", this.handleControlChange);
    // Wheel zoom is eased here instead of OrbitControls' instant steps; capturing on the host
    // keeps the event from reaching the controls' listener on the canvas.
    options.host.addEventListener("wheel", this.handleWheel, { capture: true, passive: false });

    this.flight = new CameraFlight(this.camera);

    const anisotropy = this.renderer.capabilities.getMaxAnisotropy();
    const maxTextureSize = this.renderer.capabilities.maxTextureSize;
    this.baseTexelsPerDegree = Math.min(BASE_PHOTO_WIDTH, maxTextureSize) / 360;
    this.earth = new Earth(
      options.photoUrl,
      options.tileUrl,
      anisotropy,
      maxTextureSize,
      options.reducedMotion,
      () => this.requestRender(),
    );
    this.scene.add(this.earth.group, this.orbits.group, this.points.group);

    this.resize();
    this.cameraChanged();

    this.intersection =
      typeof IntersectionObserver === "undefined"
        ? null
        : new IntersectionObserver((entries) => {
            const visible = entries.some((entry) => entry.isIntersecting);
            this.setRunning(visible);
          });
    this.intersection?.observe(options.host);
    this.setRunning(true);
  }

  private setRunning(running: boolean): void {
    if (this.disposed || running === this.running) return;
    this.running = running;
    this.renderer.setAnimationLoop(running ? this.tick : null);
  }

  private handleControlStart = (): void => {
    this.flight.cancel();
  };

  private handleControlChange = (): void => {
    this.controlsChanged = true;
  };

  private handleWheel = (event: WheelEvent): void => {
    event.preventDefault();
    event.stopPropagation();
    const scale = event.deltaMode === 1 ? 33 : event.deltaMode === 2 ? 800 : 1;
    const delta = Math.max(-WHEEL_MAX_DELTA, Math.min(WHEEL_MAX_DELTA, event.deltaY * scale));
    if (delta === 0) return;
    this.zoomBy(Math.pow(WHEEL_ZOOM_BASE, delta / 100));
  };

  /** Eases the altitude toward `zoomGoal` in log space, so every step feels the same size. */
  private stepZoom(dt: number): void {
    const goal = this.zoomGoal;
    if (goal === null) return;
    const altitude = Math.max(1e-4, this.distance() - 1);
    const k = 1 - Math.exp(-dt / ZOOM_TAU_MS);
    let next = Math.exp(Math.log(altitude) + (Math.log(goal) - Math.log(altitude)) * k);
    if (Math.abs(Math.log(goal / next)) < 0.002) {
      next = goal;
      this.zoomGoal = null;
    }
    this.camera.position.setLength(1 + next);
  }

  private tick = (now: number): void => {
    if (this.disposed) return;
    const dt = this.lastTick > 0 ? Math.min(100, now - this.lastTick) : 16;
    this.lastTick = now;
    let moved = false;
    if (this.zoomGoal !== null) {
      this.stepZoom(dt);
      moved = true;
    }
    if (this.flight.active) {
      this.flight.step(now);
      moved = true;
    } else if (this.controls.update()) {
      moved = true;
    }
    if (this.controlsChanged) {
      this.controlsChanged = false;
      moved = true;
    }
    if (moved) this.cameraChanged();

    let orbitStep = false;
    if (!this.options.reducedMotion && this.orbits.group.visible) {
      const elapsed = now - this.lastOrbit;
      if (elapsed >= ORBIT_FRAME_MS) {
        this.orbits.update(Math.min(elapsed, 100) / 1000);
        this.lastOrbit = now;
        orbitStep = true;
      }
    }

    const tilesBusy = this.earth.stepTiles(now, this.renderer);
    const pointsBusy = this.points.step(now);

    if (moved || orbitStep || tilesBusy || pointsBusy || this.dirty) {
      this.dirty = false;
      this.renderer.render(this.scene, this.camera);
    }
    if (moved) {
      this.lastMove = now;
      this.settlePending = true;
      this.options.onCameraMove();
    } else if (this.settlePending && now - this.lastMove >= SETTLE_MS) {
      this.settlePending = false;
      this.options.onSettle();
    }
  };

  requestRender(): void {
    this.dirty = true;
  }

  private distance(): number {
    return this.camera.position.length();
  }

  private fitTarget(): number {
    if (!this.frame) return this.distance();
    const half = viewHalfAngle(FOV_DEG * DEG, this.width / this.height) * FIT_PADDING;
    const d = fitDistance(this.frame.radius, half);
    return Math.min(MAX_DISTANCE, Math.max(MIN_FIT_DISTANCE, d));
  }

  private cameraChanged(): void {
    const d = this.distance();
    const altitude = Math.max(1e-3, d - 1);
    // Keep drag and wheel steps proportional to what is on screen, so close-ups stay controllable.
    this.controls.rotateSpeed = Math.min(0.5, Math.max(0.01, altitude * 0.12));
    this.controls.zoomSpeed = Math.min(1, Math.max(0.1, (altitude / d) * 2));
    const zoomRatio = (this.fitTarget() - 1) / altitude;
    this.points.setZoom(zoomRatio, this.dpr);
    this.earth.setZoom(zoomRatio);
    this.orbits.setFade(smoothstep(1.45, 2.4, d));
    this.updateDetail(d);
    this.publishViewState();
  }

  private updateDetail(d: number): void {
    const pxPerDegree = surfacePxPerDegree(d, FOV_DEG * DEG, this.height * this.dpr);
    if (pxPerDegree <= this.baseTexelsPerDegree * DETAIL_MAGNIFICATION) {
      this.earth.setDetailView(null, 0);
      return;
    }
    const p = this.camera.position;
    this.earth.setDetailView([p.x / d, p.y / d, p.z / d], visibleCapRadius(d, FOV_DEG * DEG, this.camera.aspect));
  }

  private publishViewState(): void {
    const d = this.distance();
    let isAtFit = true;
    if (this.frame) {
      const dir = this.camera.position.clone().normalize();
      const [fx, fy, fz] = this.frame.direction;
      const angle = Math.acos(Math.max(-1, Math.min(1, dir.x * fx + dir.y * fy + dir.z * fz)));
      const fit = this.fitTarget();
      isAtFit = angle < 0.004 && Math.abs(d - fit) < fit * 0.01;
    }
    const next: GlobeViewState = {
      canZoomIn: d > MIN_DISTANCE * 1.002,
      canZoomOut: d < MAX_DISTANCE * 0.998,
      isAtFit,
      orbitsVisible: this.orbits.noticeable,
    };
    const prev = this.viewState;
    if (
      prev &&
      prev.canZoomIn === next.canZoomIn &&
      prev.canZoomOut === next.canZoomOut &&
      prev.isAtFit === next.isAtFit &&
      prev.orbitsVisible === next.orbitsVisible
    ) {
      return;
    }
    this.viewState = next;
    this.options.onViewState(next);
  }

  private flyTo(direction: THREE.Vector3, distance: number, duration: number): void {
    this.zoomGoal = null;
    const clamped = Math.min(MAX_DISTANCE, Math.max(MIN_DISTANCE, distance));
    this.flight.start(direction, clamped, this.options.reducedMotion ? 0 : duration);
    this.cameraChanged();
    this.requestRender();
  }

  /** Sets the region frame; `animate` flies there, otherwise the camera jumps. */
  setFrame(frame: GeoFrame | null, animate: boolean): void {
    this.frame = frame;
    if (frame) this.fit(animate);
    else this.publishViewState();
  }

  fit(animate = true): void {
    if (!this.frame) return;
    const target = new THREE.Vector3(...this.frame.direction);
    const angle = this.camera.position.clone().normalize().angleTo(target);
    const duration = animate ? Math.min(1400, Math.max(600, 500 + angle * 450)) : 0;
    this.flyTo(target, this.fitTarget(), duration);
  }

  /** Eased zoom by an altitude factor; repeated calls compound on the pending target. */
  zoomBy(altitudeFactor: number): void {
    this.flight.cancel();
    const from = this.zoomGoal ?? this.distance() - 1;
    const goal = Math.min(MAX_DISTANCE, Math.max(MIN_DISTANCE, 1 + from * altitudeFactor)) - 1;
    if (this.options.reducedMotion) {
      this.zoomGoal = null;
      this.camera.position.setLength(1 + goal);
      this.controlsChanged = true;
      this.requestRender();
      return;
    }
    this.zoomGoal = goal;
  }

  /** Rotates by a fraction of the visible height; positive lon looks east, positive lat north. */
  rotateBy(lonSteps: number, latSteps: number): void {
    const d = this.distance();
    const stepDeg = Math.min(20, Math.max(0.05, (d - 1) * Math.tan((FOV_DEG / 2) * DEG) * 0.5 / DEG));
    const [lon, lat] = vecToLonLat(this.camera.position.x, this.camera.position.y, this.camera.position.z);
    const nextLat = Math.max(-80, Math.min(80, lat + latSteps * stepDeg));
    const target = new THREE.Vector3(...lonLatToVec(lon + lonSteps * stepDeg, nextLat));
    this.flyTo(target, d, ROTATE_MS);
  }

  setColors(colors: GlobeColors): void {
    this.earth.setColors(colors.earth);
    this.orbits.setColors(colors.orbits);
    this.points.setPalette(colors.palette);
    this.cameraChanged();
    this.requestRender();
  }

  setBorders(features: readonly GeoFeature[]): void {
    this.earth.setBorders(features);
    this.requestRender();
  }

  setSelection(features: readonly GeoFeature[]): void {
    this.earth.setSelection(features);
    this.requestRender();
  }

  setHoverFeature(feature: GeoFeature | null): void {
    this.earth.setHover(feature);
    this.requestRender();
  }

  setPointLayers(raw: GlobePointLayer, harmonized: GlobePointLayer): void {
    this.points.setLayers(raw, harmonized);
    this.requestRender();
  }

  setPointStyle(style: PointStyle): void {
    this.points.setStyle(style);
    this.requestRender();
  }

  setHighlight(hovered: HighlightTarget | null, selected: HighlightTarget | null): void {
    this.points.setHighlight(hovered, selected);
    this.requestRender();
  }

  /** Lon/lat under a canvas-relative CSS pixel, or null off the globe. */
  surfaceAt(x: number, y: number): [lon: number, lat: number] | null {
    const ndc = new THREE.Vector2((x / this.width) * 2 - 1, -(y / this.height) * 2 + 1);
    this.camera.updateMatrixWorld();
    this.raycaster.setFromCamera(ndc, this.camera);
    const hit = this.raycaster.ray.intersectSphere(this.unitSphere, new THREE.Vector3());
    return hit ? vecToLonLat(hit.x, hit.y, hit.z) : null;
  }

  projector(): ScreenProjector {
    this.camera.updateMatrixWorld();
    this.viewProjection.multiplyMatrices(this.camera.projectionMatrix, this.camera.matrixWorldInverse);
    const p = this.camera.position;
    return {
      matrix: this.viewProjection.elements,
      camera: [p.x, p.y, p.z],
      width: this.width,
      height: this.height,
    };
  }

  resize(): void {
    const width = this.options.host.clientWidth;
    const height = this.options.host.clientHeight;
    if (width === 0 || height === 0) return;
    const wasAtFit = this.viewState?.isAtFit ?? false;
    this.width = width;
    this.height = height;
    this.dpr = Math.min(2, window.devicePixelRatio || 1);
    this.renderer.setPixelRatio(this.dpr);
    this.renderer.setSize(width, height, false);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    if (wasAtFit && this.frame && !this.flight.active) this.fit(false);
    this.cameraChanged();
    this.requestRender();
  }

  dispose(): void {
    this.disposed = true;
    this.renderer.setAnimationLoop(null);
    this.intersection?.disconnect();
    this.controls.removeEventListener("start", this.handleControlStart);
    this.controls.removeEventListener("change", this.handleControlChange);
    this.options.host.removeEventListener("wheel", this.handleWheel, { capture: true });
    this.controls.dispose();
    this.earth.dispose();
    this.orbits.dispose();
    this.points.dispose();
    this.renderer.dispose();
    this.renderer.forceContextLoss();
    this.canvas.remove();
  }
}
