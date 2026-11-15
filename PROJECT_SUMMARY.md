# PyroSync — High-Level Project Summary

PyroSync is an interactive web platform that makes satellite fire observations more comparable across sensors and time. Developed for the **“Harmonization of MODIS and VIIRS Hot Spots”** challenge, it combines NASA FIRMS observations from MODIS and VIIRS into a unified burning activity calendar.

The problem is that a change in satellite technology can look like a change in fire activity. MODIS and VIIRS observe the Earth at different resolutions and times, and VIIRS detects smaller fires that MODIS can miss. Adding more satellites also increases observation opportunities. Simply combining these records can therefore introduce artificial jumps, duplicate detections and misleading long-term trends.

PyroSync addresses these differences through a Rust processing pipeline that removes overlapping observations, consolidates satellite fleets, matches detections across sensors and combines VIIRS subpixels within MODIS-scale footprints. It applies a viewing-angle correction to MODIS observations and normalizes detection counts for changes in observation effort. The platform supports historical exploration across 238 countries and regions using the 2000–2025 archive, alongside live FIRMS data access.

An interactive map, time-series charts and a burning activity calendar let users compare raw and harmonized data directly. Uncertainty information, documented assumptions and sensor-quality flags make the results easier to assess. CSV and GeoJSON exports support further analysis, while case studies explore applications in wildfire research and agricultural burning.

PyroSync matters because interpreting a fire trend requires understanding how it was observed. By exposing sensor effects and reducing artificial discontinuities, it helps researchers, land managers and policy analysts investigate changes in fire activity with greater transparency. It is a research and exploration tool, not an operational early-warning system; its validation reports retain sample limitations and comparisons where harmonization does not improve agreement.
