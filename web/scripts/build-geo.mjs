import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { feature as topoFeature, transform as topoTransform } from "topojson-client";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const WEB_DIR = path.resolve(__dirname, "..");
const WORLD_ATLAS_DIR = path.join(WEB_DIR, "node_modules", "world-atlas");
const TOPO_PATH = path.join(WORLD_ATLAS_DIR, "countries-50m.json");
const WORLD_ATLAS_PKG_PATH = path.join(WORLD_ATLAS_DIR, "package.json");
const FIRMS_TABLE_PATH = path.join(WEB_DIR, "src", "lib", "geo", "firmsCountries.json");
const OUT_PATH = path.join(WEB_DIR, "public", "geo", "regions.json");

const TOLERANCE_DEFAULT_DEG = 0.02;
// Arcs used only by very large high-latitude countries can be coarser without visible loss.
const TOLERANCE_COARSE_DEG = 0.06;
const COARSE_ARC_CODES = new Set(["RUS", "CAN", "GRL", "USA", "AUS", "CHN", "BRA"]);
const MAX_BYTES = 700 * 1024;
const RUSSIA_CODE = "RUS";

// Antarctica is dropped: no burnable land, and in a regional conic it becomes a band that dwarfs the map.
const EXCLUDED_CODES = new Set(["ATA"]);

// world-atlas carries ISO 3166-1 numeric ids (France/Norway already fixed). These features have no
// numeric id, or share one with another feature; they get their Natural Earth ADM0_A3 code instead.
const CODE_BY_NAME = new Map([
  ["N. Cyprus", "CYN"],
  ["Kosovo", "KOS"],
  ["Somaliland", "SOL"],
  ["Indian Ocean Ter.", "IOA"],
  ["Siachen Glacier", "KAS"],
  ["Ashmore and Cartier Is.", "ATC"],
]);

// Natural Earth admin-0 keeps these overseas parts inside their parent country, while FIRMS publishes
// them as separate countries. Parent polygons whose bbox lies inside `box` become their own feature.
const OVERSEAS_SPLITS = [
  { parent: "FRA", code: "GUF", name: "French Guiana", box: [-55, 1.5, -51, 6.5] },
  { parent: "FRA", code: "GLP", name: "Guadeloupe", box: [-62, 15.8, -61, 16.6] },
  { parent: "FRA", code: "MTQ", name: "Martinique", box: [-61.3, 14.3, -60.7, 14.95] },
  { parent: "FRA", code: "REU", name: "Réunion", box: [55, -21.5, 56, -20.7] },
  { parent: "FRA", code: "MYT", name: "Mayotte", box: [44.9, -13.1, 45.4, -12.5] },
  { parent: "NOR", code: "SJM", name: "Svalbard and Jan Mayen", box: [10, 74, 35, 81] },
  { parent: "NOR", code: "SJM", name: "Svalbard and Jan Mayen", box: [-9.5, 70.5, -7.5, 71.5] },
];

