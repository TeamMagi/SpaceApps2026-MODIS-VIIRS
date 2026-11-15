# PyroSync — MODIS/VIIRS Fire Hotspot Harmonization

**NASA Space Apps Challenge 2026** · *Harmonization of MODIS and VIIRS Hot Spots*

PyroSync transforms fragmented satellite fire data into a single, scientifically consistent burning activity calendar. By harmonizing detections from NASA's MODIS (Terra/Aqua, 2000–present, 1 km) and VIIRS (SNPP/NOAA-20/21, 2012–present, 375 m) instruments, it eliminates the artificial discontinuities that make multi-decadal fire analysis unreliable.

## The Problem

Fire researchers and land managers face a "sensor cliff" at 2012: detection counts jump sharply when VIIRS came online, not because fires increased, but because VIIRS sees more. Naively combining the two sensors produces misleading trends, inflated counts, and incomparable time series. Existing tools either ignore one sensor or merge them without correcting for:

- **Interscan & fleet duplicates** — overlapping scans remove 4.8–13.8% of MODIS and 12–16% of VIIRS detections, while VIIRS fleet consolidation removes another 21–22% (16–38% total reduction depending on fire density)
- **Viewing zenith angle (VZA) bias** — MODIS FRP grows up to 300% from nadir to scan edge
- **Spatial resolution mismatch** — 1 km vs 375 m pixels cannot be directly compared
- **Multi-satellite fleet effects** — VIIRS platforms (43–59 min apart) and MODIS Terra/Aqua (≥97 min apart, zero pairs within 60 min) require separate consolidation windows rather than summing raw detections

## Our Solution

PyroSync implements a seven-stage harmonization pipeline built in **Rust** for performance and determinism:

1. **Ingest** — Fetch active fire data from the NASA FIRMS API (live NRT/SP) or the local country archive
2. **Intra-Sensor Interscan Deduplication** — Remove overlapping adjacent scans per sensor using temporal and spatial overlap criteria
3. **MODIS Fleet Consolidation** — Merge Terra/Aqua observations of the same fire within a **180-minute** window
4. **VIIRS Fleet Consolidation** — Merge SNPP/NOAA-20/NOAA-21 observations within a **60-minute** window
5. **H3-Indexed Cross-Sensor Matching** — Index detections onto **H3 resolution 8** hexagons and match MODIS/VIIRS pairs within 2 km (k-ring 3 lookup) and **180 minutes** (across 26,505 co-located live pairs, zero fell below 30 min; min 34 min, median ~158 min). `Matched` means dual-sensor observation of the same fire event, not simultaneous overpass
6. **Quality-Priority Fusion & VZA Correction** — Prefer VIIRS (375 m) for matched events, retain unmatched detections from both sensors, and apply a per-record VZA correction to MODIS FRP. Because FIRMS does not publish VZA directly, VZA is derived from the along-scan pixel size anchored to three documented MODIS values (1.0 km at nadir, 4.833 km at the 55° scan edge, 65.4° max VZA). MODIS FRP is **multiplied** by the nadir-normalized ratio of Zheng et al. (2026), `R_rel(θ) = 0.2017θ² + 0.0168θ + 1.0` (θ in radians), so the correction raises off-nadir FRP rather than lowering it
7. **Monthly Calendar & Effort Normalization (Ω)** — Aggregate detections into a monthly activity calendar and normalize counts against the MODIS reference channel using an empirically measured effort factor **Ω**. Each month also reports `total_frp` and `frp_per_detection_eq` (MODIS-equivalent FRP per normalized detection), the metric that stays continuous across the 2012 VIIRS transition. Before VIIRS, cell totals are scaled by a measured latitude-band completeness factor `k_c` (1.11 / 1.18 / 1.28)

### Effort Normalization (Ω)

As the satellite fleet expands (SNPP in 2012, NOAA-20 in 2018, NOAA-21 in 2023), raw detection counts increase artificially. Rather than hardcoding a constant, PyroSync measures **Ω** per time series relative to the MODIS reference channel (observed VIIRS/MODIS yield ratio ranges from **3.3 to 7.4** across regions). On the 2000–2025 country archive, effort normalization reduces the artificial upward trend from Spearman ρ **+0.788 to +0.080** and brings the 2012 sensor-cliff step jump from **×3.35 to ×0.85**.

Ω cannot see an effort change inside the MODIS reference channel itself, so the Terra-only era (2000-11..2002-06, before Aqua) is flagged as not comparable (no normalized count or anomaly) rather than corrected; raw counts stay visible.

### Raw ↔ Harmonized Toggle

The key demo feature: users can switch between raw multi-sensor data (showing the sensor cliff and biases) and the harmonized view (showing a consistent timeline) — making the problem and solution visible in a single action.

## Architecture

