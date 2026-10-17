import { useLayoutEffect, useRef } from "react";

import { prefersReducedMotion } from "../reducedMotion";

const SELECTOR = "[data-list-motion], [data-flip]";
const DURATION = 260;
const EASING = "cubic-bezier(0.2, 0, 0, 1)";

interface ListMotionOptions {
  /** Rows behind wrappers, such as cached collections and display:contents groups. */
  descendants?: boolean;
  /** A virtualizer's visible rows: only the same rendered element and identity may move. */
  virtualized?: boolean;
}

interface Point { x: number; y: number }
interface Position extends Point { element: HTMLElement }
interface Slide extends Point { animation: Animation }
interface ListWatch { list: HTMLElement; rendered: () => void; stop: () => void }

/** Layout coordinates ignore running translations and scrolling. The list and rows share the
 * same offset-parent chain; subtracting their origins also supports display:contents wrappers. */
function origin(element: HTMLElement): Point {
  let x = 0;
  let y = 0;
  for (let at: HTMLElement | null = element; at; at = at.offsetParent as HTMLElement | null) {
    x += at.offsetLeft;
    y += at.offsetTop;
  }
  return { x, y };
}

function watchList(list: HTMLElement, { descendants = false, virtualized = false }: ListMotionOptions): ListWatch {
  let positions = new Map<string, Position>();
  let order = "";
  let width = list.clientWidth;
  let height = list.clientHeight;
  let scrollTop = list.scrollTop;
  let scrollLeft = list.scrollLeft;
  let stopped = false;
  const observed = new Set<HTMLElement>();
  const slides = new Map<HTMLElement, Slide>();
  const query = typeof window.matchMedia === "function" ? window.matchMedia("(prefers-reduced-motion: reduce)") : undefined;
  const children = () => (descendants || virtualized
    ? Array.from(list.querySelectorAll<HTMLElement>(SELECTOR))
    : Array.from(list.children).filter((child): child is HTMLElement => child instanceof HTMLElement && child.matches(SELECTOR)))
    // A marked group owns its descendants' movement. Animating both would double their travel.
    .filter((child) => {
      const parent = child.parentElement?.closest(SELECTOR);
      return !parent || parent === list || !list.contains(parent);
    });
  const offset = (element: HTMLElement): Point => {
    const slide = slides.get(element);
    const progress = slide?.animation.effect?.getComputedTiming().progress;
    const remaining = typeof progress === "number" ? 1 - progress : 0;
    return { x: (slide?.x ?? 0) * remaining, y: (slide?.y ?? 0) * remaining };
  };
  const cancel = (element: HTMLElement) => {
    slides.get(element)?.animation.cancel();
    slides.delete(element);
  };
  const measure = (animate: boolean) => {
    if (stopped) return;
    const rows = children();
    const next = new Map<string, Position>();
    const newWidth = list.clientWidth;
    const newHeight = list.clientHeight;
    const scrolled = scrollTop !== list.scrollTop || scrollLeft !== list.scrollLeft;
    const moving = animate && !prefersReducedMotion() && width === newWidth && (!virtualized || (height === newHeight && !scrolled));
    const base = virtualized ? list.getBoundingClientRect() : origin(list);
    // Read all geometry before creating or cancelling animations, keeping layout reads in one pass.
    const moves = rows.map((element) => {
      const residual = offset(element);
      const point = virtualized ? element.getBoundingClientRect() : origin(element);
      const position = { element, x: point.x - base.x + (virtualized ? list.scrollLeft - residual.x : 0), y: point.y - base.y + (virtualized ? list.scrollTop - residual.y : 0) };
      const key = element.dataset.listMotion ?? element.dataset.flip!;
      const before = positions.get(key);
      next.set(key, position);
      return { element, position, before, residual };
    });
    for (const element of slides.keys()) if (!rows.includes(element) || !moving) cancel(element);
    for (const { element, position, before, residual } of moves) {
      if (!moving) continue;
      if (!before || before.element !== element) {
        cancel(element);
        continue;
      }
      if (before.x === position.x && before.y === position.y) continue;
      const x = before.x - position.x + residual.x;
      const y = before.y - position.y + residual.y;
      cancel(element);
      // `translate` composes with a virtualizer's transform rather than replacing its layout.
      const animation = element.animate?.([{ translate: `${x}px ${y}px` }, { translate: "0px 0px" }], { duration: DURATION, easing: EASING });
      if (animation) {
        slides.set(element, { animation, x, y });
        animation.onfinish = () => { if (slides.get(element)?.animation === animation) slides.delete(element); };
      }
    }
    positions = next;
    width = newWidth;
    height = newHeight;
    scrollTop = list.scrollTop;
    scrollLeft = list.scrollLeft;
    for (const element of observed) if (!rows.includes(element)) { resize?.unobserve(element); observed.delete(element); }
    for (const element of rows) if (!observed.has(element)) { resize?.observe(element); observed.add(element); }
    order = JSON.stringify(rows.map((element) => element.dataset.listMotion ?? element.dataset.flip));
  };
  const resize = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(() => measure(true));
  resize?.observe(list);
  // A cached collection may commit after its owner; observe structural and layout changes there.
  // WAAPI changes no attributes, so observing style does not feed the animation back into itself.
  const keysChanged = () => JSON.stringify(children().map((element) => element.dataset.listMotion ?? element.dataset.flip)) !== order;
  const mutations = typeof MutationObserver === "undefined" ? undefined : new MutationObserver((records) => {
    // Ordinary content sizing belongs to ResizeObserver. Hover classes and changing text inside
    // rows must not turn every interaction into a synchronous layout pass.
    const layoutChanged = virtualized && records.some((record) => record.type === "attributes" && record.attributeName === "style" && record.target instanceof HTMLElement && (record.target.matches(SELECTOR) || record.target.querySelector(SELECTOR)));
    if (keysChanged() || layoutChanged) measure(true);
  });
  mutations?.observe(list, { childList: true, subtree: true, attributes: true, attributeFilter: virtualized ? ["data-list-motion", "data-flip", "style"] : ["data-list-motion", "data-flip"] });
  const onScroll = () => { if (virtualized) measure(false); };
  const onReducedMotion = () => { if (prefersReducedMotion()) measure(false); };
  list.addEventListener("scroll", onScroll, { passive: true });
  query?.addEventListener?.("change", onReducedMotion);
  return {
    list,
    rendered() {
      const keys = JSON.stringify(children().map((element) => element.dataset.listMotion ?? element.dataset.flip));
      if (keys !== order) measure(true);
    },
    stop() {
      stopped = true;
      resize?.disconnect();
      mutations?.disconnect();
      list.removeEventListener("scroll", onScroll);
      query?.removeEventListener?.("change", onReducedMotion);
      for (const element of slides.keys()) cancel(element);
    },
  };
}

