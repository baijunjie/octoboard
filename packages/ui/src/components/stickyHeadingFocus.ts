import type React from "react";
import { type CSSProperties, useEffect, useRef } from "react";

import { stickyScrollPaddingTop } from "./scrollFade";

/** Long enough for the reveal that follows a focus move; the hold ends sooner once focus leaves. */
const FOCUS_SCROLL_HOLD_MS = 150;

/**
 * Keeps a list from jumping as keyboard focus moves among the controls of its sticky heading. A
 * control taking focus is scrolled into view, and the list's top scroll padding
 * (`stickyScrollPaddingTop`) keeps rows clear of the heading and its fade; a control in the heading
 * sits inside that padding, so the list would scroll on every focus move though the control is
 * pinned in view. Two things exempt the heading's controls, one per engine:
 *
 * - Their scroll area is moved down by the padding's depth (`scroll-margin-block`), which Chromium
 *   honours in its focus scroll. Moved rather than shrunk from the top alone, since WebKit does not
 *   honour a negative top margin that leaves the area with no height.
 * - WebKit's own focus scroll ignores scroll margins but runs after the focus event. So when the
 *   control is in view as it takes focus, the vertical positions of the heading's ancestors are held
 *   from that event for a moment, undoing whatever scrolls them. The hold ends as soon as focus
 *   leaves the heading, so the next control's reveal is left alone.
 *
 * The props go on the sticky element; its own classes are joined with `className`. They mark it as the
 * heading (`data-sticky-heading`), which `FadedScroller` places its top fade by.
 */
export function useStickyHeadingFocus(headingHeight: number): {
  "data-sticky-heading": "";
  className: string;
  style: CSSProperties;
  onFocusCapture: (event: React.FocusEvent<HTMLElement>) => void;
  onBlurCapture: (event: React.FocusEvent<HTMLElement>) => void;
} {
  const hold = useRef<AbortController | null>(null);
  useEffect(() => () => hold.current?.abort(), []);
  const release = () => {
    hold.current?.abort();
    hold.current = null;
  };
  return {
    "data-sticky-heading": "",
    className: "[&_*]:[scroll-margin-block:calc(-1*var(--sticky-heading-shift))_var(--sticky-heading-shift)]",
    style: { "--sticky-heading-shift": `${stickyScrollPaddingTop(headingHeight)}px` } as CSSProperties,
    onFocusCapture: (event) => {
      release();
      const target = event.target.getBoundingClientRect();
      const held: [Element, number][] = [];
      for (let element = event.currentTarget.parentElement; element; element = element.parentElement) {
        // A heading not yet in view (the next section's, below the fold) is still revealed.
        if (element.scrollHeight > element.clientHeight) {
          const box = element.getBoundingClientRect();
          if (target.top < box.top || target.bottom > box.bottom) return;
        }
        held.push([element, element.scrollTop]);
      }
      const listeners = new AbortController();
      hold.current = listeners;
      for (const [element, top] of held) {
        element.addEventListener("scroll", () => {
          if (element.scrollTop !== top) element.scrollTop = top;
        }, { signal: listeners.signal });
      }
      setTimeout(() => listeners.abort(), FOCUS_SCROLL_HOLD_MS);
    },
    onBlurCapture: (event) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) release();
    },
  };
}