// Complete ISO 3166-1 numeric -> alpha-3 table.
const ISO_NUMERIC_TO_ALPHA3 = {
  "004": "AFG", "008": "ALB", "010": "ATA", "012": "DZA", "016": "ASM",
  "020": "AND", "024": "AGO", "028": "ATG", "031": "AZE", "032": "ARG",
  "036": "AUS", "040": "AUT", "044": "BHS", "048": "BHR", "050": "BGD",
  "051": "ARM", "052": "BRB", "056": "BEL", "060": "BMU", "064": "BTN",
  "068": "BOL", "070": "BIH", "072": "BWA", "074": "BVT", "076": "BRA",
  "084": "BLZ", "086": "IOT", "090": "SLB", "092": "VGB", "096": "BRN",
  "100": "BGR", "104": "MMR", "108": "BDI", "112": "BLR", "116": "KHM",
  "120": "CMR", "124": "CAN", "132": "CPV", "136": "CYM", "140": "CAF",
  "144": "LKA", "148": "TCD", "152": "CHL", "156": "CHN", "158": "TWN",
  "162": "CXR", "166": "CCK", "170": "COL", "174": "COM", "175": "MYT",
  "178": "COG", "180": "COD", "184": "COK", "188": "CRI", "191": "HRV",
  "192": "CUB", "196": "CYP", "203": "CZE", "204": "BEN", "208": "DNK",
  "212": "DMA", "214": "DOM", "218": "ECU", "222": "SLV", "226": "GNQ",
  "231": "ETH", "232": "ERI", "233": "EST", "234": "FRO", "238": "FLK",
  "239": "SGS", "242": "FJI", "246": "FIN", "248": "ALA", "250": "FRA",
  "254": "GUF", "258": "PYF", "260": "ATF", "262": "DJI", "266": "GAB",
  "268": "GEO", "270": "GMB", "275": "PSE", "276": "DEU", "288": "GHA",
  "292": "GIB", "296": "KIR", "300": "GRC", "304": "GRL", "308": "GRD",
  "312": "GLP", "316": "GUM", "320": "GTM", "324": "GIN", "328": "GUY",
  "332": "HTI", "334": "HMD", "336": "VAT", "340": "HND", "344": "HKG",
  "348": "HUN", "352": "ISL", "356": "IND", "360": "IDN", "364": "IRN",
  "368": "IRQ", "372": "IRL", "376": "ISR", "380": "ITA", "384": "CIV",
  "388": "JAM", "392": "JPN", "398": "KAZ", "400": "JOR", "404": "KEN",
  "408": "PRK", "410": "KOR", "414": "KWT", "417": "KGZ", "418": "LAO",
  "422": "LBN", "426": "LSO", "428": "LVA", "430": "LBR", "434": "LBY",
  "438": "LIE", "440": "LTU", "442": "LUX", "446": "MAC", "450": "MDG",
  "454": "MWI", "458": "MYS", "462": "MDV", "466": "MLI", "470": "MLT",
  "474": "MTQ", "478": "MRT", "480": "MUS", "484": "MEX", "492": "MCO",
  "496": "MNG", "498": "MDA", "499": "MNE", "500": "MSR", "504": "MAR",
  "508": "MOZ", "512": "OMN", "516": "NAM", "520": "NRU", "524": "NPL",
  "528": "NLD", "531": "CUW", "533": "ABW", "534": "SXM", "535": "BES",
  "540": "NCL", "548": "VUT", "554": "NZL", "558": "NIC", "562": "NER",
  "566": "NGA", "570": "NIU", "574": "NFK", "578": "NOR", "580": "MNP",
  "581": "UMI", "583": "FSM", "584": "MHL", "585": "PLW", "586": "PAK",
  "591": "PAN", "598": "PNG", "600": "PRY", "604": "PER", "608": "PHL",
  "612": "PCN", "616": "POL", "620": "PRT", "624": "GNB", "626": "TLS",
  "630": "PRI", "634": "QAT", "638": "REU", "642": "ROU", "643": "RUS",
  "646": "RWA", "652": "BLM", "654": "SHN", "659": "KNA", "660": "AIA",
  "662": "LCA", "663": "MAF", "666": "SPM", "670": "VCT", "674": "SMR",
  "678": "STP", "682": "SAU", "686": "SEN", "688": "SRB", "690": "SYC",
  "694": "SLE", "702": "SGP", "703": "SVK", "704": "VNM", "705": "SVN",
  "706": "SOM", "710": "ZAF", "716": "ZWE", "724": "ESP", "728": "SSD",
  "729": "SDN", "732": "ESH", "740": "SUR", "744": "SJM", "748": "SWZ",
  "752": "SWE", "756": "CHE", "760": "SYR", "762": "TJK", "764": "THA",
  "768": "TGO", "772": "TKL", "776": "TON", "780": "TTO", "784": "ARE",
  "788": "TUN", "792": "TUR", "795": "TKM", "796": "TCA", "798": "TUV",
  "800": "UGA", "804": "UKR", "807": "MKD", "818": "EGY", "826": "GBR",
  "831": "GGY", "832": "JEY", "833": "IMN", "834": "TZA", "840": "USA",
  "850": "VIR", "854": "BFA", "858": "URY", "860": "UZB", "862": "VEN",
  "876": "WLF", "882": "WSM", "887": "YEM", "894": "ZMB",
};

function round2(value) {
  const rounded = Math.round(value * 100) / 100;
  return rounded === 0 ? 0 : rounded;
}

function sqPerpDist(p, a, b) {
  let x = a[0];
  let y = a[1];
  let dx = b[0] - x;
  let dy = b[1] - y;
  if (dx !== 0 || dy !== 0) {
    const t = ((p[0] - x) * dx + (p[1] - y) * dy) / (dx * dx + dy * dy);
    if (t > 1) {
      x = b[0];
      y = b[1];
    } else if (t > 0) {
      x += dx * t;
      y += dy * t;
    }
  }
  dx = p[0] - x;
  dy = p[1] - y;
  return dx * dx + dy * dy;
}

