/** xterm leaves Command-arrows unencoded and uses cursor-key sequences for Option-arrows.
 * Keep Option's character entry intact by translating only these editing chords, rather than
 * enabling `macOptionIsMeta` for every Option key. */
export function macEditingSequence(event: KeyboardEvent): string | undefined {
  if (event.isComposing || event.ctrlKey || event.shiftKey) return undefined;
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
