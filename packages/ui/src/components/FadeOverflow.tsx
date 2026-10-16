import React, { useCallback, useLayoutEffect, useRef, useState } from "react";

import { AnchoredTooltip } from "./AnchoredTooltip";

interface Clipped {
  start: boolean;
  end: boolean;
  /** Whether the element lays out right to left, which flips the scroll offsets and the mask. */
  rtl: boolean;
  /** The box width in px. The fade ramps are capped against it, so a resize that keeps both edges clipped still has to be remeasured. */
  width: number;
}

/** Sub-pixel layout rounding leaves a fraction of a pixel of phantom overflow on content that
 * fits; anything within this many pixels counts as fitting. */
const OVERFLOW_EPSILON = 1;

/** How far the element is scrolled from its start edge. `scrollLeft` is 0 at the start in either
 * direction and grows toward the end, as a negative number under right-to-left. */
function scrolledFromStart(element: HTMLElement): number {
  return Math.abs(element.scrollLeft);
}

/** Puts the element `offset` px from its start edge, whichever direction it lays out in. */
function scrollToOffset(element: HTMLElement, offset: number): void {
  element.scrollLeft = getComputedStyle(element).direction === "rtl" ? -offset : offset;
}

/** How far the element can be scrolled: the width of the text beyond its box. */
function overflowExtent(element: HTMLElement): number {
  return element.scrollWidth - element.clientWidth;
}

/** Where the text sits when nothing is moving it, which is also what the layout would give it: a
 * label clipped at its end shows its beginning, one clipped at its start is held at its end. */
function restingOffset(element: HTMLElement, clip: "start" | "end"): number {
  return clip === "start" ? overflowExtent(element) : 0;
}

/** Whether a tooltip prop asks for a tooltip: `null` and `false` are what a conditional renders
 * when there is none. */
function hasTitle(title: React.ReactNode): boolean {
  return title != null && title !== false;
}

function measure(element: HTMLElement): Clipped {
  const scrolled = scrolledFromStart(element);
  return {
    start: scrolled > OVERFLOW_EPSILON,
    end: overflowExtent(element) - scrolled > OVERFLOW_EPSILON,
    rtl: getComputedStyle(element).direction === "rtl",
    width: element.clientWidth,
  };
}

// One observer and one animation-frame flush shared by every instance (the sidebar tree mounts one
// per row). Measuring is batched into a single frame so that reading layout for many rows never
// interleaves with writes.
const updaters = new Map<Element, () => void>();
const dirty = new Set<Element>();
let flushFrame: number | undefined;

function flush(): void {
  flushFrame = undefined;
  const pending = Array.from(dirty);
  dirty.clear();
  for (const element of pending) updaters.get(element)?.();
}

function scheduleMeasure(element: Element): void {
  dirty.add(element);
  flushFrame ??= requestAnimationFrame(flush);
}

let sharedObserver: ResizeObserver | undefined;

function observe(element: Element, update: () => void): () => void {
  sharedObserver ??= new ResizeObserver((entries) => {
    for (const entry of entries) scheduleMeasure(entry.target);
  });
  updaters.set(element, update);
  sharedObserver.observe(element);
  return () => {
    sharedObserver?.unobserve(element);
    updaters.delete(element);
    dirty.delete(element);
    // The frame is dropped with the last updater, so that one discarded behind our back (fake
    // timers being swapped out) cannot leave a dead id that blocks every later schedule.
    if (updaters.size === 0 && flushFrame !== undefined) {
      cancelAnimationFrame(flushFrame);
      flushFrame = undefined;
    }
  };
}

/** The mask that hides the clipped edges, along the text direction: opaque across the middle, ramping to transparent over
 * at most `fade` px at each edge that has content beyond it. `undefined` while nothing is clipped.
 *
 * The ramps shorten on a narrow box. Each one stays at most a third of the box when both edges are
 * clipped, and at most half when only one is, so an opaque run always remains. Full-length ramps
 * meet once the box is under twice the fade — a name at its floor, a branch squeezed beside it, and
 * either of them while a marquee fades both edges — and the text that still fits then paints as nothing. */