function douglasPeuckerOpen(points, tolerance) {
  const n = points.length;
  if (n <= 2) return points.slice();
  const sqTol = tolerance * tolerance;
  const keep = new Uint8Array(n);
  keep[0] = 1;
  keep[n - 1] = 1;
  const stack = [[0, n - 1]];

  while (stack.length > 0) {
    const [first, last] = stack.pop();
    let maxSqDist = 0;
    let maxIdx = -1;
    const a = points[first];
    const b = points[last];
    for (let i = first + 1; i < last; i++) {
      const d = sqPerpDist(points[i], a, b);
      if (d > maxSqDist) {
        maxSqDist = d;
        maxIdx = i;
      }
    }
    if (maxSqDist > sqTol && maxIdx !== -1) {
      keep[maxIdx] = 1;
      if (maxIdx - first > 1) stack.push([first, maxIdx]);
      if (last - maxIdx > 1) stack.push([maxIdx, last]);
    }
  }

  const out = [];
  for (let i = 0; i < n; i++) {
    if (keep[i]) out.push(points[i]);
  }
  return out;
}

function simplifyArc(points, tolerance) {
  const n = points.length;
  if (n <= 3) return points.slice();
  const first = points[0];
  const last = points[n - 1];
  const isClosed = first[0] === last[0] && first[1] === last[1];
  if (!isClosed) {
    return douglasPeuckerOpen(points, tolerance);
  }

  // Split closed ring at farthest vertex from start so DP has a non-zero baseline.
  let splitIdx = 0;
  let maxSqDist = -1;
  for (let i = 1; i < n - 1; i++) {
    const dx = points[i][0] - first[0];
    const dy = points[i][1] - first[1];
    const d = dx * dx + dy * dy;
    if (d > maxSqDist) {
      maxSqDist = d;
      splitIdx = i;
    }
  }
  if (splitIdx === 0) return points.slice();

  const part1 = douglasPeuckerOpen(points.slice(0, splitIdx + 1), tolerance);
  const part2 = douglasPeuckerOpen(points.slice(splitIdx), tolerance);
  return part1.slice(0, -1).concat(part2);
}

function collectGeometryArcs(geom, targetSet) {
  if (!geom) return;
  if (geom.type === "GeometryCollection") {
    for (const child of geom.geometries) collectGeometryArcs(child, targetSet);
    return;
  }
  const addArcRef = (ref) => targetSet.add(ref < 0 ? ~ref : ref);
  if (geom.type === "Polygon") {
    for (const ring of geom.arcs) {
      for (const ref of ring) addArcRef(ref);
    }
  } else if (geom.type === "MultiPolygon") {
    for (const poly of geom.arcs) {
      for (const ring of poly) {
        for (const ref of ring) addArcRef(ref);
      }
    }
  }
}

function ringSignedArea(ring) {
  let area = 0;
  for (let i = 0; i < ring.length - 1; i++) {
    const [x1, y1] = ring[i];
    const [x2, y2] = ring[i + 1];
    area += x1 * y2 - x2 * y1;
  }
  return area / 2;
}

function cleanRing(rawRing, unwrapAntimeridian) {
  const pts = [];
  let prevLon = null;
  for (const pos of rawRing) {
    let lon = pos[0];
    if (unwrapAntimeridian && lon < 0) {
      lon += 360;
    }
    // A few rings touch both +180 and -180 (Fiji); keep every ring continuous in longitude.
    if (prevLon !== null) {
      while (lon - prevLon > 180) lon -= 360;
      while (lon - prevLon < -180) lon += 360;
    }
    prevLon = lon;
    const rLon = round2(lon);
    const rLat = round2(pos[1]);
    const prev = pts[pts.length - 1];
    if (!prev || prev[0] !== rLon || prev[1] !== rLat) {
      pts.push([rLon, rLat]);
    }
  }
  if (pts.length === 0) return null;

  const first = pts[0];
  const last = pts[pts.length - 1];
  if (first[0] !== last[0] || first[1] !== last[1]) {
    pts.push([first[0], first[1]]);
  }
  if (pts.length < 4) return null;
  if (Math.abs(ringSignedArea(pts)) <= 1e-8) return null;
  return pts;
}

