/** Whether the system asks for reduced motion. Read when needed rather than kept, so a change of the
 * setting is honoured; false where there is no `matchMedia` (a test's DOM). */
export function prefersReducedMotion(): boolean {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

/** Calls `listener` whenever the system starts asking for reduced motion, so an effect already running
 * can be cut short rather than finish. Returns the unsubscribe; does nothing where there is no
 * `matchMedia` (a test's DOM). */
export function onReducedMotion(listener: () => void): () => void {
  const query = window.matchMedia?.("(prefers-reduced-motion: reduce)");
  if (!query?.addEventListener) return () => {};
  const changed = () => { if (query.matches) listener(); };
  query.addEventListener("change", changed);
  return () => query.removeEventListener("change", changed);
}
