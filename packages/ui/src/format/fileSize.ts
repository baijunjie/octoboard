import type { Language } from "../i18n/languages";

const UNITS = ["byte", "kilobyte", "megabyte", "gigabyte"] as const;

/** A file size in decimal units (1 kB = 1,000 bytes, as macOS shows sizes), worded in `language`. */
export function formatFileSize(language: Language, bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000;
    unit += 1;
  }
  return new Intl.NumberFormat(language, {
    style: "unit",
    unit: UNITS[unit],
    unitDisplay: unit === 0 ? "long" : "short",
    maximumFractionDigits: unit === 0 ? 0 : 1,
  }).format(value);
}

/** One side's size in a change, `—` for a side with no body. */
export function formatSideSize(language: Language, bytes: number | null): string {
  return bytes === null ? "—" : formatFileSize(language, bytes);
}
