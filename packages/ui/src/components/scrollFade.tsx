import { ScrollShadow, scrollShadowVariants, useScrollShadow, type ScrollShadowVisibility } from "@heroui/react";
import type React from "react";
import { useEffect, useLayoutEffect, useState, type CSSProperties, type Ref, type RefObject } from "react";
import { useObjectRef } from "react-aria";

import { useFocusVisibleProps } from "./useFocusVisibleProps";

/** The fade's size in pixels. A row scrolled to the list's edge has to stop this far short of it to
 * be clear of the fade, so whatever scrolls the list by hand keeps this margin too. */
export const FADE_SIZE = 24;

type Orientation = "vertical" | "horizontal";

/** The top scroll padding of a list with sticky headings `headingHeight` tall: what scrolls into view
 * stops clear of the heading and the fade below it. The controls inside the heading, pinned in view
 * already, are exempted from it by `useStickyHeadingFocus`. */
export function stickyScrollPaddingTop(headingHeight: number): number {
  return headingHeight + FADE_SIZE;
}

/** The scroll padding that keeps whatever the browser or react-aria scrolls into view (a control
 * that takes focus) clear of the fade rather than under it; it goes on the axis that fades. */
function scrollPadding(orientation: Orientation, endInset = 0): CSSProperties {
  return orientation === "horizontal"
    ? { scrollPaddingInlineStart: FADE_SIZE, scrollPaddingInlineEnd: FADE_SIZE + endInset }
    : { scrollPaddingBlock: FADE_SIZE };
}

/** What draws no fade: the scroll-driven mask and its animation, off. */
const NO_FADE: CSSProperties = { maskImage: "none", WebkitMaskImage: "none", animation: "none" };

/** The room the scrollbars take at the box's end edges, measured: the scrollbar plus the end border,
 * since both the mask and the sticky overlays are placed from the border box's edge. An overlay
 * scrollbar (macOS by default) takes none, and leaves only the border. */
function scrollbarSizes(element: HTMLElement): { inline: number; block: number } {
  const css = getComputedStyle(element);
  const border = (...sides: string[]) => sides.reduce((sum, side) => sum + (parseFloat(css.getPropertyValue(`border-${side}-width`)) || 0), 0);
  return {
    inline: Math.max(0, element.offsetWidth - element.clientWidth - border("left")),
    block: Math.max(0, element.offsetHeight - element.clientHeight - border("top")),
  };
}

/**
 * The strips the fade's mask leaves undimmed for the scrollbars, sized to the scrollbars the scroller
 * really has. HeroUI reserves a fixed 10px strip (`--scroll-shadow-scrollbar-size`) along the edge
 * its scrollbar sits on, so where the scrollbar overlays the content and takes no room, that strip
 * of content and background stays at full strength while the rest of the edge fades. The vertical
 * fade's strip is the vertical scrollbar's width, the horizontal one's the horizontal scrollbar's
 * height.
 *
 * It also says whether a vertical fade can scroll sideways, for it to leave the horizontal scrollbar
 * undimmed too (see `[data-scrolls-sideways]` in `style.css`, whose bottom strip is
 * `--scroll-shadow-scrollbar-block-size`). Only a vertical fade needs that: the library's horizontal
 * one already reserves the strip along the bottom itself.
 *
 * It runs on every commit of the caller rather than once, with no dependencies, because what makes
 * a surface start or stop scrolling sideways often leaves nothing an observer would see: the same
 * `pre` takes a new file as one text node, and the word-wrap choice changes only a class. Re-running
 * is also what re-attaches the observer to a scroller React replaced in place, which the viewer's
 * document does on a new subject. A `ResizeObserver` covers the rest — a lazy render finishing, the
 * window resizing — and reads back the same answer when nothing moved, which React drops.
 */
