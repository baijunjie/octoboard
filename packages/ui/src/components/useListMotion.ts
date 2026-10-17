import { useLayoutEffect, useRef } from "react";

import { prefersReducedMotion } from "../reducedMotion";

const SELECTOR = "[data-list-motion], [data-flip]";
const DURATION = 300;
const PRESENCE_EASING = "cubic-bezier(0.4, 0, 0.2, 1)";
const EASING = "cubic-bezier(0.2, 0, 0, 1)";

interface ListMotionOptions {
  /** Rows behind wrappers, such as cached collections and display:contents groups. */
  descendants?: boolean;
  /** A virtualizer's visible rows: only the same rendered element and identity may move. */
  virtualized?: boolean;
}

interface Point { x: number; y: number }
interface Position extends Point { element: HTMLElement; parent: HTMLElement | null; width: number; height: number; columns?: string }
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

// Presence uses transform while layout movement uses translate; neither replaces the other.
function presenceFrames(transform: string, entering: boolean): Keyframe[] {
  const base = transform && transform !== "none" ? transform : "";
  const shifted = `translateX(${entering ? -25 : 25}%)${base ? ` ${base}` : ""}`;
  return entering
    ? [{ opacity: 0, transform: shifted }, { opacity: 1, transform: base || "none" }]
    : [{ opacity: 1, transform: base || "none" }, { opacity: 0, transform: shifted }];
}

