import type { Lang, MapPeriod, MonthRef } from "../types";

/** Typographic minus; the hyphen-minus Intl emits looks wrong next to tabular digits. */
export const MINUS = "−";

const NUMBER_LOCALE: Record<Lang, string> = { en: "en-US", tr: "tr-TR" };

const cache = new Map<string, Intl.NumberFormat>();

function numberFormat(lang: Lang, options: Intl.NumberFormatOptions): Intl.NumberFormat {
  const key = `${lang}|${JSON.stringify(options)}`;
  let formatter = cache.get(key);
  if (!formatter) {
    formatter = new Intl.NumberFormat(NUMBER_LOCALE[lang], options);
    cache.set(key, formatter);
  }
  return formatter;
}

/** "6,520" / "6.520"; fixed fraction digits. Negative values use the typographic minus. */
export function formatNumber(lang: Lang, value: number, fractionDigits = 0): string {
  const text = numberFormat(lang, {
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  }).format(Math.abs(value));
  return value < 0 && text.replace(/[^1-9]/g, "") !== "" ? `${MINUS}${text}` : text;
}

/** Ratio -> "17.7%" / "%17,7". */
export function formatPercent(lang: Lang, ratio: number, fractionDigits = 1): string {
  const text = numberFormat(lang, {
    style: "percent",
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  }).format(Math.abs(ratio));
  return ratio < 0 && text.replace(/[^1-9]/g, "") !== "" ? `${MINUS}${text}` : text;
}

function withSign(value: number, unsigned: string): string {
  if (unsigned.replace(/[^1-9]/g, "") === "") return unsigned;
  return `${value < 0 ? MINUS : "+"}${unsigned}`;
}

/** Always signed: "+2.3", "−1,157". Zero stays unsigned. */
export function formatSigned(lang: Lang, value: number, fractionDigits = 0): string {
  return withSign(value, formatNumber(lang, Math.abs(value), fractionDigits));
}

/** Signed ratio: "−14.4%" / "−%14,4". */
export function formatSignedPercent(lang: Lang, ratio: number, fractionDigits = 1): string {
  return withSign(ratio, formatPercent(lang, Math.abs(ratio), fractionDigits));
}

/** Anomaly z-score: "+2.3σ" / "+2,3σ". */
export function formatSigma(lang: Lang, z: number): string {
  return `${formatSigned(lang, z, 1)}σ`;
}

/** "45.2 MW" / "45,2 MW". */
export function formatMw(lang: Lang, value: number): string {
  return `${formatNumber(lang, value, 1)} MW`;
}

// en-US, not en-GB: en-GB abbreviates September as "Sept".
const DATE_LOCALE: Record<Lang, string> = { en: "en-US", tr: "tr-TR" };
const monthCache = new Map<string, readonly string[]>();

function monthNames(lang: Lang, style: "short" | "long"): readonly string[] {
  const key = `${lang}|${style}`;
  let names = monthCache.get(key);
  if (!names) {
    const formatter = new Intl.DateTimeFormat(DATE_LOCALE[lang], { month: style, timeZone: "UTC" });
    names = Array.from({ length: 12 }, (_, i) => formatter.format(Date.UTC(2020, i, 15)));
    monthCache.set(key, names);
  }
  return names;
}

/** 1-based month -> "Aug" / "Ağu" (short) or "August" / "Ağustos" (long). */
export function monthName(lang: Lang, month: number, style: "short" | "long" = "short"): string {
  return monthNames(lang, style)[month - 1] ?? "";
}

/** Month name for use inside a sentence: "August" / "ağustos" (Turkish month names are lower-case mid-sentence). */
export function monthNameInSentence(lang: Lang, month: number): string {
  const name = monthName(lang, month, "long");
  return lang === "tr" ? name.toLocaleLowerCase("tr-TR") : name;
}

/** "Aug 2020" / "Ağu 2020"; long: "August 2020" / "Ağustos 2020". */
export function formatMonthYear(lang: Lang, ref: MonthRef, style: "short" | "long" = "short"): string {
  return `${monthName(lang, ref.month, style)} ${ref.year}`;
}

/** "15 Jul 2020 12:30 UTC" / "15 Tem 2020 12:30 UTC". Always UTC, never local time. */
export function formatUtcDateTime(lang: Lang, input: string | Date): string {
  const date = typeof input === "string" ? new Date(input) : input;
  if (Number.isNaN(date.getTime())) return "";
  const day = String(date.getUTCDate()).padStart(2, "0");
  const hour = String(date.getUTCHours()).padStart(2, "0");
  const minute = String(date.getUTCMinutes()).padStart(2, "0");
  return `${day} ${monthName(lang, date.getUTCMonth() + 1)} ${date.getUTCFullYear()} ${hour}:${minute} UTC`;
}

/** "2020-07" — used in URLs and file names. */
export function monthKey(ref: MonthRef): string {
  return `${ref.year}-${String(ref.month).padStart(2, "0")}`;
}

/** "pyrosync_turkey_2021-08.csv"; whole-year views use the year alone. */
export function csvFileName(regionSlug: string, period: MapPeriod): string {
  const suffix =
    period.year === null
      ? "archive"
      : period.month === null
        ? String(period.year)
        : monthKey({ year: period.year, month: period.month });
  return `pyrosync_${regionSlug}_${suffix}.csv`;
}

export interface Formatter {
  number: (value: number, fractionDigits?: number) => string;
  percent: (ratio: number, fractionDigits?: number) => string;
  signed: (value: number, fractionDigits?: number) => string;
  signedPercent: (ratio: number, fractionDigits?: number) => string;
  sigma: (z: number) => string;
  mw: (value: number) => string;
  monthName: (month: number, style?: "short" | "long") => string;
  monthNameInSentence: (month: number) => string;
  monthYear: (ref: MonthRef, style?: "short" | "long") => string;
  utcDateTime: (input: string | Date) => string;
}

/** All formatters bound to one language (see hooks/useCopy). */
export function createFormatter(lang: Lang): Formatter {
  return {
    number: (value, digits) => formatNumber(lang, value, digits),
    percent: (ratio, digits) => formatPercent(lang, ratio, digits),
    signed: (value, digits) => formatSigned(lang, value, digits),
    signedPercent: (ratio, digits) => formatSignedPercent(lang, ratio, digits),
    sigma: (z) => formatSigma(lang, z),
    mw: (value) => formatMw(lang, value),
    monthName: (month, style) => monthName(lang, month, style),
    monthNameInSentence: (month) => monthNameInSentence(lang, month),
    monthYear: (ref, style) => formatMonthYear(lang, ref, style),
    utcDateTime: (input) => formatUtcDateTime(lang, input),
  };
}
