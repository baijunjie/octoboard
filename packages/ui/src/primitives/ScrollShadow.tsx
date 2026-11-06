import { ScrollShadow as HeroScrollShadow, scrollShadowVariants, useScrollShadow as useHeroScrollShadow } from "@heroui/react";
import React, { type RefObject } from "react";

import type { DataAttributes } from "./props";

/** The edges a scroller has content hidden behind, as HeroUI's scroll shadow reports them: `none` while it does not
 * overflow. */
export type ScrollShadowVisibility = "none" | "top" | "bottom" | "both" | "left" | "right";

export type ScrollShadowOrientation = "vertical" | "horizontal";

/** HeroUI's scroll shadow: a scroller that fades its content out at the edges it hides content behind, `size` pixels
 * deep, and reports those edges to `onVisibilityChange`. `hideScrollBar` scrolls it without a scrollbar. The other
 * props, a `ref` among them, go to the scroller's `div`. */
export function ScrollShadow({ orientation, size, hideScrollBar, onVisibilityChange, ...props }: React.ComponentProps<"div"> & DataAttributes & {
  orientation?: ScrollShadowOrientation;
  size: number;
  hideScrollBar?: boolean;
  onVisibilityChange?: (visibility: ScrollShadowVisibility) => void;
}): React.ReactElement {
  return (
    <HeroScrollShadow
      {...props}
      orientation={orientation}
      size={size}
      hideScrollBar={hideScrollBar}
      onVisibilityChange={onVisibilityChange && ((visibility) => onVisibilityChange(visibility as ScrollShadowVisibility))}
    />
  );
}

/** HeroUI's watch of a scroller's overflow, for a scroller that is not a `ScrollShadow` itself: it reports the edges
 * the element `ref` points at hides content behind to `onVisibilityChange`, and keeps the `data-*` attributes the
 * fade of `scrollShadowClassName` follows on engines without scroll timelines. The watch attaches once per ref
 * object. */
export function useScrollShadow({ ref, orientation, onVisibilityChange }: {
  ref: RefObject<HTMLElement | null>;
  orientation: ScrollShadowOrientation;
  onVisibilityChange: (visibility: ScrollShadowVisibility) => void;
}): void {
  useHeroScrollShadow({
    containerRef: ref as RefObject<HTMLElement>,
    isEnabled: true,
    offset: 0,
    orientation,
    visibility: "auto",
    onVisibilityChange: (visibility) => onVisibilityChange(visibility as ScrollShadowVisibility),
  });
}

/** The classes of HeroUI's scroll shadow, for the fade on an element that is not a `ScrollShadow` itself (see
 * `useScrollShadow`). */
export function scrollShadowClassName(orientation: ScrollShadowOrientation): string {
  return scrollShadowVariants({ orientation }).base();
}

/** The style that sizes the strips the fade leaves undimmed for the scrollbars, in pixels: `size` along the edge the
 * scrollbar sits on (HeroUI reserves a fixed 10px there), and, for a vertical fade that also leaves the bottom edge's
 * horizontal scrollbar clear, `blockSize` for that strip. */
export function scrollShadowStripStyle({ size, blockSize }: { size: number; blockSize?: number }): React.CSSProperties {
  return {
    "--scroll-shadow-scrollbar-size": `${size}px`,
    ...(blockSize === undefined ? {} : { "--scroll-shadow-scrollbar-block-size": `${blockSize}px` }),
  } as React.CSSProperties;
}

/** The props a `scrollShadowClassName` fade needs on its element: its depth, `size` pixels, as `style` (the fade's
 * depth in every engine), and the `data-*` attribute that selects the automatic mode, which switches on the pure-CSS
 * fade where scroll timelines exist. Both are spread on the element, the style merged with the caller's own. */
export function scrollShadowAutoProps(size: number): { style: React.CSSProperties; "data-scroll-shadow-mode": "auto" } {
  return { style: { "--scroll-shadow-size": `${size}px` } as React.CSSProperties, "data-scroll-shadow-mode": "auto" };
}
