/** MethodsReferences — numbered reference list; entries still missing a verified full citation say so. */

import { PIPELINE_COPY } from "../../content/copy";
import type { ReferenceEntry } from "../../content/references";
import { useCopy } from "../../hooks/useCopy";
import styles from "./MethodsReferences.module.css";

export default function MethodsReferences({ entries }: { entries: readonly ReferenceEntry[] }) {
  const { t } = useCopy();

  return (
    <ol className={styles.list}>
      {entries.map((entry) => (
        <li key={entry.id} className={styles.entry}>
          <p className={styles.citation}>
            {entry.authors}
            {entry.year !== null && ` (${entry.year}).`}
            {entry.title && <> {entry.title}.</>}
            {entry.venue && <> <em>{entry.venue}</em>.</>}
            {entry.locator && <> {entry.locator}.</>}
          </p>
          {entry.url && (
            <p className={styles.url}>
              <a href={entry.url} target="_blank" rel="noopener noreferrer">
                {entry.url}
              </a>
            </p>
          )}
          <p className={styles.meta}>
            {t("methodsReferenceUsedIn")}: {entry.stages.map((stage) => t(PIPELINE_COPY[stage].name)).join(", ")}
            {!entry.complete && <span className={styles.pending}>{t("methodsReferencePending")}</span>}
          </p>
        </li>
      ))}
    </ol>
  );
}
