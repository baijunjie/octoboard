import { ScrollShadow, scrollShadowVariants, useScrollShadow, type ScrollShadowVisibility } from "@heroui/react";
import type React from "react";
import { useEffect, useState, type CSSProperties, type Ref, type RefObject } from "react";
import { useObjectRef } from "react-aria";

import { useFocusVisibleProps } from "./useFocusVisibleProps";

/** The fade's size in pixels. A row scrolled to the list's edge has to stop this far short of it to
 * be clear of the fade, so whatever scrolls the list by hand keeps this margin too. */
export const FADE_SIZE = 24;

type Orientation = "vertical" | "horizontal";

/** The scroll padding that keeps whatever the browser or react-aria scrolls into view (a control
 * that takes focus) clear of the fade rather than under it; it goes on the axis that fades. */
function scrollPadding(orientation: Orientation): CSSProperties {
  return orientation === "horizontal" ? { scrollPaddingInline: FADE_SIZE } : { scrollPaddingBlock: FADE_SIZE };
}

/**
 * Whether the scroller can also scroll sideways, for the fade to leave the horizontal scrollbar
 * undimmed (see `[data-scrolls-sideways]` in `style.css`). Only a vertical fade needs it: the
 * library's horizontal one already reserves the strip along the bottom itself.
 *
 * It runs on every commit of the caller rather than once, with no dependencies, because what makes
 * a surface start or stop scrolling sideways often leaves nothing an observer would see: the same
 * `pre` takes a new file as one text node, and the word-wrap choice changes only a class. Re-running
 * is also what re-attaches the observer to a scroller React replaced in place, which the viewer's
 * document does on a new subject. A `ResizeObserver` covers the rest — a lazy render finishing, the
 * window resizing — and reads back the same answer when nothing moved, which React drops.
 */
function useScrollsSideways(ref: RefObject<HTMLElement | null>, orientation: Orientation): { "data-scrolls-sideways"?: true } {
  const [sideways, setSideways] = useState(false);
  useEffect(() => {
    const element = orientation === "vertical" ? ref.current : null;
    if (!element) {
      setSideways(false);
      return;
    }
    const measure = () => setSideways(element.scrollWidth > element.clientWidth);
    const resize = new ResizeObserver(measure);
    resize.observe(element);
    for (const child of element.children) resize.observe(child);
    measure();
    return () => resize.disconnect();
  });
  return sideways ? { "data-scrolls-sideways": true } : {};
}

function joinClasses(...names: (string | undefined)[]): string {
  return names.filter(Boolean).join(" ");
}

type RegionProps = React.DOMAttributes<Element> & {
  tabIndex?: 0;
  role?: "region";
  "aria-label"?: string;
  "aria-labelledby"?: string;
  "data-focus-visible"?: true;
};

export interface FadeRegion {
  /** The scroller has keyboard focus that should show a ring; the caller draws it on the element
   * around the scroller. */
  focused: boolean;
  props: RegionProps;
  className: string;
  onVisibilityChange?: (visibility: ScrollShadowVisibility) => void;
}

/**
 * Makes a faded scroller a focusable `region` the keyboard can scroll, for `FadedScroller` or
 * `useScrollFade` to take as their `region`. A scroller nothing else on the screen would scroll
 * needs it, or the fade at its edge says there is more where the keyboard cannot go.
 *
 * The caller draws the focus ring, on the element around the scroller and from `focused`: the fade
 * is a mask that clips by the scroller's border box, so a ring on the scroller itself is cut off.
 * Which element that is differs at every site, and a scroller that spans the window needs the ring
 * inset, so it stays the caller's to place.
 *
 * By default the region exists only while the scroller overflows, since a tab stop that scrolls
 * nothing is one the user passes for no reason. `always` is for a region that is for more than
 * scrolling (the viewer's reading surfaces, where focus also carries Select All and copy).
 *
 * The name is the caller's `label`, copy from the catalog, or `labelledBy`, the id of an element
 * that already names it.
 */
export function useFadeRegion(
  { always = false, ...name }: ({ label: string } | { labelledBy: string | undefined }) & { always?: boolean },
): FadeRegion {
  const [overflows, setOverflows] = useState(false);
  const focus = useFocusVisibleProps();
  const active = always || overflows;
  return {
    focused: active && Boolean(focus["data-focus-visible"]),
    props: active
      ? {
        ...focus,
        tabIndex: 0,
        role: "region",
        ...("label" in name ? { "aria-label": name.label } : { "aria-labelledby": name.labelledBy }),
      }
      : {},
    className: active ? "outline-none" : "",
    // A region that is always there has no overflow to watch, so it asks the scroller for nothing.
    onVisibilityChange: always ? undefined : (visibility) => setOverflows(visibility !== "none"),
  };
}

