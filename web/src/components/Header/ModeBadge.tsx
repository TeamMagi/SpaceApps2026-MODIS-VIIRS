/**
 * ModeBadge — shows the data source ("FIRMS archive") and year range from the archive API.
 */

import { useCopy } from "../../hooks/useCopy";
import { useRegionData, useRegions } from "../../hooks/useRegionData";
import styles from "./ModeBadge.module.css";

export default function ModeBadge() {
  const { t } = useCopy();
  const { status, hasData, firstYear, lastYear, region } = useRegionData();
  const { regions, status: regionsStatus } = useRegions();

  const regionSummary = regions.find((r) => r.id === region.id);
  const first = hasData ? firstYear : (regionSummary?.modisYears?.[0] ?? regionSummary?.viirsYears?.[0] ?? null);
  const last = hasData ? lastYear : (regionSummary?.modisYears?.[1] ?? regionSummary?.viirsYears?.[1] ?? null);

  const label =
    first !== null && last !== null
      ? t("modeBadgeArchiveYears", { first: String(first), last: String(last) })
      : t("modeBadgeArchive");

  const hasError = status === "error" || regionsStatus === "error";

  return (
    <span className={styles.badge} data-source={hasError ? "error" : "live"}>
      <span className={styles.dot} aria-hidden="true" />
      {label}
    </span>
  );
}
