import { useEffect, useLayoutEffect, useRef } from "react";

const REDUCED_MOTION = "(prefers-reduced-motion: reduce)";

/**
 * Animates the reordering of a list: each direct child marked `data-flip` that moved since the last
 * render slides from where it was to where it is now (the FLIP technique). Positions are read as
 * `offsetTop` within the list, which `ref` must make the offset parent (`relative`), so scrolling
 * the sidebar does not read as movement. A child that just appeared is left alone. Nothing moves
 * where the system asks for reduced motion.
 *
 * React reorders keyed children by moving their DOM nodes, which blurs a focused control inside one
 * and drops focus to `<body>`; the list's focused element is remembered and given focus back.
 */
export function useFlip<T extends HTMLElement>(): React.RefObject<T | null> {
  const ref = useRef<T>(null);
  const positions = useRef(new Map<string, number>());
  const focused = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const onFocusIn = (event: FocusEvent) => {
      const target = event.target;
      focused.current = target instanceof HTMLElement && ref.current?.contains(target) ? target : null;
    };
    // A move blurs the element and the layout effect below refocuses it within the same task, so
    // forgetting it one task later only drops an element the user really left.
    const onFocusOut = (event: FocusEvent) => {
      const target = event.target;
      setTimeout(() => {
        if (focused.current === target && document.activeElement !== target) focused.current = null;
      }, 0);
    };
    document.addEventListener("focusin", onFocusIn);
    document.addEventListener("focusout", onFocusOut);
    return () => {
      document.removeEventListener("focusin", onFocusIn);
      document.removeEventListener("focusout", onFocusOut);
    };
  }, []);

  useLayoutEffect(() => {
    const list = ref.current;
    if (!list) {
      // The list is gone (collapsed project, empty state); rows that come back are new, not moved.
      positions.current = new Map();
      return;
    }
    const animate = !window.matchMedia(REDUCED_MOTION).matches;
    const next = new Map<string, number>();
    for (const child of Array.from(list.children)) {
      if (!(child instanceof HTMLElement) || child.dataset.flip === undefined) continue;
      const key = child.dataset.flip;
      const top = child.offsetTop;
      next.set(key, top);
      const before = positions.current.get(key);
      if (animate && before !== undefined && before !== top) {
        child.animate([{ transform: `translateY(${before - top}px)` }, { transform: "translateY(0)" }], {
          duration: 260,
          easing: "cubic-bezier(0.2, 0, 0, 1)",
        });
      }
    }
    positions.current = next;
    const lost = focused.current;
    if (lost?.isConnected && document.activeElement === document.body) lost.focus({ preventScroll: true });
  });

  return ref;
}