```text
crates/
  core/             Harmonization engine — models, dedup, fleet, H3 index, VZA, effort, calendar
  firms-client/     NASA FIRMS API client, CSV parser, and country archive downloader
    src/bin/        archive_seed CLI for idempotent archive seeding
  server/           Axum web API serving harmonized data + static files
data/
  archive-set.json        Country & sensor matrix (all countries, 48 sensor-years)
  archive-manifest.sha256 Checksum manifest for bit-identical cache verification
  sample/                 Offline CSV fixtures for testing
web/
  src/
    components/     React UI components (Layout, Toggle, Stats)
    hooks/          Custom React hooks (useApi)
    services/       API client layer
    types/          TypeScript type definitions
    mocks/          Sample data for development
docs/               Project documentation (architecture, UI/UX)
tools/              Utility scripts (archive verification, data fetching)
```

## Tech Stack

| Layer | Technology | Why |
|-------|-----------|-----|
| Backend | **Rust** (Axum, Tokio) | Memory-safe, fast async API server |
| Harmonization | **Rust** (`pyrosync-core`) | Deterministic numeric processing without GC pauses |
| Spatial Index | **H3** (`h3o`, resolution 8) | Uniform hexagonal indexing with k-ring 3 neighbor search |
| Data Source | **NASA FIRMS API & Archive** | Official active fire NRT/SP endpoints and country CSVs |
| Frontend | **React + TypeScript** | Interactive visualization |
| TS Compiler | **SWC** (Rust-based) | Consistent Rust toolchain throughout |

## API

| Endpoint | Purpose |
|----------|---------|
| `GET /api/health` | Health check and version |
| `GET /api/config` | Effective pipeline configuration (thresholds used by the harmonization) as JSON |
| `GET /api/availability` | Date coverage per FIRMS source |
| `GET /api/harmonize` | Harmonized hotspots, calendar and effort factors (`west`, `south`, `east`, `north`, and either `days` or `start`+`end`; optional `include_raw=true`) |
| `GET /api/compare` | Raw versus harmonized statistics for the same query; `vza.mean_frp_adjustment_pct` is the mean VZA change to MODIS FRP (positive = increase) |
| `GET /api/stats` | Totals, sensor breakdown and pipeline metrics |
| `GET /api/calendar` | Historical monthly calendar from the country archive (`country`, `from`, `to`, optional bbox); cells carry `total_frp`, `frp_per_detection_eq`, `mean_frp` and their `_lower`/`_upper` bounds |
| `GET /api/export` | Download harmonized hotspots as a file (same query as `/api/harmonize` plus `format=csv\|geojson`, default `csv`) |

- Responses include an `X-Cache: HIT|MISS` header.
- Responses include a `warnings` array and an `effort.reliable` flag; if a FIRMS source fails to fetch, effort normalization is marked unreliable and surfaced in the response.
- `/api/compare` still returns `vza.mean_frp_reduction_pct` for backward compatibility, but it is now negative because the VZA correction increases FRP; use `mean_frp_adjustment_pct`.
- In `/api/calendar`, `mean_frp` is the native-pixel mean and drops at the 2012 VIIRS transition because VIIRS pixels are smaller; use `frp_per_detection_eq` as the headline FRP metric.
- `/api/calendar` reads from the local archive and requires **no API key**, though it covers a smaller satellite fleet than live endpoints because the country archive includes MODIS, VIIRS SNPP, and VIIRS NOAA-20 (`viirs-jpss1`) and does not include NOAA-21.

## Configuration

Harmonization thresholds (interscan dedup, fleet consolidation, cross-sensor matching, VZA coefficients) are configurable. Copy `pyrosync.example.toml` to `pyrosync.toml` (or point `PYROSYNC_CONFIG` at any TOML file); individual values can also be overridden with `PYROSYNC_*` environment variables, listed in `.env.example`. An invalid configuration stops the server at startup, and `GET /api/config` shows the values in effect.

## Quick Start

> Team members: see [SETUP.md](SETUP.md) for the full first-time setup guide (in Turkish).

### With Docker

Start the server and live NRT endpoints without downloading the historical archive:

```bash
cp .env.example .env
# Set FIRMS_API_KEY in .env

make docker-up
```

Open `http://localhost:3001`.

- `make docker-up` starts the server and live NRT endpoints (`/api/harmonize`, `/api/compare`, `/api/stats`, `/api/availability`) and **does not download any historical archive data**.
- If you do not need the historical archive endpoints, this is all you need. Without the archive, `/api/calendar`, `/api/regions`, `/api/series`, and `/api/points` report that local archive data is not downloaded and suggest running `make data`.

## Historical Archive Data (optional)

Historical endpoints require the pre-packaged FIRMS country archive (~9 GB compressed download, ~41 GB unpacked on disk). Download it explicitly when you need historical data:

```bash
make data          # one time, ~9 GB download, ~41 GB on disk, no API key required
make verify-data   # confirm every file matches the checksum manifest
make precompute    # optional: precompute /api/series into .cache/pyrosync-derived if needed
```

- **Precomputing `/api/series` (`make precompute`):** The repository bundles precomputed `/api/series` results in `data/derived` (`BUNDLED_DERIVED_DIR`), so after `make data` and `make verify-data` large countries open right away without running `make precompute`. Run `make precompute` (or `make precompute-local`) only if you changed pipeline configuration or bundled results are missing for a region; it persists results to `DERIVED_CACHE_DIR` (`.cache/pyrosync-derived`) and skips regions already valid on disk or in `data/derived` (pass `--force` to recompute). Maintainer workflow to update bundled results: `make precompute` → `make bundle-derived` → commit `data/derived`.

