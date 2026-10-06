import type { Language } from "./i18n/languages";

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 3600_000],
  ["month", 30 * 24 * 3600_000],
  ["week", 7 * 24 * 3600_000],
  ["day", 24 * 3600_000],
  ["hour", 3600_000],
  ["minute", 60_000],
];

const formats = new Map<Language, Intl.RelativeTimeFormat>();

/** "5 minutes ago" for a past instant in milliseconds, in `language`; under a minute is "now". */
export function formatRelativeTime(language: Language, at: number, now = Date.now()): string {
  let format = formats.get(language);
  if (!format) {
    format = new Intl.RelativeTimeFormat(language, { numeric: "auto" });
    formats.set(language, format);
  }
  const elapsed = now - at;
  for (const [unit, size] of UNITS) {
    if (elapsed >= size) return format.format(-Math.floor(elapsed / size), unit);
  }
  return format.format(0, "second");
}
