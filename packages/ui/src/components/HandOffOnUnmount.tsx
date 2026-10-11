import { useLayoutEffect, useRef } from "react";
import { setInteractionModality } from "react-aria";

/** Renders nothing; when it unmounts while focus is inside `from`, calls `to` to move it. It is for
 * a hand-off whose trigger is a render rather than a call site, such as the daemon taking the last
 * console away. The user did not ask for the move, so the keyboard modality is set first and the
 * destination shows its focus ring.
 *
 * Put it first among the siblings that go away together. React unwinds a deleted subtree in order,
 * removing each host node from the document as it passes it and running the layout cleanups on the
 * way, so a sibling's cleanup runs with the siblings before it already detached: placed first, this
 * one still finds focus where it was, whereas a layout effect of the parent runs after the removal,
 * with focus already on `<body>`. Moving it after the elements named by `from` makes it a no-op.
 *
 * Under StrictMode the effect is torn down and set up again right after mount, which runs the
 * hand-off once; that does nothing only because focus cannot already be inside `from` at that
 * moment, so do not place this where it could be. */
export function HandOffOnUnmount({ from, to }: { from: string; to: () => void }): null {
  const latest = useRef(to);
  latest.current = to;
  useLayoutEffect(
    () => () => {
      if (!document.activeElement?.closest(from)) return;
      setInteractionModality("keyboard");
      latest.current();
    },
    [from],
  );
  return null;
}
