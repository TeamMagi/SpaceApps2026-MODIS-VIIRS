export const METHODS_SECTIONS = ["pipeline", "vza", "h3", "references", "limits"] as const;
export type MethodsSection = (typeof METHODS_SECTIONS)[number];

export type Route = { name: "dashboard" } | { name: "methods"; section: MethodsSection | null };

const METHODS_PREFIX = "#/methods";

/** True for hashes owned by the router (`#/...`); the dashboard's own `#region=...` state never starts with a slash. */
export function isRouteHash(hash: string): boolean {
  return hash.startsWith("#/");
}

export function parseRoute(hash: string): Route {
  if (hash !== METHODS_PREFIX && !hash.startsWith(`${METHODS_PREFIX}/`)) return { name: "dashboard" };
  const section = hash.slice(METHODS_PREFIX.length + 1);
  const known = METHODS_SECTIONS.find((candidate) => candidate === section);
  return { name: "methods", section: known ?? null };
}

export function methodsHash(section?: MethodsSection): string {
  return section ? `${METHODS_PREFIX}/${section}` : METHODS_PREFIX;
}
