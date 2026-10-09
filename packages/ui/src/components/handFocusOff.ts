import { setInteractionModality } from "react-aria";

/** Hands keyboard focus from `from` to `holder` itself when that is a tab stop, else to the button
 * inside it, when `from` holds focus: a control about to be taken away would otherwise drop focus
 * to `<body>` (no blur event fires), and the terminal would stop receiving keystrokes. `showRing`
 * sets the interaction modality to keyboard first, so the destination shows its focus ring; leave
 * it off where the ring should follow how the press came. The caller makes sure `holder` is there
 * whenever `from` is.
 *
 * This is the pre-emptive half of the pair: it is called while `from` is still there, by whatever
 * is about to take it away. For a control that has already gone, taking focus with it, see
 * `useFocusHandoff`. */
export function handFocusOff(
  from: Element | null | undefined,
  holder: HTMLElement | null | undefined,
  showRing: boolean,
): void {
  if (!from?.contains(document.activeElement)) return;
  if (showRing) setInteractionModality("keyboard");
  (holder && holder.tabIndex >= 0 ? holder : holder?.querySelector("button"))?.focus();
}
