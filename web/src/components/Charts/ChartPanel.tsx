import { useRef, useState, type KeyboardEvent } from "react";
import { useCopy } from "../../hooks/useCopy";
import CliffChart from "./CliffChart";
import FrpBandChart from "./FrpBandChart";
import styles from "./ChartPanel.module.css";

type ChartView = "detections" | "frp";

export default function ChartPanel() {
  const { t } = useCopy();
  const [view, setView] = useState<ChartView>("detections");
  const detectionsRef = useRef<HTMLButtonElement | null>(null);
  const frpRef = useRef<HTMLButtonElement | null>(null);

  const handleGroupKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const next: ChartView = view === "detections" ? "frp" : "detections";
      setView(next);
      (next === "detections" ? detectionsRef.current : frpRef.current)?.focus();
    }
  };

  return (
    <div className={styles.panel}>
      <div className={styles.controls}>
        <div
          role="group"
          aria-label={t("chartViewSwitchLabel")}
          className={styles.segmented}
          onKeyDown={handleGroupKeyDown}
        >
          <button
            ref={detectionsRef}
            type="button"
            className={styles.tab}
            aria-pressed={view === "detections"}
            onClick={() => setView("detections")}
          >
            {t("chartTabDetections")}
          </button>
          <button
            ref={frpRef}
            type="button"
            className={styles.tab}
            aria-pressed={view === "frp"}
            onClick={() => setView("frp")}
          >
            {t("chartTabMeanFrp")}
          </button>
        </div>
      </div>

      <div className={styles.body}>
        {view === "detections" ? <CliffChart /> : <FrpBandChart />}
      </div>
    </div>
  );
}
