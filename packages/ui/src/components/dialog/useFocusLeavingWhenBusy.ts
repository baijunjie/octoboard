import React, { useLayoutEffect, useRef } from "react";

import { useLatest } from "../hooks/useLatest";
import { partSelector } from "../../primitives";
import { DISABLED_CONTROL } from "../focus/selectors";

/** Whether `element` is a control the busy dialog has just disabled: one that reports itself disabled, or a link that
 * has just lost its `href` (a busy link is unfollowable, and a browser may blur it before it is marked), but not the
 * pending button, which carries the same mark and is the one control left. */
const disabledByBusy = (element: Element) =>
  (element.matches(DISABLED_CONTROL) || element.matches("a:not([href])")) && !element.matches(partSelector.pendingButton);

/** Whether `control` is still there and usable as a place for focus to return to. */
const usable = (control: Element): control is HTMLElement =>
  control instanceof HTMLElement && control.isConnected && !control.matches(DISABLED_CONTROL);

/** Moves keyboard focus inside `dialog` to its pending button (`partSelector.pendingButton`), the one control left, or
 * else the dialog itself, when `busy` turns true and focus is on a control the busy dialog has just disabled: a
 * dialog's Escape and Tab containment hang off focus being inside it. Focus on something the busy state leaves alone
 * (the dialog, the pending button, the scroller of a long body) stays. WebKit blurs a control to `<body>` as it becomes
 * disabled, before this runs, so focus counts as having been on one when `<body>` has it and focus was last inside the
 * dialog. A layout effect, so focus has moved before the change is painted.
 *
 * The control focus was moved off is remembered, and focus goes back to it once the dialog is no longer busy and the
 * dialog is still open, so a failed request leaves the user where they were. While busy every control but the pending
 * button is disabled, so unless the user pressed or tabbed in it, where focus is in the dialog when busy ends is not
 * their own choice: it is the landing, the dialog, or where `useFocusLeavingWhenDisabled` has just moved it (Cancel,
 * when the refusal disables the submit button). Focus goes back at once then, and also where it is on `<body>`; if the
 * control is gone or disabled by then, focus found on `<body>` goes to the landing, or the dialog, so it never stays on
 * the page. Only where focus is on something outside the dialog (an error alert, which is raised before the dialog
 * stops being busy and gives focus back to the pending button when it is acknowledged) is the return held back, once,
 * until focus next comes back into the dialog: wherever it comes back, a refusal that asks again leaving nothing to
 * submit included. What the frame itself does with focus keeps the return; the user's own moves forget the remembered
 * control (not an element named through `returnFocusRef`): a press inside the dialog, or a Tab inside it (from the
 * pending button to a scrolling body, say). Focus is never taken out of an open alert. Each busy period starts from
 * nothing remembered.
 *
 * `returnFocusRef` lets the dialog name another element for that return, however the request was started (by a press of
 * the pending button, with focus on no control of its own): what is in it when the dialog stops being busy is the
 * element focus goes back to in place of the remembered control, and it is cleared then. A change of `resetKey` forgets
 * the remembered control and that element with the subject they belonged to. */
export function useFocusLeavingWhenBusy(
  dialog: () => HTMLElement | null | undefined,
  busy: boolean,
  returnFocusRef: React.MutableRefObject<HTMLElement | null>,
  resetKey: string | undefined,
): void {
  const held = useRef<Element | null>(null);
  const landed = useRef<HTMLElement | null>(null);
  const returnTo = useRef<{ control: Element; landing: HTMLElement; armed: boolean } | null>(null);
  const dialogRef = useLatest(dialog);
  const seenKey = useRef(resetKey);
  useLayoutEffect(() => {
    const track = (event: FocusEvent) => {
      if (!(event.target instanceof Element)) return;
      const inside = dialogRef.current()?.contains(event.target) ?? false;
      if (event.type === "focusin") {
        held.current = inside ? event.target : null;
        const back = returnTo.current;
        if (!back || !inside) return;
        // Armed, focus was out of the dialog (in the alert), so coming back in anywhere is the alert closing.
        if (back.armed) {
          returnTo.current = null;
          if (usable(back.control)) back.control.focus();
        }
      // A blur that arrives once the control is disabled is the browser's doing, not the user moving on.
      } else if (inside && !disabledByBusy(event.target)) held.current = null;
    };
    // A press or a Tab the user makes in the dialog is theirs to place, so a held return never redirects it.
    const forget = (event: Event) => {
      if (event.target instanceof Node && dialogRef.current()?.contains(event.target)) returnTo.current = null;
    };
    const tab = (event: KeyboardEvent) => {
      if (event.key === "Tab") forget(event);
    };
    document.addEventListener("focusin", track);
    document.addEventListener("focusout", track);
    document.addEventListener("pointerdown", forget, true);
    document.addEventListener("keydown", tab, true);
    return () => {
      document.removeEventListener("keydown", tab, true);
      document.removeEventListener("pointerdown", forget, true);
      document.removeEventListener("focusin", track);
      document.removeEventListener("focusout", track);
    };
  }, []);
  useLayoutEffect(() => {
    if (seenKey.current === resetKey) return;
    seenKey.current = resetKey;
    returnTo.current = null;
    returnFocusRef.current = null;
  }, [resetKey]);
  useLayoutEffect(() => {
    const root = dialogRef.current();
    const active = document.activeElement;
    if (!root) return;
    if (!busy) {
      const named = returnFocusRef.current;
      returnFocusRef.current = null;
      if (named && landed.current) returnTo.current = { control: named, landing: landed.current, armed: false };
      const back = returnTo.current;
      if (!back) return;
      if (active && active !== document.body && !root.contains(active)) {
        back.armed = true;
        return;
      }
      returnTo.current = null;
      if (usable(back.control)) back.control.focus();
      else if (!active || active === document.body) (usable(back.landing) ? back.landing : root).focus();
      return;
    }
    returnTo.current = null;
    const landing = root.querySelector<HTMLElement>(partSelector.pendingButton) ?? root;
    landed.current = landing;
    const lost = !active || active === document.body;
    const stranded = lost ? held.current : root.contains(active) && active !== root && disabledByBusy(active) ? active : null;
    held.current = null;
    if (!stranded || stranded.matches(partSelector.pendingButton)) return;
    landing.focus();
    returnTo.current = { control: stranded, landing, armed: false };
  }, [busy]);
}
