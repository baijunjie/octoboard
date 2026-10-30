/** What sequential navigation can stop on: with `isShown` and `isTabbable`, adapted from react-aria 3.52.1's tabbable
 * selector and visibility test (`isTabbable`, `isElementVisible`), which it exports only from private entry points.
 * It leaves out `permission` and `data-react-aria-prevent-focus`, which this UI never renders. */
export const TABBABLE = [
  "input:not([disabled]):not([type=hidden])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "button:not([disabled])",
  "a[href]",
  "area[href]",
  "summary",
  "iframe",
  "object",
  "embed",
  "audio[controls]",
  "video[controls]",
  '[contenteditable]:not([contenteditable^="false"])',
  '[tabindex]:not([tabindex="-1"]):not([disabled])',
]
  .map((selector) => `${selector}:not([hidden]):not([tabindex="-1"])`)
  .join(",");

function isShown(element: Element): boolean {
  if (typeof element.checkVisibility === "function") return element.checkVisibility({ visibilityProperty: true });
  for (let node: Element | null = element; node; node = node.parentElement) {
    if (node.hasAttribute("hidden")) return false;
    const { display, visibility } = getComputedStyle(node);
    if (display === "none" || visibility === "hidden" || visibility === "collapse") return false;
  }
  return true;
}

export const isTabbable = (element: Element): boolean => element.matches(TABBABLE) && !element.closest("[inert]") && isShown(element);
