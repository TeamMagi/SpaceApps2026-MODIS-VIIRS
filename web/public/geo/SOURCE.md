# Geodata Provenance & License

## Source
- **Package:** `world-atlas@2.0.2` (pinned build-time devDependency, ISC License)
- **File:** `web/node_modules/world-atlas/countries-50m.json`
- **Upstream Dataset:** [Natural Earth](https://www.naturalearthdata.com/) Admin 0 – Countries (`v4.1.0`), `1:50m` medium scale
- **Natural Earth Terms:** Made with Natural Earth. Free vector and raster map data @ naturalearthdata.com (public domain).

## Political Boundary Note
Country and territory borders in `regions.json` follow Natural Earth `v4.1.0` defaults at 1:50m resolution without manual political edits.

## Mapping & Exceptions
- The 11 NASA FIRMS regions are matched strictly by ISO 3166-1 numeric `id` from `world-atlas` (`792` `TUR` → `Turkey`, `300` `GRC` → `Greece`, `100` `BGR` → `Bulgaria`, `196` `CYP` → `Cyprus`, `268` `GEO` → `Georgia`, `051` `ARM` → `Armenia`, `031` `AZE` → `Azerbaijan`, `364` `IRN` → `Iran`, `368` `IRQ` → `Iraq`, `760` `SYR` → `Syria`, `643` `RUS` → `Russian_Federation`).
- **Cyprus exception:** Natural Earth encodes `"N. Cyprus"` without an ISO numeric `id`. `build-geo.mjs` matches that single feature by exact name `"N. Cyprus"`, assigns code `"CYN"`, and maps it to `regionId: "Cyprus"` alongside `"CYP"`.
- **Context-only neighbours:** Every country/territory with at least one polygon whose bounding box intersects `lon 14..72, lat 22..60` is included with `regionId: null`. Context-only features with no ISO `id` in `world-atlas` (specifically `"Kosovo"`) are assigned deterministic running codes `X1`, `X2`, ...
- **Antimeridian:** Polygons of `Russian_Federation` (`RUS`) crossing `180°` are split in `world-atlas`; longitudes `< 0` (Chukotka / Wrangel Island) are unwrapped by `+360°` so all polygons are continuous and `bbox[2]` (`east`) exceeds `180°`.

## Simplification & Size
- Topology-aware Douglas-Peucker simplification on decoded `world-atlas` arcs (`0.02°` tolerance for standard/shared arcs, `0.06°` for Russia-only arcs) to avoid boundary slivers between neighbours.
- Coordinates rounded to `2` decimal places (`~1 km`), consecutive duplicate points removed, rings closed, and degenerate sub-tolerance islets pruned.
- Size budget: `≤ 450 KB` raw UTF-8 JSON.

## How to Regenerate
From `web/`:
```sh
node scripts/build-geo.mjs
```
The script reads only local files in `web/node_modules/world-atlas` (no network calls) and writes `web/public/geo/regions.json` deterministically.

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
