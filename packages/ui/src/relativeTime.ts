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

/** Whether `at` is under a minute before `now`, which `formatRelativeTime` words as "now": the caller says
 * "just now" where its sentence cannot take the bare "now". */
export function isJustNow(at: number, now: number): boolean {
  return now - at < UNITS[UNITS.length - 1]![1];
}

/** "5 minutes ago" for a past instant in milliseconds, in `language`; under a minute is "now". `now`
 * is taken from the caller rather than the clock so that a component passes a value it renders on
 * (`useNow`): a call reading the clock itself is cached on its other inputs and never advances. */
export function formatRelativeTime(language: Language, at: number, now: number): string {
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
