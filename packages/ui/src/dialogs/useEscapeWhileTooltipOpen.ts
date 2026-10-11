import { useEffect, useRef, type RefObject } from "react";

import { isImeKey } from "../imeKey";
import { dialogAround } from "./dialogElement";

/**
 * Closes the dialog around `marker` on Escape while a tooltip is open — the one state in which the
 * dialog's own Escape never arrives.
 *
 * react-aria dismisses a modal from a React `onKeyDown` on its overlay, and the overlay is
 * portalled to `<body>`, so React dispatches that press from a listener on `<body>`. While a
 * tooltip is open react-aria also holds a capture-phase `keydown` listener on `document`, which
 * stops Escape there to dismiss the tooltip — above `<body>` on the way down, so the dialog never
 * sees the key. Every dialog's close button carries a tooltip and a tooltip opens on keyboard
 * focus, which is why one Tab from where a dialog opens is enough to reach the state; any other
 * titled control in a dialog reaches it too, and so does a pointer resting on one. `window` is
 * above `document` in turn, so that is where this listens.
 *
 * It acts only while a tooltip is open, which is exactly when react-aria will swallow the key, so
 * no other state is touched and the dialog cannot be asked to close twice. It leaves the key to
 * propagate, so the tooltip is still dismissed and react-aria still reads the press as keyboard
 * input — what decides whether whatever focus returns to on closing draws a focus ring. A
 * modified, held or composing Escape is left alone, matching the presses react-aria's own handler
 * acts on.
 *
 * TODO: Drop this once react-aria leaves Escape to the surfaces around a tooltip: its
 * `useTooltipTrigger` stops propagation for every Escape while the tooltip is open, whatever else
 * on the page was waiting for the key (react-aria 3.52.1, unchanged in 3.53.1).
 */
export function useEscapeWhileTooltipOpen(marker: RefObject<HTMLElement | null>, onClose: () => void): void {
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.repeat || isImeKey(event)) return;
      if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
      // A tooltip fading out has already handed the key back, and is marked while it does.
      if (!document.querySelector('[role="tooltip"]:not([data-exiting])')) return;
      const focused = document.activeElement;
      // Only the dialog holding focus acts, so a dialog opened from inside another one keeps
      // Escape to itself: both are portalled to `<body>`, so neither contains the other.
      if (!dialogAround(marker)?.contains(focused)) return;
      // Stands in for the test react-aria's own dismissal makes before it closes a dialog, that the
      // dialog is the topmost overlay. A popup counts as one and Escape belongs to it, and the
      // control that opened it keeps focus meanwhile — a combo box's input with its list of
      // suggestions up — so an expanded control holding focus means the key is not the dialog's to
      // take: closing the dialog there would discard a form the user is still filling in. Any
      // expanded control counts, not only one marked `aria-haspopup`, which a combo box's input is
      // not; the cost of counting one that holds no popup is that Escape stays swallowed there,
      // which is what it is today.
      if (focused?.closest('[aria-expanded="true"]')) return;
      close.current();
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [marker]);
}