type FadedScrollerProps = Omit<React.ComponentProps<typeof ScrollShadow>, "size" | "offset" | "visibility" | "isEnabled" | "variant" | "onVisibilityChange"> & {
  region?: FadeRegion;
  /** Sticky headings paint above overlay fades; a mask would dim every descendant regardless of z-index. */
  stickyHeadingHeight?: number;
};

/**
 * A scroller with the project's fade at the edges it hides content behind: HeroUI's `ScrollShadow`
 * with the size and the matching scroll padding supplied, so no caller states either. Use it for
 * every scroller that may be a wrapper around its content; `useScrollFade` is for one that has to
 * be the element itself.
 *
 * It sets no overflow of its own beyond the vertical (or horizontal) one the fade's classes carry,
 * so a surface that scrolls the other axis too asks for it in `className`. What the fade and a
 * `region` set is applied after the caller's own props, so neither can be written over from a site.
 * While the scroller overflows sideways it carries `data-scrolls-sideways`, which keeps the
 * horizontal scrollbar clear of the mask (`style.css`).
 *
 * A sidebar list with `stickyHeadingHeight` uses overlays instead: its headings paint at z-3
 * above the z-2 fades, and the top fade and scroll padding start below the heading. The wrapper
 * supplies their shared stacking context; the measured gutter keeps the scrollbar uncovered.
 */
export function FadedScroller({ orientation = "vertical", region, style, className, ref: forwarded, stickyHeadingHeight, ...props }: FadedScrollerProps): React.ReactElement {
  const ref = useObjectRef(forwarded);
  const sideways = useScrollsSideways(ref, orientation);
  const [visibility, setVisibility] = useState<ScrollShadowVisibility>("none");
  const [gutter, setGutter] = useState(0);
  useEffect(() => {
    const element = ref.current;
    if (stickyHeadingHeight === undefined || !element) return;
    const measure = () => setGutter(element.offsetWidth - element.clientWidth);
    const resize = new ResizeObserver(measure);
    resize.observe(element);
    measure();
    return () => resize.disconnect();
  });
  const scroller = (
    <ScrollShadow
      orientation={orientation}
      size={FADE_SIZE}
      {...props}
      {...sideways}
      ref={ref}
      onVisibilityChange={(value) => {
        if (stickyHeadingHeight !== undefined) setVisibility(value);
        region?.onVisibilityChange?.(value);
      }}
      {...region?.props}
      style={{ ...scrollPadding(orientation), ...style, ...(stickyHeadingHeight === undefined ? {} : {
        maskImage: "none", WebkitMaskImage: "none", animation: "none",
        scrollPaddingTop: stickyHeadingHeight + FADE_SIZE,
      }) }}
      className={joinClasses(className, region?.className)}
    />
  );
  if (stickyHeadingHeight === undefined) return scroller;
  return <StickyFadeFrame visibility={visibility} gutter={gutter} headingHeight={stickyHeadingHeight}>{scroller}</StickyFadeFrame>;
}

function StickyFadeFrame({ children, visibility, gutter, headingHeight, bottomGutter = 0 }: {
  children: React.ReactNode;
  visibility: ScrollShadowVisibility;
  gutter: number;
  headingHeight: number;
  bottomGutter?: number;
}): React.ReactElement {
  return (
    <div className="relative isolate flex min-h-0 flex-1 flex-col">
      {children}
      {(visibility === "top" || visibility === "both") && (
        <div aria-hidden="true" className="pointer-events-none absolute start-0 z-2 bg-linear-to-b from-panel-sidebar to-transparent"
          style={{ top: headingHeight, height: FADE_SIZE, insetInlineEnd: gutter }} />
      )}
      {(visibility === "bottom" || visibility === "both") && (
        <div aria-hidden="true" className="pointer-events-none absolute bottom-0 start-0 z-2 bg-linear-to-t from-panel-sidebar to-transparent"
          style={{ bottom: bottomGutter, height: FADE_SIZE, insetInlineEnd: gutter }} />
      )}
    </div>
  );
}

/** A virtualized scroller with sticky section headings: native scrolling and its collection role
 * stay on the element, while fades paint in a shared stacking context below its headings. */
