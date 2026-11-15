import { useEffect, useState } from "react";
import { loadCaseStudy, type CaseStudySummary } from "../lib/caseStudy";

/** The Türkiye case-study figures once loaded; null while loading, on failure, or when disabled. */
export function useCaseStudy(enabled: boolean): CaseStudySummary | null {
  const [summary, setSummary] = useState<CaseStudySummary | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let active = true;
    void loadCaseStudy().then((value) => {
      if (active) setSummary(value);
    });
    return () => {
      active = false;
    };
  }, [enabled]);
  return enabled ? summary : null;
}
