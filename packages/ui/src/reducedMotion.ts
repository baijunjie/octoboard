/** Whether the system asks for reduced motion. Read when needed rather than kept, so a change of the
 * setting is honoured; false where there is no `matchMedia` (a test's DOM). */
export function prefersReducedMotion(): boolean {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}
