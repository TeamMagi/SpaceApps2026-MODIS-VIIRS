import { useState } from "react";
import type { ViewMode } from "./types";
import AppShell from "./components/Layout/AppShell";
import ViewToggle from "./components/Toggle/ViewToggle";
import StatsBar from "./components/Stats/StatsBar";
import { SAMPLE_RESPONSE } from "./mocks/sample-data";

export default function App() {
  const [mode, setMode] = useState<ViewMode>("harmonized");

  const data = SAMPLE_RESPONSE;
  const duplicates = data.raw.total_raw - data.harmonized.count;

  return (
    <AppShell>
      <div style={{ display: "flex", alignItems: "center", gap: "1.5rem", marginBottom: "1.5rem" }}>
        <ViewToggle mode={mode} onChange={setMode} />
      </div>

      <StatsBar
        rawCount={data.raw.total_raw}
        harmonizedCount={data.harmonized.count}
        duplicatesRemoved={duplicates > 0 ? duplicates : 0}
      />

      <section style={{ marginTop: "2rem" }}>
        {mode === "raw" ? (
          <>
            <h2>Raw Sensor Data</h2>
            <p>Showing unprocessed MODIS and VIIRS detections with sensor-specific biases.</p>
            <p style={{ color: "var(--color-muted, #6b7280)", fontSize: "0.875rem" }}>
              MODIS: {data.raw.modis_count.toLocaleString()} &middot; VIIRS: {data.raw.viirs_count.toLocaleString()}
            </p>
          </>
        ) : (
          <>
            <h2>Harmonized View</h2>
            <p>
              Unified burning activity calendar — interscan duplicates removed,
              VZA-corrected, cross-sensor matched, and quality-fused.
            </p>
            <p style={{ color: "var(--color-muted, #6b7280)", fontSize: "0.875rem" }}>
              {data.harmonized.count.toLocaleString()} unique fire detections &middot; {data.calendar.length} months of data
            </p>
          </>
        )}
      </section>
    </AppShell>
  );
}
