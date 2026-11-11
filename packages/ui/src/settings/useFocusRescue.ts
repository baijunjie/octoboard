import { useEffect, useRef } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { focusFellToPage } from "../components/focus/regionFocus";
import { tryEachFrame } from "../components/focus/tryEachFrame";
import { interactionIsTouch, ringForAutomaticFocus } from "../input/touchInput";

/** How many animation frames (about half a second) the rescue waits for focus to fall, as a popover that restores it
 * to a control that is gone takes a frame or more to close. */
const MAX_FRAMES_TO_WAIT = 30;

/** Hands focus to `target` when a render hides or unmounts the element that held it inside the `dialog` (a search
 * that stops matching a row once the language changes, a setting changed from another window), instead of letting it
 * fall to `<body>`. The user did not ask for the move, so the destination shows its focus ring (see "Focus moved by a
 * change the user did not make" in `docs/ui-standard.md`). After a tap the destination is `touchTarget` instead, as a
 * move into a text field raises the on-screen keyboard; it may return `"pending"` for an element not attached yet,
 * and the rescue keeps waiting. The check runs after every render of the caller, and waits for focus to actually be
 * on the page: a popover that closes with the change gives focus back to the control a frame later, and only finding
 * that gone leaves it there. Focus that has gone elsewhere in the dialog is left alone, as is a destination made
 * unreachable by a modal in front. The one exception is `fallback`, the control the dialog's focus scope falls back
 * to (its first one): while the wait is on, focus landing there is the scope catching the fall, not the user's choice,
 * so it counts as fallen. Once focus has landed the destination becomes the held element, so a later re-match that
 * hides it is rescued too; when the wait gives up the held element is forgotten, so a stale one cannot fire a late
 * rescue on an unrelated render.
 *
 * This is the last fallback, after the sections' own handoffs (`useSectionRefocus`, `handFocusOnceClosed`). It relies
 * on their passive effects and frame registrations running before this one's, which holds because the sections are
 * children of the caller: it must stay a passive effect in `SettingsDialog`, not move into a section or a layout
 * effect. */
export function useFocusRescue(
  dialog: () => HTMLElement | null | undefined,
  target: () => HTMLElement | null,
  touchTarget: () => HTMLElement | null | "pending",
  fallback: () => HTMLElement | null,
): void {
  const held = useRef<Element | null>(null);
  const waiting = useRef<(() => void) | undefined>(undefined);
  // Whether a wait for the held element's focus to fall is on.
  const waitingForLost = useRef(false);
  const latest = useLatest({ dialog, target, touchTarget, fallback });
  useEffect(() => {
    const element = latest.current.dialog();
    if (!element) return;
    const onFocusIn = (event: FocusEvent) => {
      if (waitingForLost.current && event.target === latest.current.fallback()) return;
      if (event.target instanceof Element) held.current = event.target;
    };
    element.addEventListener("focusin", onFocusIn);
    return () => {
      element.removeEventListener("focusin", onFocusIn);
      waiting.current?.();
    };
  }, []);
  // After every render: what hides or removes the element comes with a render.
  useEffect(() => {
    const lost = held.current;
    const gone = () => !lost?.isConnected || lost.closest("[hidden]") !== null;
    if (!lost || !gone()) return;
    waiting.current?.();
    waitingForLost.current = true;
    const done = () => {
      waitingForLost.current = false;
      return true;
    };
    const attempt = () => {
      if (held.current !== lost || !gone()) return done();
      const active = document.activeElement;
      if (!focusFellToPage() && active !== lost && active !== latest.current.fallback()) return false;
      const destination = interactionIsTouch() ? latest.current.touchTarget() : latest.current.target();
      if (destination === "pending") return false;
      if (!destination?.isConnected) {
        held.current = null;
        return done();
      }
      if (destination.closest("[inert]")) return false;
      ringForAutomaticFocus();
      destination.focus();
      // A destination that did not take focus (hidden, not yet attached) is tried again on the next frame.
      if (document.activeElement !== destination) return false;
      // Set here rather than left to `focusin`, which skips Close while the wait is on.
      held.current = destination;
      return done();
    };
    waiting.current = attempt() ? undefined : tryEachFrame(attempt, MAX_FRAMES_TO_WAIT, () => {
      held.current = null;
      done();
    });
  });
}
