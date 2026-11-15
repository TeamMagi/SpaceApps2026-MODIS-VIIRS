// Decorative sun-synchronous-looking rings; an illustration, not real satellite tracks.

import * as THREE from "three";

const DEG = Math.PI / 180;

const ORBITS = [
  { radius: 1.13, inclination: 98.2, raan: 20, phase: 0, speed: 0.07 },
  { radius: 1.15, inclination: 98.7, raan: 75, phase: 2.1, speed: 0.06 },
  { radius: 1.17, inclination: 98.7, raan: 130, phase: 4.2, speed: 0.065 },
] as const;

interface Satellite {
  readonly mesh: THREE.Mesh;
  readonly radius: number;
  angle: number;
  readonly speed: number;
}

export interface OrbitColors {
  readonly line: string;
  readonly body: string;
  readonly dark: boolean;
}

export class Orbits {
  readonly group = new THREE.Group();
  private readonly lineMaterial: THREE.LineDashedMaterial;
  private readonly bodyMaterial: THREE.MeshBasicMaterial;
  private readonly satellites: Satellite[] = [];
  private readonly geometries: THREE.BufferGeometry[] = [];
  private lineOpacity = 0.5;
  private fade = 1;

  constructor() {
    this.group.name = "satellite-orbits";
    this.lineMaterial = new THREE.LineDashedMaterial({
      dashSize: 0.025,
      gapSize: 0.022,
      transparent: true,
      depthWrite: false,
    });
    this.bodyMaterial = new THREE.MeshBasicMaterial({ transparent: true, depthWrite: false });

    for (const orbit of ORBITS) {
      const ring = new THREE.Group();
      ring.rotation.order = "YXZ";
      ring.rotation.y = orbit.raan * DEG;
      ring.rotation.x = (orbit.inclination - 90) * DEG;

      const points: THREE.Vector3[] = [];
      for (let step = 0; step <= 180; step += 1) {
        const angle = (step / 180) * Math.PI * 2;
        points.push(new THREE.Vector3(orbit.radius * Math.cos(angle), orbit.radius * Math.sin(angle), 0));
      }
      const geometry = new THREE.BufferGeometry().setFromPoints(points);
      const path = new THREE.Line(geometry, this.lineMaterial);
      path.computeLineDistances();

      const bodyGeometry = new THREE.BoxGeometry(0.018, 0.018, 0.018);
      const panelGeometry = new THREE.BoxGeometry(0.05, 0.003, 0.014);
      const body = new THREE.Mesh(bodyGeometry, this.bodyMaterial);
      body.add(new THREE.Mesh(panelGeometry, this.bodyMaterial));
      body.position.set(orbit.radius * Math.cos(orbit.phase), orbit.radius * Math.sin(orbit.phase), 0);

      this.geometries.push(geometry, bodyGeometry, panelGeometry);
      ring.add(path, body);
      this.group.add(ring);
      this.satellites.push({ mesh: body, radius: orbit.radius, angle: orbit.phase, speed: orbit.speed });
    }
  }

  update(delta: number): void {
    for (const satellite of this.satellites) {
      satellite.angle += delta * satellite.speed;
      satellite.mesh.position.set(
        satellite.radius * Math.cos(satellite.angle),
        satellite.radius * Math.sin(satellite.angle),
        0,
      );
      satellite.mesh.rotation.z = satellite.angle;
    }
  }

  setColors(colors: OrbitColors): void {
    this.lineMaterial.color.set(colors.line);
    this.bodyMaterial.color.set(colors.body);
    this.lineOpacity = colors.dark ? 0.5 : 0.45;
    this.setFade(this.fade);
  }

  /** 0..1 visibility; close to the surface the rings would sweep through the view, so they fade. */
  setFade(fade: number): void {
    this.fade = fade;
    this.group.visible = fade > 0.01;
    this.lineMaterial.opacity = this.lineOpacity * fade;
    this.bodyMaterial.opacity = 0.95 * fade;
  }

  /** True once the rings are clear enough on screen to warrant their legend note. */
  get noticeable(): boolean {
    return this.fade >= 0.3;
  }

  dispose(): void {
    for (const geometry of this.geometries) geometry.dispose();
    this.lineMaterial.dispose();
    this.bodyMaterial.dispose();
  }
}
