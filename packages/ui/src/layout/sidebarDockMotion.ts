import { useEffect, useLayoutEffect } from "react";

import { columnSpan, usePaneDockMotion, type DockPhase, type PaneDockMotion } from "./paneDockMotion";

/** The clip the docked column sits in. `min-w-0` lets the flex item reach 0 while the sidebar
 * inside keeps its full width, so the list does not reflow. The separator at the column's end is
 * a pseudo-element on this clip rather than the nav's own end border: the nav is wider than the
 * clip while the column eases, so its border would stay clipped away until the column is fully
 * open and then appear at once. Drawn here it rides the clip's end edge through the ease, and
 * `overflow-hidden` clips it to nothing when the clip's width is 0, where a real border would keep
 * a 1px line at the rail. It is positioned so that it paints over the nav's background.
 * `--sidebar-span` itself is eased on the document element (see the hook). */
export function columnClipClass(): string {
  return (
    "contents docked:relative docked:block docked:h-full docked:min-h-0 docked:min-w-0 docked:shrink-0 docked:overflow-hidden " +
    "docked:after:pointer-events-none docked:after:absolute docked:after:inset-y-0 docked:after:end-0 docked:after:w-px docked:after:bg-separator"
  );
}

/** Publishes one eased length for the column and its aligned top-bar segment. */
export function useSidebarDockMotion(
  shown: boolean,
  peekActive: boolean,
  width: number,
  onPhase?: (phase: DockPhase) => void,
): PaneDockMotion {
  const motion = usePaneDockMotion(shown, peekActive, onPhase);
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.style.setProperty("--sidebar-span", `${columnSpan(motion.phase, width)}px`);
    root.classList.toggle("sidebar-span-animate", motion.phase === "opening" || motion.phase === "closing");
  }, [motion.phase, width]);

  useEffect(() => {
    const root = document.documentElement;
    const onEnd = (event: TransitionEvent) => {
      if (event.target === root && event.propertyName === "--sidebar-span") motion.onColumnEnd();
    };
    root.addEventListener("transitionend", onEnd);
    return () => root.removeEventListener("transitionend", onEnd);
  }, [motion.onColumnEnd]);

  useEffect(() => () => {
    document.documentElement.style.removeProperty("--sidebar-span");
    document.documentElement.classList.remove("sidebar-span-animate");
  }, []);
  return motion;
}
