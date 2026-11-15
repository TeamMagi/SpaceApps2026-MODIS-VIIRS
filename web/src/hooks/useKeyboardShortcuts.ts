import { useEffect } from "react";
import { useAppStore } from "../store/useAppStore";

function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT";
}

/**
 * Global shortcuts. Mount once (App does).
 * - H: toggle Raw/Harmonized.
 * - P: presentation mode on/off (it hides the header, so this is also the way back out).
 * - T: start the 60-second guided tour, or end it when one is running.
 *   H, P and T are ignored with Ctrl/Meta/Alt, on key repeat, while typing, and while any drawer or
 *   the receipt is open (the keypress belongs to that panel then).
 * - Escape: close the open drawer/receipt; with none open, end a running guided tour. Panels that handle Escape themselves call
 *   preventDefault, which this listener respects.
 */
export function useKeyboardShortcuts(): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const state = useAppStore.getState();

      if (event.key === "Escape") {
        if (state.openDrawer !== null) {
          event.preventDefault();
          state.closeDrawer();
        } else if (state.tourStep !== null) {
          event.preventDefault();
          state.endTour();
        }
        return;
      }

      if (event.ctrlKey || event.metaKey || event.altKey || event.repeat) return;
      const key = event.key.toLowerCase();
      if (key !== "h" && key !== "p" && key !== "t") return;
      if (isEditable(event.target) || state.openDrawer !== null) return;

      event.preventDefault();
      if (key === "h") state.toggleMode();
      else if (key === "p") state.togglePresentation();
      else if (state.tourStep === null) state.startTour();
      else state.endTour();
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
