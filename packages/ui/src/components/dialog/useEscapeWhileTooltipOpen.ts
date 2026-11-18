import { useEffect, type KeyboardEventHandler } from "react";

import { useLatest } from "../hooks/useLatest";
import { isImeKey } from "../../input/imeKey";
import { partSelector } from "../../primitives";

/**
 * Closes the surface `surface` returns on Escape while a tooltip is open — the one state in which
 * React's Escape handling never sees the key. A view with no library dismissal passes the returned
 * handler to its root's `onKeyDown`; a dialog with its own library dismissal needs only the capture
 * listener. A surface that listens above React, as `usePaneToggles` does, does not need this.
 *
 * react-aria dismisses a modal from a React `onKeyDown` on its overlay, and the overlay is
 * portalled to `<body>`, so React dispatches that press from a listener on `<body>`; a view in the
 * page is dispatched from the container the app is mounted in, no higher. While a tooltip is open
 * react-aria also holds a capture-phase `keydown` listener on `document`, which stops Escape there
 * to dismiss the tooltip — above both on the way down, so the surface never sees the key. Every
 * dialog's close button carries a tooltip and a tooltip opens on keyboard focus, which is why one
 * Tab from where a dialog opens is enough to reach the state; any other titled control in a
 * surface reaches it too, and so does a pointer resting on one. `window` is above `document` in
 * turn, so that is where this listens.
 *
 * It acts only while a tooltip with a trigger element is open, which is exactly when react-aria
 * will swallow the key, so no other state is touched and the surface cannot be asked to close
 * twice. An anchored `Tooltip` has no trigger element, so react-aria holds no listener for it and the
 * key reaches the surface (it holds the key back itself only inside a popup that holds focus, which
 * is portalled out of every surface); it is marked `data-anchored-tooltip` and left out here, as
 * acting on it too would close the surface from this listener and from its own handler both. It
 * leaves the key to propagate, so the tooltip is still dismissed and react-aria still reads the
 * press as keyboard input — what decides whether whatever focus returns to on closing draws a
 * focus ring. A modified, held or composing Escape is left alone: react-aria's own dismissal
 * ignores those presses, so with a tooltip open they do nothing, as they do without one.
 *
 * React Aria's `useTooltipTrigger` stops propagation while a tooltip is open (observed in 3.53.1).
 * This listener is needed while the tooltip prevents Escape from reaching the surrounding surface.
 */
export function useEscapeWhileTooltipOpen(
  surface: () => HTMLElement | null | undefined,
  onClose: () => void,
): KeyboardEventHandler<HTMLElement> {
  const close = useLatest(onClose);
  const find = useLatest(surface);
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (!isDismissEscape(event)) return;
      // A tooltip fading out has already handed the key back, and is marked while it does.
      if (!hasTriggeredTooltip()) return;
      const focused = document.activeElement;
      const around = find.current();
      // Only the surface holding focus acts, so a dialog opened from inside another one keeps
      // Escape to itself: both are portalled to `<body>`, so neither contains the other. The same
      // holds for a dialog opened from a view.
      if (!around?.contains(focused)) return;
      // Stands in for the test react-aria's own dismissal makes before it closes a dialog, that the
      // dialog is the topmost overlay. A popup counts as one and Escape belongs to it, and the
      // control that opened it keeps focus meanwhile — a combo box's input with its list of
      // suggestions up — so an expanded control holding focus means the key is not the surface's to
      // take: closing the surface there would discard a form the user is still filling in. Any
      // expanded control counts, not only one marked `aria-haspopup`, which a combo box's input is
      // not; the cost of counting one that holds no popup is that Escape stays swallowed there,
      // which is what it is today.
      const expanded = focused?.closest('[aria-expanded="true"]');
      if (expanded && around.contains(expanded)) return;
      close.current();
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, []);
  return (event) => {
    const target = event.target;
    if (event.defaultPrevented || !isDismissEscape(event.nativeEvent) || !(target instanceof Node) || !surface()?.contains(target)) return;
    // A triggered tooltip has already handled this key in the capture listener.
    if (hasTriggeredTooltip()) return;
    event.preventDefault();
    onClose();
  };
}

function isDismissEscape(event: KeyboardEvent): boolean {
  return event.key === "Escape" && !event.repeat && !isImeKey(event) &&
    !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey;
}

function hasTriggeredTooltip(): boolean {
  return document.querySelector(`${partSelector.liveTooltip}:not([data-anchored-tooltip])`) !== null;
}
