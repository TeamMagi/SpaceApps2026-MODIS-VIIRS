import { lazy, Suspense } from "react";
import AppShell from "./components/Layout/AppShell";
import BottomBar from "./components/BottomBar/BottomBar";
import BurnCalendar from "./components/Calendar/BurnCalendar";
import CliffChart from "./components/Charts/CliffChart";
import BriefingDrawer from "./components/Drawers/BriefingDrawer";
import MethodsDrawer from "./components/Drawers/MethodsDrawer";
import Header from "./components/Header/Header";
import Headline from "./components/Headline/Headline";
import MapStateOverlay from "./components/Map/MapStateOverlay";
import PipelineStrip from "./components/Pipeline/PipelineStrip";
import CountDelta from "./components/Stats/CountDelta";
import ViewToggle from "./components/Toggle/ViewToggle";
import { useDataModeProbe } from "./hooks/useDataModeProbe";
import { useDocumentSync } from "./hooks/useDocumentSync";
import { useHarmonizeClock } from "./hooks/useHarmonizeTransition";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { useUrlSync } from "./hooks/useUrlSync";

// The map is the heaviest panel; loading it separately lets the headline and chart paint first.
const HotspotMap = lazy(() => import("./components/Map/HotspotMap"));

/**
 * Composition root. Opens in Raw (the question before the answer) unless the URL says otherwise.
 * The five app-wide hooks below are the only places that own timers or global listeners.
 */
export default function App() {
  useDataModeProbe();
  useHarmonizeClock();
  useUrlSync();
  useKeyboardShortcuts();
  useDocumentSync();

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
        </>
      }
    />
  );
}
