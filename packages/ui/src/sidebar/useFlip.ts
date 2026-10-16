import { useEffect, useLayoutEffect, useRef } from "react";

const REDUCED_MOTION = "(prefers-reduced-motion: reduce)";

let reducedMotionQuery: { source: typeof window.matchMedia; query: MediaQueryList } | undefined;

/** Whether the system asks for reduced motion. The query is made once and then read, as a media
 * query list stays live. */
function prefersReducedMotion(): boolean {
  if (reducedMotionQuery?.source !== window.matchMedia) {
    reducedMotionQuery = { source: window.matchMedia, query: window.matchMedia(REDUCED_MOTION) };
  }
  return reducedMotionQuery.query.matches;
}

/** The `data-flip` children of `list`. */
function flipChildren(list: HTMLElement): HTMLElement[] {
  return Array.from(list.children).filter((child): child is HTMLElement => child instanceof HTMLElement && child.dataset.flip !== undefined);
}

/** What `useFlip` keeps for the list it is watching. */
interface ListWatch {
  list: HTMLElement;
  /** The list's owner has rendered. */
  rendered: () => void;
  stop: () => void;
}

/** Watches `list` for its children moving. A render of the list's owner is when the order can
 * change, and the keys are read from the children's `data-flip` attributes, which costs no layout:
 * only a change of keys measures the list there and then. Everything else that moves a child
 * (a row above opening or closing, a card changing height) resizes some child, which the
 * `ResizeObserver` reports once the browser has laid the page out anyway, before it paints. Such a
 * move slides the children, whichever component rendered; but where the list's own width changed
 * since it was last measured (the sidebar being dragged or its width transitioning), the children
 * move without sliding, and the positions are brought up to date either way. */
function watchList(list: HTMLElement): ListWatch {
  let positions = new Map<string, number>();
  let order = "";
  let width = 0;
  const observed = new Set<HTMLElement>();

  const measure = (animate: boolean) => {
    const next = new Map<string, number>();
    const slides = animate && !prefersReducedMotion();
    for (const child of flipChildren(list)) {
      const key = child.dataset.flip!;
      const top = child.offsetTop;
      next.set(key, top);
      const before = positions.get(key);
      if (slides && before !== undefined && before !== top) {
        child.animate([{ transform: `translateY(${before - top}px)` }, { transform: "translateY(0)" }], {
          duration: 260,
          easing: "cubic-bezier(0.2, 0, 0, 1)",
        });
      }
    }
    positions = next;
    width = list.clientWidth;
  };

  const observer = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(() => measure(list.clientWidth === width));
  observer?.observe(list);

  return {
    list,
    rendered() {
      const children = flipChildren(list);
      const keys = children.map((child) => child.dataset.flip).join("\n");
      if (keys === order) return;
      order = keys;
      if (observer) {
        for (const child of observed) {
          if (!children.includes(child)) {
            observer.unobserve(child);
            observed.delete(child);
          }
        }
        for (const child of children) {
          if (!observed.has(child)) {
            observer.observe(child);
            observed.add(child);
          }
        }
      }
      measure(true);
    },
    stop() {
      observer?.disconnect();
    },
  };
}

/**
 * Animates the reordering of a list: each direct child marked `data-flip` that moved since the last
 * time the list was measured slides from where it was to where it is now (the FLIP technique). Direct children only, so
 * a list that shows one row under another keeps every row a direct child and insets the nested ones
 * rather than nesting a list inside a row, which would leave those rows out of the animation.
 * Positions are read as `offsetTop` within the list, which `ref` must make the offset parent
 * (`relative`), so scrolling the sidebar does not read as movement. A child that just appeared is
 * left alone. Nothing moves where the system asks for reduced motion.
 *
 * Reading `offsetTop` makes the browser lay the page out there and then, so a render that leaves
 * the children's keys as they were does not read it (an event that changes nothing costs
 * nothing): a child moved by another's change of size (a project opening above it) is found by a
 * `ResizeObserver` instead, once the page is laid out anyway. See `watchList`. The hook takes no
 * argument, so a caller can place it first, ahead of the values it derives.
 *
 * React reorders keyed children by moving their DOM nodes, which blurs a focused control inside one
 * and drops focus to `<body>`; the list's focused element is remembered and given focus back.
 */
export function useFlip<T extends HTMLElement>(): React.RefObject<T | null> {
  const ref = useRef<T>(null);
  const watch = useRef<ListWatch | null>(null);
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
      watch.current?.stop();
      watch.current = null;
    };
  }, []);

  useLayoutEffect(() => {
    const list = ref.current;
    if (watch.current?.list !== list) {
      // The list is gone (collapsed project, empty state) or is another one; rows that come back
      // are new, not moved.
      watch.current?.stop();
      watch.current = list ? watchList(list) : null;
    }
    watch.current?.rendered();
    const lost = focused.current;
    if (lost?.isConnected && document.activeElement === document.body) lost.focus({ preventScroll: true });
  });

  return ref;
}
