/** Puts focus on the top bar's first enabled button (not a disabled one, as Back often is, which
 * cannot hold focus): where focus goes when there is no better place to hand it. */
export function focusTopBar(): void {
  document.querySelector<HTMLElement>("[data-region=topbar] button:not([disabled])")?.focus();
}
