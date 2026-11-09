import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { interactionIsTouch, ringForAutomaticFocus } from "../input/touchInput";

/**
 * Keeps focus somewhere usable as a terminal is covered for a process that cannot be reached
 * (`pending`) and uncovered again. The terminal turns inert then, and focus left in it would fall
 * to `<body>`, so focus held in the view goes to the cover's region, which is named by its copy,
 * with a focus ring (the move is not the user's). (A Reconnect button that goes away is the caller's
 * `useFocusHandoff`.) When the cover lifts, focus is moved on only if it was on the pending region:
 * the user may have moved on, and a recovery in the background must not pull it. It goes back to the
 * terminal, with a ring, or after a tap to `focusAfterTap`, which must land on nothing that takes
 * text, as the terminal's input would raise the on-screen keyboard.
 *
 * The cover may be gone by then (a shell's), so where focus is is tracked from the view's own focus
 * events. WebKit and Firefox fire none as a focused element is removed, Chromium fires a `focusout`
 * that moves focus to nothing, which the removal of the cover on recovery must not count as the user
 * moving on (see `onFocusOut`); only focus on the pending region counts, as a focused control of an
 * earlier cover (a Reconnect that went away) may leave no event and would stay counted.
 *
 * Call it before `useTerminalReconnect`: layout effects run in declaration order, and that hook
 * reads `focusReturned` as the suspension ends, which this hook's effect must already have set.
 *
 * Returns what the callers' announcements and recovery need to know: `coverHasFocus`, whether focus
 * is on the pending region now, and `focusReturned`, whether focus was on it as the last lift of the
 * cover (and was moved on, to the terminal or after a tap to `focusAfterTap`).
 */
export function usePendingFocus({
  pending,
  rootRef,
  coverRef,
  focusTerminal,
  focusAfterTap,
}: {
  pending: boolean;
  rootRef: RefObject<HTMLElement | null>;
  coverRef: RefObject<HTMLElement | null>;
  focusTerminal: () => void;
  focusAfterTap: () => void;
}): { coverHasFocus: () => boolean; focusReturned: () => boolean } {
  const pendingRef = useLatest(pending);
  const inCoverRef = useRef(false);
  const returnedRef = useRef(false);
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const onFocusIn = (event: FocusEvent) => {
      inCoverRef.current = pendingRef.current && event.target === coverRef.current;
    };
    // Focus that leaves with the window stays on the element, and is still where it was. Focus that
    // goes to another element is the user moving on. Focus that goes to nothing is either that (a click
    // on something that cannot take focus) or Chromium blurring the cover as the recovery removes it,
    // after React has detached its ref, so the target cannot be compared with the cover: the removal's
    // commit runs the lift effect, which reads the flag, before the microtask, and has by then ended
    // the pending period, whereas a click leaves it on and the flag goes.
    const onFocusOut = (event: FocusEvent) => {
      const target = event.target;
      if (document.activeElement === target) return;
      if (event.relatedTarget !== null || !pendingRef.current) {
        inCoverRef.current = false;
        return;
      }
      queueMicrotask(() => {
        // Against the cover now shown, not `target`: pending may have ended and begun again before
        // this runs, and focus on the new cover keeps the flag.
        if (pendingRef.current && document.activeElement !== coverRef.current) inCoverRef.current = false;
      });
    };
    root.addEventListener("focusin", onFocusIn);
    root.addEventListener("focusout", onFocusOut);
    return () => {
      root.removeEventListener("focusin", onFocusIn);
      root.removeEventListener("focusout", onFocusOut);
    };
  }, []);
  const was = useRef(pending);
  useLayoutEffect(() => {
    const wasPending = was.current;
    was.current = pending;
    if (pending === wasPending) return;
    returnedRef.current = false;
    if (pending) {
      if (!rootRef.current?.contains(document.activeElement) || !coverRef.current) return;
      ringForAutomaticFocus();
      coverRef.current.focus();
    } else if (inCoverRef.current) {
      inCoverRef.current = false;
      returnedRef.current = true;
      if (interactionIsTouch()) focusAfterTap();
      else {
        ringForAutomaticFocus();
        focusTerminal();
      }
    }
  }, [pending]);
  const coverHasFocus = () => inCoverRef.current;
  const focusReturned = () => returnedRef.current;
  return { coverHasFocus, focusReturned };
}
