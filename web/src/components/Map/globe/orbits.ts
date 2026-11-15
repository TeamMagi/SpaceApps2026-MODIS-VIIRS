// Representative tracks of the five satellites behind the FIRMS data; an illustration, not real ephemerides.

import * as THREE from "three";

const DEG = Math.PI / 180;

const MODIS_COLOR = "#ff9a3c";
const VIIRS_COLOR = "#3fd8e8";

/** Shared visual angular speed (rad/s); every satellite moves alike, only the phases differ. */
const SPEED = 0.065;

interface PlaneSpec {
  readonly radius: number;
  /** Ascending-node longitude (deg). Every plane passes through both poles. */
  readonly raan: number;
}

// Five meridian planes, 36° apart (180° / 5), so each satellite gets its own great circle over the poles.
const PLANES: readonly PlaneSpec[] = [
  { radius: 1.14, raan: 0 },
  { radius: 1.14, raan: 36 },
  { radius: 1.16, raan: 72 },
  { radius: 1.16, raan: 108 },
  { radius: 1.16, raan: 144 },
];

interface SatelliteSpec {
  readonly name: string;
  readonly sensor: "modis" | "viirs";
  readonly plane: number;
  readonly phase: number;
}

// Phases are staggered so the craft never bunch up over one pole at the same moment.
const SATELLITES: readonly SatelliteSpec[] = [
  { name: "Terra", sensor: "modis", plane: 0, phase: 0 },
  { name: "Aqua", sensor: "modis", plane: 1, phase: Math.PI * 0.4 },
  { name: "S-NPP", sensor: "viirs", plane: 2, phase: Math.PI * 0.8 },
  { name: "NOAA-20", sensor: "viirs", plane: 3, phase: Math.PI * 1.2 },
  { name: "NOAA-21", sensor: "viirs", plane: 4, phase: Math.PI * 1.6 },
];

interface Satellite {
  readonly anchor: THREE.Group;
  readonly icon: THREE.Group;
  readonly radius: number;
  angle: number;
}

export interface OrbitColors {
  readonly line: string;
  readonly body: string;
  readonly dark: boolean;
}

export class Orbits {
  readonly group = new THREE.Group();
  private readonly lineMaterial: THREE.LineDashedMaterial;
  private readonly bodyMaterials: Record<"modis" | "viirs", THREE.MeshBasicMaterial>;
  private readonly satellites: Satellite[] = [];
  private readonly geometries: THREE.BufferGeometry[] = [];
  private lineOpacity = 0.3;
  private fade = 1;

  constructor() {
    this.group.name = "satellite-orbits";
    this.lineMaterial = new THREE.LineDashedMaterial({
      dashSize: 0.025,
      gapSize: 0.022,
      transparent: true,
      depthWrite: false,
    });
    this.bodyMaterials = {
      modis: new THREE.MeshBasicMaterial({ color: MODIS_COLOR, transparent: true, depthWrite: false }),
      viirs: new THREE.MeshBasicMaterial({ color: VIIRS_COLOR, transparent: true, depthWrite: false }),
    };

    const bodyGeometry = new THREE.BoxGeometry(0.02, 0.02, 0.02);
    const panelGeometry = new THREE.BoxGeometry(0.058, 0.004, 0.016);
    this.geometries.push(bodyGeometry, panelGeometry);

    const rings: THREE.Group[] = PLANES.map((plane) => {
      const ring = new THREE.Group();
      ring.rotation.y = plane.raan * DEG;

      const points: THREE.Vector3[] = [];
      for (let step = 0; step <= 180; step += 1) {
        const angle = (step / 180) * Math.PI * 2;
        points.push(new THREE.Vector3(plane.radius * Math.cos(angle), plane.radius * Math.sin(angle), 0));
      }
      const geometry = new THREE.BufferGeometry().setFromPoints(points);
      const path = new THREE.Line(geometry, this.lineMaterial);
      path.computeLineDistances();
      this.geometries.push(geometry);
      ring.add(path);
      this.group.add(ring);
      return ring;
    });

    for (const spec of SATELLITES) {
      const radius = PLANES[spec.plane].radius;
      const material = this.bodyMaterials[spec.sensor];
      const icon = new THREE.Group();
      icon.add(new THREE.Mesh(bodyGeometry, material), new THREE.Mesh(panelGeometry, material));

      const anchor = new THREE.Group();
      anchor.add(icon);
      anchor.position.set(radius * Math.cos(spec.phase), radius * Math.sin(spec.phase), 0);
      icon.rotation.z = spec.phase;
      rings[spec.plane].add(anchor);
      this.satellites.push({ anchor, icon, radius, angle: spec.phase });
    }
  }

  update(delta: number): void {
    for (const satellite of this.satellites) {
      satellite.angle += delta * SPEED;
      satellite.anchor.position.set(
        satellite.radius * Math.cos(satellite.angle),
        satellite.radius * Math.sin(satellite.angle),
        0,
      );
      satellite.icon.rotation.z = satellite.angle;
    }
  }

  setColors(colors: OrbitColors): void {
    this.lineMaterial.color.set(colors.line);
    this.lineOpacity = colors.dark ? 0.3 : 0.27;
    this.setFade(this.fade);
  }

  /** 0..1 visibility; close to the surface the rings would sweep through the view, so they fade. */
  setFade(fade: number): void {
    this.fade = fade;
    this.group.visible = fade > 0.01;
    this.lineMaterial.opacity = this.lineOpacity * fade;
    this.bodyMaterials.modis.opacity = 0.95 * fade;
    this.bodyMaterials.viirs.opacity = 0.95 * fade;
  }

  /** True once the rings are clear enough on screen to warrant their legend note. */
  get noticeable(): boolean {
    return this.fade >= 0.3;
  }

  dispose(): void {
    for (const geometry of this.geometries) geometry.dispose();
    this.lineMaterial.dispose();
    this.bodyMaterials.modis.dispose();
    this.bodyMaterials.viirs.dispose();
  }
}
