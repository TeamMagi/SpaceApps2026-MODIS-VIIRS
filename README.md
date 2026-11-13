# PyroSync — MODIS/VIIRS Fire Hotspot Harmonization

**NASA Space Apps Challenge 2026** · *Harmonization of MODIS and VIIRS Hot Spots*

PyroSync transforms fragmented satellite fire data into a single, scientifically consistent burning activity calendar. By harmonizing detections from NASA's MODIS (Terra/Aqua, 2000–present, 1 km) and VIIRS (SNPP/NOAA-20/21, 2012–present, 375 m) instruments, it eliminates the artificial discontinuities that make multi-decadal fire analysis unreliable.

## The Problem

Fire researchers and land managers face a "sensor cliff" at 2012: detection counts jump sharply when VIIRS came online, not because fires increased, but because VIIRS sees more. Naively combining the two sensors produces misleading trends, inflated counts, and incomparable time series. Existing tools either ignore one sensor or merge them without correcting for:

- **Interscan duplicates** — overlapping MODIS scans inflate FRP by ~18.3%
- **Viewing zenith angle (VZA) bias** — MODIS FRP grows up to 300% from nadir to scan edge
- **Spatial resolution mismatch** — 1 km vs 375 m pixels cannot be directly compared
- **Multi-satellite fleet effects** — three VIIRS satellites require averaging, not summing

## Our Solution

PyroSync implements a rigorous seven-stage harmonization pipeline built entirely in **Rust** for performance and correctness:

1. **Ingest** — Fetch active fire data from NASA FIRMS API (MODIS + all VIIRS sources)
2. **Deduplicate** — Remove interscan overlaps using temporal (|Δt| < 8s) and spatial (overlap > 30%) criteria
3. **VZA Correction** — Normalize MODIS FRP to nadir-equivalent using polynomial correction (R² ≥ 98.18%)
4. **Spatial Harmonization** — Aggregate detections onto a common grid for cross-sensor comparison
5. **Cross-Sensor Matching** — Link MODIS/VIIRS detections within 2 km and 5 minutes using Haversine distance
6. **Quality-Priority Fusion** — Prefer VIIRS (higher resolution) where both sensors detect, keep unmatched from each
7. **Calendar Output** — Generate unified monthly burning activity with anomaly detection across the full 2000–present timeline

### Raw ↔ Harmonized Toggle

The key demo feature: users can switch between raw multi-sensor data (showing the sensor cliff and biases) and the harmonized view (showing a consistent timeline) — making the problem and solution visible in a single action.

## Architecture

```
crates/
  core/           Harmonization engine — models, dedup, VZA correction, fusion, calendar
  firms-client/   NASA FIRMS API client with CSV parsing
  server/         Axum web API serving harmonized data
web/              React frontend (TypeScript compiled with SWC — a Rust-based compiler)
```

## Tech Stack

| Layer | Technology | Why |
|-------|-----------|-----|
| Backend | **Rust** (Axum, Tokio) | Memory-safe, fast, no competitor uses it |
| Harmonization | **Rust** (pyrosync-core) | Correct numeric processing without GC pauses |
| Data Source | **NASA FIRMS API** | Official active fire data endpoint |
| Frontend | **React + TypeScript** | Interactive visualization |
| TS Compiler | **SWC** (Rust-based) | Consistent Rust toolchain throughout |

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (1.85+)
- [Node.js](https://nodejs.org/) (20+)
- NASA FIRMS API key ([get one free](https://firms.modaps.eosdis.nasa.gov/api/area/))

### Run

```bash
# Backend
export FIRMS_API_KEY=your_key_here
cargo run -p pyrosync-server

# Frontend (separate terminal)
cd web
npm install
npm run dev
```

Open http://localhost:5173 — the frontend proxies API calls to the Rust backend on port 3001.

## Team

**TeamMagi** — NASA Space Apps Challenge 2026

## License

MIT
