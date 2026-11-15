// Reproducible UI fixtures, not outputs of the scientific Rust pipeline.
import { mkdir, writeFile } from 'node:fs/promises';
const dir = new URL('../src/mocks/', import.meta.url);
await mkdir(dir, { recursive: true });
const seed = 20261115;
let state = seed;
const random = () => ((state = (Math.imul(state, 1664525) + 1013904223) >>> 0) / 4294967296);
const round = n => Math.round(n * 1000) / 1000;
const SYNTHETIC_VIIRS_TO_MODIS_EQ = 0.35;
const season = [1, 1, 2, 3, 5, 9, 16, 18, 10, 5, 2, 1];
const monthPool = season.flatMap((weight, i) => Array(weight).fill(i + 1));
const raw = [], harmonized = [], rawPlatforms = {};
const platforms = { MODIS: ['ModisTerra', 'ModisAqua'], VIIRS: ['ViirsSNPP', 'ViirsNOAA20'] };
function addRaw(i, month, lat, lon, sensor, target, offset = 0) {
  const id = `tr-2021-r${raw.length}`;
  const platform = platforms[sensor][(i + offset) % 2];
  rawPlatforms[id] = platform;
  const point = { id, lat: round(lat), lon: round(lon), sensor, frp: round(1 + random() ** 2 * 399 * (sensor === 'VIIRS' ? SYNTHETIC_VIIRS_TO_MODIS_EQ : 1)),
    confidence: i % 7 === 0 ? 'High' : 'Nominal', acquired: `2021-${String(month).padStart(2, '0')}-${String(1 + i % 25).padStart(2, '0')}T${sensor === 'MODIS' ? '10' : '12'}:00:00Z`,
    daynight: 'Day', harmonizedId: target };
  raw.push(point);
  return point;
}
for (let i = 0; i < 500; i++) {
  const month = monthPool[Math.floor(random() * monthPool.length)];
  const lat = 36.5 + random() * 0.6, lon = 29 + random() * 2;
  const source = i % 5 === 0 ? 'Matched' : i % 5 < 3 ? 'ModisOnly' : 'ViirsOnly';
  const id = `tr-2021-h${i}`;
  const family = source === 'ViirsOnly' ? 'VIIRS' : 'MODIS';
  const members = [addRaw(i, month, lat, lon, family, id)];
  if (source === 'Matched') members.push(addRaw(i, month, lat, lon, 'VIIRS', id));
  if (i % 5 === 1) members.push(addRaw(i, month, lat, lon, family, id, 1));
  if (i % 5 === 2) {
    const duplicate = addRaw(i, month, lat, lon, family, null);
    duplicate.frp = members[0].frp;
  }
  const kept = members.at(-1);
  const corrected = source === 'ModisOnly' ? members.reduce((s, p) => s + p.frp * 1.08, 0) / members.length : kept.frp / SYNTHETIC_VIIRS_TO_MODIS_EQ;
  harmonized.push({ id, lat: kept.lat, lon: kept.lon, frp: kept.frp, frpCorrected: round(corrected), source,
    confidence: kept.confidence, acquired: kept.acquired, daynight: kept.daynight,
    vzaFactor: source === 'ModisOnly' ? 1.08 : null, nObs: members.length,
    platforms: [...new Set(members.map(p => rawPlatforms[p.id]))] });
}
const monthOf = p => Number(p.acquired.slice(5, 7));
function snapshot(month) {
  const r = raw.filter(p => month === null || monthOf(p) === month);
  const h = harmonized.filter(p => month === null || monthOf(p) === month);
  const sourceCount = source => h.filter(p => p.source === source).length;
  return { scope: { region: 'Turkey', year: 2021, month }, rawCount: r.length, harmonizedCount: h.length,
    rawFrpSum: round(r.reduce((s, p) => s + p.frp, 0)), harmonizedFrpSum: round(h.reduce((s, p) => s + p.frpCorrected, 0)),
    platforms: [...new Set(r.map(p => rawPlatforms[p.id]))],
    bySource: { modisOnly: sourceCount('ModisOnly'), viirsOnly: sourceCount('ViirsOnly'), matched: sourceCount('Matched') },
    generatedAt: '2026-11-15T00:00:00Z' };
}
const monthlyStats = Array.from({ length: 12 }, (_, i) => snapshot(i + 1));
const months = [], years = [];
for (let year = 2000; year <= 2025; year++) {
  const ym = [];
  for (let month = year === 2000 ? 11 : 1; month <= 12; month++) {
    const r = raw.filter(p => monthOf(p) === month);
    const modis = year === 2021 ? r.filter(p => p.sensor === 'MODIS').length : Math.round(season[month - 1] * (3 + random() * 2));
    const viirs = year === 2021 ? r.filter(p => p.sensor === 'VIIRS').length : year < 2012 ? 0 : Math.round(modis * (1.4 + random()));
    const count = year === 2021 ? monthlyStats[month - 1].harmonizedCount : Math.round((modis + viirs) * 0.8);
    ym.push({ year, month, rawModis: modis, rawViirs: viirs, rawTotal: modis + viirs, harmonized: count, anomalyRaw: null, anomalyHarmonized: null });
  }
  months.push(...ym);
  const sum = key => ym.reduce((s, m) => s + m[key], 0);
  const total = sum('harmonized'), rawCount = sum('rawTotal');
  const matched = year === 2021 ? 100 : year < 2012 ? 0 : Math.floor((rawCount - total) / 2);
  const modisOnly = year === 2021 ? 200 : year < 2012 ? total : Math.floor((total - matched) * 0.4);
  years.push({ year, raw: rawCount, rawModis: sum('rawModis'), rawViirs: sum('rawViirs'), harmonized: total,
    matched, modisOnly, viirsOnly: total - matched - modisOnly,
    interscanRemoved: year === 2021 ? 100 : rawCount - total - matched, fleetMerged: year === 2021 ? 100 : 0,
    firstDate: `${year}-${year === 2000 ? '11' : '01'}-01`, lastDate: `${year}-12-31`, partial: year === 2000,
    modisFirstDate: `${year}-${year === 2000 ? '11' : '01'}-01`, modisLastDate: `${year}-12-31`,
    viirsFirstDate: year < 2012 ? null : `${year}-01-01`, viirsLastDate: year < 2012 ? null : `${year}-12-31` });
}
for (const m of months) {
  const pool = months.filter(p => p.month === m.month);
  for (const [key, output] of [['rawTotal', 'anomalyRaw'], ['harmonized', 'anomalyHarmonized']]) {
    const mean = pool.reduce((s, p) => s + p[key], 0) / pool.length;
    const sd = Math.sqrt(pool.reduce((s, p) => s + (p[key] - mean) ** 2, 0) / pool.length);
    m[output] = pool.length >= 3 && sd > 0 ? round((m[key] - mean) / sd) : null;
  }
}
const sumFrp = (rows, key = 'frp') => rows.reduce((s, p) => s + p[key], 0);
const rawModisPoints = raw.filter(p => p.sensor === 'MODIS'), rawViirsPoints = raw.filter(p => p.sensor === 'VIIRS');
const meanModis = sumFrp(rawModisPoints) / rawModisPoints.length;
const meanViirs = sumFrp(rawViirsPoints) / rawViirsPoints.length;
const meanHarmonized = sumFrp(harmonized, 'frpCorrected') / harmonized.length;
const noisy = (count, mean) => round(count * mean * (1 + (random() * 2 - 1) * 0.1));
const ratio = (sum, count) => count === 0 ? null : round(sum / count);
const frpMonths = months.map(m => {
  let modisSum, viirsSum, harmonizedSum;
  if (m.year === 2021) {
    const r = raw.filter(p => monthOf(p) === m.month);
    modisSum = round(sumFrp(r.filter(p => p.sensor === 'MODIS')));
    viirsSum = round(sumFrp(r.filter(p => p.sensor === 'VIIRS')));
    harmonizedSum = round(sumFrp(harmonized.filter(p => monthOf(p) === m.month), 'frpCorrected'));
  } else {
    modisSum = noisy(m.rawModis, meanModis);
    viirsSum = noisy(m.rawViirs, meanViirs);
    harmonizedSum = noisy(m.harmonized, meanHarmonized);
  }
  return { year: m.year, month: m.month, rawModisCount: m.rawModis, rawModisFrpSum: modisSum,
    rawViirsCount: m.rawViirs, rawViirsFrpSum: viirsSum, harmonizedCount: m.harmonized, harmonizedFrpSum: harmonizedSum,
    meanFrpRaw: ratio(modisSum + viirsSum, m.rawTotal), meanFrpHarmonized: ratio(harmonizedSum, m.harmonized) };
});
const before = years.find(y => y.year === 2011), after = years.find(y => y.year === 2012);
const detailYear = years.find(y => y.year === 2021);
const span = rows => rows.map(p => p.acquired.slice(0, 10)).sort();
const dates = span(raw), modisDates = span(raw.filter(p => p.sensor === 'MODIS')), viirsDates = span(raw.filter(p => p.sensor === 'VIIRS'));
Object.assign(detailYear, { firstDate: dates[0], lastDate: dates.at(-1), modisFirstDate: modisDates[0], modisLastDate: modisDates.at(-1),
  viirsFirstDate: viirsDates[0], viirsLastDate: viirsDates.at(-1) });
