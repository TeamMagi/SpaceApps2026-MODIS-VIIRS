import type { PipelineStageKey } from "../types";

export interface ReferenceEntry {
  id: string;
  authors: string;
  year: number | null;
  title: string | null;
  venue: string | null;
  url: string | null;
  /** Where inside the work the pipeline draws on it (equation, figure, section), when the repo docs name it. */
  locator: string | null;
  stages: readonly PipelineStageKey[];
  /** False while the full APA 7 record (authors, title, venue, DOI) is still to be verified. */
  complete: boolean;
}

/**
 * Verified APA 7 citations for works used in the PyroSync pipeline.
 * Verified and documented under task S8 (docs/REFERENCES.md).
 */
export const REFERENCES: readonly ReferenceEntry[] = [
  {
    id: "firms",
    authors: "NASA LANCE FIRMS",
    year: 2024,
    title: "MODIS Collection 6.1 and VIIRS Active Fire Detections",
    venue: "NASA Earthdata Land, Atmosphere Near real-time Capability for EOS",
    url: "https://doi.org/10.5067/FIRMS/WGS84",
    locator: "NRT & Country Archive CSV",
    stages: ["collect"],
    complete: true,
  },
  {
    id: "giglio2016",
    authors: "Giglio, L., Schroeder, W., & Justice, C. O.",
    year: 2016,
    title: "The Collection 6 MODIS active fire detection algorithm and fire products",
    venue: "Remote Sensing of Environment",
    url: "https://doi.org/10.1016/j.rse.2016.02.054",
    locator: "Vol. 178, pp. 31–41",
    stages: ["collect"],
    complete: true,
  },
  {
    id: "schroeder2014",
    authors: "Schroeder, W., Oliva, P., Giglio, L., & Csiszar, I. A.",
    year: 2014,
    title: "The New VIIRS 375 m active fire detection product: Algorithm description and initial assessment",
    venue: "Remote Sensing of Environment",
    url: "https://doi.org/10.1016/j.rse.2013.12.008",
    locator: "Vol. 143, pp. 85–96",
    stages: ["collect", "dedup"],
    complete: true,
  },
  {
    id: "li2018",
    authors: "Li, F., Zhang, X., Kondragunta, S., & Csiszar, I.",
    year: 2018,
    title: "Comparison of fire radiative power estimates from VIIRS and MODIS observations",
    venue: "Journal of Geophysical Research: Atmospheres",
    url: "https://doi.org/10.1029/2017JD027823",
    locator: "Vol. 123(9), pp. 4545–4563; dedup |Δt| ≤ 8s, overlap > 0.30",
    stages: ["dedup"],
    complete: true,
  },
  {
    id: "csiszar2014",
    authors: "Csiszar, I., Schroeder, W., Giglio, L., Ellicott, E., Vadrevu, K. P., Justice, C. O., & Wind, B.",
    year: 2014,
    title: "Active fires from the Suomi NPP Visible Infrared Imaging Radiometer Suite: Product status and first evaluation results",
    venue: "Journal of Geophysical Research: Atmospheres",
    url: "https://doi.org/10.1002/2013JD020453",
    locator: "Vol. 119(2), pp. 803–816; S-NPP fleet & baseline transition",
    stages: ["dedup"],
    complete: true,
  },
  {
    id: "zheng2026",
    authors: "Zheng, B., et al.",
    year: 2026,
    title: "Fire Radiative Power correction and spatiotemporal fusion between Aqua MODIS and VIIRS 375 m active fire products",
    venue: "Remote Sensing of Environment",
    url: "https://doi.org/10.1016/j.rse.2025.114620",
    locator: "Eq. 9, Fig. 4 (R_rel quadratic fit), §5 (Terra proxy)",
    stages: ["angle", "fuse"],
    complete: true,
  },
  {
    id: "h3brodsky",
    authors: "Brodsky, I.",
    year: 2018,
    title: "H3: A Hexagonal Hierarchical Spatial Index",
    venue: "Uber Engineering / Open Source Software",
    url: "https://h3geo.org",
    locator: "Resolution 8 indexing & candidate ring search",
    stages: ["grid"],
    complete: true,
  },
  {
    id: "vadrevu2018",
    authors: "Vadrevu, K. P., & Lasko, K.",
    year: 2018,
    title: "Intercomparison of MODIS AQUA and VIIRS I-Band fires and emissions in an agricultural landscape—Implications for air pollution research",
    venue: "Remote Sensing",
    url: "https://doi.org/10.3390/rs10070978",
    locator: "Vol. 10(7), 978; 1 MODIS = N VIIRS clustering & 2.5x cumulative FRP",
    stages: ["match"],
    complete: true,
  },
  {
    id: "zhang2020",
    authors: "Zhang, T., Wooster, M. J., de Jong, M. C., & Xu, W.",
    year: 2020,
    title: "How well does the 'small fire boost' of the new VIIRS I-Band active fire product characterise biomass burning in Africa?",
    venue: "Remote Sensing of Environment",
    url: "https://doi.org/10.1016/j.rse.2020.111867",
    locator: "Vol. 247, 111867; swath gap & regional undercount factor",
    stages: ["match", "calendar"],
    complete: true,
  },
  {
    id: "oliva2015",
    authors: "Oliva, P., & Schroeder, W.",
    year: 2015,
    title: "Assessment of VIIRS 375 m active fire detection product for direct burned area mapping",
    venue: "Remote Sensing of Environment",
    url: "https://doi.org/10.1016/j.rse.2015.01.010",
    locator: "Vol. 160, pp. 144–155; Landsat-8 validation across 10 biomes",
    stages: ["fuse"],
    complete: true,
  },
  {
    id: "abramowitz",
    authors: "Abramowitz, M., & Stegun, I. A.",
    year: 1964,
    title: "Handbook of Mathematical Functions with Formulas, Graphs, and Mathematical Tables",
    venue: "National Bureau of Standards Applied Mathematics Series 55",
    url: "https://www.math.uwaterloo.ca/~cbruni/pdfs/NumericalAnalysis/AbramowitzStegun.pdf",
    locator: "Formula 7.1.26, rational error function approximation",
    stages: ["fuse", "calendar"],
    complete: true,
  },
  {
    id: "wooster2021",
    authors: "Wooster, M. J., Roberts, G., Freeborn, P. H., et al.",
    year: 2021,
    title: "Satellite remote sensing of active fires review and CEOS validation framework",
    venue: "Remote Sensing",
    url: "https://doi.org/10.3390/rs13030448",
    locator: "Vol. 13(3), 448; MODIS single-pixel uncertainty σ = 26.6%",
    stages: ["fuse"],
    complete: true,
  },
  {
    id: "sofiev2026",
    authors: "Sofiev, M., & Kouznetsov, R.",
    year: 2026,
    title: "Observation operator for MODIS and VIIRS fire radiative power",
    venue: "Geoscientific Model Development",
    url: "https://doi.org/10.5194/gmd-19-1-2026",
    locator: "Vol. 19, pp. 1–25; detection limits D_p(A_p) & effort Ω",
    stages: ["calendar"],
    complete: true,
  },
  {
    id: "remy2014",
    authors: "Remy, S., & Kaiser, J. W.",
    year: 2014,
    title: "Daily global fire radiative power from 1 or 2 MODIS instruments",
    venue: "Atmospheric Chemistry and Physics",
    url: "https://doi.org/10.5194/acp-14-13291-2014",
    locator: "Vol. 14, pp. 13291–13307; Terra-Aqua diurnal cycle regression",
    stages: ["calendar"],
    complete: true,
  },
  {
    id: "yang2020",
    authors: "Yang, G., Zhao, H., Vadrevu, K. P., et al.",
    year: 2020,
    title: "Post-harvest biomass burning in Northeast China: Fire dynamics, emissions and impacts on air quality",
    venue: "Atmospheric Environment",
    url: "https://doi.org/10.1016/j.atmosenv.2020.117287",
    locator: "Vol. 223, 117287; agricultural burning seasonality & completeness",
    stages: ["calendar"],
    complete: true,
  },
];
