import type { Language } from "./languages";

// Cached per language, as `relativeTime.ts`'s `formats` and `catalog.ts`'s `pluralRules` are: a
// project row's git badge and a session row's binding fact each build this twice per render, and
// `Intl.ListFormat`'s own constructor is the expensive part of formatting, not the `format` call.
const listFormats = new Map<Language, Intl.ListFormat>();

/** Joins independent phrases into one sentence, correctly punctuated for `language` —
 * `Intl.ListFormat` rather than a hardcoded separator, since the right punctuation between list
 * items differs by language (a plain ASCII comma reads wrong in a Chinese sentence, for one).
 * `narrow` uses the shortest joining form, since these are independent facts read off a status
 * or a row. Its joining words depend on the runtime's locale data. */
export function joinPhrases(language: Language, phrases: string[]): string {
  let format = listFormats.get(language);
  if (!format) {
    format = new Intl.ListFormat(language, { style: "narrow", type: "conjunction" });
    listFormats.set(language, format);
  }
  if (language !== "zh-Hans") return format.format(phrases);
  // Only adjust the separators: user-provided names and errors must stay verbatim. Directional
  // isolates do not change whether Chinese and Latin text meet at the separator's edge.
  const parts = format.formatToParts(phrases);
  return parts
    .map((part, index) => {
      if (part.type !== "literal") return part.value;
      const previous = parts[index - 1]?.value.replace(/[\u2066-\u2069]/g, "") ?? "";
      const next = parts[index + 1]?.value.replace(/[\u2066-\u2069]/g, "") ?? "";
      const before = /[\p{Script=Latin}\p{N}]$/u.test(previous) && /^\p{Script=Han}/u.test(part.value) ? " " : "";
      const after = /\p{Script=Han}$/u.test(part.value) && /^[\p{Script=Latin}\p{N}]/u.test(next) ? " " : "";
      return before + part.value + after;
    })
    .join("");
}
