/** Every language Octoboard offers, ordered by tag. The order is the order of the options in
 * Settings, and it breaks ties when a system language matches more than one entry (see
 * `matchLanguage`), which is why a bare `zh` lands on `zh-Hans`. */
export const LANGUAGES = [
  "ar",
  "de",
  "en",
  "es",
  "fr",
  "hi",
  "id",
  "it",
  "ja",
  "ko",
  "pt-BR",
  "ru",
  "th",
  "tr",
  "vi",
  "zh-Hans",
  "zh-Hant",
] as const;

export type Language = (typeof LANGUAGES)[number];

/** What the UI renders when no system language maps onto the list, and what a message missing from
 * a language's catalog falls back to. Named on its own rather than taken from the list's order. */
export const FALLBACK_LANGUAGE: Language = "en";

/** Each language's name in that language itself, as the Settings option shows it. */
export const LANGUAGE_NAMES: Record<Language, string> = {
  ar: "العربية",
  de: "Deutsch",
  en: "English",
  es: "Español",
  fr: "Français",
  hi: "हिन्दी",
  id: "Bahasa Indonesia",
  it: "Italiano",
  ja: "日本語",
  ko: "한국어",
  "pt-BR": "Português (Brasil)",
  ru: "Русский",
  th: "ไทย",
  tr: "Türkçe",
  vi: "Tiếng Việt",
  "zh-Hans": "简体中文",
  "zh-Hant": "繁體中文",
};

const RIGHT_TO_LEFT: ReadonlySet<Language> = new Set<Language>(["ar"]);

export function textDirection(language: Language): "ltr" | "rtl" {
  return RIGHT_TO_LEFT.has(language) ? "rtl" : "ltr";
}

export function isLanguage(value: unknown): value is Language {
  return typeof value === "string" && (LANGUAGES as readonly string[]).includes(value);
}
