import React, { useContext, useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";
import { flushSync } from "react-dom";

import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../../components/announce/StatusAnnouncer";
import { useLatest } from "../../components/hooks/useLatest";
import { isPrimaryModifier } from "../../input/keys";
import { isolate } from "../../i18n/isolate";
import { useCurrentLanguage, useT } from "../../i18n/react";
import { useHost } from "../../platform/react";
import { setKeyboardModality, useFocusVisibleProps } from "../../primitives";
import { FrameSurface } from "../FrameSurface";
import { anchoredScroll, FIT, formatPercent, stepScale, wheelFactor, zoomBounds, zoomKey, type ImageZoom, type Size, type ZoomBounds } from "./zoom";
import { ZoomBar } from "./ZoomBar";
import { ZoomGroup, ZoomGroupContext } from "./zoomGroup";

/** The padding around the image inside its scroller (Tailwind's `p-2`), which the fitted scale leaves free. */
const PADDING = 8;

/** A zoom asked for whose scroll position is still to be set, once the image is drawn at its new scale. */
interface PendingAnchor {
  anchor: { x: number; y: number };
  previous: number;
  scroll: { left: number; top: number };
  imageOffset: { left: number; top: number };
  /** How far the content is to move besides, for a pinch whose fingers also moved. */
  shift: { x: number; y: number };
}

/** Safari's trackpad pinch, which it sends as gesture events rather than as wheel events with Ctrl. */
type GestureEvent = UIEvent & { scale: number; clientX: number; clientY: number };

/**
 * An image the reader can zoom, as in a system image viewer. It opens fitted to the view, never scaled up past its
 * actual size, and zooms with the bar of buttons floating over its top trailing corner, with ⌘+ / ⌘- / ⌘0 (Ctrl
 * elsewhere) anywhere in the viewer, with a trackpad pinch or a wheel turned with ⌘ or Ctrl held, around the pointer,
 * with two fingers on a touch screen, and with a double-click, which goes between fitted and actual size. An image
 * larger than the view pans by scrolling and by dragging. Inside a `ZoomGroupContext` (the two sides of an image
 * change) it zooms and scrolls with the group's other images, and only the group's leader shows the bar and takes the
 * keys (`ZoomGroup`); outside one it is a group of its own.
 *
 * The scroller is always laid out left to right: the image has no direction, and the zoom's arithmetic reads
 * `scrollLeft` as left-to-right layouts report it. Its own bar takes the trailing corner of the window's direction.
 * `onError` is the image's decode failure, which the caller shows in its place.
 */
export function ZoomableImage({ url, name, onError }: { url: string; name: string; onError: () => void }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const host = useHost();
  const scrollerRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  const [natural, setNatural] = useState<Size | null>(null);
  const [box, setBox] = useState<Size>({ width: 0, height: 0 });
  const [announced, setAnnounced] = useState<string | undefined>(undefined);
  const pending = useRef<PendingAnchor | null>(null);
  const announceTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(announceTimer.current), []);
  /** The pointers down on the image, where each last was. */
  const pointers = useRef(new Map<number, { x: number; y: number }>());

  const joined = useContext(ZoomGroupContext);
  const [ownGroup] = useState(() => new ZoomGroup());
  const group = joined?.group ?? ownGroup;
  const priority = joined?.priority ?? 0;
  const focusVisible = useFocusVisibleProps();
  const [id] = useState(() => Symbol("zoomable image"));
  // Everything the render reads of the group comes from its view, a new object on every change.
  const view = useSyncExternalStore(group.subscribe, group.getSnapshot);

  const ownBounds: ZoomBounds | null = natural ? zoomBounds(natural, box) : null;
  const scale = view.scales.get(id) ?? ownBounds?.fit ?? 1;
  const leads = ownBounds !== null && view.lead === id;
  // The group's bounds and scale, in the leader's terms; the leader alone shows the bar and takes the keys.
  const bounds = leads ? view.bounds : null;
  const leadScale = view.leadScale ?? scale;
  const zoom = view.zoom;
  const overflows = natural !== null && (natural.width * scale > box.width + 0.5 || natural.height * scale > box.height + 0.5);

  // The view's size, less the padding, for the fitted scale. The scroller's own box, scrollbars included, so a zoomed
  // image's scrollbars do not change what fitting means.
  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    if (!scroller) return;
    const measure = () => setBox({ width: Math.max(0, scroller.offsetWidth - 2 * PADDING), height: Math.max(0, scroller.offsetHeight - 2 * PADDING) });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(scroller);
    return () => observer.disconnect();
  }, []);

  // Joins the group once the image's size is known, and keeps its bounds there current.
  const fit = ownBounds?.fit;
  const max = ownBounds?.max;
  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    if (fit === undefined || max === undefined || !scroller) return;
    group.update(id, { priority, bounds: { fit, max }, scroller });
  }, [group, id, priority, fit, max]);
  useLayoutEffect(() => () => group.update(id, null), [group, id]);

  // Once drawn at the new scale, scroll so the point that was under the pointer (or the view's centre) still is, and
  // bring the rest of the group to the same place.
  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    const image = imageRef.current;
    const anchor = pending.current;
    pending.current = null;
    if (!scroller || !image || !anchor) return;
    const next = anchoredScroll({
      scroll: anchor.scroll,
      anchor: anchor.anchor,
      previous: anchor.previous,
      next: scale,
      imageOffsetBefore: anchor.imageOffset,
      imageOffsetAfter: { left: image.offsetLeft, top: image.offsetTop },
    });
    scroller.scrollLeft = next.left - anchor.shift.x;
    scroller.scrollTop = next.top - anchor.shift.y;
    group.syncScroll(id);
  }, [scale]);

  const latest = useLatest({ scale, leads, overflows });

  /** Sets the group's zoom to `target` (in the leader's scale), keeping the image point under `anchor` (client
   * coordinates; the view's centre when left out) where it is. Rendered at once, so the next wheel or pointer event
   * reads the layout it left. */
  const zoomBy = (target: ImageZoom, options: { anchor?: { clientX: number; clientY: number }; shift?: { x: number; y: number }; announce?: boolean; fromKey?: boolean } = {}) => {
    const scroller = scrollerRef.current;
    const image = imageRef.current;
    if (!scroller || !image || !group.bounds()) return;
    const rect = scroller.getBoundingClientRect();
    const anchor = options.anchor
      ? { x: options.anchor.clientX - rect.left - scroller.clientLeft, y: options.anchor.clientY - rect.top - scroller.clientTop }
      : { x: scroller.clientWidth / 2, y: scroller.clientHeight / 2 };
    const shift = options.shift ?? { x: 0, y: 0 };
    pending.current = {
      anchor,
      previous: latest.current.scale,
      scroll: { left: scroller.scrollLeft, top: scroller.scrollTop },
      imageOffset: { left: image.offsetLeft, top: image.offsetTop },
      shift,
    };
    const before = group.getSnapshot();
    const focused = document.activeElement instanceof HTMLElement && document.activeElement !== document.body ? document.activeElement : null;
    flushSync(() => group.setZoom(target));
    // The scale did not change (held at a bound, or only fitting again at the same scale): the fingers' movement
    // still pans.
    if (pending.current) {
      pending.current = null;
      scroller.scrollLeft -= shift.x;
      scroller.scrollTop -= shift.y;
      group.syncScroll(id);
    }
    if (group.getSnapshot() === before) return;
    // Here rather than at each button: keys and a double-click can disable a control or end a tab stop too.
    const handedOn = focused !== null && handFocusOn(focused, scroller, options.fromKey === true);
    const now = group.leadScale();
    clearTimeout(announceTimer.current);
    // A zoom that is not announced empties the region, so a later one that lands on the same percentage is spoken.
    // One that moved focus waits until the focus move has been announced, which would otherwise replace it.
    const text = options.announce && now !== undefined ? t("viewer.image.zoomLevel", { percent: isolate(formatPercent(language, now)) }) : undefined;
    if (handedOn && text !== undefined) {
      setAnnounced(undefined);
      announceTimer.current = setTimeout(() => setAnnounced(text), AFTER_FOCUS_MOVE_DELAY_MS);
    } else setAnnounced(text);
  };

  const step = (direction: 1 | -1, fromKey = false) => {
    const groupBounds = group.bounds();
    const current = group.leadScale();
    if (groupBounds && current !== undefined) zoomBy(stepScale(current, direction, groupBounds), { announce: true, fromKey });
  };
  /** Fitted when not, actual size when fitted: the bar's last button and a double-click. */
  const toggleFit = (anchor?: { clientX: number; clientY: number }) => {
    const groupBounds = group.bounds();
    if (!groupBounds) return;
    if (group.current().mode !== "fit") zoomBy(FIT, { anchor, announce: true });
    else if (groupBounds.fit < 1) zoomBy({ mode: "scale", scale: 1 }, { anchor, announce: true });
  };
  /** Zooms the group by `factor`, as a wheel, a pinch or two fingers do, wherever they are. */
  const zoomByFactor = (factor: number, options: { anchor: { clientX: number; clientY: number }; shift?: { x: number; y: number } }) => {
    const current = group.leadScale();
    if (current !== undefined) zoomBy({ mode: "scale", scale: current * factor }, options);
  };
  // For the listeners added once.
  const actions = useLatest({ zoomBy, step, zoomByFactor });

  // Wheel, pinch and keys, as native listeners: React's wheel listener is passive, so it could not keep the page from
  // zooming instead, and the keys are the viewer's wherever focus is in it.
  useEffect(() => {
    const scroller = scrollerRef.current;
    if (!scroller) return;
    const onWheel = (event: WheelEvent) => {
      // A pinch arrives with Ctrl held; ⌘ is macOS's own modifier for zooming with a wheel. Without either, the wheel
      // scrolls, as in any other view of the viewer.
      if (!event.ctrlKey && !event.metaKey) return;
      event.preventDefault();
      actions.current.zoomByFactor(wheelFactor(event), { anchor: event });
    };
    let gestureScale = 1;
    const onGestureStart = (event: Event) => {
      event.preventDefault();
      gestureScale = 1;
    };
    const onGestureChange = (event: Event) => {
      event.preventDefault();
      // On a touch screen the pointer events below already zoom.
      if (pointers.current.size > 0) return;
      const gesture = event as GestureEvent;
      if (gesture.scale <= 0) return;
      // The gesture's scale runs from its start; each event zooms by what it adds to the last.
      actions.current.zoomByFactor(gesture.scale / gestureScale, { anchor: gesture });
      gestureScale = gesture.scale;
    };
    scroller.addEventListener("wheel", onWheel, { passive: false });
    scroller.addEventListener("gesturestart", onGestureStart);
    scroller.addEventListener("gesturechange", onGestureChange);
    const dialog = scroller.closest<HTMLElement>("[role=dialog]");
    const onKeyDown = (event: KeyboardEvent) => {
      // The group zooms as one, so its leader alone takes the key.
      if (!latest.current.leads || !isPrimaryModifier(event, host)) return;
      const command = zoomKey(event);
      if (!command) return;
      event.preventDefault();
      event.stopPropagation();
      if (command === "fit") actions.current.zoomBy(FIT, { announce: true, fromKey: true });
      else actions.current.step(command === "in" ? 1 : -1, true);
    };
    dialog?.addEventListener("keydown", onKeyDown);
    return () => {
      scroller.removeEventListener("wheel", onWheel);
      scroller.removeEventListener("gesturestart", onGestureStart);
      scroller.removeEventListener("gesturechange", onGestureChange);
      dialog?.removeEventListener("keydown", onKeyDown);
    };
  }, [host]);

  // Dragging pans an image larger than the view; two fingers on a touch screen pinch it.
  const drag = useRef<{ x: number; y: number; left: number; top: number } | null>(null);
  const startDrag = (scroller: HTMLElement) => {
    const [first] = pointers.current.values();
    drag.current = first ? { x: first.x, y: first.y, left: scroller.scrollLeft, top: scroller.scrollTop } : null;
  };
  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || !natural) return;
    const scroller = event.currentTarget;
    // A press on the scrollbar is the scrollbar's.
    const rect = scroller.getBoundingClientRect();
    if (event.clientX - rect.left - scroller.clientLeft > scroller.clientWidth || event.clientY - rect.top - scroller.clientTop > scroller.clientHeight) return;
    pointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    scroller.setPointerCapture?.(event.pointerId);
    startDrag(scroller);
    if (latest.current.overflows || pointers.current.size > 1) scroller.dataset.dragging = "";
  };
  const onPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const previous = pointers.current.get(event.pointerId);
    if (!previous) return;
    const scroller = event.currentTarget;
    if (pointers.current.size === 1) {
      pointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
      const start = drag.current;
      if (!start || !latest.current.overflows) return;
      scroller.scrollLeft = start.left - (event.clientX - start.x);
      scroller.scrollTop = start.top - (event.clientY - start.y);
      return;
    }
    const [a, b] = [...pointers.current.values()];
    if (!a || !b) return;
    const before = { distance: Math.hypot(a.x - b.x, a.y - b.y), x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    pointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    const [c, d] = [...pointers.current.values()];
    if (!c || !d) return;
    const after = { distance: Math.hypot(c.x - d.x, c.y - d.y), x: (c.x + d.x) / 2, y: (c.y + d.y) / 2 };
    if (before.distance === 0) return;
    zoomByFactor(after.distance / before.distance, {
      anchor: { clientX: before.x, clientY: before.y },
      shift: { x: after.x - before.x, y: after.y - before.y },
    });
  };
  const onPointerEnd = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!pointers.current.delete(event.pointerId)) return;
    const scroller = event.currentTarget;
    startDrag(scroller);
    if (pointers.current.size === 0) delete scroller.dataset.dragging;
  };

  const sized = natural !== null;
  return (
    // The ring is the frame's, drawn over the image: the scroller's own outline would be clipped by the dialog.
    <FrameSurface focused={overflows && focusVisible["data-focus-visible"] === true}>
      <div
        ref={scrollerRef}
        dir="ltr"
        // A tab stop only while there is something to scroll to, named by the image it shows (the side of a change
        // around it is named by its own heading).
        role={overflows ? "region" : undefined}
        aria-label={overflows ? name : undefined}
        tabIndex={overflows ? 0 : undefined}
        {...focusVisible}
        className={`viewer-checkerboard viewer-image-scroller relative flex min-h-0 flex-1 touch-none overflow-auto p-2 outline-none select-none ${overflows ? "cursor-grab data-dragging:cursor-grabbing" : ""} ${sized ? "" : "items-center justify-center"}`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerEnd}
        onPointerCancel={onPointerEnd}
        onScroll={() => group.scrolled(id)}
        onDoubleClick={(event) => toggleFit(event)}
      >
        {/* The `error` event is the signal: an image's size is no test (an SVG with only a `viewBox`
            has none of its own) and `decode()` has wrongly rejected valid SVGs in older WebKit. Auto margins centre
            it while it is smaller than the view and, unlike centring by alignment, leave all of it reachable once it
            is larger. */}
        <img
          ref={imageRef}
          src={url}
          alt={name}
          draggable={false}
          onError={onError}
          onLoad={(event) => {
            const { naturalWidth, naturalHeight } = event.currentTarget;
            // An image with no size of its own (an SVG with only a `viewBox`) stays fitted by the browser, unzoomed.
            if (naturalWidth > 0 && naturalHeight > 0) setNatural({ width: naturalWidth, height: naturalHeight });
          }}
          style={natural ? { width: natural.width * scale, height: natural.height * scale } : undefined}
          className={
            natural
              ? // Past actual size each image pixel is drawn as a square, so a screenshot's pixels can be told apart
            // rather than blurred together; below it the image is smoothed as usual.
              `m-auto max-w-none shrink-0 ${scale > 1 ? "[image-rendering:pixelated]" : ""}`
              : "max-h-full max-w-full object-contain"
          }
        />
      </div>
      {bounds && <ZoomBar bounds={bounds} scale={leadScale} fitted={zoom.mode === "fit"} onStep={step} onToggleFit={() => toggleFit()} />}
      <StatusAnnouncer text={announced} clearAfterRead />
    </FrameSurface>
  );
}

/**
 * Hands keyboard focus on when a zoom took away the element holding it, which would otherwise fall to the page and
 * take the viewer's keys with it: a bar button the zoom disabled (Zoom in at the largest scale, Zoom out or the fit
 * button back at the fitted scale), or an image's scroller, a tab stop only while its image overflows. Focus goes to
 * an enabled step of the bar, which can always move from there. Returns whether it moved focus.
 */
function handFocusOn(focused: HTMLElement, scroller: HTMLElement, fromKey: boolean): boolean {
  // Any image's scroller, not only this one's: a key the leader takes also fits the side that holds focus.
  const lost = (focused instanceof HTMLButtonElement && focused.disabled) || focused.matches(".viewer-image-scroller:not([tabindex])");
  if (!lost) return false;
  const bar = (scroller.closest("[role=dialog]") ?? document).querySelector<HTMLElement>("[data-zoom-bar]");
  const target = bar?.querySelector<HTMLButtonElement>("button:not([disabled])");
  if (!target) return false;
  // react-aria does not count a ⌘ chord as keyboard input, so a key that hands focus on says so for the ring.
  if (fromKey) setKeyboardModality();
  target.focus();
  return true;
}
