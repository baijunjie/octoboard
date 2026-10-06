import React, { useLayoutEffect, useRef, useState } from "react";

type Axis = "x" | "y";

interface Clipped {
  start: boolean;
  end: boolean;
}

/** Sub-pixel layout rounding leaves a fraction of a pixel of phantom overflow on content that
 * fits; anything within this many pixels counts as fitting. */
const OVERFLOW_EPSILON = 1;

function measure(element: HTMLElement, axis: Axis): Clipped {
  const [size, client, offset] =
    axis === "x"
      ? [element.scrollWidth, element.clientWidth, element.scrollLeft]
      : [element.scrollHeight, element.clientHeight, element.scrollTop];
  return {
    start: offset > OVERFLOW_EPSILON,
    end: size - client - offset > OVERFLOW_EPSILON,
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

/** The mask that hides the clipped edges: opaque across the middle, ramping to transparent over
 * `fade` px at each edge that has content beyond it. `undefined` while nothing is clipped. */
function maskImage(axis: Axis, fade: number, { start, end }: Clipped): string | undefined {
  if (!start && !end) return undefined;
  const stops = [
    start ? `transparent 0, #000 ${fade}px` : "#000 0",
    end ? `#000 calc(100% - ${fade}px), transparent 100%` : "#000 100%",
  ];
  return `linear-gradient(${axis === "x" ? "to right" : "to bottom"}, ${stops.join(", ")})`;
}

interface FadeOverflowProps {
  /** The axis along which content may be clipped. The element is made to clip (never wrap) along
   * it: `x` is a single line cut at the right edge, `y` a vertical scroll container. */
  axis: Axis;
  /** Length of the fade at each clipped edge, in px. */
  fade?: number;
  /** The element to render. */
  as?: "div" | "span";
  className?: string;
  /** For `x` only: which edge the content is cut at. `end` (the default) shows the content's
   * beginning; `start` aligns it to the end, so what is lost is its beginning (a path whose last
   * folder must stay readable). */
  clip?: "start" | "end";
  /** Set as the element's `title` only while an edge is clipped, so a name that fits does not
   * grow a tooltip repeating itself. */
  titleWhenClipped?: string;
  children: React.ReactNode;
}

/**
 * Content that fades out at an edge where it is cut off, instead of ending hard or in an ellipsis.
 * An edge fades only while content is actually hidden behind it: nothing when everything fits, and
 * for a scroll container the start edge only once scrolled away from the start, the end edge only
 * while there is more to scroll. The fade is a CSS mask on this element alone, so a background on
 * an ancestor (a hovered or selected row) is not faded with it.
 *
 * It sets `overflow` and `whitespace-nowrap` (for `x`) itself, so the caller supplies only the
 * sizing (`min-w-0 flex-1` inside a flex row, a height for a scroll area). The first measurement
 * is made before the first paint, so a label that does not fit never shows a hard cut.
 */
export function FadeOverflow({
  axis,
  fade = 24,
  as: Tag = "div",
  className = "",
  clip = "end",
  titleWhenClipped,
  children,
}: FadeOverflowProps): React.ReactElement {
  const ref = useRef<HTMLElement>(null);
  const [clipped, setClipped] = useState<Clipped>({ start: false, end: false });

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const update = () => {
      // Pinned to the end here rather than by layout: a clipped start edge is the scroll position.
      if (axis === "x" && clip === "start") {
        const end = element.scrollWidth - element.clientWidth;
        if (Math.abs(element.scrollLeft - end) > 0.5) element.scrollLeft = end;
      }
      const next = measure(element, axis);
      setClipped((prev) => (prev.start === next.start && prev.end === next.end ? prev : next));
    };
    update();
    const stopObserving = observe(element, update);
    const onScroll = () => scheduleMeasure(element);
    element.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      stopObserving();
      element.removeEventListener("scroll", onScroll);
    };
  }, [axis, clip]);

  // Content can change without its container's box changing (a renamed row, a row added to a
  // scroll area), which the observer never reports, so every render re-measures.
  useLayoutEffect(() => {
    if (ref.current) scheduleMeasure(ref.current);
  });

  const mask = maskImage(axis, fade, clipped);
  const overflow = axis === "x" ? "overflow-hidden whitespace-nowrap" : "overflow-y-auto";
  return (
    <Tag
      ref={ref as React.RefObject<never>}
      className={`${overflow} ${className}`}
      title={clipped.start || clipped.end ? titleWhenClipped : undefined}
      style={mask ? { maskImage: mask, WebkitMaskImage: mask } : undefined}
    >
      {children}
    </Tag>
  );
}