- **Scope:** FIRMS country archive covering **all countries** (208–220 countries) across **48 sensor-years**: MODIS (2000–2025), VIIRS SNPP (2012–2025), and VIIRS NOAA-20 (`viirs-jpss1`, 2018–2025). **VIIRS NOAA-21 is not in the archive** and is available on live endpoints only.
- **Endpoints that use the archive:** `/api/calendar`, `/api/regions`, `/api/series`, and `/api/points`. When archive data is missing locally, these endpoints respond with a message pointing to `make data` (and `/api/calendar` returns `404 Not Found` if no local data exists for the requested country).
- **No automatic downloads:** The server never downloads archive files on its own (the container mounts `.cache/firms-archive` read-only and uses an offline `ArchiveClient`). Data enters `.cache/firms-archive` only when you explicitly run `make data` (or `make seed`).
- **Idempotent & resumable:** If a download is interrupted (for example with `Ctrl+C`), re-run `make data` to resume where it left off; no partial files are left behind. **No FIRMS API key is required and no FIRMS transactions are consumed.**
- **Docker-free alternative:** If you already have a Rust toolchain installed, run `make seed` instead of `make data`. Both write to the same `.cache/firms-archive` host directory (`ARCHIVE_CACHE_HOST_DIR`) that Docker bind-mounts into the container, so no duplicate copy is stored on disk and `make verify-data` verifies the exact files the container reads.
- **Linux users:** set `DOCKER_USER=$(id -u):$(id -g)` in `.env` so the `seed` container has write permission to the bind-mounted host directory (`.cache/firms-archive`). Docker Desktop on Windows/macOS handles bind-mount permissions with the default `999:999`.

### Development (manual)

**Prerequisites:** `make`, [Docker](https://docs.docker.com/get-docker/), [Rust](https://rustup.rs/) **1.85+** (required by `edition = "2024"`; the Docker build uses Rust 1.100), [Node.js](https://nodejs.org/) 20+, and a [NASA FIRMS API key](https://firms.modaps.eosdis.nasa.gov/api/area/)

```bash
cp .env.example .env
# Set FIRMS_API_KEY in .env

# Optional: seed the local archive cache (default: .cache/firms-archive; override with ARCHIVE_CACHE_DIR)
# make seed

# Backend
make dev-backend

# Frontend (separate terminal)
make dev-frontend
```

Open `http://localhost:5173` — the frontend proxies API calls to the Rust backend on port 3001.

### Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `FIRMS_API_KEY` | — | NASA FIRMS key. Without it the server still boots, but live FIRMS endpoints return 500 |
| `PORT` | `3001` | Server port |
| `STATIC_DIR` | `web/dist` | Frontend build directory (`/app/static` in Docker) |
| `RUST_LOG` | `pyrosync=info` | Log level |
| `ARCHIVE_CACHE_DIR` | `.cache/firms-archive` | FIRMS country archive cache (`/app/.cache/firms-archive` in Docker) |
| `ARCHIVE_CACHE_HOST_DIR` | `./.cache/firms-archive` | Host directory bind-mounted into the Docker container at `/app/.cache/firms-archive` |
| `DOCKER_USER` | `999:999` | Container `uid:gid` for Docker; set to `$(id -u):$(id -g)` on Linux for bind-mount write access |
| `CACHE_BUDGET_MB` | `128` | Response cache memory budget; `0` disables caching |
| `REQUEST_TIMEOUT_SECS` | `300` | Request timeout; `0` disables it |
| `AREA_CACHE_DIR` | `.cache/firms-area` | On-disk cache of FIRMS Area API windows (cache hits cost no transactions); empty disables it |
| `AREA_CACHE_NRT_TTL_SECS` | `600` | How long a cached near-real-time window stays valid; science-quality (SP) windows never expire; `0` never serves NRT from cache |
| `CORS_ALLOWED_ORIGINS` | — | Comma-separated allowed origins; unset means permissive CORS |

### Useful Commands

```bash
make help          # Show all available commands
make build         # Build backend + frontend
make test          # Run all tests
make data          # Download full FIRMS country archive via Docker (~9 GB download, ~41 GB on disk, no API key)
make seed          # Docker-free alternative to 'make data' (requires Rust toolchain, no API key)
make docker-seed   # Alias for 'make data' (prefer 'make data')
make verify-data   # Verify local archive cache against data/archive-manifest.sha256
make manifest      # Regenerate data/archive-manifest.sha256 from local cache
make docker-up     # Start containers with Docker (does not download archive data)
make docker-down   # Stop containers
make docker-logs   # Tail container logs
make docker-shell  # Open a shell inside the running container
make clean-cache   # Remove local FIRMS archive cache (.cache/)
```

## Team

**TeamMagi** — NASA Space Apps Challenge 2026

## License

MIT
