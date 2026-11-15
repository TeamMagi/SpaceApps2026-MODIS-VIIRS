/**
 * MethodsPage — full-page, printable methods reference at `#/methods[/section]`.
 *
 * Top bar (back to map, language, theme, print), the disclaimer banner under it, then the sections
 * the section nav links to: pipeline, viewing angle (VZA), spatial grid (H3), references, limits.
 * Static content renders immediately; only the parameter values come from `GET /api/config`, with
 * the code defaults as fallback.
 */

import { useEffect } from "react";
import { REFERENCES } from "../../content/references";
import { useCopy } from "../../hooks/useCopy";
import { useLeaveMethods } from "../../hooks/useRoute";
import { METHODS_SECTIONS, methodsHash, type MethodsSection } from "../../lib/route";
import { useAppStore } from "../../store/useAppStore";
import type { Theme } from "../../types";
import LangSwitch from "../Header/LangSwitch";
import DisclaimerBanner from "./DisclaimerBanner";
import H3GridFigure from "./H3GridFigure";
import MethodsPipeline from "./MethodsPipeline";
import MethodsReferences from "./MethodsReferences";
import VzaCurveFigure from "./VzaCurveFigure";
import { useMethodsConfig } from "./useMethodsConfig";
import styles from "./MethodsPage.module.css";

const THEMES: readonly Theme[] = ["dark", "light"];

const NAV_COPY = {
  pipeline: "methodsNavPipeline",
  vza: "methodsNavVza",
  h3: "methodsNavH3",
  references: "methodsNavReferences",
  limits: "methodsNavLimits",
} as const satisfies Record<MethodsSection, string>;

const sectionId = (section: MethodsSection) => `methods-${section}`;

export default function MethodsPage({ section }: { section: MethodsSection | null }) {
  const { t, lang } = useCopy();
  const leave = useLeaveMethods();
  const theme = useAppStore((state) => state.theme);
  const setTheme = useAppStore((state) => state.setTheme);
  const methodsConfig = useMethodsConfig();
  const { config } = methodsConfig;

  useEffect(() => {
    const previous = document.title;
    document.title = `${t("methodsPageTitle")} · ${t("appName")}`;
    return () => {
      document.title = previous;
    };
  }, [t, lang]);

  useEffect(() => {
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const behavior = reduceMotion ? "auto" : "smooth";
    if (section) document.getElementById(sectionId(section))?.scrollIntoView({ behavior });
    else window.scrollTo({ top: 0, behavior: "auto" });
  }, [section]);

  return (
    <div className={styles.page}>
      <header className={styles.bar}>
        <button type="button" className={styles.back} onClick={leave}>
          {t("methodsBackToMap")}
        </button>
        <span className={styles.crumb}>
          {t("appName")} · {t("navMethods")}
        </span>
        <div className={styles.tools}>
          <LangSwitch />
          <div className={styles.themes} role="group" aria-label={t("themeLabel")}>
            {THEMES.map((option) => (
              <button
                key={option}
                type="button"
                className={styles.themeOption}
                aria-pressed={theme === option}
                onClick={() => setTheme(option)}
              >
                {option === "dark" ? t("themeDark") : t("themeLight")}
              </button>
            ))}
          </div>
          <button type="button" className={styles.print} onClick={() => window.print()}>
            {t("methodsPrint")}
          </button>
        </div>
      </header>

      <main className={styles.main} data-tour="methods">
        <DisclaimerBanner />

        <h1 className={styles.title}>{t("methodsPageTitle")}</h1>
        <p className={styles.lead}>{t("methodsPageLead")}</p>

        <nav className={styles.nav} aria-label={t("methodsNavLabel")}>
          {METHODS_SECTIONS.map((id) => (
            <a key={id} href={methodsHash(id)} className={styles.navLink}>
              {t(NAV_COPY[id])}
            </a>
          ))}
        </nav>

        <section id={sectionId("pipeline")} className={styles.section} aria-labelledby="methods-pipeline-title">
          <h2 id="methods-pipeline-title" className={styles.heading}>
            {t("methodsPipelineTitle")}
          </h2>
          <p className={styles.intro}>{t("methodsPipelineIntro")}</p>
          <MethodsPipeline {...methodsConfig} />
        </section>

        <section id={sectionId("vza")} className={styles.section} aria-labelledby="methods-vza-title">
          <h2 id="methods-vza-title" className={styles.heading}>
            {t("methodsVzaTitle")}
          </h2>
          <p className={styles.intro}>{t("methodsVzaIntro")}</p>
          <VzaCurveFigure vza={config.vza} uncertainty={config.uncertainty} />
        </section>

        <section id={sectionId("h3")} className={styles.section} aria-labelledby="methods-h3-title">
          <h2 id="methods-h3-title" className={styles.heading}>
            {t("methodsH3Title")}
          </h2>
          <p className={styles.intro}>{t("methodsH3Intro")}</p>
          <H3GridFigure matchRadiusKm={config.matching.radiusKm} />
        </section>

        <section id={sectionId("references")} className={styles.section} aria-labelledby="methods-references-title">
          <h2 id="methods-references-title" className={styles.heading}>
            {t("methodsReferencesTitle")}
          </h2>
          <p className={styles.intro}>{t("methodsReferencesIntro")}</p>
          <MethodsReferences entries={REFERENCES} />
        </section>

        <section id={sectionId("limits")} className={styles.section} aria-labelledby="methods-limits-title">
          <h2 id="methods-limits-title" className={styles.heading}>
            {t("methodsLimitsTitle")}
          </h2>
          <p className={styles.intro}>{t("disclaimer")}</p>
          <p className={styles.intro}>{t("methodsDisclaimerBody")}</p>
        </section>
      </main>
    </div>
  );
}
