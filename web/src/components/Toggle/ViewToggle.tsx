import type { ViewMode } from "../../types";

interface ViewToggleProps {
  mode: ViewMode;
  onChange: (mode: ViewMode) => void;
}

export default function ViewToggle({ mode, onChange }: ViewToggleProps) {
  return (
    <div
      style={{
        display: "inline-flex",
        borderRadius: "8px",
        border: "1px solid var(--color-border, #e5e7eb)",
        overflow: "hidden",
      }}
    >
      <button
        onClick={() => onChange("raw")}
        style={{
          padding: "0.5rem 1.25rem",
          border: "none",
          cursor: "pointer",
          fontWeight: mode === "raw" ? 600 : 400,
          background: mode === "raw" ? "var(--color-raw, #ef4444)" : "transparent",
          color: mode === "raw" ? "#fff" : "inherit",
        }}
      >
        Raw Data
      </button>
      <button
        onClick={() => onChange("harmonized")}
        style={{
          padding: "0.5rem 1.25rem",
          border: "none",
          cursor: "pointer",
          fontWeight: mode === "harmonized" ? 600 : 400,
          background: mode === "harmonized" ? "var(--color-harmonized, #22c55e)" : "transparent",
          color: mode === "harmonized" ? "#fff" : "inherit",
        }}
      >
        Harmonized
      </button>
    </div>
  );
}
