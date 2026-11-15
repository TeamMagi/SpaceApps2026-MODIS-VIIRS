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
 * Only works already named in the repo docs (docs/TASARIM_METHODS.md §6, README.md,
 * crates/core/CLAUDE.md). Fields the docs do not give are null rather than guessed.
 * Samet (task S8) replaces this list with verified citations.
 */
export const REFERENCES: readonly ReferenceEntry[] = [
  {
    id: "li2018",
    authors: "Li et al.",
    year: 2018,
    title: "Comparison of Fire Radiative Power Estimates From VIIRS and MODIS Observations",
    venue: "Journal of Geophysical Research: Atmospheres",
    url: "https://doi.org/10.1029/2017JD027823",
    locator: null,
    stages: ["dedup"],
    complete: false,
  },
  {
    id: "zheng2026",
    authors: "Zheng et al.",
    year: 2026,
    title: null,
    venue: null,
    url: null,
    locator: "Eq. 9, Fig. 4, §5",
    stages: ["angle", "fuse"],
    complete: false,
  },
  {
    id: "abramowitz",
    authors: "Abramowitz, M., & Stegun, I. A.",
    year: null,
    title: null,
    venue: null,
    url: null,
    locator: "7.1.26",
    stages: ["fuse", "calendar"],
    complete: false,
  },
  {
    id: "li2020",
    authors: "Li et al.",
    year: 2020,
    title: null,
    venue: null,
    url: null,
    locator: null,
    stages: ["calendar"],
    complete: false,
  },
  {
    id: "sofiev2026",
    authors: "Sofiev, M., & Kouznetsov, R.",
    year: 2026,
    title: null,
    venue: null,
    url: null,
    locator: null,
    stages: ["calendar"],
    complete: false,
  },
  {
    id: "firms",
    authors: "NASA FIRMS",
    year: null,
    title: null,
    venue: null,
    url: "https://firms.modaps.eosdis.nasa.gov",
    locator: null,
    stages: ["collect"],
    complete: false,
  },
];
