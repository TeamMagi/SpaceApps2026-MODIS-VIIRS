import { DayNight, type RawPoint } from "../types";

/**
 * Pure geometry and detection-threshold math for the MODIS vs VIIRS Footprint Lens (C8).
 * Numbers follow docs/REFERANS_ANALIZI.md P2/P3:
 * - MODIS pixel: 1.0 km² at nadir -> 9.66–9.69 km² at the 55° scan edge [01, 16].
 * - VIIRS 375 m I-band pixel: 0.14–0.62 km² across its swath [09].
 * - Minimum detectable FRP: MODIS 5.7 -> 40 MW (day, nadir -> edge), VIIRS 0.7 -> 3.7 MW (night -> day) [16, Conclusions; Sect. 5.2].
 *   These are the paper's quoted typical values, not Eq. 5 outputs (Eq. 5 + Tables 1–2 give 4.96 -> 43.41 MW for MODIS day).
 */

export const EARTH_RADIUS_KM = 6371;
export const MODIS_ORBIT_ALTITUDE_KM = 705;

export const MODIS_NADIR_PIXEL_KM = 1;
export const MODIS_NADIR_AREA_KM2 = 1;
export const MODIS_EDGE_SCAN_ANGLE_DEG = 55;
export const MODIS_MID_SCAN_ANGLE_DEG = 35;

export const VIIRS_PIXEL_KM = 0.375;
export const VIIRS_NADIR_AREA_KM2 = VIIRS_PIXEL_KM * VIIRS_PIXEL_KM;
export const VIIRS_EDGE_AREA_KM2 = 0.62;

/** Area ratio flare-chronos reports for its Footprint Lens; shown as a reference, never used in math. */
export const FLARE_CHRONOS_AREA_RATIO = 74.23;

/** Paper's quoted typical values [16, Conclusions; Sect. 5.2], used only for the Footprint Lens display. */
export const MODIS_THRESHOLD_DAY_NADIR_MW = 5.7;
export const MODIS_THRESHOLD_DAY_EDGE_MW = 40;
export const MODIS_THRESHOLD_NIGHT_NADIR_MW = 3.7;
export const MODIS_THRESHOLD_NIGHT_EDGE_MW = 39;
export const VIIRS_THRESHOLD_NIGHT_MW = 0.7;
export const VIIRS_THRESHOLD_DAY_MW = 3.7;
export const THRESHOLD_SCALE_MAX_MW = 42;

/** Eq. 5 (D_p = lambda * A_p + mu, 50 % detection mid-point) at 1 km²; mirrors vza.rs / fusion.rs. */
export const MODIS_NADIR_D50_DAY_MW = 4.44 * 1.0 + 0.52;
export const MODIS_NADIR_D50_NIGHT_MW = 4.43 * 1.0 + 1.01;

/** Same value as `MATCH_WINDOW_SECS` in crates/core/src/harmonize.rs (180 min). */
export const MATCH_WINDOW_SECS = 10_800;

/** Default `VzaConfig` in crates/core/src/config.rs: R_rel(θ) = a·θ² + b·θ + 1 (θ in radians). */
const VZA_COEFF_A = 0.2017;
const VZA_COEFF_B = 0.0168;

const KM_PER_DEG_LAT = 111.32;
const ORBIT_RATIO = (EARTH_RADIUS_KM + MODIS_ORBIT_ALTITUDE_KM) / EARTH_RADIUS_KM;
const DEG = Math.PI / 180;

/** Where the footprint size came from: FIRMS scan/track, the point's VZA factor, or a swath-position model. */
export type FootprintSource = "measured" | "vza" | "model";

