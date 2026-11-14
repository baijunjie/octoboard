import React from "react";

import { useT } from "../i18n/react";
import type { PaneSide, PaneWidth } from "./paneWidth";
import { ResizeSeparator } from "../components/panel/ResizeSeparator";

/** What differs between the sides. The sidebar is on the start side and the aside on the end side,
 * so `edge` is the logical side of the window the handle is anchored to, and `grow` is the
 * direction, along the reading direction, that the handle moves in when the pane gets wider: toward
 * the end for the sidebar, whose edge is its end one, toward the start for the aside. The
 * pointer and the arrow keys work in physical directions, so `ResizeSeparator` turns it into
 * +1 (right) or -1 (left) for the direction in force. `offset` is how far the pane's own edge is
 * from the window's: the sidebar starts where the left rail ends, the aside is flush with
 * the window. The docked sidebar's handle sits on the physical right of its seam, clear of either
 * scroller's right-hand scrollbar. The docked aside's handle shares the terminal gutter outside the cards' edge.
 * Floating handles sit in their gap. */
const SIDES = {
  sidebar: {
    label: "pane.resizeSidebar",
    edge: "insetInlineStart",
    offset: "var(--rail-width)",
    grow: 1,
    centre: "0px",
    appearance: "line",
    layout: "hidden sidebar-docked:block w-1.5 translate-x-[3px] [&>div]:ml-0 [&>div]:mr-auto",
  },
  aside: {
    label: "pane.resizeAside",
    edge: "insetInlineEnd",
    offset: "0px",
    grow: -1,
    centre: "calc(var(--panel-grip-gap) - 7px)",
    appearance: "grip",
    layout: "w-3.5",
  },
} as const;

/** The width of the hit area of each side's handle, in px, `layout`'s `w-*`. */
const HIT_WIDTH = { sidebar: 6, aside: 14 } as const;

/**
 * The drag handle on a docked pane's inner edge (the sidebar's end, the aside's start): a
 * `ResizeSeparator` on that edge. It runs the pane's full height, below the top bar and above
 * the connection banner, so the line covers the whole edge. It is
 * rendered right after its pane, so Tab reaches it next to the pane, but it is positioned against the
 * window for a docked pane: its nearest positioned ancestor must be the app root. A floating
 * group carries the handle inside its own frame, so the hit area follows its actual edge and is
 * part of its pointer watch. Mobile interaction has no width handle.
 */
export function PaneResizeHandle({
  side,
  paneWidth,
  label: ownLabel,
  region,
  floating = false,
}: {
  side: PaneSide;
  /** Positioned inside a floating pane so its hit area moves with the resized edge. */
  floating?: boolean;
  paneWidth: PaneWidth;
  /** The handle's accessible name, when the caller has a better one than the side's. */
  label?: string;
  /** `data-region` of the handle: F6 from here moves on from that region, as from a control in it. */
  region?: string;
}): React.ReactElement {
  const t = useT();
  const { width, min, max, setWidth, persist, reset } = paneWidth;
  const { label, edge, offset, grow, centre, appearance: dockedAppearance, layout } = SIDES[side];
  const appearance = floating ? "grip" : dockedAppearance;
  return (
    <ResizeSeparator
      orientation="vertical"
      value={width}
      min={min}
      max={max}
      label={ownLabel ?? t(label)}
      grow={grow}
      onChange={(next, options) => setWidth(next, { persist: options?.commit !== false })}
      onCommit={persist}
      onReset={reset}
      pane={side}
      region={region ?? side}
      appearance={appearance}
      // Align the outer grip with the column grip, 3px from the card, without moving either target.
      gripInlineOffset={side === "aside" ? floating ? -1 : 2 : undefined}
      tooltip={appearance === "grip" ? t("pane.resize") : undefined}
      tooltipPlacement={side === "sidebar" ? "start" : "end"}
      // Docked handles follow the pane's logical inner edge; floating handles straddle its gap.
      style={floating ? side === "sidebar" ? { insetInlineEnd: "calc((var(--panel-grip-gap) + 24px) / -2)" } : { insetInlineStart: "calc((var(--panel-grip-gap) - 24px) / 2)" } : { [edge]: `calc(${width - HIT_WIDTH[side] / 2}px - ${centre} + ${offset})` }}
      className={floating ? "absolute inset-y-0 z-50 w-6 pointer-events-auto" : `absolute top-(--top-chrome-height) bottom-(--bottom-chrome-height) z-10 ${layout}`}
    />
  );
}