const change = (a, b) => a === 0 ? null : (b - a) / a;
const rawChange = change(before.raw, after.raw), harmChange = change(before.harmonized, after.harmonized);
const series = { region: 'Turkey', filters: { fireType: [0], confidence: 'all', area: 'synthetic Turkey scenario', sensors: ['MODIS', 'VIIRS'] }, years, months,
  cliff: { fromYear: 2011, toYear: 2012, year: 2012, rawFrom: before.raw, rawTo: after.raw, rawChangeRatio: rawChange, rawChangePct: rawChange * 100,
    harmonizedFrom: before.harmonized, harmonizedTo: after.harmonized, harmonizedChangeRatio: harmChange, harmonizedChangePct: harmChange * 100 },
  coverage: { modis: { first: 2000, last: 2025 }, viirs: { first: 2012, last: 2025 } }, frpMonths };
const files = {
  'metadata.json': { schemaVersion: 2, sourceKind: 'synthetic', seed, generatedAt: '2026-11-15T00:00:00Z', region: 'Turkey', detailYear: 2021,
    calendarSlots: 312, outsideCoverageSlots: 10, summaryYears: [2000, 2025], scientificPipelineApplied: false,
    note: '2021 details describe a synthetic Mediterranean cluster, not national coverage. Other years are synthetic summaries only. No claim of sensor-cliff correction. calendar.json also carries a synthetic monthly FRP series (frpMonths); VIIRS-to-MODIS FRP bridge 0.35 is visual-only.' },
  'hotspots.json': { region: 'Turkey', year: 2021, month: null, truncated: false, limit: 800, raw, harmonized,
    stages: { rawModis: raw.filter(p => p.sensor === 'MODIS').length, rawViirs: raw.filter(p => p.sensor === 'VIIRS').length,
      interscanRemoved: 100, fleetMerged: 100, matchedPairs: 100, harmonized: 500 }, bbox: { west: 29, south: 36.5, east: 31, north: 37.1 } },
  'calendar.json': series, 'stats.json': { annual: snapshot(null), months: monthlyStats }, 'raw-platforms.json': rawPlatforms,
};
for (const [name, value] of Object.entries(files)) await writeFile(new URL(name, dir), JSON.stringify(value, null, 2) + '\n');
