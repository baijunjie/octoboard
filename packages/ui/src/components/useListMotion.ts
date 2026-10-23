import { useLayoutEffect, useRef, useState } from "react";

import { onReducedMotion, prefersReducedMotion } from "../reducedMotion";

const SELECTOR = "[data-list-motion], [data-flip]";
const DURATION = 300;
const PRESENCE_EASING = "cubic-bezier(0.4, 0, 0.2, 1)";
const EASING = "cubic-bezier(0.2, 0, 0, 1)";

interface ListMotionOptions {
  /** Rows behind wrappers, such as cached collections and display:contents groups. */
  descendants?: boolean;
  /** A virtualizer's visible rows: only the same rendered element and identity may move. */
  virtualized?: boolean;
  /** Logical visible identities, including rows outside the virtualizer's DOM window. */
  keys?: ReadonlySet<string>;
  /** How those identities nest, so that rows a parent folds in or out appear and vanish with it. */
  folds?: ListFolds;
  /** A row of items as wide as its content and aligned to its reading start, such as a tab strip: it grows and
   * shrinks at its reading end with its items, so a change of its width that comes with a change of items is that
   * change, not a resize, and positions are taken from its reading-start edge, which stays put as it does (its
   * right edge under RTL). */
  contentWidth?: boolean;
}

/** A tree's rows by their parent row, and the parents whose rows are shown. A row added or removed
 * under a parent listed on both sides but not shown on both folds with it, as do the rows under a
 * parent that folds; the others, top-level rows among them, enter or leave. */
export interface ListFolds {
  parents: ReadonlyMap<string, string>;
  open: ReadonlySet<string>;
}

interface Point { x: number; y: number }
interface Position extends Point { element: HTMLElement; parent: HTMLElement | null; width: number; height: number; columns?: string }
interface Slide extends Point { animation: Animation }
interface ListWatch { list: HTMLElement; prepare: (keys: ReadonlySet<string>, folds?: ListFolds) => void; release: () => void; rendered: () => void; stop: () => void }

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
// An exit fades from the opacity the row was shown at, which a row already marked as going may have lowered.
function presenceFrames(transform: string, entering: boolean, opacity = "1"): Keyframe[] {
  const base = transform && transform !== "none" ? transform : "";
  const shifted = `translateX(${entering ? -25 : 25}%)${base ? ` ${base}` : ""}`;
  return entering
    ? [{ opacity: 0, transform: shifted }, { opacity: 1, transform: base || "none" }]
    : [{ opacity: Number(opacity || 1), transform: base || "none" }, { opacity: 0, transform: shifted }];
}

/** Whether `key`, listed on one side of a change and not the other, folds with its parent: the
 * parent is listed on both sides but not shown on both, or is itself missing from the other side and folds. */
function folded(key: string, side: ListFolds, other: ListFolds, otherKeys: ReadonlySet<string>): boolean {
  const parent = side.parents.get(key);
  if (parent === undefined) return false;
  if (!otherKeys.has(parent)) return folded(parent, side, other, otherKeys);
  return !(side.open.has(parent) && other.open.has(parent));
}

/** The offset a row's own `transform` gives it, as computed: mid-transition where one is running. */
function transformOffset(element: HTMLElement): Point {
  const transform = getComputedStyle(element).transform;
  if (!transform || transform === "none" || typeof DOMMatrixReadOnly !== "function") return { x: 0, y: 0 };
  const matrix = new DOMMatrixReadOnly(transform);
  return { x: matrix.m41, y: matrix.m42 };
}

/** The motion watching each list, for `releaseListMotion` to find from one of its rows. */
const watching = new WeakMap<HTMLElement, ListWatch>();

/** For rows a caller has been moving by hand with an inline `transform` (a drag), called as it lets go, before it
 * clears those transforms and commits the new order: the rows' movement to their new places starts from where they
 * are shown rather than from where they were laid out. Holds for that change only. `row` is any row of the
 * collection: the handler of a row a react-aria collection renders can belong to another render of the hook's
 * owner than the one holding the list (see `followList`), so the list is found from the row rather than a ref. */
