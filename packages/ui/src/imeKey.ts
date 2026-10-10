/** Whether a key press is an input method's: it is composing text, or (`keyCode` 229) the engine
 * handled the key for it without saying so, as some do. The composition owns such a key, so
 * whatever else would act on it leaves it alone. A plain `isComposing` check is enough where the
 * handler already requires a named key: a key the engine consumed is reported with `key` "Process",
 * which that check rejects, so the 229 arm would add nothing. */
export function isImeKey(event: Pick<KeyboardEvent, "isComposing" | "keyCode">): boolean {
  return event.isComposing || event.keyCode === 229;
}