function maskImage(fade: number, { start, end, rtl, width }: Clipped): string | undefined {
  if (!start && !end) return undefined;
  const edges = (start ? 1 : 0) + (end ? 1 : 0);
  const fadePx = width > 0 ? Math.min(fade, Math.floor(width / (edges + 1))) : fade;
  if (fadePx <= 0) return undefined;
  const stops = [
    start ? `transparent 0, #000 ${fadePx}px` : "#000 0",
    end ? `#000 calc(100% - ${fadePx}px), transparent 100%` : "#000 100%",
  ];
  return `linear-gradient(to ${rtl ? "left" : "right"}, ${stops.join(", ")})`;
}

/** Marquee pacing: a pause where the label rests, a scroll at reading speed, a pause at the far
 * edge it runs to, a quicker scroll back, and again while the pointer stays. The two pauses are
 * named for the run rather than for the text, whose beginning is the far edge of a label clipped at
 * its start. */
const MARQUEE_SPEED = 60; // px per second
const MARQUEE_RETURN_SPEED = 160;
const MARQUEE_PAUSE_REST = 700; // ms
const MARQUEE_PAUSE_FAR = 1200;

/** The label itself, unless a single-label control explicitly owns its hover region. */
function marqueeScope(element: HTMLElement): HTMLElement {
  return element.closest<HTMLElement>("[data-marquee-scope]") ?? element;
}

/**
 * Scrolls a clipped label through its whole text while the pointer is over its scope, and back to
 * where it rests when the pointer leaves. It moves the element's own scroll position, so the edge
 * fades follow by themselves: an edge fades in as text passes it, and stops fading once the text
 * beyond it is in view. Which edge the label is clipped at decides only which way it runs: clipped
 * at its end it rests at its start and runs toward its end, clipped at its start it rests at its
 * end and runs back toward its beginning. Off where the system asks for reduced motion.
 *
 * `holds` is the protocol between this and the pin that otherwise holds a start-clipped label at
 * its end: it is raised for as long as the marquee owns the scroll position — through the run and
 * the return after it — and the pin stands aside while it is up, instead of dragging the text back
 * every frame. Whatever gives the position up lowers it again, and nothing moves the position while
 * it is down, so that the label is never left away from where it rests with no one to put it back.
 * A lowered flag therefore means nothing is running and no frame is pending, which is what lets the
 * pointer leaving a label the marquee never took return at once.
 *
 * The pointer being over the scope is remembered, not only the moment it entered. A row's hover
 * chrome grows in after that moment and the two labels trade width while it does, so a label can
 * start overflowing late, or fit for a frame and overflow again. Forgetting the hover at the first
 * frame that fits leaves one label running and the other still.
 */
