import { createPersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";

export const MIN_TERMINAL_FONT_SIZE = 10;
export const MAX_TERMINAL_FONT_SIZE = 24;

/** xterm's own default size. */
export const DEFAULT_TERMINAL_FONT_SIZE = 15;

/** The whole size within the range that `size` stands for; the default for a value that is not a number. */
export function clampTerminalFontSize(size: number): number {
  if (!Number.isFinite(size)) return DEFAULT_TERMINAL_FONT_SIZE;
  return Math.min(MAX_TERMINAL_FONT_SIZE, Math.max(MIN_TERMINAL_FONT_SIZE, Math.round(size)));
}

/** The size a stored text names; the default for no text or one that is not a number. */
export function parseTerminalFontSize(raw: string | null): number {
  return raw === null || raw.trim() === "" ? DEFAULT_TERMINAL_FONT_SIZE : clampTerminalFontSize(Number(raw));
}

/** The terminals' font size in pixels: the user's choice in Settings (or the font size shortcuts), kept in this
 * client's `localStorage` and applied to every open terminal at once. */
export const terminalFontSize = createPersistedPreference<number>(
  PREFERENCE_KEYS.terminalFontSize,
  parseTerminalFontSize,
  (value) => (value === DEFAULT_TERMINAL_FONT_SIZE ? null : String(value)),
);

/** Moves the size by `delta` pixels, held within the range. */
export function stepTerminalFontSize(delta: number): void {
  terminalFontSize.set(clampTerminalFontSize(terminalFontSize.get() + delta));
}

export function resetTerminalFontSize(): void {
  terminalFontSize.set(DEFAULT_TERMINAL_FONT_SIZE);
}