export interface ModisFootprint {
  /** Along-scan size (km); roughly east–west for the near-polar Terra/Aqua orbits. */
  readonly scanKm: number;
  /** Along-track size (km); roughly north–south. */
  readonly trackKm: number;
  readonly areaKm2: number;
  readonly scanAngleDeg: number;
  readonly vzaDeg: number;
  readonly source: FootprintSource;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

function isPositive(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

/** Scan angle β (0–55°) to view zenith angle on a spherical Earth: sin(VZA) = (R + h) / R · sin(β). */
export function scanAngleToVzaDeg(scanAngleDeg: number): number {
  const beta = clamp(scanAngleDeg, 0, MODIS_EDGE_SCAN_ANGLE_DEG) * DEG;
  return Math.asin(clamp(ORBIT_RATIO * Math.sin(beta), -1, 1)) / DEG;
}

export const MODIS_EDGE_VZA_DEG = scanAngleToVzaDeg(MODIS_EDGE_SCAN_ANGLE_DEG);

export function vzaToScanAngleDeg(vzaDeg: number): number {
  const theta = clamp(vzaDeg, 0, MODIS_EDGE_VZA_DEG) * DEG;
  return clamp(Math.asin(Math.sin(theta) / ORBIT_RATIO) / DEG, 0, MODIS_EDGE_SCAN_ANGLE_DEG);
}

/** Inverts R_rel(θ) back to VZA in degrees; factors at or below 1 mean nadir. */
export function vzaDegFromFactor(vzaFactor: number): number {
  if (!Number.isFinite(vzaFactor) || vzaFactor <= 1) return 0;
  const discriminant = VZA_COEFF_B * VZA_COEFF_B + 4 * VZA_COEFF_A * (vzaFactor - 1);
  const thetaRad = (-VZA_COEFF_B + Math.sqrt(discriminant)) / (2 * VZA_COEFF_A);
  return clamp(thetaRad / DEG, 0, MODIS_EDGE_VZA_DEG);
}

/**
 * MODIS pixel size at scan angle β (Ichoku & Kaufman 2005, the same relation `vza.rs` uses):
 * track = p · slant / h and scan = track / cos(VZA), with p = 1 km at nadir.
 * Nadir gives 1 × 1 km; the 55° edge gives ~4.8 × 2.0 km ≈ 9.7 km².
 */
export function modisFootprintAtScanAngle(
  scanAngleDeg: number,
  source: FootprintSource = "model",
): ModisFootprint {
  const betaDeg = clamp(scanAngleDeg, 0, MODIS_EDGE_SCAN_ANGLE_DEG);
  const vzaDeg = scanAngleToVzaDeg(betaDeg);
  const beta = betaDeg * DEG;
  const vza = vzaDeg * DEG;
  const slantKm =
    (EARTH_RADIUS_KM + MODIS_ORBIT_ALTITUDE_KM) * Math.cos(beta) - EARTH_RADIUS_KM * Math.cos(vza);
  const trackKm = (MODIS_NADIR_PIXEL_KM * slantKm) / MODIS_ORBIT_ALTITUDE_KM;
  const scanKm = trackKm / Math.cos(vza);
  return { scanKm, trackKm, areaKm2: scanKm * trackKm, scanAngleDeg: betaDeg, vzaDeg, source };
}

/** Scan angle whose modelled along-scan size equals `scanKm` (bisection, like `vza_from_scan`). */
export function scanAngleFromScanKm(scanKm: number): number {
  if (!isPositive(scanKm) || scanKm <= MODIS_NADIR_PIXEL_KM) return 0;
  let low = 0;
  let high = MODIS_EDGE_SCAN_ANGLE_DEG;
  if (scanKm >= modisFootprintAtScanAngle(high).scanKm) return high;
  for (let i = 0; i < 50; i++) {
    const mid = (low + high) / 2;
    if (modisFootprintAtScanAngle(mid).scanKm < scanKm) low = mid;
    else high = mid;
  }
  return (low + high) / 2;
}

export interface ModisFootprintInput {
  readonly scanKm?: number | null;
  readonly trackKm?: number | null;
  readonly vzaFactor?: number | null;
  /** Swath position used when neither scan/track nor a VZA factor is known. */
  readonly fallbackScanAngleDeg?: number;
}

/** FIRMS scan × track when present, else the VZA factor, else the swath model at the fallback angle. */
export function computeModisFootprint(input: ModisFootprintInput = {}): ModisFootprint {
  const { scanKm, trackKm, vzaFactor, fallbackScanAngleDeg } = input;
  if (isPositive(scanKm) && isPositive(trackKm)) {
    const scanAngleDeg = scanAngleFromScanKm(scanKm);
    return {
      scanKm,
      trackKm,
      areaKm2: scanKm * trackKm,
      scanAngleDeg,
      vzaDeg: scanAngleToVzaDeg(scanAngleDeg),
      source: "measured",
    };
  }
  if (isPositive(vzaFactor)) {
    return modisFootprintAtScanAngle(vzaToScanAngleDeg(vzaDegFromFactor(vzaFactor)), "vza");
  }
  return modisFootprintAtScanAngle(fallbackScanAngleDeg ?? MODIS_EDGE_SCAN_ANGLE_DEG, "model");
}

export interface ViirsTiling {
  /** MODIS footprint area / one 375 m VIIRS pixel (0.140625 km²). */
  readonly areaRatio: number;
  /** `areaRatio` rounded: how many 375 m pixels tile the footprint. */
  readonly pixelCount: number;
  /** Same ratio against the largest (swath-edge, 0.62 km²) VIIRS pixel. */
  readonly areaRatioVsViirsEdge: number;
  /** Grid columns / rows of 375 m cells needed to cover the footprint (last ones may be partial). */
  readonly cols: number;
  readonly rows: number;
}

export function computeViirsTiling(footprint: Pick<ModisFootprint, "scanKm" | "trackKm" | "areaKm2">): ViirsTiling {
  const areaRatio = footprint.areaKm2 / VIIRS_NADIR_AREA_KM2;
  return {
    areaRatio,
    pixelCount: Math.max(1, Math.round(areaRatio)),
    areaRatioVsViirsEdge: footprint.areaKm2 / VIIRS_EDGE_AREA_KM2,
    cols: Math.max(1, Math.ceil(footprint.scanKm / VIIRS_PIXEL_KM - 1e-9)),
    rows: Math.max(1, Math.ceil(footprint.trackKm / VIIRS_PIXEL_KM - 1e-9)),
  };
}

export interface DetectionThresholds {
  /** MODIS minimum detectable FRP at this pixel area (MW). */
  readonly modisMw: number;
  readonly modisNadirMw: number;
  readonly modisEdgeMw: number;
  /** VIIRS 375 m minimum detectable FRP for this pass (MW): 3.7 by day, 0.7 by night. */
  readonly viirsMw: number;
  readonly viirsMinMw: number;
  readonly viirsMaxMw: number;
  /** modisMw / viirsMw. */
  readonly ratio: number;
}

/**
 * MODIS limit grows linearly with pixel area between the [16] nadir and edge values,
 * which matches the linear form D_p(A_p) = λ·A_p + μ of [16, Eq. 5].
 */
export function detectionThresholds(areaKm2: number, daynight: DayNight): DetectionThresholds {
  const edgeArea = modisFootprintAtScanAngle(MODIS_EDGE_SCAN_ANGLE_DEG).areaKm2;
  const t = clamp((areaKm2 - MODIS_NADIR_AREA_KM2) / (edgeArea - MODIS_NADIR_AREA_KM2), 0, 1);
  const night = daynight === DayNight.Night;
  const modisNadirMw = night ? MODIS_THRESHOLD_NIGHT_NADIR_MW : MODIS_THRESHOLD_DAY_NADIR_MW;
  const modisEdgeMw = night ? MODIS_THRESHOLD_NIGHT_EDGE_MW : MODIS_THRESHOLD_DAY_EDGE_MW;
  const modisMw = modisNadirMw + t * (modisEdgeMw - modisNadirMw);
  const viirsMw = night ? VIIRS_THRESHOLD_NIGHT_MW : VIIRS_THRESHOLD_DAY_MW;
  return {
    modisMw,
    modisNadirMw,
    modisEdgeMw,
    viirsMw,
    viirsMinMw: VIIRS_THRESHOLD_NIGHT_MW,
    viirsMaxMw: VIIRS_THRESHOLD_DAY_MW,
    ratio: modisMw / viirsMw,
  };
}

/** Position of a threshold on the shared 0–42 MW bar, in [0, 1]. */
export function thresholdFraction(mw: number): number {
  return clamp(mw / THRESHOLD_SCALE_MAX_MW, 0, 1);
}

export interface FootprintCenter {
  readonly lat: number;
  readonly lon: number;
  readonly acquired: string;
}

export interface ViirsInFootprint {
  readonly point: RawPoint;
  /** Offset from the MODIS pixel centre, km (east +, north +). */
  readonly dxKm: number;
  readonly dyKm: number;
  readonly inside: boolean;
  /** Fused into the same harmonized point; only ever true when the caller passed `linkedHarmonizedId`. */
  readonly linked: boolean;
}

export interface ViirsSearchOptions {
  /** Pass only when the harmonized answer may be shown; Raw mode leaves it unset. */
  readonly linkedHarmonizedId?: string | null;
  readonly windowSecs?: number;
}

/**
 * VIIRS detections inside the MODIS scan × track box (scan east–west, track north–south) and
 * within the match window, plus, when `linkedHarmonizedId` is given, the ones fused with it.
 */
export function findViirsInFootprint(
  center: FootprintCenter,
  footprint: Pick<ModisFootprint, "scanKm" | "trackKm">,
  rawPoints: readonly RawPoint[],
  options: ViirsSearchOptions = {},
): ViirsInFootprint[] {
  const linkedId = options.linkedHarmonizedId ?? null;
  const windowMs = (options.windowSecs ?? MATCH_WINDOW_SECS) * 1000;
  const centerMs = Date.parse(center.acquired);
  const kmPerDegLon = KM_PER_DEG_LAT * Math.max(0.01, Math.cos(center.lat * DEG));
  const halfScan = footprint.scanKm / 2;
  const halfTrack = footprint.trackKm / 2;

  const found: ViirsInFootprint[] = [];
  for (const point of rawPoints) {
    if (point.sensor !== "VIIRS") continue;
    const linked = linkedId !== null && point.harmonizedId === linkedId;
    let lonDelta = point.lon - center.lon;
    if (lonDelta > 180) lonDelta -= 360;
    else if (lonDelta < -180) lonDelta += 360;
    const dxKm = lonDelta * kmPerDegLon;
    const dyKm = (point.lat - center.lat) * KM_PER_DEG_LAT;
    const pointMs = Date.parse(point.acquired);
    const inWindow =
      !Number.isFinite(centerMs) || !Number.isFinite(pointMs) || Math.abs(pointMs - centerMs) <= windowMs;
    const inside = inWindow && Math.abs(dxKm) <= halfScan && Math.abs(dyKm) <= halfTrack;
    if (inside || linked) found.push({ point, dxKm, dyKm, inside, linked });
  }
  found.sort((a, b) => Math.hypot(a.dxKm, a.dyKm) - Math.hypot(b.dxKm, b.dyKm));
  return found;
}

/** The MODIS detection fused into a harmonized point, to centre the lens on the actual MODIS pixel. */
export function findModisMember(harmonizedId: string, rawPoints: readonly RawPoint[]): RawPoint | null {
  for (const point of rawPoints) {
    if (point.sensor === "MODIS" && point.harmonizedId === harmonizedId) return point;
  }
  return null;
}