function normalizePolygons(geometry, unwrapAntimeridian) {
  if (!geometry) return [];
  const rawPolygons =
    geometry.type === "Polygon"
      ? [geometry.coordinates]
      : geometry.type === "MultiPolygon"
        ? geometry.coordinates
        : [];

  const polygons = [];
  for (const rawRings of rawPolygons) {
    if (!Array.isArray(rawRings) || rawRings.length === 0) continue;
    const outer = cleanRing(rawRings[0], unwrapAntimeridian);
    if (!outer) continue;
    const rings = [outer];
    for (let i = 1; i < rawRings.length; i++) {
      const hole = cleanRing(rawRings[i], unwrapAntimeridian);
      if (hole) rings.push(hole);
    }
    polygons.push(rings);
  }
  return polygons;
}

function ringBBox(ring) {
  let west = Infinity;
  let south = Infinity;
  let east = -Infinity;
  let north = -Infinity;
  for (const [lon, lat] of ring) {
    if (lon < west) west = lon;
    if (lat < south) south = lat;
    if (lon > east) east = lon;
    if (lat > north) north = lat;
  }
  return [west, south, east, north];
}

/**
 * Longitude extent is the shortest arc covering every polygon, so countries split by the
 * antimeridian (USA, Fiji, Kiribati, Russia) get `east > 180` instead of a -180..180 span that
 * would centre their regional projection on the wrong side of the globe.
 */
function featureBBox(polygons) {
  let south = Infinity;
  let north = -Infinity;
  const spans = [];
  for (const poly of polygons) {
    const [w, s, e, n] = ringBBox(poly[0]);
    if (s < south) south = s;
    if (n > north) north = n;
    const start = ((((w + 180) % 360) + 360) % 360) - 180;
    spans.push([start, start + (e - w)]);
  }
  spans.sort((a, b) => a[0] - b[0]);

  // The widest gap between consecutive (circularly merged) spans is the part of the globe to leave out.
  let reachEast = spans[0][1];
  let bestGap = spans[0][0] + 360 - Math.max(...spans.map((s) => s[1]));
  let west = spans[0][0];
  let east = Math.max(...spans.map((s) => s[1]));
  for (let i = 1; i < spans.length; i++) {
    const gap = spans[i][0] - reachEast;
    if (gap > bestGap) {
      bestGap = gap;
      west = spans[i][0];
      east = reachEast + 360;
    }
    if (spans[i][1] > reachEast) reachEast = spans[i][1];
  }
  return [round2(west), south, round2(east), north];
}

function bboxInside(inner, outer) {
  return inner[0] >= outer[0] && inner[1] >= outer[1] && inner[2] <= outer[2] && inner[3] <= outer[3];
}

function featureCode(geom) {
  const name = typeof geom.properties?.name === "string" ? geom.properties.name : "";
  const byName = CODE_BY_NAME.get(name);
  if (byName) return byName;
  if (geom.id == null) {
    throw new Error(`Feature "${name}" has no ISO numeric id and no CODE_BY_NAME entry.`);
  }
  const numeric = String(geom.id).padStart(3, "0");
  const code = ISO_NUMERIC_TO_ALPHA3[numeric];
  if (!code) throw new Error(`Feature "${name}" has unmapped ISO numeric id "${numeric}".`);
  return code;
}

function firmsIdByCode(table) {
  const byCode = new Map();
  for (const [firmsId, codes] of Object.entries(table.countries)) {
    for (const code of codes) {
      if (byCode.has(code)) {
        throw new Error(`ISO code ${code} is claimed by both ${byCode.get(code)} and ${firmsId}.`);
      }
      byCode.set(code, firmsId);
    }
  }
  return byCode;
}

