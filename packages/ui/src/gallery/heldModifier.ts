import { IS_MAC } from "../useWindowShortcut";

/**
 * Holds the shortcut modifier (⌘ on an Apple keyboard, Ctrl elsewhere) down for the page, so that
 * the new-session "+" buttons show their quick-create form without anyone keeping a key pressed.
 * `useQuickCreateModifier` reads the modifier off every key and pointer event and lets go on a
 * plain one, so a pointer moving over the window would end the state at once; this stamps the
 * modifier onto each pointer move, and the move after the window lost focus or a key was pressed
 * holds it again. Call it before the app mounts, so that it is the first window listener. Nothing
 * is held until `press`.
 */
export function holdPrimaryModifier(): { press: () => void } {
  let pressed = false;
  const stamp = (event: Event) => {
    if (!pressed) return;
    Object.defineProperty(event, IS_MAC ? "metaKey" : "ctrlKey", { value: true });
    Object.defineProperty(event, IS_MAC ? "ctrlKey" : "metaKey", { value: false });
  };
  window.addEventListener("pointermove", stamp, { capture: true });
  return {
    press: () => {
      pressed = true;
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: IS_MAC ? "Meta" : "Control", metaKey: IS_MAC, ctrlKey: !IS_MAC }),
      );
    },
  };
}
