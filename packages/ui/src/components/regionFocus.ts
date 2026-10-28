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

/** Marks the control a dialog's opening focus goes to, the one given `autoFocus`: react-aria leaves no trace of it in
 * the page, and for a press without a pointer it drops that focus if anything else took focus first (an alert raised as
 * the dialog opens), so `focusOpenDialog` finds it by this mark. `Button` and `TextInput` set it from their `autoFocus`;
 * a dialog that focuses a control of its own on opening sets it on that control. */
export function openingFocus(autoFocus: boolean | undefined): { "data-opening-focus"?: "" } {
  return autoFocus ? { "data-opening-focus": "" } : {};
}

/** Returns to the foremost modal instead of an inert window region. A remembered control is
 * preferred while it still belongs to that dialog; the dialog itself survives replaced content.
 * With `opening`, focus that has no remembered control goes where the dialog's opening focus goes
 * (the control it opens on, marked by `openingFocus`) rather than to the dialog: the
 * dialog may never have had that focus, and the dialog itself is where only a dialog that opens on
 * itself (a destructive confirmation, an unprompted question) starts. */
export function focusOpenDialog(previous?: Element | null, { opening = false }: { opening?: boolean } = {}): boolean {
  const dialogs = document.querySelectorAll<HTMLElement>(
    '[role="dialog"]:not([aria-modal="false"]), [role="alertdialog"]:not([aria-modal="false"])',
  );
  const dialog = Array.from(dialogs).reverse().find((element) => !element.closest('[inert], [aria-hidden="true"]'));
  if (!dialog) return false;
  const usable = (element: Element | null | undefined): element is HTMLElement =>
    element instanceof HTMLElement && dialog.contains(element) && !element.matches(":disabled") && !element.closest("[hidden]");
  const landing = opening ? Array.from(dialog.querySelectorAll("[data-opening-focus]")).find(usable) : undefined;
  const target = usable(previous) ? previous : landing ?? dialog;
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
 * while focus is on `<body>`): the dialog still open (the window's regions are inert behind it), where its
 * opening focus goes (see `focusOpenDialog`), else
 * wherever `focusTerminal` puts it, which for the selected session's terminal is the terminal; the empty
 * terminal and the welcome screen decline from `<body>`, so then the first of the sidebar and the rail that has
 * a control takes it, each landing where F6 would. With none to take it, focus stays on `<body>`. Keyboard modality
 * gives the fallback a visible focus ring even when a pointer acknowledged the alert, except on a touch screen
 * (`ringForAutomaticFocus`). */
export function focusAfterLoss(focusTerminal: () => void): void {
  ringForAutomaticFocus();
  if (focusOpenDialog(undefined, { opening: true })) return;
  focusTerminal();
  if (document.activeElement && document.activeElement !== document.body) return;
  for (const region of ["sidebar", "rail"] as const) {
    if (focusRegion(region, focusTerminal)) return;
  }
}

let modalLossFallback: (() => void) | undefined;
let modalLossTimer: ReturnType<typeof setTimeout> | undefined;
/** Long enough for a closing modal's exit transition and react-aria's focus restore, a frame after it unmounts. */
const MODAL_CLOSE_SETTLE_MS = 400;

/** Registers where focus goes when a modal that came up unasked (an error alert, a pending question) closes and leaves
 * it on `<body>` (see `focusAfterModalCloses`); the window passes `focusAfterLoss` with its terminal. Returns what
 * unregisters it. */
export function setFocusAfterModalLoss(fallback: () => void): () => void {
  modalLossFallback = fallback;
  return () => {
    if (modalLossFallback !== fallback) return;
    modalLossFallback = undefined;
    cancelFocusAfterModalCloses();
  };
}

/** Called by each kind of modal that comes up unasked as its own last one closes: the last error alert acknowledged, the
 * last pending question gone. Focus goes back to whatever had it when the modal opened, but there may be none: a
 * control gone with a dialog that closed before the modal came, or no focus at all, as when the modal came in the same
 * task as a press without a pointer that opened a dialog, whose own opening focus react-aria then dropped. Focus left
 * on `<body>` once the modal's exit transition and focus restore are over, which would otherwise undo it, goes to the
 * registered fallback. */
export function focusAfterModalCloses(): void {
  cancelFocusAfterModalCloses();
  const fallback = modalLossFallback;
  if (!fallback) return;
  modalLossTimer = setTimeout(() => {
    modalLossTimer = undefined;
    if (!document.activeElement || document.activeElement === document.body) fallback();
  }, MODAL_CLOSE_SETTLE_MS);
}

/** Drops a fallback `focusAfterModalCloses` has put off, for a modal that comes up in the meantime and focuses itself. */
export function cancelFocusAfterModalCloses(): void {
  clearTimeout(modalLossTimer);
  modalLossTimer = undefined;
}

/** Where focus goes when something inside `region` goes while it (or a popover it opened) holds focus: the region's
 * first control. React suspends focus events during removal, so it waits until the surviving control can observe its
 * focus and draw its ring; another hand-off in the same commit takes precedence. */
export function handFocusToRegionStart(region: Region): void {
  queueMicrotask(() => {
    if (document.activeElement === document.body) focusAfterLoss(() => focusFirstIn(region));
  });
}
