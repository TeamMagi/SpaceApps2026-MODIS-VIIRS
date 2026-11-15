import * as THREE from "three";
import { easeInOutCubic } from "./geoMath";

interface Flight {
  readonly start: number;
  readonly duration: number;
  readonly fromDirection: THREE.Vector3;
  readonly rotation: THREE.Quaternion;
  fromDistance: number;
  toDistance: number;
}

/** Moves the camera along the great circle toward a direction while easing its distance. */
export class CameraFlight {
  private flight: Flight | null = null;
  private readonly partial = new THREE.Quaternion();
  private readonly identity = new THREE.Quaternion();
  private readonly direction = new THREE.Vector3();

  constructor(private readonly camera: THREE.PerspectiveCamera) {}

  get active(): boolean {
    return this.flight !== null;
  }

  start(toDirection: THREE.Vector3, toDistance: number, duration: number): void {
    const fromDirection = this.camera.position.clone().normalize();
    const target = toDirection.clone().normalize();
    this.flight = {
      start: performance.now(),
      duration: Math.max(0, duration),
      fromDirection,
      rotation: new THREE.Quaternion().setFromUnitVectors(fromDirection, target),
      fromDistance: this.camera.position.length(),
      toDistance,
    };
    if (duration <= 0) this.step(this.flight.start);
  }

  /**
   * Aims the running flight at a new final distance (the view was resized) without touching its
   * direction or remaining time; the start distance is re-derived so the camera does not jump.
   */
  retargetDistance(toDistance: number, now = performance.now()): void {
    const flight = this.flight;
    if (!flight) return;
    const progress = flight.duration > 0 ? Math.min(1, (now - flight.start) / flight.duration) : 1;
    const eased = easeInOutCubic(progress);
    const current = this.camera.position.length();
    flight.fromDistance = eased < 0.999 ? (current - toDistance * eased) / (1 - eased) : current;
    flight.toDistance = toDistance;
  }

  cancel(): void {
    this.flight = null;
  }

  /** Advances the flight; returns true while it is still running. */
  step(now: number): boolean {
    const flight = this.flight;
    if (!flight) return false;
    const progress = flight.duration > 0 ? Math.min(1, (now - flight.start) / flight.duration) : 1;
    const eased = easeInOutCubic(progress);
    this.partial.copy(this.identity).slerp(flight.rotation, eased);
    this.direction.copy(flight.fromDirection).applyQuaternion(this.partial).normalize();
    this.camera.position.copy(
      this.direction.multiplyScalar(flight.fromDistance + (flight.toDistance - flight.fromDistance) * eased),
    );
    this.camera.lookAt(0, 0, 0);
    if (progress >= 1) {
      this.flight = null;
      return false;
    }
    return true;
  }
}
