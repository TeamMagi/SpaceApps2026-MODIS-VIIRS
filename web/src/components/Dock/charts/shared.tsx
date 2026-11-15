import type { Translate } from "../../../content/copy";
import styles from "./charts.module.css";

export function ChartNote({ text }: { text: string }) {
  return (
    <p className={styles.note} role="status">
      {text}
    </p>
  );
}

/** 45000 -> "45k" / "45 bin"; small values keep one decimal at most. */
export function formatTick(t: Translate, value: number): string {
  const trim = (n: number) => String(Math.round(n * 10) / 10);
  if (value >= 1_000_000) return t("chartTickM", { n: trim(value / 1_000_000) });
  if (value >= 1000) return t("chartTickK", { n: trim(value / 1000) });
  return trim(value);
}

export const VIEW_W = 504;
export const VIEW_H = 250;
export const Y_TOP = 20;
export const Y_BASE = 210;