function useMarquee(
  ref: React.RefObject<HTMLElement | null>,
  enabled: boolean,
  clip: "start" | "end",
  holds: React.RefObject<boolean>,
): void {
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element || !enabled) return;
    const scope = marqueeScope(element);
    let frame: number | undefined;
    let running = false;
    // The pointer is still over the scope. A label that fits on entry, or for one frame of the
    // hover chrome's width animation, has to be able to start again without a new entry.
    let over = false;
    // Kept here rather than read back from `scrollLeft`, which WebKit rounds to whole pixels: a
    // frame's step at reading speed is under a pixel, so reading it back would never advance.
    let position = scrolledFromStart(element);

    const setOffset = (offset: number) => {
      position = offset;
      scrollToOffset(element, offset);
    };
    // Moves toward `target` at `speed` px/s, then waits `pause` ms, then calls `next`.
    const move = (target: number, speed: number, pause: number, next?: () => void) => {
      let last: number | undefined;
      let waitedSince: number | undefined;
      const step = (now: number) => {
        const current = position;
        if (Math.abs(target - current) > 0.5) {
          const elapsed = last === undefined ? 0 : (now - last) / 1000;
          last = now;
          const delta = Math.min(Math.abs(target - current), speed * elapsed);
          setOffset(current + Math.sign(target - current) * delta);
          frame = requestAnimationFrame(step);
          return;
        }
        if (waitedSince === undefined) waitedSince = now;
        if (now - waitedSince < pause) {
          frame = requestAnimationFrame(step);
          return;
        }
        frame = undefined;
        next?.();
      };
      frame = requestAnimationFrame(step);
    };
    // Measured each time rather than once: the text and the room it has can both change under the
    // pointer. `far` is the edge the run reveals, the one the label is cut at.
    const resting = () => restingOffset(element, clip);
    const far = () => (clip === "start" ? 0 : overflowExtent(element));

    const giveBack = () => {
      running = false;
      if (frame !== undefined) cancelAnimationFrame(frame);
      // Only a position the marquee took is put back: where it never started — the text fits, or
      // the system asks for no motion — the pin has the label and `position` is stale.
      if (!holds.current) return;
      // The hold is given up only once the text is back where it rests, so the return is scrolled
      // rather than snapped away by the pin. The pointer may still be over the scope; the next
      // resize starts the run again if the text no longer fits.
      move(resting(), MARQUEE_RETURN_SPEED, 0, () => {
        holds.current = false;
      });
    };
    const cycle = () => {
      if (!running || !over) return;
      // Nothing left to run through. Give the position back, but stay armed: the room can
      // disappear for a frame while the row's hover chrome grows and come back on the next.
      if (overflowExtent(element) <= OVERFLOW_EPSILON) {
        giveBack();
        return;
      }
      move(far(), MARQUEE_SPEED, MARQUEE_PAUSE_FAR, () => move(resting(), MARQUEE_RETURN_SPEED, MARQUEE_PAUSE_REST, cycle));
    };
    const start = () => {
      if (!over || running || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
      if (overflowExtent(element) <= OVERFLOW_EPSILON) return;
      running = true;
      holds.current = true;
      if (frame !== undefined) cancelAnimationFrame(frame);
      position = scrolledFromStart(element);
      move(resting(), MARQUEE_RETURN_SPEED, MARQUEE_PAUSE_REST, cycle);
    };
    const enter = () => {
      over = true;
      start();
    };
    const leave = () => {
      over = false;
      giveBack();
    };
    // Fires for the hover chrome's width animation, which happens after `pointerenter` and is
    // what makes one of the row's labels start or stop overflowing.
    const onResize: ResizeObserverCallback = () => {
      if (!over) return;
      if (overflowExtent(element) <= OVERFLOW_EPSILON) giveBack();
      else start();
    };
    const resizeObserver = new ResizeObserver(onResize);
    resizeObserver.observe(element);

    scope.addEventListener("pointerenter", enter);
    scope.addEventListener("pointerleave", leave);
    return () => {
      over = false;
      running = false;
      holds.current = false;
      if (frame !== undefined) cancelAnimationFrame(frame);
      resizeObserver.disconnect();
      scope.removeEventListener("pointerenter", enter);
      scope.removeEventListener("pointerleave", leave);
    };
  }, [enabled, clip]);
}

interface FadeOverflowProps {
  /** Length of the fade at each clipped edge, in px. */
  fade?: number;
  /** The element to render. */
  as?: "div" | "span";
  className?: string;
  /** The element's text direction, for content whose direction is not the UI's: `ltr` for a path,
   * `auto` for a name the user typed. The fade follows the direction the element resolves to, while
   * text that fits stays aligned the way its surroundings are. */
  dir?: "ltr" | "rtl" | "auto";
  /** Language of the text when it differs from the surrounding UI. */
  lang?: string;
  /** Which edge the content is cut at. `end` (the default) shows the content's beginning; `start`
   * aligns it to the end, so what is lost is its beginning (a path whose last folder must stay
   * readable). */
  clip?: "start" | "end";
  /** The tooltip the text carries only while an edge is clipped, so a name that fits does not
   * grow a tooltip repeating itself. `null`, `false` and `undefined` mean none. */
  titleWhenClipped?: React.ReactNode;
  /** The tooltip the text carries whatever is clipped, for text that stands for something longer
   * than it shows even while it fits (an abbreviated path); takes the place of
   * `titleWhenClipped`. `null`, `false` and `undefined` mean none. */
  title?: React.ReactNode;
  /** Off to keep a clipped label still on hover (see `useMarquee`); on by default. */
  marquee?: boolean;
  children: React.ReactNode;
}

