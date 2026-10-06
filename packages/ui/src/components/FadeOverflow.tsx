import React, { useLayoutEffect, useRef, useState } from "react";

interface Clipped {
  start: boolean;
  end: boolean;
  /** Whether the element lays out right to left, which flips the scroll offsets and the mask. */
  rtl: boolean;
}

/** Sub-pixel layout rounding leaves a fraction of a pixel of phantom overflow on content that
 * fits; anything within this many pixels counts as fitting. */
const OVERFLOW_EPSILON = 1;

/** How far the element is scrolled from its start edge. `scrollLeft` is 0 at the start in either
 * direction and grows toward the end, as a negative number under right-to-left. */
function scrolledFromStart(element: HTMLElement): number {
  return Math.abs(element.scrollLeft);
}

function measure(element: HTMLElement): Clipped {
  const scrolled = scrolledFromStart(element);
  return {
    start: scrolled > OVERFLOW_EPSILON,
    end: element.scrollWidth - element.clientWidth - scrolled > OVERFLOW_EPSILON,
    rtl: getComputedStyle(element).direction === "rtl",
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
  };
}

/** The mask that hides the clipped edges, along the text direction: opaque across the middle, ramping to transparent over
 * `fade` px at each edge that has content beyond it. `undefined` while nothing is clipped. */
function maskImage(fade: number, { start, end, rtl }: Clipped): string | undefined {
  if (!start && !end) return undefined;
  const stops = [
    start ? `transparent 0, #000 ${fade}px` : "#000 0",
    end ? `#000 calc(100% - ${fade}px), transparent 100%` : "#000 100%",
  ];
  return `linear-gradient(to ${rtl ? "left" : "right"}, ${stops.join(", ")})`;
}

/** Marquee pacing: a pause at the start, a scroll at reading speed, a pause at the end, a quicker
 * scroll back, and again while the pointer stays. */
const MARQUEE_SPEED = 60; // px per second
const MARQUEE_RETURN_SPEED = 160;
const MARQUEE_PAUSE_START = 700; // ms
const MARQUEE_PAUSE_END = 1200;

/** The element the pointer has to be over for a label's marquee to run: the nearest ancestor marked
 * `data-marquee-scope` (a whole row), or the label itself. */
function marqueeScope(element: HTMLElement): HTMLElement {
  return element.closest<HTMLElement>("[data-marquee-scope]") ?? element;
}

/**
 * Scrolls a clipped label through its whole text while the pointer is over its scope, and back to
 * its start when the pointer leaves. It moves the element's own scroll position, so the edge fades
 * follow by themselves: the start fades in as text passes it, the end fades out once the last of it
 * is in view. Off where the system asks for reduced motion, and for a label clipped at its start.
 */
function useMarquee(ref: React.RefObject<HTMLElement | null>, enabled: boolean): void {
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element || !enabled) return;
    const scope = marqueeScope(element);
    let frame: number | undefined;
    let running = false;
    // Kept here rather than read back from `scrollLeft`, which WebKit rounds to whole pixels: a
    // frame's step at reading speed is under a pixel, so reading it back would never advance.
    let position = scrolledFromStart(element);

    const setOffset = (offset: number) => {
      position = offset;
      element.scrollLeft = getComputedStyle(element).direction === "rtl" ? -offset : offset;
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
        waitedSince ??= now;
        if (now - waitedSince < pause) {
          frame = requestAnimationFrame(step);
          return;
        }
        frame = undefined;
        next?.();
      };
      frame = requestAnimationFrame(step);
    };
    const cycle = () => {
      if (!running) return;
      const end = element.scrollWidth - element.clientWidth;
      if (end <= OVERFLOW_EPSILON) {
        running = false;
        return;
      }
      move(end, MARQUEE_SPEED, MARQUEE_PAUSE_END, () => move(0, MARQUEE_RETURN_SPEED, MARQUEE_PAUSE_START, cycle));
    };
    const start = () => {
      if (running || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
      if (element.scrollWidth - element.clientWidth <= OVERFLOW_EPSILON) return;
      running = true;
      if (frame !== undefined) cancelAnimationFrame(frame);
      position = scrolledFromStart(element);
      move(0, MARQUEE_RETURN_SPEED, MARQUEE_PAUSE_START, cycle);
    };
    const stop = () => {
      running = false;
      if (frame !== undefined) cancelAnimationFrame(frame);
      move(0, MARQUEE_RETURN_SPEED, 0);
    };

    scope.addEventListener("pointerenter", start);
    scope.addEventListener("pointerleave", stop);
    return () => {
      running = false;
      if (frame !== undefined) cancelAnimationFrame(frame);
      scope.removeEventListener("pointerenter", start);
      scope.removeEventListener("pointerleave", stop);
    };
  }, [enabled]);
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
  /** Which edge the content is cut at. `end` (the default) shows the content's beginning; `start`
   * aligns it to the end, so what is lost is its beginning (a path whose last folder must stay
   * readable). */
  clip?: "start" | "end";
  /** Set as the element's `title` only while an edge is clipped, so a name that fits does not
   * grow a tooltip repeating itself. */
  titleWhenClipped?: string;
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
 * no way to pin a line to its end (a clipped start edge) or to offer a title only while clipped.
 *
 * While clipped at its end, the text runs as a marquee while the pointer is over it, or over the
 * nearest ancestor marked `data-marquee-scope` (a row), so all of it can be read without a tooltip.
 *
 * It sets `overflow` and `whitespace-nowrap` itself, so the caller supplies only the sizing
 * (`min-w-0 flex-1` inside a flex row). The first measurement is made before the first paint, so a
 * label that does not fit never shows a hard cut.
 */
export function FadeOverflow({
  fade = 24,
  as: Tag = "div",
  className = "",
  dir,
  clip = "end",
  titleWhenClipped,
  marquee = true,
  children,
}: FadeOverflowProps): React.ReactElement {
  const ref = useRef<HTMLElement>(null);
  const [clipped, setClipped] = useState<Clipped>({ start: false, end: false, rtl: false });

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const update = () => {
      // Pinned to the end here rather than by layout: a clipped start edge is the scroll position.
      if (clip === "start") {
        const end = element.scrollWidth - element.clientWidth;
        if (Math.abs(scrolledFromStart(element) - end) > 0.5) {
          element.scrollLeft = getComputedStyle(element).direction === "rtl" ? -end : end;
        }
      }
      const next = measure(element);
      setClipped((prev) => (prev.start === next.start && prev.end === next.end && prev.rtl === next.rtl ? prev : next));
    };
    update();
    const stopObserving = observe(element, update);
    const onScroll = () => scheduleMeasure(element);
    element.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      stopObserving();
      element.removeEventListener("scroll", onScroll);
    };
  }, [clip]);

  // Content can change without its container's box changing (a renamed row), which the observer
  // never reports, so every render re-measures.
  useLayoutEffect(() => {
    if (ref.current) scheduleMeasure(ref.current);
  });

  useMarquee(ref, marquee && clip === "end");

  const mask = maskImage(fade, clipped);
  return (
    <Tag
      ref={ref as React.RefObject<never>}
      dir={dir}
      className={`overflow-hidden whitespace-nowrap${dir ? " align-match-parent" : ""} ${className}`}
      title={clipped.start || clipped.end ? titleWhenClipped : undefined}
      style={mask ? { maskImage: mask, WebkitMaskImage: mask } : undefined}
    >
      {children}
    </Tag>
  );
}
