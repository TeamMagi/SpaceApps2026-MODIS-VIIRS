import type { Theme } from "../../../types";
import { readPalette } from "../engine/renderer";
import type { GlobeColors } from "./scene";

/**
 * Globe colours from the design tokens. Points reuse the flat map palette; the selected region
 * uses the warm --anom-pos-2 amber, which keeps the same hue in both themes (the FRP ramp flips
 * on light, so --frp-7 would turn purple there).
 */
export function readGlobeColors(el: HTMLElement, theme: Theme): GlobeColors {
  const palette = readPalette(el, theme);
  const style = getComputedStyle(el);
  const read = (name: string, fallback: string): string =>
    style.getPropertyValue(name).trim() || fallback;
  const dark = theme !== "light";
  return {
    palette,
    earth: {
      borderHalo: palette.border,
      borderLine: palette.borderSelected,
      selection: read("--anom-pos-2", dark ? "#e68a2a" : "#e08214"),
      hover: palette.ink,
      atmosphere: palette.borderSelected,
      ground: read("--color-bg", dark ? "#0d0f13" : "#f6f7f9"),
      dark,
    },
    orbits: {
      line: read("--color-text-muted", dark ? "#949cab" : "#525b6b"),
      body: read("--color-text", dark ? "#f1f3f7" : "#12161d"),
      dark,
    },
  };
}
