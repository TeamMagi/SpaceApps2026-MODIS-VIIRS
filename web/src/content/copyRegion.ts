/** region copy fragment merged into copy.ts. Keys here must not collide with copy.ts keys. */

export const regionCopyEn = {
  regionTriggerAria: "{label}: {value}",
  regionSearchLabel: "Search regions",
  regionSearchPlaceholder: "Search country or region…",
  regionListboxLabel: "Available regions",
  regionNoMatches: "No matching regions",
  regionNoneAvailable: "No regions available",
  regionSelectedMarker: "selected",
  regionCoverageModis: "MODIS {span}",
  regionCoverageViirs: "VIIRS {span}",
} as const;

export const regionCopyTr: { readonly [K in keyof typeof regionCopyEn]: string } = {
  regionTriggerAria: "{label}: {value}",
  regionSearchLabel: "Bölge ara",
  regionSearchPlaceholder: "Ülke veya bölge ara…",
  regionListboxLabel: "Kullanılabilir bölgeler",
  regionNoMatches: "Eşleşen bölge bulunamadı",
  regionNoneAvailable: "Kullanılabilir bölge yok",
  regionSelectedMarker: "seçili",
  regionCoverageModis: "MODIS {span}",
  regionCoverageViirs: "VIIRS {span}",
};
