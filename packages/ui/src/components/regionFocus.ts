/** What a region's focus lands on, beyond the terminal's own method: its first control a keyboard
 * can reach. A report page's own frame counts, as the last thing in the aside; the connection
 * banner's is its Retry, and it has none while it only says it is reconnecting. */
export const FOCUSABLE =
  'a[href], button:not([disabled]), iframe, [tabindex]:not([tabindex="-1"]):not([aria-disabled="true"])';

/** Puts focus on the first control a keyboard can reach in the region marked `data-region`; a
 * disabled control cannot hold focus and is skipped. */
export function focusFirstIn(region: string): void {
  document.querySelector<HTMLElement>(`[data-region=${region}] :is(${FOCUSABLE})`)?.focus();
}

/** Puts focus on the top bar's first control: where focus goes when there is no better place to
 * hand it. Its Back button is often disabled, in which case the next control takes it. */
export function focusTopBar(): void {
  focusFirstIn("topbar");
}
