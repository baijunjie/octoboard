import React from "react";

import { usePaneDockMotion } from "./paneDockMotion";
import { PANE_ID, usePanePeekPointer } from "./paneOverlay";
import type { PanePeek } from "./usePaneToggles";

/** Where and how the aside is on screen right now; `App.tsx` builds it from `usePaneToggles` and
 * `usePaneWidth` and hands it to the pane. */
export interface AsideLayout {
  /** The user's chosen width (`usePaneWidth`), for the docked column and the floating cards alike. */
  width: number;
  /** The docked column's floor, shared with its width clamp and resize handle. */
  minWidth?: number;
  /** The hover reveal of the aside while it has no panel open: the cards float over the terminal,
   * showing the panel whose toggle the pointer is on. `undefined` while the pane is docked. */
  peek?: PanePeek;
  /** Docked with no card in its column, its one open panel being maximized over the agent
   * terminal's area (`panels/PanelStack.tsx`): it takes no room in the row, so that area, and the
   * panel with it, reaches the window's end. It is then not the pane the toggles name and the
   * floating pane's rules look for (`id`, `data-pane`): a panel floating in comes in a pane of its
   * own (`panels/RightPane.tsx`). */
  folded?: boolean;
}

/**
 * The aside, the pane at the end side: a column of panel cards (`panels/PanelStack.tsx`), each
 * rounded, edged and shadowed by itself. The pane has no surface of its own: it pads the cards by
 * `--peek-gap`, the gap the sidebar's floating card keeps from its surroundings, on the top, end
 * and bottom, and by `--panel-grip-gap` on the terminal side, which holds the width grip. It shows
 * nothing in that gap but what is behind it, the agent terminal's background, so the cards read as
 * lying on the content.
 *
 * While it has a panel open it is docked, a row sibling of the terminal that takes its room from it
 * and shrinks to the row's available room down to `minWidth` (320px by default); it does so at every width, there is
 * no drawer form, and docking (and going, when its last panel closes) is at once. With no panel open
 * it is nothing, except while `peek` is active (the pointer is on a panel kind's toggle): then it is
 * a fixed overlay at `--aside-width` (capped at 92vw), so only its cards are on the terminal, whose
 * pointer events the gap does not take. The cards look the same docked and floating
 * (`PANEL_CARD_CLASS`, the sidebar's floating card). It floats in and out as the sidebar's
 * floating card does: fading in as it rises 6px, and the other way out, in `PANE_INSET_MS` with the
 * sidebar's easing and reduced-motion handling (`usePaneDockMotion` holds both), and is not
 * rendered once it has gone.
 *
 * `data-escape-scope`: one of the origins `usePaneToggles`'s capture-phase Escape listener closes a
 * floating pane for. The pointer handlers, with `usePanePeekPointer` (called here) for the gap
 * around the pane, keep a floating pane up while the pointer is on it; a panel whose content sends
 * this document no pointer events of its own (a report page's sandboxed frame) wires the same
 * `peek` callbacks onto that element too.
 */
export function AsidePane({ layout, children }: { layout: AsideLayout; children?: React.ReactNode }): React.ReactElement | null {
  const { width, minWidth = 320, peek, folded = false } = layout;
  // Only its floating half is used, as if the pane were never docked: it docks and goes at once, so
  // there is no column to ease out before the card floats in.
  const dock = usePaneDockMotion(false, peek?.active ?? false);
  usePanePeekPointer("aside", peek);
  if (peek && !dock.inset) return null;
  return (
    <div
      id={folded ? undefined : PANE_ID.aside}
      data-pane={folded ? undefined : "aside"}
      data-escape-scope
      style={{ "--aside-width": `${width}px`, "--aside-min-width": `${minWidth}px` } as React.CSSProperties}
      onTransitionEnd={dock.onInsetTransitionEnd}
      onPointerEnter={peek?.keep}
      onPointerLeave={peek?.leave}
      className={`flex min-h-0 flex-col ${folded ? "" : "ps-(--panel-grip-gap) pe-(--peek-gap) py-(--peek-gap)"} ${
        peek
          ? `pointer-events-none fixed bottom-(--bottom-chrome-height) top-(--top-chrome-height) end-0 z-40 w-(--aside-width) max-w-[92vw] transition-[translate,opacity,visibility] duration-240 ease-[cubic-bezier(0.2,0,0,1)] motion-reduce:transition-none ${dock.insetShown ? "translate-y-0 opacity-100" : "translate-y-1.5 opacity-0 invisible"}${dock.preparingInset ? " transition-none" : ""}`
          : folded
            ? "h-full w-0"
            : "h-full w-(--aside-width) min-w-(--aside-min-width) shrink"
      }`}
    >
      {children}
    </div>
  );
}
