import { isImeKey } from "../input/imeKey";

/** xterm leaves Command-arrows unencoded and uses cursor-key sequences for Option-arrows.
 * Keep Option's character entry intact by translating only these editing chords, rather than
 * enabling `macOptionIsMeta` for every Option key. */
export function macEditingSequence(event: KeyboardEvent): string | undefined {
  if (isImeKey(event) || event.ctrlKey || event.shiftKey) return undefined;
  if (event.metaKey && !event.altKey) {
    if (event.key === "ArrowLeft") return "\x01";
    if (event.key === "ArrowRight") return "\x05";
    if (event.key === "Backspace") return "\x15";
  } else if (event.altKey && !event.metaKey) {
    if (event.key === "ArrowLeft") return "\x1bb";
    if (event.key === "ArrowRight") return "\x1bf";
  }
  return undefined;
}

/** xterm sends a carriage return for `Shift+Enter`, the same as for `Enter`, so a program cannot tell
 * them apart. Escape followed by a carriage return (Meta+Enter) is what Claude Code's terminal setup
 * binds `Shift+Enter` to in terminals without a distinct encoding for it, and a prompt that reads
 * Meta+Enter as a newline then gets one instead of a submit. A key that sends something of its own uses the full
 * input-method check, `isImeKey`. */
export function shiftEnterSequence(event: KeyboardEvent): string | undefined {
  if (isImeKey(event) || event.key !== "Enter" || !event.shiftKey) return undefined;
  if (event.ctrlKey || event.metaKey || event.altKey) return undefined;
  return "\x1b\r";
}
