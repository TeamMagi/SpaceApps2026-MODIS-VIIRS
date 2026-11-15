import { useState } from "react";
import { useCopy } from "../../hooks/useCopy";
import BurnCalendar from "../Calendar/BurnCalendar";
import ChartPanel from "../Charts/ChartPanel";
import PipelineStrip from "../Pipeline/PipelineStrip";
import styles from "./DetailedDock.module.css";

/** Full-detail analysis panel: the harmonization pipeline, the trend charts and the burn calendar in one scrollable surface. */
export default function DetailedDock() {
  const { t } = useCopy();
  const [hidden, setHidden] = useState(false);

  return (
    <div className={styles.column} data-hidden={hidden ? "true" : undefined}>
      <div className={styles.head}>
        <span className={styles.headTitle}>{t("dockLabel")}</span>
        <button type="button" className={styles.toggle} aria-expanded={!hidden} onClick={() => setHidden((h) => !h)}>
          {hidden ? t("dockShow") : t("dockHide")}
        </button>
      </div>
      {!hidden && (
        <div className={styles.body}>
          <section className={styles.section}>
            <h3 className={styles.sectionTitle}>{t("dockSectionPipeline")}</h3>
            <div className={styles.fit}>
              <PipelineStrip />
            </div>
          </section>
          <div className={styles.grid}>
          <section className={styles.section} data-tour="chart">
            <h3 className={styles.sectionTitle}>{t("dockSectionChart")}</h3>
            <div className={styles.fit}>
              <ChartPanel />
            </div>
          </section>
          <section className={styles.section} data-tour="calendar">
            <h3 className={styles.sectionTitle}>{t("dockSectionCalendar")}</h3>
            <div className={styles.fit}>
              <BurnCalendar />
            </div>
          </section>
          </div>
        </div>
      )}
    </div>
  );
}
