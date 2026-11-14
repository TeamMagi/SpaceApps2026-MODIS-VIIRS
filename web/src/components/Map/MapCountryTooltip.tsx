import type { Ref } from "react";
import styles from "./MapCountryTooltip.module.css";

export interface MapCountryTooltipProps {
  readonly ref?: Ref<HTMLDivElement>;
  readonly name: string;
  readonly detail: string | null;
  readonly hint: string | null;
  readonly available: boolean;
}

/** Pointer-following country label; HotspotMap moves it through the `--x`/`--y` custom properties. */
export default function MapCountryTooltip({ ref, name, detail, hint, available }: MapCountryTooltipProps) {
  return (
    <div
      ref={ref}
      className={styles.tooltip}
      data-available={available ? "true" : "false"}
      aria-hidden="true"
    >
      <span className={styles.name}>{name}</span>
      {detail && <span className={styles.detail}>{detail}</span>}
      {hint && <span className={styles.hint}>{hint}</span>}
    </div>
  );
}
