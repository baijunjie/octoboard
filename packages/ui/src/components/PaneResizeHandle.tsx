import React from "react";

import { useT } from "../i18n/react";
import type { PaneSide, PaneWidth } from "../layout/paneWidth";
import { ResizeSeparator } from "./ResizeSeparator";

/** What differs between the sides. The sidebar is on the start side and the aside on the end side,
 * so `edge` is the logical side of the window the handle is anchored to, and `grow` is the
 * direction, along the reading direction, that the handle moves in when the pane gets wider: toward
 * the end for the sidebar, whose edge is its end one, toward the start for the aside. The
 * pointer and the arrow keys work in physical directions, so `ResizeSeparator` turns it into
 * +1 (right) or -1 (left) for the direction in force. `offset` is how far the pane's own edge is
 * from the window's: the sidebar starts where the left rail ends, the aside is flush with
 * the window. The sidebar's edge is a line on a seam; the aside's is the gap its cards leave
 * (`--panel-grip-gap`), so the handle is centred in that gap (`centre` inside the edge) and draws a grip. */
const SIDES = {
  sidebar: {
    label: "pane.resizeSidebar",
    edge: "insetInlineStart",
    offset: "var(--rail-width)",
    grow: 1,
    centre: "0px",
    appearance: "line",
    layout: "hidden docked:block w-1.5",
  },
  aside: {
    label: "pane.resizeAside",
    edge: "insetInlineEnd",
    offset: "0px",
    grow: -1,
    centre: "calc(var(--panel-grip-gap) / 2)",
    appearance: "grip",
    layout: "w-6",
  },
} as const;

/** The width of the hit area of each side's handle, in px, `layout`'s `w-*`. */
const HIT_WIDTH = { sidebar: 6, aside: 24 } as const;

/**
 * The drag handle on a docked pane's inner edge (the sidebar's end, the aside's start): a
 * `ResizeSeparator` on that edge. It runs the pane's full height, below the top bar and above
 * the connection banner, so the line covers the whole edge. It is
 * rendered right after its pane, so Tab reaches it next to the pane, but it is positioned against the
 * window: the nearest positioned ancestor must be the app root, as tall as the window, and no element
 * between them may be positioned. `App.tsx` renders it only while its pane is docked and shown. The
 * sidebar's is hidden below the `docked` breakpoint, where that pane is a fixed-width drawer; the
 * aside's is there wherever the aside docks, and gone in a window too narrow for that, where its
 * panel floats over the terminal instead.
 */
export function PaneResizeHandle({
  side,
  paneWidth,
  label: ownLabel,
  region,
}: {
  side: PaneSide;
  paneWidth: PaneWidth;
  /** The handle's accessible name, when the caller has a better one than the side's. */
  label?: string;
  /** `data-region` of the handle: F6 from here moves on from that region, as from a control in it. */
  region?: string;
}): React.ReactElement {
  const t = useT();
  const { width, min, max, setWidth, persist, reset } = paneWidth;
  const { label, edge, offset, grow, centre, appearance, layout } = SIDES[side];
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
      tooltip={appearance === "grip" ? t("pane.resize") : undefined}
      // Centred on the pane's inner edge, or in the gap it leaves, anchored to the logical side the
      // pane is on.
      style={{ [edge]: `calc(${width - HIT_WIDTH[side] / 2}px - ${centre} + ${offset})` }}
      className={`absolute top-(--top-chrome-height) bottom-(--bottom-chrome-height) z-10 ${layout}`}
    />
  );
}
