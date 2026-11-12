import { setKeyboardModality } from "../primitives";

/** A device whose primary input cannot hover: a phone, or a tablet's touch screen. What depends on
 * the device's pointer rather than on one interaction keys on this query: no pane floats in on a
 * rest of the pointer, and controls otherwise revealed on hover are always shown (in CSS spelled
 * `[@media(hover:none)]:`, as Tailwind needs the literal). */
export const CANNOT_HOVER = "(hover: none)";

/** A field that takes text: focusing it raises a touch screen's keyboard. */
export const TEXT_ENTRY =
  'input:not([type=checkbox], [type=radio], [type=range], [type=button], [type=submit], [type=reset], [type=color], [type=file]), textarea, [contenteditable]:not([contenteditable="false"])';

/** Keys an on-screen keyboard does not send, which only a hardware keyboard can have pressed. */
const NAVIGATION_KEYS = new Set([
  "Tab", "Escape", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown",
  "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
]);

/** Whether `event` came from a hardware keyboard rather than a touch screen's on-screen one, which
 * sends keydowns for what is typed into a text field too: composition, and Android's
 * `Unidentified` (keyCode 229), never count; in a field that takes text only a navigation key or a
 * shortcut with a modifier does, as on-screen keyboards have neither. */
function fromHardwareKeyboard(event: KeyboardEvent): boolean {
  if (event.isComposing || event.key === "Unidentified" || event.key === "Process") return false;
  // The first node of the path, as a key inside a shadow root reaches `document` retargeted to its host.
  const origin = event.composedPath()[0];
  const target = origin instanceof Element ? origin : null;
  if (!target?.closest(TEXT_ENTRY)) return true;
  return NAVIGATION_KEYS.has(event.key) || event.ctrlKey || event.metaKey || event.altKey;
}

/** Whether a press of this pointer type is a tap: a finger or a pen, which has no hover. */
function isTouchPointer(pointerType: string | null | undefined): boolean {
  return pointerType === "touch" || pointerType === "pen";
}

/** Whether `event` is a press with no pointer behind it, which a screen reader or a script sends.
 * Follows react-aria's `isVirtualPointerEvent` (`utils/isVirtualEvent`) except in two places: no pointer size marks
 * one, but TalkBack's double tap, which arrives with a size of 1, is told apart by the rest of its signature on
 * Android only (WebKit reports a pressure of 0 for real mouse presses, so react-aria's broader 1x1 rule could mark
 * those virtual); and an empty pointer type counts as virtual, being a screen reader's too (JAWS or NVDA in
 * Firefox). */
function isVirtualPress(event: PointerEvent): boolean {
  if (!event.pointerType) return true;
  if (/Android/i.test(navigator.userAgent)) {
    return event.width === 1 && event.height === 1 && event.pressure === 0 && event.detail === 0 && event.pointerType === "mouse";
  }
  return event.width === 0 && event.height === 0;
}

/** What the interaction under way came from: the last press's pointer (a finger and a pen count as
 * a tap, a mouse or a trackpad as a pointer), or a hardware keyboard's key. A virtual press leaves
 * it as it was. Listens on `window` in the capture phase, registered at load ahead of every
 * component's listener, so a handler that stops a key or a press before `document` (Escape closing
 * a floating panel) cannot hide it. */
let lastInput: "touch" | "pointer" | "key" | undefined;
if (typeof window !== "undefined") {
  window.addEventListener("pointerdown", (event) => {
    if (isVirtualPress(event)) return;
    lastInput = isTouchPointer(event.pointerType) ? "touch" : "pointer";
  }, true);
  window.addEventListener("keydown", (event) => {
    if (fromHardwareKeyboard(event)) lastInput = "key";
  }, true);
}

/** Whether the interaction under way is a tap: its press came from a finger or a pen, not a mouse,
 * a trackpad or a hardware keyboard's key (an iPad's keyboard keeps the desktop's behaviour; typing
 * on an on-screen one keeps the tap's). Before any
 * interaction, whether the device cannot hover. A tap never moves focus into a field that takes
 * text, as that raises the on-screen keyboard over what was just shown, and never turns a focus
 * ring on (`ringForAutomaticFocus`). */
export function interactionIsTouch(): boolean {
  return lastInput === undefined ? (window.matchMedia?.(CANNOT_HOVER).matches ?? false) : lastInput === "touch";
}

/** Called before a focus move the user did not ask for (a handoff as something goes away, a
 * return after a press elsewhere): the ring follows the last input. After a hardware keyboard's key
 * its destination shows a focus ring; after a press, of a finger, a pen or a mouse, it never does,
 * as a ring is for finding one's place with the keyboard, and on a tap shows nothing a finger can
 * use while the tooltip it opens would stay up with nothing to send it away. Before any input it
 * shows one unless the device cannot hover. Call it right before the `focus()` that actually happens, after every
 * guard: called earlier, it sets the keyboard modality even when nothing moves, and a dialog's later
 * restore after a pointer close then shows a ring. */
export function ringForAutomaticFocus(): void {
  if (lastInput === undefined ? !interactionIsTouch() : lastInput === "key") setKeyboardModality();
}
