import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { feature as topoFeature, transform as topoTransform } from "topojson-client";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const WEB_DIR = path.resolve(__dirname, "..");
const WORLD_ATLAS_DIR = path.join(WEB_DIR, "node_modules", "world-atlas");
const TOPO_PATH = path.join(WORLD_ATLAS_DIR, "countries-50m.json");
const WORLD_ATLAS_PKG_PATH = path.join(WORLD_ATLAS_DIR, "package.json");
const OUT_PATH = path.join(WEB_DIR, "public", "geo", "regions.json");

const TOLERANCE_DEFAULT_DEG = 0.02;
const TOLERANCE_RUSSIA_DEG = 0.06;
const MAX_BYTES = 450 * 1024;
const CONTEXT_BBOX = [14, 22, 72, 60]; // [west, south, east, north]
const NORTH_CYPRUS_NAME = "N. Cyprus";
const NORTH_CYPRUS_CODE = "CYN";
const RUSSIA_NUMERIC_ID = "643";

const MAPPED_REGIONS = new Map([
  ["792", { code: "TUR", regionId: "Turkey" }],
  ["300", { code: "GRC", regionId: "Greece" }],
  ["100", { code: "BGR", regionId: "Bulgaria" }],
  ["196", { code: "CYP", regionId: "Cyprus" }],
  ["268", { code: "GEO", regionId: "Georgia" }],
  ["051", { code: "ARM", regionId: "Armenia" }],
  ["031", { code: "AZE", regionId: "Azerbaijan" }],
  ["364", { code: "IRN", regionId: "Iran" }],
  ["368", { code: "IRQ", regionId: "Iraq" }],
  ["760", { code: "SYR", regionId: "Syria" }],
  ["643", { code: "RUS", regionId: "Russian_Federation" }],
]);