async function main() {
  const [topoRaw, pkgRaw, firmsRaw] = await Promise.all([
    readFile(TOPO_PATH, "utf8"),
    readFile(WORLD_ATLAS_PKG_PATH, "utf8"),
    readFile(FIRMS_TABLE_PATH, "utf8"),
  ]);

  const topology = JSON.parse(topoRaw);
  const worldAtlasPkg = JSON.parse(pkgRaw);
  const firmsByCode = firmsIdByCode(JSON.parse(firmsRaw));
  const countriesObj = topology?.objects?.countries;

  if (!countriesObj || !Array.isArray(countriesObj.geometries)) {
    throw new Error("Invalid world-atlas TopoJSON: missing objects.countries.geometries");
  }

  const geometries = countriesObj.geometries
    .map((geom) => ({ geom, code: featureCode(geom) }))
    .filter(({ code }) => !EXCLUDED_CODES.has(code));

  const coarseArcs = new Set();
  const fineArcs = new Set();
  for (const { geom, code } of geometries) {
    collectGeometryArcs(geom, COARSE_ARC_CODES.has(code) ? coarseArcs : fineArcs);
  }

  // Simplify each shared arc once in world degrees to prevent boundary slivers between neighbours.
  const decodePoint = topoTransform(topology.transform);
  const decodedArcs = topology.arcs.map((arc) => arc.map((pt, i) => decodePoint(pt, i)));
  const simplifiedTopology = {
    ...topology,
    transform: undefined,
    arcs: decodedArcs.map((arc, idx) => {
      const coarse = coarseArcs.has(idx) && !fineArcs.has(idx);
      return simplifyArc(arc, coarse ? TOLERANCE_COARSE_DEG : TOLERANCE_DEFAULT_DEG);
    }),
  };
  const unsimplifiedTopology = { ...topology, transform: undefined, arcs: decodedArcs };

  const features = [];
  const seenCodes = new Set();
  const pushFeature = (code, name, polygons) => {
    if (seenCodes.has(code)) throw new Error(`Duplicate feature code "${code}" (feature "${name}").`);
    seenCodes.add(code);
    features.push({
      code,
      name,
      regionId: firmsByCode.get(code) ?? null,
      bbox: featureBBox(polygons),
      polygons,
    });
  };

  for (const { geom, code } of geometries) {
    const name = typeof geom.properties?.name === "string" ? geom.properties.name : "";
    const unwrap = code === RUSSIA_CODE;
    let polygons = normalizePolygons(topoFeature(simplifiedTopology, geom).geometry, unwrap);
    if (polygons.length === 0) {
      // Islands smaller than the tolerance collapse; keep them at full detail so they stay clickable.
      polygons = normalizePolygons(topoFeature(unsimplifiedTopology, geom).geometry, unwrap);
    }
    if (polygons.length === 0) {
      console.warn(`Skipping "${name}" (${code}): no polygon survives rounding.`);
      continue;
    }

    const splits = OVERSEAS_SPLITS.filter((s) => s.parent === code);
    if (splits.length > 0) {
      const parts = new Map();
      const home = [];
      for (const poly of polygons) {
        const box = ringBBox(poly[0]);
        const split = splits.find((s) => bboxInside(box, s.box));
        if (!split) {
          home.push(poly);
          continue;
        }
        const part = parts.get(split.code) ?? { name: split.name, polygons: [] };
        part.polygons.push(poly);
        parts.set(split.code, part);
      }
      for (const split of splits) {
        if (!parts.has(split.code)) {
          throw new Error(`Overseas split ${split.code} found no polygon inside ${code}.`);
        }
      }
      polygons = home;
      for (const [partCode, part] of parts) pushFeature(partCode, part.name, part.polygons);
    }

    pushFeature(code, name, polygons);
  }

  features.sort((a, b) => (a.code < b.code ? -1 : a.code > b.code ? 1 : 0));

  const withoutGeometry = [...firmsByCode.entries()]
    .filter(([code]) => !seenCodes.has(code))
    .map(([code, firmsId]) => `${firmsId} (${code})`);
  if (withoutGeometry.length > 0) {
    console.log(`FIRMS countries without geometry: ${withoutGeometry.join(", ")}`);
  }

  const dataset = {
    meta: {
      package: worldAtlasPkg.name,
      packageVersion: worldAtlasPkg.version,
      source: "Natural Earth",
      sourceVersion: "4.1.0",
      license: "ISC (world-atlas) / Public Domain (Natural Earth)",
      resolution: "1:50m",
      simplification: `topology-aware Douglas-Peucker ${TOLERANCE_DEFAULT_DEG}deg (${TOLERANCE_COARSE_DEG}deg for arcs of ${[...COARSE_ARC_CODES].join("/")}), rounded 2 decimals`,
    },
    features,
  };

  const json = JSON.stringify(dataset);
  const byteLength = Buffer.byteLength(json, "utf8");
  if (byteLength > MAX_BYTES) {
    throw new Error(
      `Generated regions.json is ${byteLength} bytes, exceeding ${MAX_BYTES} byte budget.`,
    );
  }

  await mkdir(path.dirname(OUT_PATH), { recursive: true });
  await writeFile(OUT_PATH, json, "utf8");

  const linked = features.filter((f) => f.regionId !== null).length;
  console.log(
    `Wrote ${features.length} features (${linked} linked to FIRMS ids, ${byteLength} bytes) to ${path.relative(WEB_DIR, OUT_PATH)}`,
  );
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err);
  process.exit(1);
});