function useScrollbarStrips(ref: RefObject<HTMLElement | null>, orientation: Orientation): {
  marker: { "data-scrolls-sideways"?: true };
  style: CSSProperties;
  /** The vertical scrollbar's measured width, for a caller that keeps its own overlays clear of it. */
  inline: number;
} {
  const [sideways, setSideways] = useState(false);
  const [inline, setInline] = useState(0);
  const [block, setBlock] = useState(0);
  // Before paint: the inline value replaces HeroUI's default, so a first frame at 0 would dim a classic scrollbar.
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) {
      setSideways(false);
      return;
    }
    const measure = () => {
      setSideways(orientation === "vertical" && element.scrollWidth > element.clientWidth);
      const sizes = scrollbarSizes(element);
      setInline(sizes.inline);
      setBlock(sizes.block);
    };
    const resize = new ResizeObserver(measure);
    resize.observe(element);
    for (const child of element.children) resize.observe(child);
    measure();
    return () => resize.disconnect();
  });
  return {
    marker: sideways ? { "data-scrolls-sideways": true } : {},
    style: orientation === "vertical"
      ? { "--scroll-shadow-scrollbar-size": `${inline}px`, "--scroll-shadow-scrollbar-block-size": `${block}px` } as CSSProperties
      : { "--scroll-shadow-scrollbar-size": `${block}px` } as CSSProperties,
    inline,
  };
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
  /** The scroller overflows, so it is the focusable region `props` make it (always, with `always`). */
  overflows: boolean;
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
    overflows,
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
  /** Sticky headings paint above overlay fades; a mask would dim every descendant regardless of z-index. The
   * top fade follows the first element marked `data-sticky-heading` (see `useStickyHeadingFocus`). */
  stickyHeadingHeight?: number;
  /** The surface the overlay fades fade into, with `stickyHeadingHeight`: the side panes' by default. */
  stickySurface?: StickySurface;
  /** Draws no fade, keeping the rest: the scroll padding, the overflow watch and the focusable region. For a scroller
   * with a control pinned at its end edge, which a mask would dim; the pinned control's own edge says there is more. */
  fade?: boolean;
  /** Room at the end edge that something pinned there takes, in pixels, added to the scroll padding there so that
   * focus scrolled into view stops clear of it as well as of the fade. */
  endInset?: number;
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
 * A list with sticky headings (the sidebar's, the Settings agent strip) passes `stickyHeadingHeight`
 * and uses overlays instead: its headings paint at z-3 above the z-2 fades, and the top fade and
 * scroll padding start below the heading. The top fade follows the first element marked `data-sticky-heading`: at the
 * scroller's edge while that one still sits further down, below it once it is pinned. The wrapper
 * supplies their shared stacking context; the measured gutter keeps the scrollbar uncovered.
 */
export function FadedScroller({ orientation = "vertical", region, style, className, ref: forwarded, stickyHeadingHeight, stickySurface, fade = true, endInset, ...props }: FadedScrollerProps): React.ReactElement {
  const ref = useObjectRef(forwarded);
  const strips = useScrollbarStrips(ref, orientation);
  const [visibility, setVisibility] = useState<ScrollShadowVisibility>("none");
  const [topOffset, setTopOffset] = useState(stickyHeadingHeight ?? 0);
  // Like `useScrollbarStrips`, it re-runs on every commit, to pick up children React replaced.
  useEffect(() => {
    const element = ref.current;
    if (stickyHeadingHeight === undefined || !element) return;
    const place = () => {
      const sticky = element.querySelector("[data-sticky-heading]");
      const gap = sticky ? sticky.getBoundingClientRect().top - element.getBoundingClientRect().top : 0;
      setTopOffset(Math.min(stickyHeadingHeight, Math.max(0, stickyHeadingHeight - gap)));
    };
    // The heading's place moves with a resize or a re-wrap as well as with scrolling, so what the content
    // is made of is watched too.
    const resize = new ResizeObserver(place);
    resize.observe(element);
    for (const child of element.children) resize.observe(child);
    element.addEventListener("scroll", place, { passive: true });
    place();
    return () => {
      resize.disconnect();
      element.removeEventListener("scroll", place);
    };
  });
  const scroller = (
    <ScrollShadow
      orientation={orientation}
      size={FADE_SIZE}
      {...props}
      {...strips.marker}
      ref={ref}
      onVisibilityChange={(value) => {
        if (stickyHeadingHeight !== undefined) setVisibility(value);
        region?.onVisibilityChange?.(value);
      }}
      {...region?.props}
      style={{ ...scrollPadding(orientation, endInset), ...(fade ? {} : NO_FADE), ...style, ...strips.style, ...(stickyHeadingHeight === undefined ? {} : {
        maskImage: "none", WebkitMaskImage: "none", animation: "none",
        scrollPaddingTop: stickyScrollPaddingTop(stickyHeadingHeight),
      }) }}
      className={joinClasses(className, region?.className)}
    />
  );
  if (stickyHeadingHeight === undefined) return scroller;
  return <StickyFadeFrame visibility={visibility} gutter={strips.inline} topOffset={topOffset} surface={stickySurface}>{scroller}</StickyFadeFrame>;
}

type StickySurface = "sidebar" | "overlay";

/** Whole class names, for Tailwind to find: the fades' start colour is the surface the list sits on. */
const FADE_FROM: Record<StickySurface, string> = { sidebar: "from-panel-sidebar", overlay: "from-overlay" };

function StickyFadeFrame({ children, visibility, gutter, topOffset, bottomGutter = 0, surface = "sidebar" }: {
  children: React.ReactNode;
  visibility: ScrollShadowVisibility;
  gutter: number;
  topOffset: number;
  bottomGutter?: number;
  surface?: StickySurface;
}): React.ReactElement {
  return (
    <div className="relative isolate flex min-h-0 flex-1 flex-col">
      {children}
      {(visibility === "top" || visibility === "both") && (
        // Cover the heading's side gutters too: shadows can extend beyond its inset background.
        <div aria-hidden="true" className={`pointer-events-none absolute top-0 start-0 z-2 bg-linear-to-b ${FADE_FROM[surface]} from-[length:var(--sticky-fade-offset)] to-transparent`}
          style={{ "--sticky-fade-offset": `${topOffset}px`, height: topOffset + FADE_SIZE, insetInlineEnd: gutter } as CSSProperties} />
      )}
      {(visibility === "bottom" || visibility === "both") && (
        <div aria-hidden="true" className={`pointer-events-none absolute bottom-0 start-0 z-2 bg-linear-to-t ${FADE_FROM[surface]} to-transparent`}
          style={{ bottom: bottomGutter, height: FADE_SIZE, insetInlineEnd: gutter }} />
      )}
    </div>
  );
}

/** A scroller with sticky section headings or one pinned row, virtualized or not, that has to be the
 * element itself: native scrolling and its collection role stay on the element, while fades paint in
 * a shared stacking context below the pinned element. Unlike `FadedScroller` it does not follow
 * `data-sticky-heading`: the top fade sits at the fixed `stickyHeadingHeight`, so a caller whose
 * pinned element can be absent passes 0 then. */
export function useStickyScrollFade({ ref: forwarded, className, stickyHeadingHeight, stickySurface }: {
  ref?: Ref<HTMLDivElement>;
  className: string;
  stickyHeadingHeight: number;
  /** The surface the overlay fades fade into: the side panes' by default. */
  stickySurface?: StickySurface;
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
      const sizes = scrollbarSizes(element);
      setGutter(sizes.inline);
      setBottomGutter(sizes.block);
    };
    const resize = new ResizeObserver(measure);
    resize.observe(element);
    measure();
    return () => resize.disconnect();
  });
  return {
    props: { ref, className, style: { ...scrollPadding("vertical"), scrollPaddingTop: stickyScrollPaddingTop(stickyHeadingHeight) } },
    wrap: (children) => <StickyFadeFrame visibility={visibility} gutter={gutter} topOffset={stickyHeadingHeight} bottomGutter={bottomGutter} surface={stickySurface}>{children}</StickyFadeFrame>,
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
  const strips = useScrollbarStrips(ref, orientation);
  return {
    ...region?.props,
    ...strips.marker,
    ref,
    className: joinClasses(scrollShadowVariants({ orientation }).base(), className, region?.className),
    style: { "--scroll-shadow-size": `${FADE_SIZE}px`, ...scrollPadding(orientation), ...style, ...strips.style } as CSSProperties,
    "data-scroll-shadow-mode": "auto",
  };
}