/**
 * A single line of text that fades out at an edge where it is cut off, instead of ending hard or in
 * an ellipsis. An edge fades only while text is actually hidden behind it, nothing when it all
 * fits. The fade is a CSS mask on this element alone, so a background on an ancestor (a hovered or
 * selected row) is not faded with it.
 *
 * This is hand-built rather than HeroUI's `ScrollShadow`: that turns its element into a vertical or
 * horizontal scroll container with a scrollbar, fades only an end that can be scrolled to, and has
 * no way to pin a line to its end (a clipped start edge) or to offer a tooltip only while clipped.
 *
 * While clipped, the text runs as a marquee while the pointer is over it. A single-label control
 * may use `data-marquee-scope` for its whole target; independent labels and actions must not share
 * that scope. See docs/ui-accessibility.md's "Hover marquees belong to one text label" section.
 *
 * It sets `overflow` and `whitespace-nowrap` itself, so the caller supplies only the sizing
 * (`min-w-0 flex-1` inside a flex row). It has to lay out as a block or a flex item: rendered
 * `as="span"` in running text it stays inline, never overflows, and so never clips, fades or grows
 * a tooltip. The first measurement is made before the first paint, so a label that does not fit
 * never shows a hard cut.
 */
export function FadeOverflow({
  fade = 24,
  as: Tag = "div",
  className = "",
  dir,
  lang,
  clip = "end",
  titleWhenClipped,
  title,
  marquee = true,
  children,
}: FadeOverflowProps): React.ReactElement {
  const ref = useRef<HTMLElement>(null);
  // The tooltip hangs off the box around the text rather than the text's own element: the marquee
  // moves that element's scroll position, and react-aria closes a tooltip as soon as anything
  // containing its anchor scrolls, so a label anchored to itself would lose its tooltip to its own
  // marquee. A scroll of the list or pane around it still closes it, which is the point.
  const anchor = useRef<Element | null>(null);
  const attach = useCallback((element: HTMLElement | null) => {
    ref.current = element;
    anchor.current = element?.parentElement ?? element;
  }, []);
  const marqueeHolds = useRef(false);
  const [clipped, setClipped] = useState<Clipped>({ start: false, end: false, rtl: false, width: 0 });
  const [hovered, setHovered] = useState(false);

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const update = () => {
      // Pinned to the end here rather than by layout: a clipped start edge is the scroll position.
      // Not while the marquee owns the position (see `useMarquee`), or the pin would undo every
      // step it takes.
      if (clip === "start" && !marqueeHolds.current) {
        const rest = restingOffset(element, clip);
        if (Math.abs(scrolledFromStart(element) - rest) > 0.5) scrollToOffset(element, rest);
      }
      const next = measure(element);
      setClipped((prev) =>
        prev.start === next.start && prev.end === next.end && prev.rtl === next.rtl && Math.abs(prev.width - next.width) < 0.5 ? prev : next,
      );
    };
    update();
    const stopObserving = observe(element, update);
    const remeasure = () => scheduleMeasure(element);
    element.addEventListener("scroll", remeasure, { passive: true });
    // What the label holds can change with the pointer over its scope (a tag's hover-only spacer)
    // without its box changing, which the observer never reports.
    const scope = marqueeScope(element);
    scope.addEventListener("pointerenter", remeasure);
    scope.addEventListener("pointerleave", remeasure);
    return () => {
      stopObserving();
      element.removeEventListener("scroll", remeasure);
      scope.removeEventListener("pointerenter", remeasure);
      scope.removeEventListener("pointerleave", remeasure);
    };
  }, [clip]);

  // Content can change without its container's box changing (a renamed row), which the observer
  // never reports, so every render re-measures.
  useLayoutEffect(() => {
    if (ref.current) scheduleMeasure(ref.current);
  });

  useMarquee(ref, marquee, clip, marqueeHolds);

  const mask = maskImage(fade, clipped);
  const tooltip = hasTitle(title) ? title : clipped.start || clipped.end ? titleWhenClipped : undefined;
  // The pointer is followed only where a tooltip was asked for, and from the prop rather than from
  // what is clipped right now, so that a label clipped while the pointer is already on it cannot
  // be left hovered for good.
  const follows = hasTitle(title) || hasTitle(titleWhenClipped);
  return (
    <Tag
      ref={attach}
      dir={dir}
      lang={lang}
      className={`overflow-hidden whitespace-nowrap ${dir ? "align-match-parent" : ""} ${className}`}
      style={mask ? { maskImage: mask, WebkitMaskImage: mask } : undefined}
      onPointerEnter={follows ? () => setHovered(true) : undefined}
      onPointerLeave={follows ? () => setHovered(false) : undefined}
    >
      {children}
      {tooltip !== undefined && <AnchoredTooltip tooltip={tooltip} anchor={anchor} hovered={hovered} />}
    </Tag>
  );
}
