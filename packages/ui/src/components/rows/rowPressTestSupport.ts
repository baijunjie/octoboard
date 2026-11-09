const PRESSABLE_INPUT_TYPES = new Set(["radio", "checkbox", "button", "submit", "reset", "file", "image"]);

/** Emulates WebKit's `AccessibilityNodeObject::nativeActionElement` for an element with `role="button"`: the first
 * descendant in document order (depth-first) that is a `<button>`, a `<select>`, or an enabled `<input>` of a
 * button-like type, with no rendering or visibility check. An accessibility press (AXPress, which VoiceOver's
 * VO-Space performs) is a simulated click on that element. */
export function accessibilityActionElement(root: Element): HTMLElement | null {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const element = node as HTMLElement;
    if (element instanceof HTMLButtonElement || element instanceof HTMLSelectElement) return element;
    if (element instanceof HTMLInputElement && !element.disabled && PRESSABLE_INPUT_TYPES.has(element.type)) return element;
  }
  return null;
}
