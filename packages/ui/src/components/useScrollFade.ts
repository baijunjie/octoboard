import { scrollShadowVariants, useScrollShadow } from "@heroui/react";
import type { CSSProperties, Ref, RefObject } from "react";
import { useObjectRef } from "react-aria";

/** The fade's size in pixels, the same as the `ScrollShadow`s elsewhere in the app. A row scrolled
 * to the list's edge has to stop this far short of it to be clear of the fade, so whatever scrolls
 * the list by hand keeps this margin too. */
export const FADE_SIZE = 24;

const FADE_CLASS = scrollShadowVariants().base();

/** The vertical top and bottom fade of HeroUI's `ScrollShadow` on a scroll container that is not a
 * `ScrollShadow` itself: a virtualized list must stay the scroll container of its own rows, so a
 * `ScrollShadow` around it would not be the one that scrolls. HeroUI's mask comes from its classes,
 * so this applies those to the list's own element, and the fade is the same as everywhere else.
 *
 * Where `animation-timeline: scroll()` is supported the fade is pure CSS, keyed on the
 * `data-scroll-shadow-mode="auto"` this sets. `useScrollShadow` is called for the engines without
 * scroll timelines (an older WKWebView, say): the `data-*` attributes it sets on the element are
 * what the fade follows there.
 *
 * The style's `scroll-padding` is the fade's size, which `scrollIntoView` of react-aria (a row that
 * takes focus) honors, so a row scrolled to an edge lands clear of the fade rather than under it.
 *
 * Give the scroll container `ref` and `props`, and add `className` to its own. `forwarded` is a ref
 * the caller wants on the same element. */
export function useScrollFade(forwarded?: Ref<HTMLDivElement>): {
  ref: RefObject<HTMLDivElement | null>;
  className: string;
  props: { "data-scroll-shadow-mode": "auto"; style: CSSProperties };
} {
  const ref = useObjectRef(forwarded);
  useScrollShadow({ containerRef: ref as RefObject<HTMLElement>, isEnabled: true, offset: 0, orientation: "vertical", visibility: "auto" });
  return {
    ref,
    className: FADE_CLASS,
    props: { "data-scroll-shadow-mode": "auto", style: { "--scroll-shadow-size": `${FADE_SIZE}px`, scrollPaddingBlock: FADE_SIZE } as CSSProperties },
  };
}
