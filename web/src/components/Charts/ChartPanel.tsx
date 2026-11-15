import { useRef, useState, type KeyboardEvent } from "react";
import { useCopy } from "../../hooks/useCopy";
import CliffChart from "./CliffChart";
import FrpBandChart from "./FrpBandChart";
import SensorCharts from "./SensorCharts";
import styles from "./ChartPanel.module.css";

type ChartView = "detections" | "frp" | "sensors";

const VIEW_ORDER: readonly ChartView[] = ["detections", "frp", "sensors"];

export default function ChartPanel() {
  const { t } = useCopy();
  const [view, setView] = useState<ChartView>("detections");
  const tabRefs = useRef<Record<ChartView, HTMLButtonElement | null>>({ detections: null, frp: null, sensors: null });

  const handleGroupKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    e.preventDefault();
    const step = e.key === "ArrowRight" ? 1 : VIEW_ORDER.length - 1;
    const next = VIEW_ORDER[(VIEW_ORDER.indexOf(view) + step) % VIEW_ORDER.length];
    setView(next);
    tabRefs.current[next]?.focus();
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
            ref={(el) => {
              tabRefs.current.detections = el;
            }}
            type="button"
            className={styles.tab}
            aria-pressed={view === "detections"}
            onClick={() => setView("detections")}
          >
            {t("chartTabDetections")}
          </button>
          <button
            ref={(el) => {
              tabRefs.current.frp = el;
            }}
            type="button"
            className={styles.tab}
            aria-pressed={view === "frp"}
            onClick={() => setView("frp")}
          >
            {t("chartTabMeanFrp")}
          </button>
          <button
            ref={(el) => {
              tabRefs.current.sensors = el;
            }}
            type="button"
            className={styles.tab}
            aria-pressed={view === "sensors"}
            onClick={() => setView("sensors")}
          >
            {t("chartTabSensors")}
          </button>
        </div>
      </div>

      <div className={styles.body}>
        {view === "detections" && <CliffChart />}
        {view === "frp" && <FrpBandChart />}
        {view === "sensors" && <SensorCharts />}
      </div>
    </div>
  );
}