function watchList(list: HTMLElement, { descendants = false, virtualized = false }: ListMotionOptions): ListWatch {
  let positions = new Map<string, Position>();
  let initialized = false;
  let order = "";
  let width = list.clientWidth;
  let height = list.clientHeight;
  let scrollTop = list.scrollTop;
  let scrollLeft = list.scrollLeft;
  let stopped = false;
  const observed = new Set<HTMLElement>();
  const slides = new Map<HTMLElement, Slide>();
  const entrances = new Map<HTMLElement, Animation>();
  const exits = new Map<string, { element: HTMLElement; animation: Animation }>();
  const query = typeof window.matchMedia === "function" ? window.matchMedia("(prefers-reduced-motion: reduce)") : undefined;
  const children = () => (descendants || virtualized
    ? Array.from(list.querySelectorAll<HTMLElement>(SELECTOR))
    : Array.from(list.children).filter((child): child is HTMLElement => child instanceof HTMLElement && child.matches(SELECTOR)))
    // A marked group owns its descendants' movement. Animating both would double their travel.
    .filter((child) => {
      if (child.closest("[data-list-motion-ghost]")) return false;
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
  const clearEntrance = (element: HTMLElement) => {
    entrances.get(element)?.cancel();
    entrances.delete(element);
  };
  const clearExit = (key: string) => {
    const exit = exits.get(key);
    if (!exit) return;
    exits.delete(key);
    exit.animation.cancel();
    exit.element.remove();
  };
  const leave = (key: string, before: Position) => {
    // React removes the real control immediately. Its inert copy is only a visual; it never
    // holds focus, participates in the collection, or keeps a removed action reachable.
    const ghost = before.element.cloneNode(true) as HTMLElement;
    for (const node of [ghost, ...ghost.querySelectorAll<HTMLElement>("*")]) {
      for (const attribute of Array.from(node.attributes)) {
        if (attribute.name === "id" || attribute.name === "name" || attribute.name === "autofocus" || attribute.name === "data-list-motion" || attribute.name === "data-flip" || attribute.name.startsWith("on")) node.removeAttribute(attribute.name);
      }
    }
    ghost.dataset.listMotionGhost = "";
    ghost.setAttribute("aria-hidden", "true");
    ghost.setAttribute("inert", "");
    const residual = offset(before.element);
    let parent = before.parent && list.contains(before.parent) ? before.parent : list;
    let scrollX = 0;
    let scrollY = 0;
    for (let at = before.parent; at && at !== list; at = at.parentElement) {
      const style = getComputedStyle(at);
      if (/(auto|scroll|hidden|clip)/.test(`${style.overflowX} ${style.overflowY}`)) parent = list;
      scrollX += at.scrollLeft;
      scrollY += at.scrollTop;
    }
    // A shrinking inner scroller would clip its last departing row and retain phantom overflow.
    // Lift that visual to the collection, preserving its position within nested scrollers.
    const escaped = parent !== before.parent;
    const visual = document.createElement("div");
    visual.dataset.listMotionExit = "";
    visual.setAttribute("aria-hidden", "true");
    visual.setAttribute("inert", "");
    // Bound the departing copy to its previous box so its rightward travel cannot enlarge
    // an ancestor's scrollable area. The box survives outside a shrinking inner scroller.
    Object.assign(visual.style, { position: "absolute", width: `${before.width}px`, height: `${before.height}px`, overflow: "clip", pointerEvents: "none", translate: `${residual.x}px ${residual.y}px` });
    Object.assign(ghost.style, { position: "absolute", right: "auto", bottom: "auto", margin: "0", width: `${before.width}px`, height: `${before.height}px`, boxSizing: "border-box", gridTemplateColumns: before.columns ?? ghost.style.gridTemplateColumns, pointerEvents: "none", translate: "0px 0px" });
    visual.appendChild(ghost);
    parent.appendChild(visual);
    const base = origin(list);
    const containing = visual.offsetParent instanceof HTMLElement ? origin(visual.offsetParent) : { x: 0, y: 0 };
    visual.style.left = `${before.x + base.x - containing.x - (escaped ? scrollX : 0)}px`;
    visual.style.top = `${before.y + base.y - containing.y - (escaped ? scrollY : 0)}px`;
    ghost.style.left = "0px";
    ghost.style.top = "0px";
    const animation = ghost.animate?.(presenceFrames(getComputedStyle(ghost).transform, false), { duration: DURATION, easing: PRESENCE_EASING });
    if (!animation) { visual.remove(); return; }
    exits.set(key, { element: visual, animation });
    animation.onfinish = () => { if (exits.get(key)?.animation === animation) clearExit(key); };
    animation.oncancel = () => {
      if (exits.get(key)?.animation !== animation) return;
      exits.delete(key);
      visual.remove();
    };
  };
  const measure = (animate: boolean) => {
    if (stopped) return;
    const rows = children();
    const next = new Map<string, Position>();
    const newWidth = list.clientWidth;
    const newHeight = list.clientHeight;
    const scrolled = scrollTop !== list.scrollTop || scrollLeft !== list.scrollLeft;
    const moving = initialized && animate && !prefersReducedMotion() && width === newWidth && (!virtualized || (height === newHeight && !scrolled));
    const base = virtualized ? list.getBoundingClientRect() : origin(list);
    // Read all geometry before creating or cancelling animations, keeping layout reads in one pass.
    const moves = rows.map((element) => {
      const residual = offset(element);
      const point = virtualized ? element.getBoundingClientRect() : origin(element);
      const style = virtualized ? undefined : getComputedStyle(element);
      // Absolute elements cannot inherit subgrid tracks. Full-span rows keep the parent's
      // resolved tracks so their visual copy retains the same column widths after removal.
      const columns = style?.gridTemplateColumns.startsWith("subgrid") && style.gridColumn === "1 / -1" && element.parentElement
        ? getComputedStyle(element.parentElement).gridTemplateColumns : undefined;
      const position = { element, columns, parent: element.parentElement, width: virtualized ? 0 : element.offsetWidth, height: virtualized ? 0 : element.offsetHeight, x: point.x - base.x + (virtualized ? list.scrollLeft - residual.x : 0), y: point.y - base.y + (virtualized ? list.scrollTop - residual.y : 0) };
      const key = element.dataset.listMotion ?? element.dataset.flip!;
      const before = positions.get(key);
      next.set(key, position);
      return { element, position, before, residual, transform: style?.transform ?? "none" };
    });
    for (const key of exits.keys()) if (!moving || next.has(key)) clearExit(key);
    if (moving && !virtualized) {
      for (const [key, before] of positions) if (!next.has(key)) leave(key, before);
    }
    for (const element of entrances.keys()) if (!rows.includes(element) || !moving) clearEntrance(element);
    for (const element of slides.keys()) if (!rows.includes(element) || !moving) cancel(element);
    for (const { element, position, before, residual, transform } of moves) {
      if (!moving) continue;
      if (!before || before.element !== element) {
        cancel(element);
        clearEntrance(element);
        if (!virtualized && !before) {
          const animation = element.animate?.(presenceFrames(transform, true), { duration: DURATION, easing: PRESENCE_EASING });
          if (animation) {
            entrances.set(element, animation);
            animation.onfinish = () => { if (entrances.get(element) === animation) entrances.delete(element); };
          }
        }
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
    initialized = initialized || rows.length > 0;
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
  // Cached collections can populate in a second commit during the same mount. Once that
  // commit settles, a first addition to a genuinely empty collection is a live update.
  queueMicrotask(() => { if (!stopped) initialized = true; });
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
  const onScroll = () => {
    if (virtualized) measure(false);
    else for (const key of exits.keys()) clearExit(key);
  };
  const onReducedMotion = () => { if (prefersReducedMotion()) measure(false); };
  list.addEventListener("scroll", onScroll, { passive: true, capture: true });
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
      list.removeEventListener("scroll", onScroll, true);
      query?.removeEventListener?.("change", onReducedMotion);
      for (const element of slides.keys()) cancel(element);
      for (const element of entrances.keys()) clearEntrance(element);
      for (const key of exits.keys()) clearExit(key);
    },
  };
}

/** Keyed rows enter, leave and slide to their new positions in either axis. Removed controls
 * leave only inert visual copies; actions and focus handoffs take effect immediately. Virtual
 * collections animate surviving rows only, never newly rendered or recycled identities. */
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
