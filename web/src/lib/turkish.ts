/**
 * Turkish case suffixes for formatted numbers ("45,2'den", "1.157'si", "%17,7'si").
 * The suffix follows vowel and consonant harmony of the LAST SPOKEN WORD of the number,
 * so it cannot be hard-coded in copy strings once the number is dynamic.
 */

export type TurkishSuffix = "abl" | "poss" | "loc";

const DIGIT_WORDS = ["sıfır", "bir", "iki", "üç", "dört", "beş", "altı", "yedi", "sekiz", "dokuz"];
const TEN_WORDS = ["", "on", "yirmi", "otuz", "kırk", "elli", "altmış", "yetmiş", "seksen", "doksan"];
const VOWELS = "aeıioöuü";
const BACK_VOWELS = "aıou";
const HARD_CONSONANTS = "çfhkpsşt";

/** Last word when the number in a tr-TR formatted string is read aloud. */
function lastSpokenWord(formatted: string): string {
  const cleaned = formatted.replace(/[^\d,]/g, "");
  const comma = cleaned.lastIndexOf(",");
  if (comma >= 0 && comma < cleaned.length - 1) {
    return DIGIT_WORDS[Number(cleaned[cleaned.length - 1])];
  }
  const n = Number(cleaned.replace(/,/g, ""));
  if (!Number.isFinite(n) || n === 0) return "sıfır";
  if (n % 10 !== 0) return DIGIT_WORDS[n % 10];
  if (n % 100 !== 0) return TEN_WORDS[(n % 100) / 10];
  if (n % 1000 !== 0) return "yüz";
  if (n % 1_000_000 !== 0) return "bin";
  if (n % 1_000_000_000 !== 0) return "milyon";
  return "milyar";
}

/** Suffix without the apostrophe: ablative ("den/dan/ten/tan") or 3sg possessive ("si/sı/i/ı/..."). */
export function trSuffix(formatted: string, kind: TurkishSuffix): string {
  const word = lastSpokenWord(formatted);
  const chars = [...word];
  const lastChar = chars[chars.length - 1];
  const lastVowel = [...chars].reverse().find((c) => VOWELS.includes(c)) ?? "e";
  const back = BACK_VOWELS.includes(lastVowel);

  if (kind === "abl") {
    const d = HARD_CONSONANTS.includes(lastChar) ? "t" : "d";
    return d + (back ? "an" : "en");
  }

  if (kind === "loc") {
    const d = HARD_CONSONANTS.includes(lastChar) ? "t" : "d";
    return d + (back ? "a" : "e");
  }

  const fourWay = "aı".includes(lastVowel) ? "ı" : "ei".includes(lastVowel) ? "i" : "ou".includes(lastVowel) ? "u" : "ü";
  return VOWELS.includes(lastChar) ? `s${fourWay}` : fourWay;
}
