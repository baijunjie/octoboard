/** What a region's focus lands on, beyond the terminal's own method: its first control a keyboard
 * can reach. A report page's own frame counts, as the last thing in the aside; the connection
 * banner's is its Retry, and it has none while it only says it is reconnecting. */
export const FOCUSABLE =
  'a[href], button:not([disabled]), iframe, [tabindex]:not([tabindex="-1"]):not([aria-disabled="true"])';

/** Focuses where F6 lands in `root`, a region's element: the control it marks `data-region-landing`,
 * else its first control a keyboard can reach; says whether focus is there now. */
export function focusLanding(root: Element): boolean {
  const target = root.querySelector<HTMLElement>("[data-region-landing]") ?? root.querySelector<HTMLElement>(FOCUSABLE);
  target?.focus();
  return target != null && target.ownerDocument.activeElement === target;
}

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

/** Returns to the foremost modal instead of an inert window region. A remembered control is
 * preferred while it still belongs to that dialog; the dialog itself survives replaced content. */
export function focusOpenDialog(previous?: Element | null): boolean {
  const dialogs = document.querySelectorAll<HTMLElement>(
    '[role="dialog"]:not([aria-modal="false"]), [role="alertdialog"]:not([aria-modal="false"])',
  );
  const dialog = Array.from(dialogs).reverse().find((element) => !element.closest('[inert], [aria-hidden="true"]'));
  if (!dialog) return false;
  const target = previous instanceof HTMLElement && dialog.contains(previous) && !previous.matches(":disabled")
    ? previous
    : dialog;
  target.focus();
  return dialog.contains(document.activeElement);
}
