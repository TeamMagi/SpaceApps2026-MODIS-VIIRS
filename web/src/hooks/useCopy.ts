import { useMemo } from "react";
import { createTranslator, type Translate } from "../content/copy";
import { createFormatter, type Formatter } from "../lib/format";
import { useAppStore } from "../store/useAppStore";
import type { Lang } from "../types";

export interface CopyApi {
  lang: Lang;
  /** `t("countSummary", { raw, unique })` — vars are type-checked per key. */
  t: Translate;
  /** Number/date formatters bound to the current language. */
  fmt: Formatter;
}

/** Current-language copy and formatters; re-renders only when the language changes. */
export function useCopy(): CopyApi {
  const lang = useAppStore((state) => state.lang);
  return useMemo(() => ({ lang, t: createTranslator(lang), fmt: createFormatter(lang) }), [lang]);
}
