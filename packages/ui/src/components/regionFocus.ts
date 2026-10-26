import { ringForAutomaticFocus, TEXT_ENTRY } from "../touchInput";

/** A window region, named by its `data-region`. */
export type Region = string;

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

/** Where a tap puts focus in `root`: the control it marks `data-touch-landing` (a report page), else
 * where `focusLanding` would unless that is a field that takes text (the Terminal panel's shell,
 * say), and then `root`'s first control that is not; says whether focus is there now. A tap must
 * not focus such a field, which would raise the on-screen keyboard (`interactionIsTouch`). */
export function focusLandingWithoutText(root: Element): boolean {
  const landing = root.querySelector<HTMLElement>("[data-touch-landing]") ?? root.querySelector<HTMLElement>("[data-region-landing]");
  const target = landing && !landing.matches(TEXT_ENTRY)
    ? landing
    : Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).find((control) => !control.matches(TEXT_ENTRY));
  target?.focus();
  return target != null && target.ownerDocument.activeElement === target;
}

/** Puts focus on the first control a keyboard can reach in the region marked `data-region`; a
 * disabled control cannot hold focus and is skipped. */
export function focusFirstIn(region: Region): void {
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

/** A toast that is on screen, not one fading out. */
const LIVE_TOAST = '[data-slot="toast"]:not([data-exiting="true"]):not([data-hidden="true"])';

export function regionElements(region: Region): Element[] {
  return Array.from(document.querySelectorAll(`[data-region=${region}]`));
}

/** Focuses where `region` takes focus and says whether it did: a control the region marks with
 * `data-region-landing` (a panel that failed to load, whose Try again follows its header), else the
 * sidebar's selected row or the rail's current console, else the first control, as the other regions
 * that are not the terminal do. The toast stack lists its newest toast first, so its first control
 * is that toast. */
export function focusRegion(region: Region, focusTerminal: () => void): boolean {
  if (region === "terminal") {
    focusTerminal();
    return regionElements(region).some((el) => el.contains(document.activeElement));
  }
  const root = regionElements(region)[0];
  if (!root) return false;
  const target =
    region === "toast"
      ? root.querySelector<HTMLElement>(LIVE_TOAST)
      : root.querySelector<HTMLElement>("[data-region-landing]") ||
        ((region === "sidebar" || region === "rail") && root.querySelector<HTMLElement>('[aria-current="true"]')) ||
        root.querySelector<HTMLElement>(FOCUSABLE);
  if (!target) return false;
  target.focus();
  return target.ownerDocument.activeElement === target;
}

/** Where focus goes when the control that held it has gone and nothing hands it back (an alert acknowledged
 * while focus is on `<body>`): the dialog still open (the window's regions are inert behind it), else
 * wherever `focusTerminal` puts it, which for the selected session's terminal is the terminal; the empty
 * terminal and the welcome screen decline from `<body>`, so then the first of the sidebar and the rail that has
 * a control takes it, each landing where F6 would. With none to take it, focus stays on `<body>`. Keyboard modality
 * gives the fallback a visible focus ring even when a pointer acknowledged the alert, except on a touch screen
 * (`ringForAutomaticFocus`). */
export function focusAfterLoss(focusTerminal: () => void): void {
  ringForAutomaticFocus();
  if (focusOpenDialog()) return;
  focusTerminal();
  if (document.activeElement && document.activeElement !== document.body) return;
  for (const region of ["sidebar", "rail"] as const) {
    if (focusRegion(region, focusTerminal)) return;
  }
}

/** Where focus goes when something inside `region` goes while it (or a popover it opened) holds focus: the region's
 * first control. React suspends focus events during removal, so it waits until the surviving control can observe its
 * focus and draw its ring; another hand-off in the same commit takes precedence. */
export function handFocusToRegionStart(region: Region): void {
  queueMicrotask(() => {
    if (document.activeElement === document.body) focusAfterLoss(() => focusFirstIn(region));
  });
}
