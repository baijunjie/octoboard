import type { Language } from "./languages";

// Cached per language, as `relativeTime.ts`'s `formats` and `catalog.ts`'s `pluralRules` are: a
// project row's git badge and a session row's binding fact each build this twice per render, and
// `Intl.ListFormat`'s own constructor is the expensive part of formatting, not the `format` call.
const listFormats = new Map<Language, Intl.ListFormat>();

/** Joins independent phrases into one sentence, correctly punctuated for `language` —
 * `Intl.ListFormat` rather than a hardcoded separator, since the right punctuation between list
 * items differs by language (a plain ASCII comma reads wrong in a Chinese sentence, for one).
 * `narrow` drops the joining word before the last item ("and" / "、"), since these are independent
 * facts read off a status or a row, not items in a conjunction. */
export function joinPhrases(language: Language, phrases: string[]): string {
  let format = listFormats.get(language);
  if (!format) {
    format = new Intl.ListFormat(language, { style: "narrow", type: "conjunction" });
    listFormats.set(language, format);
  }
  return format.format(phrases);
}