export function useStickyScrollFade({ ref: forwarded, className, stickyHeadingHeight }: {
  ref?: Ref<HTMLDivElement>;
  className: string;
  stickyHeadingHeight: number;
}): {
  props: { ref: RefObject<HTMLDivElement | null>; className: string; style: CSSProperties };
  wrap: (children: React.ReactNode) => React.ReactElement;
} {
  const ref = useObjectRef(forwarded);
  const [visibility, setVisibility] = useState<ScrollShadowVisibility>("none");
  const [gutter, setGutter] = useState(0);
  const [bottomGutter, setBottomGutter] = useState(0);
  useScrollShadow({ containerRef: ref as RefObject<HTMLElement>, isEnabled: true, offset: 0, orientation: "vertical", visibility: "auto", onVisibilityChange: setVisibility });
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const measure = () => {
      setGutter(element.offsetWidth - element.clientWidth);
      setBottomGutter(element.offsetHeight - element.clientHeight);
    };
    const resize = new ResizeObserver(measure);
    resize.observe(element);
    measure();
    return () => resize.disconnect();
  });
  return {
    props: { ref, className, style: { ...scrollPadding("vertical"), scrollPaddingTop: stickyHeadingHeight + FADE_SIZE } },
    wrap: (children) => <StickyFadeFrame visibility={visibility} gutter={gutter} headingHeight={stickyHeadingHeight} bottomGutter={bottomGutter}>{children}</StickyFadeFrame>,
  };
}

/**
 * The same fade on a scroll container that is not a `ScrollShadow` itself, for a scroller that has
 * to be the element. Three reasons make it so, and a site has one of them:
 *
 * - It is virtualized: a list inside react-aria's `Virtualizer` must stay the scroll container of
 *   its own rows, so a wrapper around it would not be the one that scrolls.
 * - It carries its own role, name and key handling (a tree, a list box, a reading surface), which a
 *   wrapper cannot take over.
 * - Its ancestor animates away: the fade's animation never finishes, and an exit transition waits
 *   on the animations of its own element, so the fade goes on the scrolling list inside a popover,
 *   never on the popover.
 *
 * HeroUI's mask comes from its classes, so this applies those to the element itself. Where
 * `animation-timeline: scroll()` is supported the fade is pure CSS, keyed on the
 * `data-scroll-shadow-mode="auto"` this sets; `useScrollShadow` is called for the engines without
 * scroll timelines (an older WKWebView, say), whose `data-*` attributes the fade follows there.
 *
 * The element is a `div` unless the type parameter says otherwise (a `pre` of preformatted text).
 *
 * The caller's own `style`, `className` and `ref` go in here and come back in the one `props`
 * object to spread on the element, so none of them can be lost beside it. The caller also spells out
 * its overflow in `className`, never leaving it to the fade's `overflow-y`: the class is a
 * single-class rule in the same cascade layer as the library's own, so which wins would depend on the
 * order of the library's imports (`.list-box { overflow: clip }`), and a surface that scrolls
 * sideways too (unwrapped code, a wide table, a tree of long paths) asks for that axis there, since
 * only one edge fades. The fade keeps that sideways scrollbar clear of the mask all the same, from
 * the `data-scrolls-sideways` this sets while the element overflows that way.
 */
export function useScrollFade<T extends HTMLElement = HTMLDivElement>({
  orientation = "vertical",
  region,
  ref: forwarded,
  style,
  className,
}: {
  orientation?: Orientation;
  region?: FadeRegion;
  ref?: Ref<T>;
  style?: CSSProperties;
  className?: string;
} = {}): {
  ref: RefObject<T | null>;
  className: string;
  style: CSSProperties;
  "data-scroll-shadow-mode": "auto";
  "data-scrolls-sideways"?: true;
} & RegionProps {
  const ref = useObjectRef(forwarded);
  useScrollShadow({
    containerRef: ref as RefObject<HTMLElement>,
    isEnabled: true,
    offset: 0,
    onVisibilityChange: region?.onVisibilityChange,
    orientation,
    visibility: "auto",
  });
  const sideways = useScrollsSideways(ref, orientation);
  return {
    ...region?.props,
    ...sideways,
    ref,
    className: joinClasses(scrollShadowVariants({ orientation }).base(), className, region?.className),
    style: { "--scroll-shadow-size": `${FADE_SIZE}px`, ...scrollPadding(orientation), ...style } as CSSProperties,
    "data-scroll-shadow-mode": "auto",
  };
}
