import type { ReactNode } from "react";

interface AppShellProps {
  children: ReactNode;
}

export default function AppShell({ children }: AppShellProps) {
  return (
    <div style={{ minHeight: "100vh", fontFamily: "system-ui, sans-serif" }}>
      <header
        style={{
          padding: "1rem 2rem",
          borderBottom: "1px solid var(--color-border, #e5e7eb)",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <div>
          <h1 style={{ margin: 0, fontSize: "1.5rem" }}>PyroSync</h1>
          <p style={{ margin: 0, fontSize: "0.875rem", color: "var(--color-muted, #6b7280)" }}>
            MODIS / VIIRS Fire Hotspot Harmonization
          </p>
        </div>
      </header>
      <main style={{ padding: "1.5rem 2rem" }}>{children}</main>
    </div>
  );
}
