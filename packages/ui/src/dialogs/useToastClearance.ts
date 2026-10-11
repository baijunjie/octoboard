import { useLayoutEffect, type RefObject } from "react";

import { TOAST_BREAKPOINT, TOAST_MARGIN, TOAST_WIDTH } from "../components/toastGeometry";
import { dialogAround } from "./dialogElement";

/** The custom property `style.css` lifts the toast region by, from the window's bottom edge. */
const PROPERTY = "--dialog-footer-clearance";

const FOOTER = '[data-slot="modal-footer"], [data-slot="alert-dialog-footer"]';

type Entry = { measure: () => void };

/** The mounted dialogs that have a footer, the innermost last. Only the innermost sets the property,
 * and when it goes the one beneath it measures again. */
const stack: Entry[] = [];

/** Whether `footer` reaches into the bottom end corner the toasts occupy: its inline-end side is
 * past the region's inline-start side (the right side in a left-to-right document, the left side in
 * a right-to-left one). Never in a narrow window, where the toasts are at the top. */
function overlapsToastCorner(footer: DOMRect): boolean {
  if (window.innerWidth < TOAST_BREAKPOINT) return false;
  const rtl = getComputedStyle(document.documentElement).direction === "rtl";
  return rtl ? footer.left < TOAST_MARGIN + TOAST_WIDTH : footer.right > window.innerWidth - TOAST_MARGIN - TOAST_WIDTH;
}

/**
 * Keeps the toasts clear of the footer of the dialog `inside` sits in, for as long as it is open and
 * only when the footer reaches the bottom end corner where the stack sits (in a window wide enough
 * for it to be there, see `Toasts`): a large dialog's footer
 * buttons (the file viewer's Previous and Next) would sit under a toast there, while a small centred
 * dialog's are clear of it. The clearance is the distance from the footer's top edge to the window's
 * bottom, measured again as the dialog settles from its entrance animation, resizes, or the window
 * does. A dialog without a footer sets none, and with dialogs nested the innermost one decides.
 */
export function useToastClearance(inside: RefObject<HTMLElement | null>, hasFooter: boolean): void {
  useLayoutEffect(() => {
    const dialog = dialogAround(inside);
    const footer = hasFooter ? dialog?.querySelector<HTMLElement>(FOOTER) : null;
    if (!dialog || !footer) return;
    const root = document.documentElement;
    let frame = 0;
    const apply = () => {
      const rect = footer.getBoundingClientRect();
      if (overlapsToastCorner(rect)) {
        root.style.setProperty(PROPERTY, `${Math.round(Math.max(0, window.innerHeight - rect.top))}px`);
      } else {
        root.style.removeProperty(PROPERTY);
      }
    };
    const entry: Entry = {
      measure: () => {
        cancelAnimationFrame(frame);
        frame = requestAnimationFrame(() => {
          if (stack[stack.length - 1] === entry) apply();
        });
      },
    };
    stack.push(entry);
    entry.measure();
    // Absent only outside a browser (the component tests' jsdom); the window's own resize and the
    // dialog's animations still remeasure there.
    const resize = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(entry.measure);
    resize?.observe(footer);
    resize?.observe(dialog);
    window.addEventListener("resize", entry.measure);
    // The entrance animates the dialog's container, not the dialog, so its end is listened for on
    // the document.
    document.addEventListener("animationend", entry.measure, true);
    document.addEventListener("transitionend", entry.measure, true);
    return () => {
      cancelAnimationFrame(frame);
      resize?.disconnect();
      window.removeEventListener("resize", entry.measure);
      document.removeEventListener("animationend", entry.measure, true);
      document.removeEventListener("transitionend", entry.measure, true);
      const wasTop = stack[stack.length - 1] === entry;
      stack.splice(stack.indexOf(entry), 1);
      if (!wasTop) return;
      const next = stack[stack.length - 1];
      if (next) next.measure();
      else root.style.removeProperty(PROPERTY);
    };
  }, [inside, hasFooter]);
}
