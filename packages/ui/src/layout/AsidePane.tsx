import React from "react";

import { AsidePeekHotZone, drawerClass, PANE_ID } from "./paneOverlay";
import type { PanePeek } from "./usePaneToggles";

/** Where and how the aside is on screen right now; `App.tsx` builds it from `usePaneToggles` and
 * `usePaneWidth` and hands it to whichever owner is rendering the aside. */
export interface AsideLayout {
  /** Whether the drawer is open below the `docked` breakpoint. */
  open: boolean;
  /** The user's chosen width (`usePaneWidth`) for the docked and the floating forms; the drawer
   * below the breakpoint ignores it. */
  width: number;
  /** The hover reveal of the aside while the user has hidden the docked one from the rail (which
   * has no effect below the breakpoint, where `open` decides): the same pane, kept as a fixed overlay
   * that floats in over the terminal, with its shadow and rounded edge. `undefined` while the docked
   * aside is shown. */
  peek?: PanePeek;
}

/**
 * The frame of the aside, the pane at the end side, whichever owner fills it (a console session's
 * report panel, a project's browser). It is never unmounted by closing its drawer or hiding the
 * docked pane — only by its owner changing — so what it shows is not lost and requested again on
 * every reopen.
 *
 * `drawerClass` fits the drawer between `--top-chrome-height` and `--bottom-chrome-height`, leaving
 * the window chrome (the top bar, and the rail with the aside's toggle) and the connection banner
 * visible while it is open, and puts it back as a plain row sibling at or above the breakpoint — see
 * that function's own comment for the geometry. The drawer below the breakpoint is a fixed 420px,
 * capped at 92vw. The docked pane is `flex: 0 1` at `--aside-width`, the chosen width already held
 * back to what the row affords; the shrink and the 300px floor are only a safety net. With the
 * docked pane hidden, `drawerClass` instead keeps it the overlay at every width, floating in while
 * `peek` is active at `--aside-width` (still capped at 92vw); `overflow-hidden` clips what it holds to
 * the rounded edge. In every form the pane is painted `--panel`, the content panel's own colour, so
 * a drawer or a floating pane keeps an empty state from showing the terminal through.
 *
 * `data-escape-scope`: one of the origins `usePaneToggles`'s capture-phase Escape listener closes a
 * drawer for. The pointer handlers keep a floating pane up while the pointer is on it; an owner whose
 * content sends this document no pointer events of its own (a report page's sandboxed frame) wires
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
  const { open, width, peek } = layout;
  const overlay = peek
    ? `docked:w-(--aside-width) docked:rounded-s-xl docked:overflow-hidden ${drawerClass("end", "floating", open, peek.active)}`
    : `docked:w-auto docked:max-w-none docked:min-w-[300px] docked:flex-[0_1_var(--aside-width)] ${drawerClass("end", "drawer", open)}`;
  return (
    <>
      <div
        id={PANE_ID.aside}
        data-pane="aside"
        data-region="aside"
        data-escape-scope
        role={label ? "region" : undefined}
        aria-label={label}
        style={{ "--aside-width": `${width}px` } as React.CSSProperties}
        onPointerEnter={peek?.keep}
        onPointerMove={peek?.keep}
        onPointerLeave={peek?.leave}
        className={`flex min-h-0 flex-col border-s border-separator bg-panel w-[420px] max-w-[92vw] ${overlay} ${className}`}
      >
        {children}
      </div>
      {peek && <AsidePeekHotZone peek={peek} />}
    </>
  );
}
