import React from "react";

import { useFadeUp, usePaneDockMotion } from "./paneDockMotion";
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
  /** The window is too narrow to dock the pane beside the terminal (`useAsideFloats`): with a panel
   * open it floats over the agent terminal's area instead, at its chosen width capped to that area
   * less a strip to dismiss it by, and takes no room in the row. */
  overlay?: boolean;
  /** The kind of the panel floating over the terminal in that window, while one is (`overlay`). */
  floatingKind?: string;
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
 * and shrinks to the row's available room down to `minWidth` (320px by default), and docking (and
 * going, when its last panel closes) is at once. In a window too narrow for that (`overlay`, a phone)
 * it floats over the terminal's area instead, holding one panel at a time
 * (`panels/useFloatingPanel.ts`), positioned in `main` beneath the top bar, padded by `--peek-gap`
 * all round as there is no width grip, and above the scrim that dismisses it (`z-35`, the scrim's
 * `z-30`). It leaves a strip of the scrim beside it to dismiss it by, a rail's width, or 24px in a
 * viewport 360px wide or less (a small phone, or the website's preview frame on one) where the
 * panel needs the room more, and passes presses in its gaps through to the scrim. It fades into
 * place and back out of it, sliding in from the end edge and back to it (`useFadeUp`, replayed when
 * another panel takes its place, at once under reduced motion), staying inert while it goes, with no hover's card taking its place until it
 * has gone; the terminal under it keeps its size, and `data-floating-panel` marks it for the
 * terminal's focus hand-off. With no panel open it is nothing, except while `peek` is active (the
 * pointer is on a panel kind's toggle): then it is a fixed overlay at `--aside-width` (capped at
 * 92vw), so only its cards are on the terminal, whose pointer events the gap does not take. The
 * cards look the same docked and floating (`PANEL_CARD_CLASS`, the sidebar's floating card). It
 * floats in and out as the sidebar's floating card does: fading in as each card slides in from the
 * end edge, and the other way out (only the desktop hover's peek rises by 4% of the card's height),
 * in `PANE_INSET_MS` with the sidebar's easing and reduced-motion handling (`usePaneDockMotion`
 * holds both), and is not rendered once it has gone.
 *
 * `children` can be a function of the hover's `peek` the pane is following, which is none while a
 * floating panel fades away, so the stack keeps the leaving card rather than the hovered one.
 *
 * `data-escape-scope`: one of the origins `usePaneToggles`'s capture-phase Escape listener closes a
 * floating pane for. The pointer handlers, with `usePanePeekPointer` (called here) for the gap
 * around the pane, keep a floating pane up while the pointer is on it; a panel whose content sends
 * this document no pointer events of its own (a report page's sandboxed frame) wires the same
 * `peek` callbacks onto that element too.
 */
/** Where a floating card is in its rise: in place, below it, or below it waiting a painted frame to
 * rise without a transition back down first. */
function motionOf(shown: boolean, preparing: boolean): "shown" | "away" | "preparing" {
  return preparing ? "preparing" : shown ? "shown" : "away";
}

export function AsidePane({ layout, children }: { layout: AsideLayout; children?: React.ReactNode | ((peek: PanePeek | undefined) => React.ReactNode) }): React.ReactElement | null {
  const { width, minWidth = 320, peek: hoverPeek, folded = false, overlay = false, floatingKind } = layout;
  // A panel floating over the terminal fades in as it comes, again for another panel in its place,
  // and back out as it goes; it is that, not a hover's card, until it has gone.
  const open = overlay && floatingKind !== undefined;
  const fade = useFadeUp(open, floatingKind);
  const floating = overlay && fade.rendered;
  const peek = floating ? undefined : hoverPeek;
  // Only its floating half is used, as if the pane were never docked: it docks and goes at once, so
  // there is no column to ease out before the card floats in.
  const dock = usePaneDockMotion(false, peek?.active ?? false);
  usePanePeekPointer("aside", peek);
  if (peek && !dock.inset) return null;
  return (
    <div
      id={folded ? undefined : PANE_ID.aside}
      data-pane={folded ? undefined : "aside"}
      data-floating-panel={open ? "" : undefined}
      // A panel on its way out takes no presses and no focus.
      inert={floating && !open}
      // The pane fades; each card in it moves (below `phone` sliding from the end edge, else rising by
      // 4% of its own height, `style.css`), so the card's frame, background and shadow move with its
      // content and the percentage is the card's.
      data-card-motion={peek ? motionOf(dock.insetShown, dock.preparingInset) : floating ? motionOf(fade.shown, fade.preparing) : undefined}
      data-escape-scope
      style={{ "--aside-width": `${width}px`, "--aside-min-width": `${minWidth}px` } as React.CSSProperties}
      onTransitionEnd={floating ? fade.onTransitionEnd : dock.onInsetTransitionEnd}
      onPointerEnter={peek?.keep}
      onPointerLeave={peek?.leave}
      className={`flex min-h-0 flex-col ${folded ? "" : floating ? "p-(--peek-gap)" : "ps-(--panel-grip-gap) pe-(--peek-gap) py-(--peek-gap)"} ${
        peek
          ? `pointer-events-none fixed bottom-(--bottom-chrome-height) top-(--top-chrome-height) end-0 z-40 w-(--aside-width) max-w-[92vw] transition-[opacity,visibility] duration-240 ease-[cubic-bezier(0.2,0,0,1)] motion-reduce:transition-none ${dock.insetShown ? "opacity-100" : "opacity-0 invisible"}${dock.preparingInset ? " transition-none" : ""}`
          : folded
            ? "h-full w-0"
            : floating
              ? `pointer-events-none absolute inset-y-0 end-0 z-35 w-(--aside-width) max-w-[calc(100%-var(--rail-width))] [@media(max-width:360px)]:max-w-[calc(100%-24px)] transition-opacity duration-240 ease-[cubic-bezier(0.2,0,0,1)] motion-reduce:transition-none ${fade.shown ? "opacity-100" : "opacity-0"}${fade.preparing ? " transition-none" : ""}`
              : "h-full w-(--aside-width) min-w-(--aside-min-width) shrink"
      }`}
    >
      {typeof children === "function" ? children(peek) : children}
    </div>
  );
}