/** Surviving keyed rows slide to their new positions in either axis. Mark rows with
 * data-list-motion={stableId}; additions and removals take effect immediately. Layout changes
 * caused by scrolling or container width changes update the baseline without animation. */
export function useListMotion<T extends HTMLElement>({ descendants = false, virtualized = false }: ListMotionOptions = {}): React.RefObject<T | null> {
  const ref = useRef<T>(null);
  const watch = useRef<ListWatch | null>(null);
  const focused = useRef<HTMLElement | null>(null);
  useLayoutEffect(() => {
    const onFocus = (event: FocusEvent) => {
      focused.current = event.target instanceof HTMLElement && ref.current?.contains(event.target) ? event.target : null;
    };
    const pending = new Set<ReturnType<typeof setTimeout>>();
    const onBlur = (event: FocusEvent) => {
      const target = event.target;
      const timer = setTimeout(() => {
        pending.delete(timer);
        if (focused.current === target && document.activeElement !== target) focused.current = null;
      }, 0);
      pending.add(timer);
    };
    // Accessible virtual collections own their focus handoffs, including removed-row fallbacks.
    if (!virtualized) {
      document.addEventListener("focusin", onFocus);
      document.addEventListener("focusout", onBlur);
    }
    return () => {
      document.removeEventListener("focusin", onFocus);
      document.removeEventListener("focusout", onBlur);
      for (const timer of pending) clearTimeout(timer);
      watch.current?.stop();
      watch.current = null;
    };
  }, [virtualized]);
  useLayoutEffect(() => {
    const list = ref.current;
    if (watch.current?.list !== list) {
      watch.current?.stop();
      watch.current = list ? watchList(list, { descendants, virtualized }) : null;
    }
    watch.current?.rendered();
    const lost = focused.current;
    if (!virtualized && lost?.isConnected && list?.contains(lost) && document.activeElement === document.body) lost.focus({ preventScroll: true });
  });
  return ref;
}
