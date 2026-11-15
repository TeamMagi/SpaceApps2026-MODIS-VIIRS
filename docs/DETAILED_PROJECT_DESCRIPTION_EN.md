# PyroSync — Detailed Project Description

**TeamMagi | NASA Space Apps Challenge 2026**  
**Challenge: Harmonization of MODIS and VIIRS Hot Spots**

## Project overview

PyroSync is an interactive, open-source platform that turns satellite fire detections from different instruments into a more consistent record of burning activity. It combines observations from MODIS aboard Terra and Aqua with VIIRS aboard Suomi NPP, NOAA-20 and NOAA-21, helping users explore where and when fires occur while understanding how changes in satellite coverage affect the results.

The platform connects a scientific harmonization engine to a browser-based map, monthly burning activity calendar, comparison charts and downloadable datasets. Users can switch between raw and harmonized observations to see the effects of duplicate removal, sensor matching, viewing-angle correction and observation-effort normalization. Historical analysis uses the 2000–2025 FIRMS country archive; live endpoints support recent observations from NASA FIRMS.

## The problem we address

Long-term fire analysis becomes misleading when observations from different satellites are simply added together. MODIS active-fire pixels have a nominal resolution of approximately 1 km, while the VIIRS product used by PyroSync has a nominal resolution of 375 m. VIIRS can detect smaller fires and distinguish multiple hotspots within an area represented by a single MODIS pixel. When VIIRS enters the record in 2012, the resulting increase in detections can resemble a sudden increase in fire activity.

Additional satellites create more opportunities to observe the same event. Overlapping scans, repeated satellite passes, differences in viewing geometry and unequal pixel footprints introduce further inconsistencies. Researchers, public agencies and land managers therefore need a way to distinguish changes in observation capability from changes in burning activity.

PyroSync addresses this measurement problem by making the processing steps visible and preserving the distinction between observed data, corrected values and model estimates.

## How the project works

The backend processes fire detections through seven stages:

1. **Data ingestion.** PyroSync reads active-fire records from NASA FIRMS live services or a locally downloaded country archive. Records include geographic coordinates, observation time, satellite identity, confidence information and fire radiative power (FRP), a measure of the rate at which a fire emits radiant energy.

2. **Interscan deduplication.** Spatial and temporal overlap rules identify repeated observations caused by adjacent scans within each sensor stream. Removing these duplicates reduces artificial inflation before the instruments are combined.

3. **MODIS fleet consolidation.** Terra and Aqua observations are consolidated using a default 180-minute window to reduce repeated counting of the same fire across the MODIS fleet.

4. **VIIRS fleet consolidation.** Suomi NPP, NOAA-20 and NOAA-21 observations are consolidated using a separate default 60-minute window. The separate windows reflect differences in the platforms' observation times.

5. **Cross-sensor matching.** An H3 hexagonal spatial index at resolution 8 makes it efficient to find nearby MODIS and VIIRS detections. Default matching uses a 2 km distance limit and a 180-minute time window. Multiple VIIRS pixels from one pass within a MODIS footprint are grouped and their FRP values are summed, allowing a footprint-level comparison. A narrower, near-simultaneous subset is retained for radiometric comparisons because fire intensity can change between passes.

6. **Fusion and viewing-angle correction.** The standard fusion model prefers VIIRS measurements for matched events and retains unmatched detections from both sensors. MODIS FRP receives a viewing-zenith-angle correction derived from published relationships, with the angle estimated from along-scan pixel size because FIRMS does not directly provide it. Users can also inspect equal-weight and MODIS-calibrated fusion options for the selected month's map.

7. **Calendar generation and effort normalization.** The engine aggregates observations into monthly activity calendars and estimates an observation-effort factor, Ω, relative to the MODIS reference channel. This reduces artificial count increases associated with the expanding VIIRS fleet. The calendar reports normalized activity, FRP metrics, anomaly scores, uncertainty information and data-quality flags. Earlier MODIS-only FRP totals also use latitude-band completeness factors.

Months from the early Terra-only period are explicitly flagged as unsuitable for direct normalized comparison. A separately labelled recovered-count estimate is available with bounds; it does not replace the missing normalized count or anomaly. This keeps the limitations of the early record visible.

## What users can do

