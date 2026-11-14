# Geodata Provenance & License

## Source
- **Package:** `world-atlas@2.0.2` (pinned build-time devDependency, ISC License)
- **File:** `web/node_modules/world-atlas/countries-50m.json`
- **Upstream Dataset:** [Natural Earth](https://www.naturalearthdata.com/) Admin 0 – Countries (`v4.1.0`), `1:50m` medium scale
- **Natural Earth Terms:** Made with Natural Earth. Free vector and raster map data @ naturalearthdata.com (public domain).
- **FIRMS country table:** `web/src/lib/geo/firmsCountries.json`, built from NASA FIRMS' own archive index
  (`https://firms.modaps.eosdis.nasa.gov/data/country/yearly_summary_files.txt`, 238 country ids). The same
  list is kept as a test fixture in `web/tests/fixtures/firms-countries.txt`.

## Political Boundary Note
Country and territory borders in `regions.json` follow Natural Earth `v4.1.0` defaults at 1:50m resolution without manual political edits.

## Coverage & Codes
- **Whole world:** every Natural Earth admin-0 feature is included (246 features) except **Antarctica**, which has no
  burnable land and in a regional conic projection turns into a band that dwarfs the rest of the map.
- **Feature `code`:** ISO 3166-1 alpha-3, from the ISO numeric `id` carried by `world-atlas` (which already applies the
  usual Natural Earth `-99` fixes, e.g. France `250`, Norway `578`). Features without a usable numeric id get their
  Natural Earth `ADM0_A3` code: `N. Cyprus` → `CYN`, `Kosovo` → `KOS`, `Somaliland` → `SOL`, `Indian Ocean Ter.` →
  `IOA`, `Siachen Glacier` → `KAS`, and `Ashmore and Cartier Is.` (shares `036` with Australia) → `ATC`.
- **Overseas splits:** Natural Earth admin-0 keeps some overseas parts inside their parent, while FIRMS publishes them
  as separate countries. Parent polygons inside a fixed lon/lat box become their own feature: French Guiana (`GUF`),
  Guadeloupe (`GLP`), Martinique (`MTQ`), Réunion (`REU`), Mayotte (`MYT`) from France, and Svalbard + Jan Mayen
  (`SJM`) from Norway.
- **`regionId`:** the canonical FIRMS country id whose entry in `firmsCountries.json` lists the feature code
  (`TUR` → `Turkey`, `CIV` → `Cote_d_Ivoire`, `CYN` → `Cyprus`, `SOL` → `Somalia`), or `null`. At runtime
  `regionMatch.ts` also resolves aliases from the same table (`North_Macedonia`, `Ivory_Coast`, `Türkiye`, …) and
  falls back to name matching.
- **FIRMS ids without geometry:** `Antarctica` (dropped) and `United_States_Minor_Outlying_Islands` (not in 1:50m).
- **Antimeridian:** Polygons of Russia (`RUS`) crossing `180°` are unwrapped by `+360°`. Every ring is kept continuous in
  longitude (one Fiji ring touches both `±180`), and each feature's `bbox` uses the shortest longitude arc covering
  its polygons, so USA, Fiji, Kiribati, New Zealand and Russia get `bbox[2] > 180` instead of a `-180..180` span.

## Simplification & Size
- Topology-aware Douglas-Peucker simplification on decoded `world-atlas` arcs: `0.02°` for standard/shared arcs,
  `0.06°` for arcs used only by RUS/CAN/GRL/USA/AUS/CHN/BRA. Shared arcs are simplified once, so neighbours never sliver.
- Coordinates rounded to `2` decimal places (`~1 km`), consecutive duplicate points removed, rings closed, and degenerate
  islets pruned; a feature whose every polygon collapses keeps its unsimplified outline.
- Output: ~627 KB raw UTF-8 JSON (~203 KB gzip), 41.5k vertices. Size budget: `≤ 700 KB` raw.
- The renderer additionally draws simplified level-of-detail outlines when zoomed out (`engine/lod.ts`); hit testing
  always uses the full geometry.

## How to Regenerate
From `web/`:
```sh
node scripts/build-geo.mjs
```
The script reads only local files (`web/node_modules/world-atlas` and `web/src/lib/geo/firmsCountries.json`, no network
calls) and writes `web/public/geo/regions.json` deterministically.

## world-atlas License (ISC)
```text
Copyright 2013-2019 Michael Bostock

Permission to use, copy, modify, and/or distribute this software for any purpose
with or without fee is hereby granted, provided that the above copyright notice
and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND
FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS
OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER
TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
THIS SOFTWARE.
```
