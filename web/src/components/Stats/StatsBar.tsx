interface StatCardProps {
  label: string;
  value: string | number;
  subtitle?: string;
}

function StatCard({ label, value, subtitle }: StatCardProps) {
  return (
    <div
      style={{
        padding: "1rem 1.5rem",
        borderRadius: "8px",
        border: "1px solid var(--color-border, #e5e7eb)",
        minWidth: "140px",
      }}
    >
      <div style={{ fontSize: "0.75rem", color: "var(--color-muted, #6b7280)", marginBottom: "0.25rem" }}>
        {label}
      </div>
      <div style={{ fontSize: "1.5rem", fontWeight: 700 }}>{value}</div>
      {subtitle && (
        <div style={{ fontSize: "0.75rem", color: "var(--color-muted, #6b7280)" }}>{subtitle}</div>
      )}
    </div>
  );
}

interface StatsBarProps {
  rawCount: number;
  harmonizedCount: number;
  duplicatesRemoved: number;
}

export default function StatsBar({ rawCount, harmonizedCount, duplicatesRemoved }: StatsBarProps) {
  const reductionPct = rawCount > 0 ? ((duplicatesRemoved / rawCount) * 100).toFixed(1) : "0";

  return (
    <div style={{ display: "flex", gap: "1rem", flexWrap: "wrap" }}>
      <StatCard label="Raw Detections" value={rawCount.toLocaleString()} />
      <StatCard label="Harmonized" value={harmonizedCount.toLocaleString()} />
      <StatCard label="Duplicates Removed" value={duplicatesRemoved.toLocaleString()} subtitle={`${reductionPct}% reduction`} />
    </div>
  );
}
