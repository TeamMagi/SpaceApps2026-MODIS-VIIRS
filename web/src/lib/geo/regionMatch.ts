import firmsCountries from "./firmsCountries.json";
import type { GeoFeature } from "./types.ts";

export interface RegionRef {
  readonly id: string;
  readonly name: string;
}

export interface RegionFeatureMatch {
  /** Index-aligned with the feature list; the archive region id each feature belongs to. */
  readonly regionByFeature: readonly (string | null)[];
  readonly featuresByRegion: ReadonlyMap<string, readonly number[]>;
}

interface NameTokens {
  readonly words: readonly string[];
  /** Natural Earth shortens names with a trailing dot ("Bosnia and Herz.", "Dem. Rep. Congo"). */
  readonly abbreviated: readonly boolean[];
  readonly key: string;
}

const FILLER_WORDS = new Set(["the", "of", "and"]);
const ISO_ALPHA3 = /^[A-Za-z]{3}$/;

export function tokenizeGeoName(name: string): NameTokens {
  const folded = name
    .replace(/İ/g, "I")
    .replace(/ı/g, "i")
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[_\-&,/()'’]/g, " ");

  const words: string[] = [];
  const abbreviated: boolean[] = [];
  for (const part of folded.split(/\s+/)) {
    const word = part.replace(/[^a-z0-9]/g, "");
    if (word.length === 0 || FILLER_WORDS.has(word)) continue;
    words.push(word);
    abbreviated.push(part.endsWith("."));
  }
  return { words, abbreviated, key: words.join(" ") };
}

// Every FIRMS archive country id (plus common aliases) mapped to the feature codes it covers; shared
// with scripts/build-geo.mjs, which stamps the canonical id onto each feature as `regionId`.
const FIRMS_CODES_BY_KEY: ReadonlyMap<string, readonly string[]> = (() => {
  const countries: Readonly<Record<string, readonly string[]>> = firmsCountries.countries;
  const aliases: Readonly<Record<string, string>> = firmsCountries.aliases;
  const map = new Map<string, readonly string[]>();
  for (const [id, codes] of Object.entries(countries)) map.set(tokenizeGeoName(id).key, codes);
  for (const [alias, id] of Object.entries(aliases)) {
    const codes = countries[id];
    if (codes) map.set(tokenizeGeoName(alias).key, codes);
  }
  return map;
})();

/** Feature codes for a FIRMS country id, alias, display name or ISO alpha-3 code; null if unknown. */
export function firmsCodesFor(idOrName: string): readonly string[] | null {
  const codes = FIRMS_CODES_BY_KEY.get(tokenizeGeoName(idOrName).key);
  if (codes) return codes;
  return ISO_ALPHA3.test(idOrName) ? [idOrName.toUpperCase()] : null;
}

function abbreviationMatch(a: NameTokens, b: NameTokens): boolean {
  if (a.words.length === 0 || a.words.length !== b.words.length) return false;
  for (let i = 0; i < a.words.length; i++) {
    const wa = a.words[i];
    const wb = b.words[i];
    if (wa === wb) continue;
    if (a.abbreviated[i] && wb.startsWith(wa)) continue;
    if (b.abbreviated[i] && wa.startsWith(wb)) continue;
    return false;
  }
  return true;
}

/**
 * Links archive regions to boundary features. Each region stops at the first rule that yields a
 * feature, and a feature is never claimed twice:
 *   1. the feature's build-time `regionId` link,
 *   2. the FIRMS country table (id, alias or name -> feature codes), or an ISO alpha-3 region id,
 *   3. equal normalized names (region id or name vs feature name),
 *   4. a unique abbreviation-aware name match ("Bosnia and Herz." vs "Bosnia_and_Herzegovina").
 */
export function matchRegionsToFeatures(
  features: readonly GeoFeature[],
  regions: readonly RegionRef[],
): RegionFeatureMatch {
  const regionByFeature: (string | null)[] = features.map(() => null);
  const featuresByRegion = new Map<string, number[]>();

  const claim = (regionId: string, featureIndex: number) => {
    regionByFeature[featureIndex] = regionId;
    const list = featuresByRegion.get(regionId);
    if (list) list.push(featureIndex);
    else featuresByRegion.set(regionId, [featureIndex]);
  };

  const uniqueRegions: RegionRef[] = [];
  const seenIds = new Set<string>();
  for (const region of regions) {
    if (seenIds.has(region.id)) continue;
    seenIds.add(region.id);
    uniqueRegions.push(region);
  }

  for (let i = 0; i < features.length; i++) {
    const linked = features[i].regionId;
    if (linked !== null && seenIds.has(linked)) claim(linked, i);
  }

  let pending = uniqueRegions.filter((r) => !featuresByRegion.has(r.id));

  if (pending.length > 0) {
    const byCode = new Map<string, number>();
    for (let i = 0; i < features.length; i++) {
      if (regionByFeature[i] === null) byCode.set(features[i].code.toUpperCase(), i);
    }
    for (const region of pending) {
      const codes = firmsCodesFor(region.id) ?? firmsCodesFor(region.name) ?? [];
      for (const code of codes) {
        const idx = byCode.get(code);
        if (idx !== undefined && regionByFeature[idx] === null) claim(region.id, idx);
      }
    }
    pending = pending.filter((r) => !featuresByRegion.has(r.id));
  }

  if (pending.length === 0) {
    return { regionByFeature, featuresByRegion };
  }

  const featureTokens = features.map((f) => tokenizeGeoName(f.name));
  const regionTokens = new Map<string, NameTokens[]>();
  for (const region of pending) {
    const variants = [tokenizeGeoName(region.id)];
    const fromName = tokenizeGeoName(region.name);
    if (fromName.key !== variants[0].key) variants.push(fromName);
    regionTokens.set(region.id, variants.filter((v) => v.key.length > 0));
  }

  const byKey = new Map<string, number[]>();
  for (let i = 0; i < features.length; i++) {
    if (regionByFeature[i] !== null || featureTokens[i].key.length === 0) continue;
    const list = byKey.get(featureTokens[i].key);
    if (list) list.push(i);
    else byKey.set(featureTokens[i].key, [i]);
  }
  for (const region of pending) {
    for (const variant of regionTokens.get(region.id) ?? []) {
      const hits = byKey.get(variant.key)?.filter((idx) => regionByFeature[idx] === null);
      if (hits && hits.length > 0) {
        for (const idx of hits) claim(region.id, idx);
        break;
      }
    }
  }
  pending = pending.filter((r) => !featuresByRegion.has(r.id));

  for (const region of pending) {
    const variants = regionTokens.get(region.id) ?? [];
    let found = -1;
    let ambiguous = false;
    for (let i = 0; i < features.length && !ambiguous; i++) {
      if (regionByFeature[i] !== null) continue;
      if (!variants.some((v) => abbreviationMatch(v, featureTokens[i]))) continue;
      if (found === -1) found = i;
      else ambiguous = true;
    }
    if (found !== -1 && !ambiguous) claim(region.id, found);
  }

  return { regionByFeature, featuresByRegion };
}
