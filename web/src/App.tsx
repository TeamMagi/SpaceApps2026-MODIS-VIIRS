import { lazy, Suspense } from "react";
import AppShell from "./components/Layout/AppShell";
import BottomBar from "./components/BottomBar/BottomBar";
import InsightDock from "./components/Dock/InsightDock";
import BriefingDrawer from "./components/Drawers/BriefingDrawer";
import CellDiagnosticDrawer from "./components/Drawers/CellDiagnosticDrawer";
import SettingsDrawer from "./components/Drawers/SettingsDrawer";
import GuidedTour from "./components/Tour/GuidedTour";
import Header from "./components/Header/Header";
import Headline from "./components/Headline/Headline";
import MethodsPage from "./components/Methods/MethodsPage";
import MapStateOverlay from "./components/Map/MapStateOverlay";
import CountDelta from "./components/Stats/CountDelta";
import ViewToggle from "./components/Toggle/ViewToggle";
import { useDataModeProbe } from "./hooks/useDataModeProbe";
import { useDocumentSync } from "./hooks/useDocumentSync";
import { useGuidedTour } from "./hooks/useGuidedTour";
import { useHarmonizeClock } from "./hooks/useHarmonizeTransition";
import { useRoute } from "./hooks/useRoute";
import { useAppStore } from "./store/useAppStore";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { useUrlSync } from "./hooks/useUrlSync";

// The map is the heaviest panel; loading it separately lets the headline and chart paint first.
const HotspotMap = lazy(() => import("./components/Map/HotspotMap"));

/**
 * Composition root. Opens in Raw (the question before the answer) unless the URL says otherwise.
 * The six app-wide hooks below are the only places that own timers or global listeners.
 */
export default function App() {
  useDataModeProbe();
  useHarmonizeClock();
  useUrlSync();
  useKeyboardShortcuts();
  useDocumentSync();
  useGuidedTour();
  const route = useRoute();
  const worldView = !useAppStore((state) => state.regionPicked);

  if (route.name === "methods") {
    return (
      <>
        <MethodsPage section={route.section} />
        <GuidedTour />
      </>
    );
  }

  return (
    <AppShell
      worldView={worldView}
      header={<Header />}
      headline={<Headline />}
      toggle={<ViewToggle />}
      count={<CountDelta />}
      map={
        <>
          <Suspense fallback={<MapStateOverlay state="loading" />}>
            <HotspotMap />
          </Suspense>
          <InsightDock />
        </>
      }
      bottomBar={<BottomBar />}
      overlays={
        <>
          <SettingsDrawer />
          <BriefingDrawer />
          <CellDiagnosticDrawer />
          <GuidedTour />
        </>
      }
    />
  );
}
