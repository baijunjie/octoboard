import { useMemo, type Ref, type RefCallback } from "react";
import { mergeRefs } from "react-aria";

/** What sequential navigation can stop on: with `isShown` and `isTabbable`, adapted from react-aria 3.52.1's tabbable
 * selector and visibility test (`isTabbable`, `isElementVisible`), which it exports only from private entry points.
 * It leaves out `permission` and `data-react-aria-prevent-focus`, which the collections it serves never hold. */
const TABBABLE = [
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

const isTabbable = (element: Element): boolean => element.matches(TABBABLE) && !element.closest("[inert]") && isShown(element);

function onKeyDown(event: KeyboardEvent): void {
  if (event.key !== "Tab" || !event.altKey || event.ctrlKey || event.metaKey) return;
  const collection = event.currentTarget as HTMLElement;
  if (event.shiftKey) {
    collection.focus({ preventScroll: true });
    return;
  }
  const last = [...collection.querySelectorAll<HTMLElement>(TABBABLE)].reverse().find(isTabbable);
  const active = document.activeElement;
  if (last && (!last.contains(active) || (active !== null && !isTabbable(active)))) last.focus({ preventScroll: true });
}

/**
 * Makes Option+Tab and Option+Shift+Tab leave a react-aria collection (a `Tree`, a `GridList`) the
 * way Tab and Shift+Tab do, in one step, past the controls inside its rows. Returns `forwarded`
 * merged with a ref for the collection's own element.
 *
 * The collection is one tab stop although its rows' controls (an Actions button) are tabbable:
 * react-aria's `useSelectableCollection` takes Tab and Shift+Tab and moves focus to the
 * collection's last tabbable element, or to the collection itself, so that the browser's own move
 * goes on from there and out. Its shortcuts match the modifiers exactly (react-aria 3.52.1), so it
 * leaves a Tab with Option held alone, and in WebKit that is the key that reaches every control
 * while macOS's Keyboard navigation setting is off, as it is by default: left to the browser, it
 * stops on each row's controls in turn. The same moves are made here for that key, before the
 * browser's. When react-aria is upgraded, check them against `useSelectableCollection`'s `tab` and
 * `shiftTab`, and the copy of its tabbable selector and visibility test above against its own.
 */
export function useCollectionOptionTab(forwarded: Ref<HTMLDivElement>): Ref<HTMLDivElement> {
  return useMemo(() => {
    const listen: RefCallback<HTMLDivElement> = (element) => {
      if (!element) return;
      element.addEventListener("keydown", onKeyDown, true);
      return () => element.removeEventListener("keydown", onKeyDown, true);
    };
    return mergeRefs(forwarded, listen);
  }, [forwarded]);
}
