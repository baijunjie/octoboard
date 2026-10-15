import React, { useLayoutEffect, useRef } from "react";

import { AsidePeekHotZone, drawerClass, PANE_ID, usePanePeekPointer } from "./paneOverlay";
import { columnSpan, usePaneDockMotion, type DockPhase } from "./paneDockMotion";
import type { PanePeek } from "./usePaneToggles";

/** Where and how the aside is on screen right now; `App.tsx` builds it from `usePaneToggles` and
 * `usePaneWidth` and hands it to whichever owner is rendering the aside. */
export interface AsideLayout {
  /** Whether the drawer is open below the `docked` breakpoint. */
  open: boolean;
  /** The user's chosen width (`usePaneWidth`) for the docked and the floating forms; the drawer
   * below the breakpoint ignores it. */
  width: number;
  /** The hover reveal of the aside while the user has hidden the docked one from the top bar (which
   * has no effect below the breakpoint, where `open` decides): the same pane, kept as a fixed
   * overlay that floats in over the terminal, as a card with its shadow and rounded corners.
   * `undefined` while the docked aside is shown. */
  peek?: PanePeek;
  /** Reports the column ease so its width clamp and resize handle follow the visible column. */
  onColumnPhase?: (phase: DockPhase) => void;
}

/**
 * The frame of the aside, the pane at the end side, whichever owner fills it (a console session's
 * report panel, a project's browser). It is never unmounted by closing its drawer or hiding the
 * docked pane — only by its owner changing — so what it shows is not lost and requested again on
 * every reopen.
 *
 * `drawerClass` fits the drawer between `--top-chrome-height` and `--bottom-chrome-height`, leaving
 * the window chrome (the top bar, with the aside's toggle, and the rail) and the connection banner
 * visible while it is open, inset as a card, and puts it back as a plain row sibling at or above
 * the breakpoint — see that function's own comment for the geometry. The drawer below the
 * breakpoint is a fixed 420px, capped at 92vw and the space past the rail, with an 8px gap at
 * either edge. The open docked pane shrinks to the row's available room, with a 300px floor.
 * During its own toggle the pane keeps its rendered width inside a clip that eases to zero,
 * so its content never reflows during the close. With the docked pane hidden,
 * `drawerClass` keeps it an overlay, fading in while
 * `peek` is active at `--aside-width` (still capped at 92vw), as a rounded, bordered card like the
 * floating sidebar; `overflow-hidden` clips what it holds to the rounded edge. In every form the
 * pane is painted `--panel-sidebar`, the sidebar's colour, so a drawer or a floating pane keeps
 * an empty state from showing the terminal through.
 *
 * `data-escape-scope`: one of the origins `usePaneToggles`'s capture-phase Escape listener closes a
 * drawer for. The pointer handlers, with `usePanePeekPointer` (called here) for the gap around the
 * card, keep a floating pane up while the pointer is on it; an owner whose content sends this
 * document no pointer events of its own (a report page's sandboxed frame) wires
 * the same `peek` callbacks onto that element too.
 */
export function AsidePane({
  layout,
  label,
  className = "",
  children,
}: {
  layout: AsideLayout;
  /** The pane's accessible name, when it is a landmark of its own. */
  label?: string;
  /** Classes for the pane's content layout, on top of its frame's. */
  className?: string;
  children?: React.ReactNode;
}): React.ReactElement {
  const { open, width, peek, onColumnPhase } = layout;
  const dock = usePaneDockMotion(peek === undefined, peek?.active ?? false, onColumnPhase);
  usePanePeekPointer("aside", peek);
  const clipRef = useRef<HTMLDivElement>(null);
  const paneRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (dock.phase !== "open") return;
    const clip = clipRef.current;
    const pane = paneRef.current;
    if (!clip || !pane || typeof ResizeObserver === "undefined") return;
    // A sibling's ease can temporarily constrain this column below its chosen width. Preserve
    // that rendered width if this pane starts closing before the sibling has settled.
    const measure = () => {
      const renderedWidth = `${clip.getBoundingClientRect().width}px`;
      // Flex basis keeps negotiating space while the width property tracks the actual size;
      // a closing width transition must start from this size rather than the chosen basis.
      clip.style.setProperty("--aside-rendered-width", renderedWidth);
      pane.style.setProperty("--aside-close-width", renderedWidth);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(clip);
    return () => observer.disconnect();
  }, [dock.phase]);
  const overlay = dock.inset
    ? `docked:w-(--aside-width) docked:rounded-xl docked:border docked:overflow-hidden ${drawerClass("end", "floating", open, dock.insetShown)}${dock.preparingInset ? " docked:transition-none" : ""}`
    : `${dock.phase === "open" ? "docked:w-full" : dock.phase === "closing" ? "docked:w-[var(--aside-close-width,var(--aside-width))]" : "docked:w-(--aside-width)"} docked:max-w-none docked:shrink-0 docked:h-full ${drawerClass("end", "drawer", open)}${dock.phase === "closed" ? " docked:invisible" : ""}`;
  return (
    <>
      <div
        ref={clipRef}
        className={dock.inset ? "contents" : `contents docked:flex docked:justify-end docked:h-full docked:min-h-0 docked:overflow-hidden ${dock.phase === "open" ? "docked:w-[var(--aside-rendered-width,var(--aside-span))] docked:min-w-[300px] docked:flex-[0_1_var(--aside-span)]" : "docked:w-(--aside-span) docked:min-w-0 docked:shrink-0"} ${dock.phase === "opening" || dock.phase === "closing" ? " docked:transition-[width] docked:duration-200 docked:ease-in-out motion-reduce:transition-none" : ""}`}
        style={{ "--aside-span": `${columnSpan(dock.phase, width)}px` } as React.CSSProperties}
        onTransitionEnd={dock.onColumnTransitionEnd}
      >
        <div
          ref={paneRef}
          id={PANE_ID.aside}
          data-pane="aside"
          data-region="aside"
          data-escape-scope
          role={label ? "region" : undefined}
          aria-label={label}
          style={{ "--aside-width": `${width}px` } as React.CSSProperties}
          onTransitionEnd={dock.onInsetTransitionEnd}
          onPointerEnter={peek?.keep}
          onPointerLeave={peek?.leave}
          className={`flex min-h-0 flex-col border-s border-separator bg-panel-sidebar w-[420px] max-w-[92vw] max-docked:max-w-[calc(100vw-var(--rail-width)-2*var(--peek-gap))] ${overlay} ${className}`}
        >
          {children}
        </div>
      </div>
      {peek && <AsidePeekHotZone peek={peek} />}
    </>
  );
}