The interface lets users explore regions and time periods, inspect hotspot locations and compare raw observations with harmonized results. The monthly calendar highlights seasonal patterns and unusual activity, while sensor charts explain how MODIS and VIIRS contribute to the combined record.

Month-level diagnostics expose the raw counts, matched observations, removed duplicates, normalization factor, FRP statistics, uncertainty bands and quality flags. An orbital-drift indicator helps explain changes in MODIS observation timing. An integrated methods page documents the processing stages and effective configuration.

Users can download the current selection as CSV or GeoJSON for further analysis in spreadsheets, GIS software or research workflows. A printable regional briefing summarizes activity and historical context. Case studies include the 2021 Mediterranean fires in Türkiye and agricultural burning policy in China. A guided tour introduces the main workflow, and a labelled synthetic demo supports exploration without a live data connection.

## Benefits and intended users

For researchers, PyroSync provides a reproducible starting point for studying multi-year fire patterns across changes in satellite instrumentation. For environmental agencies and land managers, the calendar and regional briefings help identify seasonal peaks, compare periods and place recent activity in historical context. For educators and the public, the raw-to-harmonized comparison makes satellite measurement bias understandable through direct interaction.

The project's main benefit is a more interpretable fire record: users can assess how much an apparent change depends on the observing system. Open exports, configurable processing and visible quality information make the results easier to inspect and reuse.

PyroSync supports research, environmental assessment and situational awareness. Its hotspot products describe satellite observations; they do not provide fire perimeters or forecasts.

## Goals, validation and limitations

Our goal is to make long-term satellite fire analysis more comparable, transparent and accessible. The project combines a working processing engine, historical data workflow, live-data integration and an interactive explanation of the underlying methods.

Repository validation includes sensor-transition comparisons, placebo transition tests and leave-one-year-out checks. In a documented country-archive analysis, effort normalization reduced the artificial upward count trend from a Spearman correlation of +0.788 to +0.080 and changed the 2012 transition ratio from 3.35 to 0.85. These results describe the evaluated dataset and method; they are not a guarantee for every region or fire regime. The delivered implementation also includes automated backend and frontend checks and deployment smoke tests.

Uncertainty remains because satellite detections depend on cloud cover, overpass timing, fire size and intensity, pixel geometry and instrument sensitivity. Normalization cannot recover every missed fire. Country-level results can be weaker than pooled results, and FRP continuity still has regional limitations. PyroSync exposes uncertainty bands, validation information and flags for known sensor-era issues so users can judge these constraints alongside the results.

## Software, programming languages and hardware

**Rust** powers the harmonization engine, FIRMS client and web API. The backend uses **Axum** for HTTP services and **Tokio** for asynchronous execution. Spatial matching uses **H3 through the h3o library**, while structured data and configuration use formats including JSON, CSV and TOML.

The frontend uses **TypeScript and React**, with **Three.js** for geographic visualization, **Zustand** for interface state and **Motion** for animation. **Vite and SWC** support frontend development and production builds. Supporting automation includes Python scripts and shell tools. **Docker and Docker Compose** package the application, and **GitHub Actions** runs continuous integration. The project is released under the **MIT license**.

The application runs on a standard computer or server and is accessed through a modern web browser. Geographic rendering uses the browser's graphics capabilities. No custom electronics, ground sensors or dedicated field hardware are required: the observing hardware consists of the existing MODIS and VIIRS satellite instruments whose data NASA FIRMS distributes.

Historical workflows require local storage for the downloaded archive, approximately 9 GB compressed and 41 GB unpacked for the documented full dataset. The historical country archive includes MODIS, Suomi NPP VIIRS and NOAA-20 VIIRS; NOAA-21 is available through the live-data workflow. Historical endpoints can operate without a FIRMS API key after archive setup, while live requests require a configured key. Bundled derived results and caching reduce repeated computation.

## Expected impact

PyroSync helps users interpret changes in fire activity with a clearer understanding of changes in satellite observation. By bringing harmonization, diagnostics, visualization and export into one workflow, it makes a complex remote-sensing problem accessible to people who need trustworthy context for environmental research and decisions.

---

Technical basis: repository README, workspace and frontend dependency manifests, delivery verification notes in `docs/G6_TESLIM.md`, and validation documentation in `docs/DOGRULAMA_PAKETI.md`. Reported results retain the scope and limitations documented in those sources.
