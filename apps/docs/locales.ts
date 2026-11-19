// The docs' locale table for Starlight, built from the application's language list, as the website's is. English is
// the root locale (no path prefix) and the fallback: Starlight shows the English page, marked as untranslated, where a
// language has none of its own. A locale's path segment and its `lang` are both the language's tag, as the website's
// are: the tag is the page's `lang` and `hreflang`, and the one Starlight looks its own UI strings up by.
import { FALLBACK_LANGUAGE, LANGUAGE_NAMES, LANGUAGES, textDirection } from "@octoboard/ui/languages";

type StarlightLocale = { label: string; lang: string; dir?: "rtl" | "ltr" };

export const locales: Record<string, StarlightLocale> = Object.fromEntries(
  LANGUAGES.map((language) => [
    language === FALLBACK_LANGUAGE ? "root" : language,
    { label: LANGUAGE_NAMES[language], lang: language, dir: textDirection(language) },
  ]),
);
