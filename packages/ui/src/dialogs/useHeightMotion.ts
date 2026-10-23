import { useEffect, type RefObject } from "react";

import { onReducedMotion, prefersReducedMotion } from "../reducedMotion";
import { dialogAround } from "./dialogElement";

/**
 * Eases a dialog from its old height to its new one when its content changes it (a tab switched, an
 * error shown, a section opened), instead of letting it jump. `marker` is an element inside the
 * dialog, which finds its own element from it (see `dialogAround`).
 *
 * The dialog's own height is animated, as a Web Animation on the element that HeroUI's enter and
 * exit animations leave alone (those act on the container). The dialog centres itself in its
 * container, so the box grows or shrinks about its centre. The trade-off is in a grow: the body is
 * held shorter than its content for the 200ms, so its scroller overflows meanwhile. The bottom
 * fade shows, and a body that did not scroll becomes a focusable region for that time
 * (`useFadeRegion`). Heights are taken from a `ResizeObserver`, so nothing here
 * forces a layout, and the observer's own reports while the animation runs (the frames of the
 * height it sets) are ignored. Not animated: the first size, a change of the window's size (a
 * window being dragged would restart the animation on every frame), and anything under reduced
 * motion. A running animation is cancelled when reduced motion is requested, which leaves the
 * dialog at its new height.
 */
export function useHeightMotion(marker: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const dialog = dialogAround(marker);
    if (!dialog || typeof ResizeObserver === "undefined") return;
    let height: number | undefined;
    let viewport = "";
    let animation: Animation | undefined;
    const observer = new ResizeObserver((entries) => {
      if (animation?.playState === "running") return;
      const entry = entries[entries.length - 1];
      const next = entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height;
      const now = `${window.innerWidth}x${window.innerHeight}`;
      const before = height;
      const resized = now !== viewport;
      height = next;
      viewport = now;
      if (before === undefined || resized || Math.abs(next - before) < 1) return;
      if (prefersReducedMotion()) return;
      animation = dialog.animate([{ height: `${before}px` }, { height: `${next}px` }], {
        duration: 200,
        easing: "cubic-bezier(0.2, 0, 0, 1)",
      });
    });
    observer.observe(dialog);
    const stopReduced = onReducedMotion(() => animation?.cancel());
    return () => {
      stopReduced();
      observer.disconnect();
      animation?.cancel();
    };
  }, [marker]);
}
