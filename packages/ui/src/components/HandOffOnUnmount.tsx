import { useLayoutEffect } from "react";

import { ringForAutomaticFocus } from "../touchInput";
import { useLatest } from "./useLatest";

/** Renders nothing; when it unmounts while focus is inside `from` (a selector, or a function finding the
 * element when it is not one a selector can name), calls `to` to move it. It is for
 * a hand-off whose trigger is a render rather than a call site, such as the daemon taking the last
 * console away. The user did not ask for the move, so the keyboard modality is set first and the
 * destination shows its focus ring, except on a touch screen (`ringForAutomaticFocus`). With `ring` false
 * that is left to `to`, for one that moves focus only on some of its calls.
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
    },
    [selector],
  );
  return null;
}
