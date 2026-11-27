/** Whether a key press is an input method's: it is composing text, or (`keyCode` 229) the engine
 * handled the key for it without saying so, as some do. The composition owns such a key, so
 * whatever else would act on it leaves it alone. Engines usually report a key they consumed with
 * `key` "Process", which a handler requiring a named key already rejects; a handler that would send or
 * insert something on the key still checks 229, against an engine that reports the committing key by
 * its own name, since a leak there lands in the user's text. */
export function isImeKey(event: Pick<KeyboardEvent, "isComposing" | "keyCode">): boolean {
  return event.isComposing || event.keyCode === 229;
}