// Complete ISO 3166-1 numeric -> alpha-3 table so context neighbours never use name matching.
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
  for (const pos of rawRing) {
    let lon = pos[0];
    if (unwrapAntimeridian && lon < 0) {
      lon += 360;
    }
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

function featureBBox(polygons) {
  let west = Infinity;
  let south = Infinity;
  let east = -Infinity;
  let north = -Infinity;
  for (const poly of polygons) {
    const [w, s, e, n] = ringBBox(poly[0]);
    if (w < west) west = w;
    if (s < south) south = s;
    if (e > east) east = e;
    if (n > north) north = n;
  }
  return [west, south, east, north];
}

function bboxIntersects(a, b) {
  return a[2] >= b[0] && a[0] <= b[2] && a[3] >= b[1] && a[1] <= b[3];
}

async function main() {
  const [topoRaw, pkgRaw] = await Promise.all([
    readFile(TOPO_PATH, "utf8"),
    readFile(WORLD_ATLAS_PKG_PATH, "utf8"),
  ]);

  const topology = JSON.parse(topoRaw);
  const worldAtlasPkg = JSON.parse(pkgRaw);
  const countriesObj = topology?.objects?.countries;

  if (!countriesObj || !Array.isArray(countriesObj.geometries)) {
    throw new Error("Invalid world-atlas TopoJSON: missing objects.countries.geometries");
  }

  const russiaArcs = new Set();
  const otherArcs = new Set();
  for (const geom of countriesObj.geometries) {
    const id = geom.id == null ? null : String(geom.id).padStart(3, "0");
    collectGeometryArcs(geom, id === RUSSIA_NUMERIC_ID ? russiaArcs : otherArcs);
  }

  // Simplify each shared arc once in world degrees to prevent boundary slivers between neighbours.
  const decodePoint = topoTransform(topology.transform);
  const simplifiedArcs = topology.arcs.map((arc, idx) => {
    const decoded = arc.map((pt, pointIdx) => decodePoint(pt, pointIdx));
    const isRussiaOnly = russiaArcs.has(idx) && !otherArcs.has(idx);
    const tol = isRussiaOnly ? TOLERANCE_RUSSIA_DEG : TOLERANCE_DEFAULT_DEG;
    return simplifyArc(decoded, tol);
  });

  const simplifiedTopology = {
    ...topology,
    transform: undefined,
    arcs: simplifiedArcs,
  };

  const features = [];
  const foundMappedIds = new Set();
  const seenCodes = new Set();
  let foundNorthCyprus = false;
  let unknownContextIndex = 0;

  for (const geom of countriesObj.geometries) {
    const rawId = geom.id == null ? null : String(geom.id).padStart(3, "0");
    const name = typeof geom.properties?.name === "string" ? geom.properties.name : "";

    const mapped = rawId !== null ? MAPPED_REGIONS.get(rawId) : undefined;
    const isNorthCyprus = rawId === null && name === NORTH_CYPRUS_NAME;
    const isRussia = rawId === RUSSIA_NUMERIC_ID;

    const geojsonFeature = topoFeature(simplifiedTopology, geom);
    const polygons = normalizePolygons(geojsonFeature.geometry, isRussia);

    if (mapped) {
      if (polygons.length === 0) {
        throw new Error(`Mapped region ${mapped.regionId} (id=${rawId}) has no valid polygons.`);
      }
      foundMappedIds.add(rawId);
      if (seenCodes.has(mapped.code)) {
        throw new Error(`Duplicate feature code "${mapped.code}".`);
      }
      seenCodes.add(mapped.code);
      features.push({
        code: mapped.code,
        name,
        regionId: mapped.regionId,
        bbox: featureBBox(polygons),
        polygons,
      });
      continue;
    }

    if (isNorthCyprus) {
      if (polygons.length === 0) {
        throw new Error('Cyprus exception feature "N. Cyprus" has no valid polygons.');
      }
      foundNorthCyprus = true;
      if (seenCodes.has(NORTH_CYPRUS_CODE)) {
        throw new Error(`Duplicate feature code "${NORTH_CYPRUS_CODE}".`);
      }
      seenCodes.add(NORTH_CYPRUS_CODE);
      features.push({
        code: NORTH_CYPRUS_CODE,
        name,
        regionId: "Cyprus",
        bbox: featureBBox(polygons),
        polygons,
      });
      continue;
    }

    if (polygons.length === 0) continue;

    // Test per-polygon bbox so transoceanic territories (e.g. USA/France) do not span the context box.
    const intersectsContext = polygons.some((poly) =>
      bboxIntersects(ringBBox(poly[0]), CONTEXT_BBOX),
    );
    if (!intersectsContext) continue;

    let code;
    if (rawId !== null) {
      code = ISO_NUMERIC_TO_ALPHA3[rawId];
      if (!code) {
        throw new Error(
          `Context feature "${name}" has unmapped ISO numeric id "${rawId}".`,
        );
      }
    } else {
      unknownContextIndex += 1;
      code = `X${unknownContextIndex}`;
      console.warn(
        `Context-only feature without ISO id inside context window: "${name}" -> assigned code "${code}".`,
      );
    }

    if (seenCodes.has(code)) {
      throw new Error(`Duplicate feature code "${code}" (feature "${name}").`);
    }
    seenCodes.add(code);

    features.push({
      code,
      name,
      regionId: null,
      bbox: featureBBox(polygons),
      polygons,
    });
  }

  for (const [id, entry] of MAPPED_REGIONS.entries()) {
    if (!foundMappedIds.has(id)) {
      throw new Error(
        `Missing required mapped region "${entry.regionId}" (ISO numeric id "${id}").`,
      );
    }
  }
  if (!foundNorthCyprus) {
    throw new Error(`Missing required Cyprus exception feature "${NORTH_CYPRUS_NAME}".`);
  }

  features.sort((a, b) => (a.code < b.code ? -1 : a.code > b.code ? 1 : 0));

  const dataset = {
    meta: {
      package: worldAtlasPkg.name,
      packageVersion: worldAtlasPkg.version,
      source: "Natural Earth",
      sourceVersion: "4.1.0",
      license: "ISC (world-atlas) / Public Domain (Natural Earth)",
      resolution: "1:50m",
      simplification: `topology-aware Douglas-Peucker ${TOLERANCE_DEFAULT_DEG}deg (${TOLERANCE_RUSSIA_DEG}deg RUS), rounded 2 decimals`,
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

  console.log(
    `Wrote ${features.length} features (${byteLength} bytes) to ${path.relative(WEB_DIR, OUT_PATH)}`,
  );
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err);
  process.exit(1);
});