export function releaseListMotion(row: Element): void {
  for (let at = row.parentElement; at; at = at.parentElement) {
    const watch = watching.get(at);
    if (watch) {
      watch.release();
      return;
    }
  }
}

function watchList(list: HTMLElement, { descendants = false, virtualized = false, keys, folds, contentWidth = false }: ListMotionOptions): ListWatch {
  let logical = keys;
  let logicalFolds = folds;
  const added = new Set<string>();
  const departing = new Map<string, Position>();
  let additionTimer: ReturnType<typeof setTimeout> | undefined;
  let positions = new Map<string, Position>();
  let initialized = false;
  let order = "";
  let width = list.clientWidth;
  let outerWidth = list.offsetWidth;
  let height = list.clientHeight;
  let scrollTop = list.scrollTop;
  let scrollLeft = list.scrollLeft;
  let stopped = false;
  // Where rows moved by hand stand as their caller releases them (`release`), for the next change only.
  let released: Map<HTMLElement, Point> | undefined;
  let releaseTimer: ReturnType<typeof setTimeout> | undefined;
  // A content-width list's own inline styles while its items' motion is held inside it (`hold`).
  let held: { minWidth: string; overflow: string } | undefined;
  let holdTimer: ReturnType<typeof setTimeout> | undefined;
  const observed = new Set<HTMLElement>();
  const slides = new Map<HTMLElement, Slide>();
  const entrances = new Map<HTMLElement, Animation>();
  const exits = new Map<string, { element: HTMLElement; animation: Animation }>();
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
    const carried = released?.get(element);
    return { x: (slide?.x ?? 0) * remaining + (carried?.x ?? 0), y: (slide?.y ?? 0) * remaining + (carried?.y ?? 0) };
  };
  // A content-width row grows and shrinks at its end, so its start edge is the fixed point: under RTL, its right edge.
  const anchor = (): Point => {
    const at = origin(list);
    return contentWidth && getComputedStyle(list).direction === "rtl" ? { x: at.x + list.offsetWidth, y: at.y } : at;
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
    const ghost = virtualized ? before.element : before.element.cloneNode(true) as HTMLElement;
    for (const node of [ghost, ...ghost.querySelectorAll<HTMLElement>("*")]) {
      for (const attribute of Array.from(node.attributes)) {
        if (attribute.name === "id" || attribute.name === "name" || attribute.name === "autofocus" || attribute.name === "data-list-motion" || attribute.name === "data-flip" || attribute.name.startsWith("on")) node.removeAttribute(attribute.name);
      }
    }
    ghost.dataset.listMotionGhost = "";
    ghost.setAttribute("aria-hidden", "true");
    ghost.setAttribute("inert", "");
    const residual = virtualized ? { x: 0, y: 0 } : offset(before.element);
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
    const base = anchor();
    const containing = visual.offsetParent instanceof HTMLElement ? origin(visual.offsetParent) : { x: 0, y: 0 };
    visual.style.left = `${before.x + base.x - containing.x - (escaped ? scrollX : 0)}px`;
    visual.style.top = `${before.y + base.y - containing.y - (escaped ? scrollY : 0)}px`;
    ghost.style.left = "0px";
    ghost.style.top = "0px";
    const shown = getComputedStyle(ghost);
    const animation = ghost.animate?.(presenceFrames(shown.transform, false, shown.opacity), { duration: DURATION, easing: PRESENCE_EASING });
    if (!animation) { visual.remove(); return; }
    exits.set(key, { element: visual, animation });
    animation.onfinish = () => { if (exits.get(key)?.animation === animation) clearExit(key); };
    animation.oncancel = () => {
      if (exits.get(key)?.animation !== animation) return;
      exits.delete(key);
      visual.remove();
    };
  };
  const settle = () => {
    clearTimeout(holdTimer);
    if (!held) return;
    list.style.minWidth = held.minWidth;
    list.style.overflow = held.overflow;
    held = undefined;
  };
  // A content-width row keeps its items' travel inside itself until it has played out, so that travel cannot
  // overflow an enclosing scroller and set it showing its scroll controls or scrolling: it clips, as an item
  // entering at its end (its left under RTL) starts past it, and a narrower row keeps its old width, as the items
  // leaving and those sliding up start past its new end. It is let go once they are done.
  const hold = (newOuterWidth: number) => {
    held ??= { minWidth: list.style.minWidth, overflow: list.style.overflow };
    list.style.overflow = "clip";
    if (newOuterWidth < outerWidth) list.style.minWidth = `${outerWidth}px`;
    clearTimeout(holdTimer);
    holdTimer = setTimeout(settle, DURATION);
  };
  const measure = (animate: boolean) => {
    if (stopped) return;
    const rows = children();
    const next = new Map<string, Position>();
    const newWidth = list.clientWidth;
    const newOuterWidth = contentWidth ? list.offsetWidth : outerWidth;
    const newHeight = list.clientHeight;
    const scrolled = scrollTop !== list.scrollTop || scrollLeft !== list.scrollLeft;
    const newOrder = JSON.stringify(rows.map((element) => element.dataset.listMotion ?? element.dataset.flip));
    const resized = width !== newWidth && !(contentWidth && newOrder !== order);
    const moving = initialized && animate && !prefersReducedMotion() && !resized && (!virtualized || (height === newHeight && !scrolled));
    const base = virtualized ? list.getBoundingClientRect() : anchor();
    // Read all geometry before creating or cancelling animations, keeping layout reads in one pass.
    const moves = rows.map((element) => {
      const residual = offset(element);
      // Virtual wrappers carry layout transforms; the row itself may be entering. Measure
      // through its parent so that presence never becomes a spurious layout displacement.
      const parent = virtualized && logical ? element.parentElement : null;
      const wrapper = parent?.getBoundingClientRect();
      const local = parent ? origin(element) : undefined;
      const parentOrigin = parent ? origin(parent) : undefined;
      const point = wrapper && local && parentOrigin && parent
        ? { x: wrapper.x + local.x - parentOrigin.x - parent.scrollLeft, y: wrapper.y + local.y - parentOrigin.y - parent.scrollTop }
        : virtualized ? element.getBoundingClientRect() : origin(element);
      const style = virtualized ? undefined : getComputedStyle(element);
      // Absolute elements cannot inherit subgrid tracks. Full-span rows keep the parent's
      // resolved tracks so their visual copy retains the same column widths after removal.
      const columns = style?.gridTemplateColumns.startsWith("subgrid") && style.gridColumn === "1 / -1" && element.parentElement
        ? getComputedStyle(element.parentElement).gridTemplateColumns : undefined;
      const position = { element, columns, parent: element.parentElement, width: virtualized ? 0 : element.offsetWidth, height: virtualized ? 0 : element.offsetHeight, x: point.x - base.x + (virtualized ? list.scrollLeft - (parent ? 0 : residual.x) : 0), y: point.y - base.y + (virtualized ? list.scrollTop - (parent ? 0 : residual.y) : 0) };
      const key = element.dataset.listMotion ?? element.dataset.flip!;
      const before = positions.get(key);
      next.set(key, position);
      return { element, position, before, residual, transform: style?.transform ?? "none" };
    });
    // A pure reorder at the same width (a drag let go) brings nothing in or out, so it has no travel to hold in.
    const reordered = next.size === positions.size && newOuterWidth === outerWidth && [...next.keys()].every((key) => positions.has(key));
    // Before any exit is placed, as holding the list's width can move its box.
    if (!moving) settle();
    else if (contentWidth && newOrder !== order && !reordered) hold(newOuterWidth);
    for (const key of exits.keys()) if (!moving || next.has(key)) clearExit(key);
    if (moving && virtualized) {
      for (const [key, before] of departing) {
        if (next.has(key)) continue;
        leave(key, before);
        departing.delete(key);
      }
    }
    if (!moving) { departing.clear(); added.clear(); }
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
        if ((!virtualized && !before) || (virtualized && added.has(element.dataset.listMotion ?? element.dataset.flip!))) {
          const animation = element.animate?.(presenceFrames(transform, true), { duration: DURATION, easing: PRESENCE_EASING });
          if (animation) {
            added.delete(element.dataset.listMotion ?? element.dataset.flip!);
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
      if (x === 0 && y === 0) continue;
      // `translate` composes with a virtualizer's transform rather than replacing its layout.
      const animation = element.animate?.([{ translate: `${x}px ${y}px` }, { translate: "0px 0px" }], { duration: DURATION, easing: EASING });
      if (animation) {
        slides.set(element, { animation, x, y });
        animation.onfinish = () => { if (slides.get(element)?.animation === animation) slides.delete(element); };
      }
    }
    initialized = initialized || rows.length > 0;
    positions = next;
    // A content-width list's own width may have just been held or let go.
    width = contentWidth ? list.clientWidth : newWidth;
    outerWidth = contentWidth ? list.offsetWidth : outerWidth;
    height = newHeight;
    scrollTop = list.scrollTop;
    scrollLeft = list.scrollLeft;
    for (const element of observed) if (!rows.includes(element)) { resize?.unobserve(element); observed.delete(element); }
    for (const element of rows) if (!observed.has(element)) { resize?.observe(element); observed.add(element); }
    order = newOrder;
    released = undefined;
  };
  const resize = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(() => measure(true));
  resize?.observe(list);
  // Cached collections can populate in a second commit during the same mount. Once that
  // commit settles, a first addition to a genuinely empty collection is a live update. Rows that
  // commit brought are taken as they stand: the observer's report of them can come after this.
  queueMicrotask(() => {
    if (stopped) return;
    if (!initialized) measure(false);
    initialized = true;
  });
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
    if (virtualized) { added.clear(); measure(false); } else for (const key of exits.keys()) clearExit(key);
  };
  list.addEventListener("scroll", onScroll, { passive: true, capture: true });
  const stopReducedMotion = onReducedMotion(() => measure(false));
  const watch: ListWatch = {
    list,
    prepare(nextKeys, nextFolds) {
      if (!virtualized || !logical) return;
      const previousFolds = logicalFolds;
      for (const key of nextKeys) {
        if (!logical.has(key) && !(previousFolds && nextFolds && folded(key, nextFolds, previousFolds, logical))) added.add(key);
        departing.delete(key);
      }
      for (const key of added) if (!nextKeys.has(key)) added.delete(key);
      if (!prefersReducedMotion()) {
        const base = list.getBoundingClientRect();
        for (const element of children()) {
          const key = element.dataset.listMotion ?? element.dataset.flip!;
          if (!logical.has(key) || nextKeys.has(key) || (previousFolds && nextFolds && folded(key, previousFolds, nextFolds, nextKeys))) continue;
          // Capture only real removals, before React can recycle their elements or wrappers.
          // The viewport rectangle already includes every virtual wrapper's transform.
          const rect = element.getBoundingClientRect();
          const copy = element.cloneNode(true) as HTMLElement;
          copy.style.transform = "none";
          departing.set(key, { element: copy, parent: list, width: rect.width, height: rect.height, x: rect.x - base.x + list.scrollLeft, y: rect.y - base.y + list.scrollTop });
        }
      }
      logical = nextKeys;
      logicalFolds = nextFolds;
      clearTimeout(additionTimer);
      additionTimer = setTimeout(() => added.clear(), 0);
    },
    release() {
      if (stopped || prefersReducedMotion()) return;
      released = new Map();
      for (const element of children()) {
        if (!element.style.transform) continue;
        const carried = transformOffset(element);
        if (carried.x !== 0 || carried.y !== 0) released.set(element, carried);
      }
      // Kept only for the change the caller is about to commit; a release with nothing following is forgotten.
      clearTimeout(releaseTimer);
      releaseTimer = setTimeout(() => { released = undefined; }, 0);
    },
    rendered() {
      const keys = JSON.stringify(children().map((element) => element.dataset.listMotion ?? element.dataset.flip));
      if (keys !== order || departing.size > 0) measure(true);
    },
    stop() {
      stopped = true;
      if (watching.get(list) === watch) watching.delete(list);
      clearTimeout(additionTimer);
      clearTimeout(releaseTimer);
      settle();
      departing.clear();
      resize?.disconnect();
      mutations?.disconnect();
      list.removeEventListener("scroll", onScroll, true);
      stopReducedMotion();
      for (const element of slides.keys()) cancel(element);
      for (const element of entrances.keys()) clearEntrance(element);
      for (const key of exits.keys()) clearExit(key);
    },
  };
  watching.set(list, watch);
  return watch;
}

