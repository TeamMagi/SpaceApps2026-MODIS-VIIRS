import { useState } from "react";

type ViewMode = "raw" | "harmonized";

export default function App() {
  const [mode, setMode] = useState<ViewMode>("harmonized");

  return (
    <div style={{ fontFamily: "system-ui, sans-serif", padding: "2rem" }}>
      <header>
        <h1>PyroSync</h1>
        <p>MODIS / VIIRS Fire Hotspot Harmonization</p>
      </header>

      <div style={{ margin: "1rem 0" }}>
        <button
          onClick={() => setMode("raw")}
          style={{ fontWeight: mode === "raw" ? "bold" : "normal", marginRight: "0.5rem" }}
        >
          Raw Data
        </button>
        <button
          onClick={() => setMode("harmonized")}
          style={{ fontWeight: mode === "harmonized" ? "bold" : "normal" }}
        >
          Harmonized
        </button>
      </div>

      <main>
        {mode === "raw" ? (
          <section>
            <h2>Raw Sensor Data</h2>
            <p>Showing unprocessed MODIS and VIIRS detections with sensor-specific biases.</p>
          </section>
        ) : (
          <section>
            <h2>Harmonized View</h2>
            <p>
              Unified burning activity calendar — interscan duplicates removed,
              VZA-corrected, cross-sensor matched, and quality-fused.
            </p>
          </section>
        )}
      </main>
    </div>
  );
}
