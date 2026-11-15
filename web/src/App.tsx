import { lazy, Suspense } from "react";
import AppShell from "./components/Layout/AppShell";
import BottomBar from "./components/BottomBar/BottomBar";
import BurnCalendar from "./components/Calendar/BurnCalendar";
import CliffChart from "./components/Charts/CliffChart";
import BriefingDrawer from "./components/Drawers/BriefingDrawer";
import MethodsDrawer from "./components/Drawers/MethodsDrawer";
import GuidedTour from "./components/Tour/GuidedTour";
import Header from "./components/Header/Header";
import Headline from "./components/Headline/Headline";
import MethodsPage from "./components/Methods/MethodsPage";
import MapStateOverlay from "./components/Map/MapStateOverlay";
import PipelineStrip from "./components/Pipeline/PipelineStrip";
import CountDelta from "./components/Stats/CountDelta";
import ViewToggle from "./components/Toggle/ViewToggle";
import { useDocumentSync } from "./hooks/useDocumentSync";
import { useGuidedTour } from "./hooks/useGuidedTour";
import { useHarmonizeClock } from "./hooks/useHarmonizeTransition";
import { useRoute } from "./hooks/useRoute";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { useUrlSync } from "./hooks/useUrlSync";

// The map is the heaviest panel; loading it separately lets the headline and chart paint first.
const HotspotMap = lazy(() => import("./components/Map/HotspotMap"));

/**
 * Composition root. Opens in Raw (the question before the answer) unless the URL says otherwise.
 * The five app-wide hooks below are the only places that own timers or global listeners.
 */
export default function App() {
  useHarmonizeClock();
  useUrlSync();
  useKeyboardShortcuts();
  useDocumentSync();
  useGuidedTour();
  const route = useRoute();

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
      header={<Header />}
      headline={<Headline />}
      toggle={<ViewToggle />}
      count={<CountDelta />}
      pipeline={<PipelineStrip />}
      map={
        <Suspense fallback={<MapStateOverlay state="loading" />}>
          <HotspotMap />
        </Suspense>
      }
      chart={<CliffChart />}
      calendar={<BurnCalendar />}
      bottomBar={<BottomBar />}
      overlays={
        <>
          <MethodsDrawer />
          <BriefingDrawer />
          <GuidedTour />
        </>
      }
    />
  );
}