export interface ListMotionRef<T extends HTMLElement> extends React.RefObject<T | null> {
  prepare: (keys: ReadonlySet<string>, folds?: ListFolds) => void;
}

interface ListFollower<T extends HTMLElement> {
  ref: ((list: T | null) => void) & ListMotionRef<T>;
  /** After each render of the owner: takes the latest options and records any change of the rows. */
  rendered: (options: ListMotionOptions) => void;
  stop: () => void;
}

/** The list can mount, unmount or be replaced in a commit that does not render the hook's owner: a
 * react-aria collection renders its parts from the collection it built, so a tab panel's list appears
 * when the selection changes, not when the component that holds it renders. The ref callback follows the
 * list itself and takes its starting layout as it attaches; an effect would notice only on the owner's
 * next render, which already carries that render's change, so the first removal or addition would go
 * unanimated. The ref also carries `current` for callers that read the list. */
function followList<T extends HTMLElement>(initial: ListMotionOptions): ListFollower<T> {
  let options = initial;
  let watch: ListWatch | null = null;
  const attach = () => {
    const list = ref.current;
    if (watch?.list === list) return;
    watch?.stop();
    watch = list ? watchList(list, options) : null;
  };
  const ref = Object.assign((list: T | null) => {
    ref.current = list;
    attach();
    watch?.rendered();
  }, { current: null as T | null, prepare: (keys: ReadonlySet<string>, folds?: ListFolds) => watch?.prepare(keys, folds) });
  return {
    ref,
    rendered(next) {
      options = next;
      // Restarts a watcher stopped by a change of options.
      attach();
      watch?.rendered();
    },
    stop() {
      watch?.stop();
      watch = null;
    },
  };
}

/** Keyed rows enter, leave and slide to their new positions in either axis. Removed controls
 * leave only inert visual copies; actions and focus handoffs take effect immediately. Virtual
 * collections distinguish logical changes from newly rendered or recycled identities when supplied keys. */
export function useListMotion<T extends HTMLElement>({ descendants = false, virtualized = false, keys, folds, contentWidth = false }: ListMotionOptions = {}): ListMotionRef<T> {
  const [motion] = useState(() => followList<T>({ descendants, virtualized, keys, folds, contentWidth }));
  const focused = useRef<HTMLElement | null>(null);
  useLayoutEffect(() => {
    const onFocus = (event: FocusEvent) => {
      focused.current = event.target instanceof HTMLElement && motion.ref.current?.contains(event.target) ? event.target : null;
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
      motion.stop();
    };
  }, [motion, virtualized]);
  useLayoutEffect(() => {
    motion.rendered({ descendants, virtualized, keys, folds, contentWidth });
    const list = motion.ref.current;
    const lost = focused.current;
    if (!virtualized && lost?.isConnected && list?.contains(lost) && document.activeElement === document.body) lost.focus({ preventScroll: true });
  });
  return motion.ref;
}
