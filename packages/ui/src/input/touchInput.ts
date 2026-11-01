import { setInteractionModality } from "react-aria";

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

/** What the interaction under way came from: the last press's pointer (a finger and a pen alike
 * count as a tap), or a hardware keyboard's key. */
let lastInput: "touch" | "other" | undefined;
if (typeof document !== "undefined") {
  document.addEventListener("pointerdown", (event) => (lastInput = isTouchPointer(event.pointerType) ? "touch" : "other"), true);
  document.addEventListener("keydown", (event) => {
    if (fromHardwareKeyboard(event)) lastInput = "other";
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

/** Called before a focus move the user did not ask for (a hand-off as something goes away, a
 * return after a press elsewhere): its destination shows a focus ring, as for the keyboard. Not
 * after a tap, where a ring shows nothing a finger can use and the tooltip it opens would stay up
 * with nothing to send it away. */
export function ringForAutomaticFocus(): void {
  if (!interactionIsTouch()) setInteractionModality("keyboard");
}
