import { useLayoutEffect, type RefObject } from "react";

import { TOAST_BREAKPOINT, TOAST_MARGIN, TOAST_WIDTH } from "../components/toastGeometry";
import { dialogAround } from "./dialogElement";

/** The custom property `style.css` lifts the toast region by, from the window's bottom edge. */
const PROPERTY = "--dialog-footer-clearance";

const REGION = '[data-slot="toast-region"]';
const TOAST = '[data-slot="toast"]';
const STRIPS = ["--connection-banner-height", "--update-notice-height"];

const FOOTER = '[data-slot="modal-footer"], [data-slot="alert-dialog-footer"]';

type Entry = { measure: () => void };

/** The mounted dialogs that have a footer, the innermost last. Only the innermost sets the property,
 * and when it goes the one beneath it measures again. */
const stack: Entry[] = [];

/** Reserves the full expanded stack at its ordinary bottom position, without dialog clearance.
 * HeroUI's children are absolute (the region has no height), so its settled layout variables
 * supply their bounds. Reserving expansion even while collapsed keeps a hover from moving the
 * stack off the pointer, collapsing it, and bringing it back under the pointer repeatedly. */
function overlapsToasts(footer: DOMRect, region: HTMLElement | null): boolean {
  if (window.innerWidth < TOAST_BREAKPOINT || !region) return false;
  const rtl = getComputedStyle(document.documentElement).direction === "rtl";
  const left = rtl ? TOAST_MARGIN : window.innerWidth - TOAST_MARGIN - TOAST_WIDTH;
  if (footer.right <= left || footer.left >= left + TOAST_WIDTH) return false;
  const chrome = STRIPS.reduce((height, name) => height + (parseFloat(document.documentElement.style.getPropertyValue(name)) || 0), 0);
  const bottom = window.innerHeight - TOAST_MARGIN - chrome;
  return [...region.querySelectorAll<HTMLElement>(TOAST)].some((toast) => {
    if (toast.dataset.hidden === "true" || toast.dataset.exiting === "true") return false;
    const value = (name: string) => parseFloat(toast.style.getPropertyValue(name));
    const height = value("--toast-height") || toast.offsetHeight;
    const offset = value("--offset-expanded") || 0;
    return height > 0 && bottom - offset > footer.top && bottom - offset - height < footer.bottom;
  });
}

/** Keeps the toast stack's expanded footprint clear of the innermost dialog's footer only when
 * it would overlap at its ordinary bottom position. Dialog and toast resizes, stack additions
 * and removals, and the bottom strips can each change that intersection. */
export function useToastClearance(inside: RefObject<HTMLElement | null>, hasFooter: boolean): void {
  useLayoutEffect(() => {
    const dialog = dialogAround(inside);
    const footer = hasFooter ? dialog?.querySelector<HTMLElement>(FOOTER) : null;
    if (!dialog || !footer) return;
    const root = document.documentElement;
    let frame = 0;
    let waiting = false;
    let disposed = false;
    let region: HTMLElement | null = null;
    const setClearance = (value: string) => {
      if (root.style.getPropertyValue(PROPERTY) === value) return;
      if (value) root.style.setProperty(PROPERTY, value);
      else root.style.removeProperty(PROPERTY);
    };
    const apply = () => {
      const rect = footer.getBoundingClientRect();
      if (overlapsToasts(rect, region)) {
        setClearance(`${Math.round(Math.max(0, window.innerHeight - rect.top))}px`);
      } else {
        setClearance("");
      }
    };
    const entry: Entry = {
      measure: () => {
        // While the dialog's own height eases (`useHeightMotion`) it resizes every frame, and each
        // measurement would set a property on `<html>`, restyling the whole document. So the
        // measurement waits for the animation to end (`animationend` is not sent for one made
        // with `animate()`), and checks for it only once a frame has passed: this observer is
        // told of a change before `useHeightMotion`'s is, so the report that starts the ease
        // arrives before the animation exists. `getAnimations` flushes style in WebKit, hence
        // nothing asks it while a wait is under way.
        if (waiting) return;
        cancelAnimationFrame(frame);
        frame = requestAnimationFrame(() => {
          if (stack[stack.length - 1] !== entry) return;
          const running = dialog.getAnimations?.().filter((animation) => animation.playState === "running") ?? [];
          if (running.length === 0) {
            apply();
            return;
          }
          waiting = true;
          void Promise.allSettled(running.map((animation) => animation.finished)).then(() => {
            waiting = false;
            if (!disposed) entry.measure();
          });
        });
      },
    };
    stack.push(entry);
    entry.measure();
    const resize = new ResizeObserver(entry.measure);
    resize.observe(footer);
    resize.observe(dialog);
    // Observe the absolute toast children themselves, and rebind when the provider is mounted
    // or replaced. Only the region's mutations matter; the rest of the app changes frequently.
    const toastChanges = new MutationObserver(() => {
      observeToasts();
      entry.measure();
    });
    let observedToasts: HTMLElement[] = [];
    const observeToasts = () => {
      const next = [...(region?.querySelectorAll<HTMLElement>(TOAST) ?? [])];
      for (const toast of observedToasts) if (!next.includes(toast)) resize.unobserve(toast);
      for (const toast of next) if (!observedToasts.includes(toast)) resize.observe(toast);
      observedToasts = next;
    };
    const findRegion = () => {
      const next = document.querySelector<HTMLElement>(REGION);
      if (next === region) return;
      region = next;
      toastChanges.disconnect();
      if (region) toastChanges.observe(region, {
        childList: true,
        subtree: true,
        attributes: true,
        attributeFilter: ["style", "data-hidden", "data-exiting"],
      });
      observeToasts();
      entry.measure();
    };
    const providers = new MutationObserver((records) => {
      if (records.some((record) => [...record.addedNodes, ...record.removedNodes].some((node) =>
        node instanceof Element && (node.matches(REGION) || node.querySelector(REGION)),
      ))) findRegion();
    });
    providers.observe(document.body, { childList: true, subtree: true });
    findRegion();
    let stripHeights = STRIPS.map((name) => root.style.getPropertyValue(name)).join(";");
    const strips = new MutationObserver(() => {
      const next = STRIPS.map((name) => root.style.getPropertyValue(name)).join(";");
      if (next === stripHeights) return;
      stripHeights = next;
      entry.measure();
    });
    strips.observe(root, { attributes: true, attributeFilter: ["style"] });
    window.addEventListener("resize", entry.measure);
    // The entrance animates the dialog's container, not the dialog, so its end is listened for on
    // the document.
    document.addEventListener("animationend", entry.measure, true);
    document.addEventListener("transitionend", entry.measure, true);
    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      resize.disconnect();
      toastChanges.disconnect();
      providers.disconnect();
      strips.disconnect();
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
