import { useEffect, useRef } from "react";
import { setPanel } from "../lib/dockPanel";
import { isRouteHash, methodsHash } from "../lib/route";
import { nextTourStep, TOUR_STEP_MS, TOUR_STEPS } from "../lib/tour";
import { useAppStore } from "../store/useAppStore";
import { dashboardHashFromStore, useRoute } from "./useRoute";

/** Seconds into the first step at which the tour flips Raw to Harmonized for the viewer. */
const AUTO_TOGGLE_MS = 5000;
const CASE_REGION = "China";
const CASE_YEAR = 2018;

function ensureDashboardRoute(): void {
  if (isRouteHash(window.location.hash)) window.location.hash = dashboardHashFromStore();
}

/** The chart and calendar steps point at dock panels, which exist only once a country is picked. */
function revealDock(stepId: string): void {
  if (stepId !== "toggle" && stepId !== "case") {
    return;
  }
  const store = useAppStore.getState() as { regionPicked?: boolean; setRegionPicked?: (value: boolean) => void };
  if (store.regionPicked === false) store.setRegionPicked?.(true);
  // After the region change has been applied, which closes any open panel.
  window.setTimeout(() => setPanel(stepId === "toggle" ? "detections" : "calendar"), 50);
}

function enterStep(index: number): void {
  const step = TOUR_STEPS[index];
  if (!step) return;
  const store = useAppStore.getState();

  if (step.id === "validation") {
    window.location.hash = methodsHash("limits");
    return;
  }
  ensureDashboardRoute();
  revealDock(step.id);

  if (step.id === "toggle") {
    store.applyUrlState({ mode: "raw" });
  } else if (step.id === "case") {
    store.setRegion(CASE_REGION);
    useAppStore.getState().setCalendarYear(CASE_YEAR);
    if (useAppStore.getState().targetMode !== "harmonized") {
      useAppStore.getState().applyUrlState({ mode: "harmonized" });
    }
  }
}

function clearHighlights(): void {
  for (const element of document.querySelectorAll("[data-tour-active]")) {
    element.removeAttribute("data-tour-active");
  }
}

/**
 * App-wide owner of the guided tour's timers and DOM highlighting (mount once).
 * Step side effects run when the step changes; the 15 s auto-advance and the step-1 auto-toggle
 * are pause-aware: on pause the elapsed time is subtracted, on resume the remainder is scheduled.
 */
export function useGuidedTour(): void {
  const tourStep = useAppStore((state) => state.tourStep);
  const paused = useAppStore((state) => state.tourPaused);
  const tourRun = useAppStore((state) => state.tourRun);
  const route = useRoute();
  const remaining = useRef({ advance: TOUR_STEP_MS, toggle: null as number | null });

  useEffect(() => {
    if (tourStep === null) return;
    remaining.current = {
      advance: TOUR_STEP_MS,
      toggle: TOUR_STEPS[tourStep]?.id === "toggle" ? AUTO_TOGGLE_MS : null,
    };
    enterStep(tourStep);
  }, [tourStep, tourRun]);

  useEffect(() => {
    if (tourStep === null || paused) return;
    const startedAt = performance.now();
    const left = remaining.current;
    const timers: number[] = [];

    timers.push(
      window.setTimeout(() => {
        remaining.current.advance = 0;
        const next = nextTourStep(tourStep);
        const store = useAppStore.getState();
        if (next === null) store.endTour();
        else store.setTourStep(next);
      }, left.advance),
    );
    if (left.toggle !== null) {
      timers.push(
        window.setTimeout(() => {
          remaining.current.toggle = null;
          const store = useAppStore.getState();
          if (store.targetMode === "raw" && !store.tourPaused) store.toggleMode();
        }, left.toggle),
      );
    }

    return () => {
      for (const timer of timers) window.clearTimeout(timer);
      const elapsed = performance.now() - startedAt;
      const current = remaining.current;
      current.advance = Math.max(0, current.advance - elapsed);
      if (current.toggle !== null) current.toggle = Math.max(0, current.toggle - elapsed);
    };
  }, [tourStep, tourRun, paused]);

  useEffect(() => {
    if (tourStep === null) return;
    const step = TOUR_STEPS[tourStep];
    if (!step) return;
    let scrolled = false;
    const highlight = () => {
      let first: Element | null = null;
      for (const target of step.targets) {
        for (const element of document.querySelectorAll(`[data-tour="${target}"]:not([data-tour-active])`)) {
          element.setAttribute("data-tour-active", "true");
          first ??= element;
        }
      }
      if (!first || scrolled) return;
      scrolled = true;
      const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      // The card floats at the bottom, so on the long Methods page the highlighted block goes to the top.
      first.scrollIntoView({
        block: step.route === "methods" ? "start" : "nearest",
        behavior: reduceMotion ? "auto" : "smooth",
      });
    };
    const frame = window.requestAnimationFrame(() => {
      clearHighlights();
      highlight();
    });
    // Dock panels open (and the Methods page mounts) after the step starts, so late targets are picked up as they appear.
    const observer = new MutationObserver(highlight);
    observer.observe(document.body, { childList: true, subtree: true });
    return () => {
      observer.disconnect();
      window.cancelAnimationFrame(frame);
      clearHighlights();
    };
  }, [tourStep, tourRun, route.name]);
}
