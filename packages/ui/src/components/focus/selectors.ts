import { partSelector } from "../../primitives";

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

/** What a region's focus lands on: its first control a keyboard can reach. A narrower list than `TABBABLE`, as a
 * landing is a link, a button, a frame or an element that opts in with a `tabindex` (a form field does not count unless
 * it does), and one marked `aria-disabled` is passed over. A report page's own frame counts, as the last thing in the
 * aside; the connection banner's landing is its Retry, and it has none while it only says it is reconnecting. */
export const FOCUSABLE =
  'a[href], button:not([disabled]), iframe, [tabindex]:not([tabindex="-1"]):not([aria-disabled="true"])';

/** A modal dialog or alert dialog; a toast is an `alertdialog` too, but a non-modal one. */
export const MODAL_DIALOG = '[role="dialog"]:not([aria-modal="false"]), [role="alertdialog"]:not([aria-modal="false"])';

/** What disables a control, natively or as a state the control reports. */
export const DISABLED_CONTROL = `:disabled, [aria-disabled="true"], ${partSelector.disabledControl}`;

/** A dialog or a menu is open: ordinary regions stay unavailable. */
export const MODAL_OPEN = `${MODAL_DIALOG}, [role="menu"]`;

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
