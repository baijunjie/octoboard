/** Hands focus to the terminal when a change of what is on screen left it on nothing, or on a
 * control that has just become disabled. */
export function refocusIfLost(focusTerminal: () => void): void {
  const active = document.activeElement;
  if (!active || active === document.body || active.matches(":disabled")) focusTerminal();
}
