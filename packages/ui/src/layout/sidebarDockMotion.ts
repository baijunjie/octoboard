import { useEffect, useLayoutEffect } from "react";

import { columnSpan, usePaneDockMotion, type DockPhase, type PaneDockMotion } from "./paneDockMotion";

/** The clip the docked column sits in. `min-w-0` lets the flex item reach 0 while the sidebar
 * inside keeps its full width, so the list does not reflow. The inset shadow is the separator the
 * nav's own end border draws once the column is open: a real border would keep a 1px line when the
 * clip's width is 0. `--sidebar-span` itself is eased on the document element (see the hook). */
export function columnClipClass(): string {
  return (
    "contents docked:block docked:h-full docked:min-h-0 docked:min-w-0 docked:shrink-0 docked:overflow-hidden " +
    "docked:shadow-[inset_-1px_0_0_0_var(--separator)] docked:rtl:shadow-[inset_1px_0_0_0_var(--separator)]"
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
