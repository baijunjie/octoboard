import { type ReactElement, useLayoutEffect, useRef } from "react";

import { ringForAutomaticFocus } from "../../input/touchInput";
import { useLatest } from "../hooks/useLatest";
import { focusFellToPage } from "./regionFocus";

/** Renders nothing; when it unmounts while focus is inside `from` (a selector, or a function finding the
 * element when it is not one a selector can name), calls `to` to move it. It is for
 * a hand-off whose trigger is a render rather than a call site, such as the daemon taking the last
 * console away. The user did not ask for the move, so the keyboard modality is set first and the
 * destination shows its focus ring, except on a touch screen (`ringForAutomaticFocus`). With `ring` false
 * that is left to `to`, for one that moves focus only on some of its calls.
 *
 * `to` runs inside React's commit, where React drops the focus events it would pass to its handlers, so
 * a destination focused there would not know it holds focus (react-aria's `data-focus-visible` ring, a
 * focus-within state, a `useFocusHandoff` wrapper's note of the element). Once the commit is over, while
 * the destination still holds focus, it is sent a `focusin` of its own, which React's handlers take.
 *
 * Put it first among the siblings that go away together. React unwinds a deleted subtree in order,
 * removing each host node from the document as it passes it and running the layout cleanups on the
 * way, so a sibling's cleanup runs with the siblings before it already detached: placed first, this
 * one still finds focus where it was, whereas a layout effect of the parent runs after the removal,
 * with focus already on `<body>`. Moving it after the elements named by `from` makes it a no-op.
 *
 * A `from` function cannot read a ref on an enclosing element: React detaches an ancestor's ref
 * before this cleanup runs. Keep the element in a variable while mounted instead (a ref callback
 * that stores only non-null values).
 *
 * Under StrictMode the effect is torn down and set up again right after mount, which runs the
 * hand-off once; that does nothing only because focus cannot already be inside `from` at that
 * moment, so do not place this where it could be. */
export function HandOffOnUnmount({ from, to, ring = true }: { from: string | (() => Element | null); to: () => void; ring?: boolean }): null {
  const latest = useLatest(to);
  const source = useLatest(from);
  const ringOn = useLatest(ring);
  const selector = typeof from === "string" ? from : undefined;
  useLayoutEffect(
    () => () => {
      const active = document.activeElement;
      const scope = source.current;
      if (!(typeof scope === "string" ? active?.closest(scope) : scope()?.contains(active))) return;
      if (ringOn.current) ringForAutomaticFocus();
      latest.current();
      // A `focusin` alone, not a blur and a refocus: a field that closes on blur would take that `focusout` as
      // the user leaving it.
      const moved = document.activeElement;
      if (!moved || moved === active || moved === document.body) return;
      queueMicrotask(() => {
        if (document.activeElement === moved) moved.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
      });
    },
    [selector],
  );
  return null;
}

/** Moves focus on to `target`, a move the user did not ask for, if it is in the document and enabled and focus is not
 * held elsewhere, turning the focus ring on just before (`ringForAutomaticFocus`), so a target that turns out not to be
 * there leaves the modality alone. Focus is held elsewhere unless it has fallen to `<body>` (or nowhere); with
 * `duringRemoval`, from a `HandOffOnUnmount`'s `to`, it is still on the source, which that already checked. */
export function focusHandOffTarget(target: HTMLElement | null | undefined, { duringRemoval = false } = {}): void {
  if (!target?.isConnected || target.matches(":disabled") || (!duringRemoval && !focusFellToPage())) return;
  ringForAutomaticFocus();
  target.focus();
}

/** The hand-off of a source the calling component renders: render what the returned function gives for the source's
 * selector as `HandOffOnUnmount` is placed. The cleanup only notes that focus was inside the source; the caller's
 * layout effect, later in the commit that removes it, then moves focus with `focusHandOffTarget`.
 *
 * By then the commit has applied the render's attributes, so a target the same render disabled is seen as such and
 * left alone (focused during the removal, WebKit would blur it as it is disabled); the effects of an enclosing
 * `useFocusHandoff` or `useSectionRefocus`, which would take focus from `<body>` elsewhere, have not run yet; focus a
 * descendant's layout effect placed in the same commit stays; and React passes the focus on to react-aria, which draws
 * the ring. A caller unmounted with the source, as when a dialog closes with focus in it, runs no effect: nothing is
 * moved and the modality is left for the dialog's restore, whose ring follows the input that closed it.
 *
 * This is the default, wrapper or no wrapper around the source. The one exception is a source whose own component
 * goes with it, removed by its parent, as the sharing switch's mark goes with its warning: that component runs no
 * layout effect. Such a source uses a synchronous `HandOffOnUnmount` with `ring` false, whose `to` calls
 * `focusHandOffTarget` with `duringRemoval`, for a target that the same render cannot disable. */
export function useHandOffAfterRemoval(to: () => HTMLElement | null | undefined): (from: string) => ReactElement {
  const held = useRef(false);
  useLayoutEffect(() => {
    if (!held.current) return;
    held.current = false;
    focusHandOffTarget(to());
  });
  return (from) => <HandOffOnUnmount from={from} ring={false} to={() => { held.current = true; }} />;
}
