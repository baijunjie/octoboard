import { useEffect } from "react";

import { useLatest } from "../components/useLatest";
import { MODAL_OPEN } from "./useRegionCycle";
import type { SwipeToward } from "./paneSwipe";

/** How far a finger has to travel sideways before the move counts as a swipe. */
export const SWIPE_DISTANCE_PX = 56;

/** A swipe is dominantly horizontal: it goes at least this many times further sideways than up or
 * down, so a diagonal drag while scrolling is not one. */
export const SWIPE_AXIS_RATIO = 2;

/** Once a move has gone this far up or down, and further so than sideways, it is a scroll and the
 * rest of that touch is not looked at. */
const VERTICAL_SLOP_PX = 12;

/** What takes a horizontal drag of its own: a pane's resize separator, a slider. */
const OWN_DRAG = '[role="separator"], [role="slider"], input[type="range"]';

/** A menu's or popover's trigger that is expanded, as `usePaneToggles` looks for one. */
const POPUP_OPEN = "[aria-haspopup][aria-expanded=true]";

/** Whether `element` has content out of view that a finger moving by `dx` (positive: rightwards)
 * would scroll into view. */
function scrollsBy(element: Element, dx: number): boolean {
  if (!(element instanceof HTMLElement)) return false;
  const style = getComputedStyle(element);
  if (style.overflowX !== "auto" && style.overflowX !== "scroll") return false;
  const hidden = element.scrollWidth - element.clientWidth;
  if (hidden <= 1) return false;
  // `scrollLeft` is 0 at the start edge and runs negative towards the end in right-to-left text.
  const hiddenLeft = style.direction === "rtl" ? hidden + element.scrollLeft : element.scrollLeft;
  const hiddenRight = hidden - hiddenLeft;
  return dx > 0 ? hiddenLeft > 1 : hiddenRight > 1;
}

/** Whether a touch that began on `origin` is a horizontal scroll of an element that can still scroll
 * the way it went (a code view, a diff, a wide table): that scroll is the element's, not a swipe. */
export function scrollsHorizontally(origin: Element | null, dx: number): boolean {
  for (let element = origin; element; element = element.parentElement) {
    if (scrollsBy(element, dx)) return true;
  }
  return false;
}

/**
 * Calls `onSwipe` when a finger swipes sideways anywhere in the window. It fires as the move passes
 * `SWIPE_DISTANCE_PX` while still dominantly horizontal, not on release, and does not follow the
 * finger. It listens to touch events rather than pointer events: once the browser starts panning a
 * touch it cancels the pointer's events (`pointercancel`) but keeps sending the touch's. A pen is
 * therefore not covered. It ignores a touch that starts on a control that takes a drag of its own
 * (`OWN_DRAG`) or on an element that can scroll sideways the way the finger goes
 * (`scrollsHorizontally`); one that turns vertical; a second finger; and any touch while a dialog, a
 * menu or a popover is open. `rtl` is the reading direction, which decides which way is `end`.
 */
export function useSwipe(enabled: boolean, rtl: boolean, onSwipe: (toward: SwipeToward) => void): void {
  const latest = useLatest(onSwipe);
  useEffect(() => {
    if (!enabled) return;
    let touch: { id: number; x: number; y: number; origin: Element | null } | undefined;
    const onStart = (event: TouchEvent) => {
      // A second finger is a pinch or a two-finger scroll, whatever the first was doing.
      if (event.touches.length > 1) {
        touch = undefined;
        return;
      }
      const first = event.changedTouches[0];
      if (!first) return;
      const origin = event.target instanceof Element ? event.target : null;
      if (origin?.closest(OWN_DRAG) || document.querySelector(`${MODAL_OPEN}, ${POPUP_OPEN}`)) return;
      touch = { id: first.identifier, x: first.clientX, y: first.clientY, origin };
    };
    const onMove = (event: TouchEvent) => {
      if (!touch) return;
      const moved = Array.from(event.changedTouches).find((candidate) => candidate.identifier === touch!.id);
      if (!moved) return;
      const dx = moved.clientX - touch.x;
      const dy = moved.clientY - touch.y;
      if (Math.abs(dy) > VERTICAL_SLOP_PX && Math.abs(dy) > Math.abs(dx)) {
        touch = undefined;
        return;
      }
      if (Math.abs(dx) < SWIPE_DISTANCE_PX || Math.abs(dx) < SWIPE_AXIS_RATIO * Math.abs(dy)) return;
      const origin = touch.origin;
      touch = undefined;
      if (scrollsHorizontally(origin, dx)) return;
      latest.current(dx > 0 !== rtl ? "end" : "start");
    };
    const onEnd = () => {
      touch = undefined;
    };
    const options = { capture: true, passive: true };
    window.addEventListener("touchstart", onStart, options);
    window.addEventListener("touchmove", onMove, options);
    window.addEventListener("touchend", onEnd, options);
    window.addEventListener("touchcancel", onEnd, options);
    return () => {
      window.removeEventListener("touchstart", onStart, { capture: true });
      window.removeEventListener("touchmove", onMove, { capture: true });
      window.removeEventListener("touchend", onEnd, { capture: true });
      window.removeEventListener("touchcancel", onEnd, { capture: true });
    };
  }, [enabled, rtl]);
}
