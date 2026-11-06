import { ringForAutomaticFocus } from "../../input/touchInput";
import { focusFirstIn, focusRegion, type Region } from "./regionFocus";
import { MODAL_DIALOG } from "./selectors";

/** Returns to the foremost modal instead of an inert window region. A remembered control is
 * preferred while it still belongs to that dialog; the dialog itself survives replaced content.
 * With `opening`, focus that has no remembered control goes where the dialog's opening focus goes
 * (the control it opens on, marked by `openingFocus` of `primitives`) rather than to the dialog: the
 * dialog may never have had that focus, and the dialog itself is where only a dialog that opens on
 * itself (a destructive confirmation, an unprompted question) starts. */
export function focusOpenDialog(previous?: Element | null, { opening = false }: { opening?: boolean } = {}): boolean {
  const dialogs = document.querySelectorAll<HTMLElement>(MODAL_DIALOG);
  const dialog = Array.from(dialogs).reverse().find((element) => !element.closest('[inert], [aria-hidden="true"]'));
  if (!dialog) return false;
  const usable = (element: Element | null | undefined): element is HTMLElement =>
    element instanceof HTMLElement && dialog.contains(element) && !element.matches(":disabled") && !element.closest("[hidden]");
  const landing = opening ? Array.from(dialog.querySelectorAll("[data-opening-focus]")).find(usable) : undefined;
  const target = usable(previous) ? previous : landing ?? dialog;
  target.focus();
  return dialog.contains(document.activeElement);
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

/** Registers where focus goes when a modal that came up unasked (an error alert, a pending question), or Settings with
 * no dialog under it, closes and leaves it on `<body>` (see `focusAfterModalCloses`); the window passes `focusAfterLoss`
 * with its terminal. Returns what unregisters it. */
export function setFocusAfterModalLoss(fallback: () => void): () => void {
  modalLossFallback = fallback;
  return () => {
    if (modalLossFallback !== fallback) return;
    modalLossFallback = undefined;
    cancelFocusAfterModalCloses();
  };
}

/** Called by each kind of modal that comes up unasked as its own last one closes (the last error alert acknowledged, the
 * last pending question gone), and by Settings as it closes with no dialog under it. React Aria gives focus back to
 * whatever had it when the modal opened, but there may be none: a control gone with a dialog that closed before the
 * modal came, or no focus at all, as when the modal came in the same task as a press without a pointer that opened a
 * dialog, whose own opening focus react-aria then dropped. Focus left on `<body>` once the modal's exit transition and
 * that restore are over, which would otherwise undo the move, goes to the registered fallback, which for the window is
 * `focusAfterLoss`. */
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
 * focus and draw its ring; another handoff in the same commit takes precedence. */
export function handFocusToRegionStart(region: Region): void {
  queueMicrotask(() => {
    if (document.activeElement === document.body) focusAfterLoss(() => focusFirstIn(region));
  });
}
