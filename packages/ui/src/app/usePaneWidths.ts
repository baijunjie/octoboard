import { useState } from "react";

import type { AsideLayout } from "../layout/AsidePane";
import type { DockPhase } from "../layout/paneDockMotion";
import { NARROW_TERMINAL_PANEL_ASIDE_MIN, usePaneWidth } from "../layout/paneWidth";
import type { PaneToggles } from "../layout/usePaneToggles";
import type { PanelKind } from "../panels/stackState";

/**
 * The widths of the sidebar's column and of the right pane, and how the right pane is laid out:
 * docked, folded with a panel floating in a pane of its own, or floating over the agent terminal.
 * The sidebar column's dock phase is set by the sidebar as its ease runs (`setSidebarColumnPhase`).
 */
export function usePaneWidths({
  noConsoles,
  isNarrow,
  sidebarDocked,
  terminalPanelStacked,
  asideDocked,
  paneShown,
  columnShown,
  asideFloats,
  asidePeek,
  floatingKind,
}: {
  noConsoles: boolean;
  isNarrow: boolean;
  sidebarDocked: boolean;
  /** The Terminal panel is one of those in the right pane's column. */
  terminalPanelStacked: boolean;
  asideDocked: boolean;
  paneShown: boolean;
  columnShown: boolean;
  asideFloats: boolean;
  asidePeek: PaneToggles["asidePeek"];
  floatingKind: PanelKind | undefined;
}) {
  // The sidebar's handle sits at its column's settled edge. During the ease that edge is still
  // moving, and the handle's own position is the settled width, so it stays off until the ease has
  // finished. The aside does not ease.
  const [sidebarColumnPhase, setSidebarColumnPhase] = useState<DockPhase>(sidebarDocked ? "open" : "closed");
  // A closing sidebar column keeps its width clamp until it leaves the row, so its content cannot
  // widen and reflow on the first frame. The sidebar is a column in the row only at and above the
  // breakpoint, where it is docked; below it, a drawer, which takes no room from the aside beside it.
  const asideMin = isNarrow && terminalPanelStacked ? NARROW_TERMINAL_PANEL_ASIDE_MIN : 320;
  const sidebarWidth = usePaneWidth("sidebar", {
    sidebar: !noConsoles && !isNarrow && (sidebarDocked || sidebarColumnPhase !== "closed"),
    aside: asideDocked,
  }, isNarrow, asideMin);
  const asideWidth = usePaneWidth("aside", {
    sidebar: !noConsoles && !isNarrow && sidebarDocked,
    aside: asideDocked,
  }, isNarrow, asideMin);
  const asideLayout: AsideLayout = {
    width: asideWidth.width,
    minWidth: asideWidth.min,
    peek: paneShown ? undefined : asidePeek,
    folded: paneShown && !columnShown,
    overlay: asideFloats,
    floatingKind,
  };
  // While the column is folded, a panel floats in a pane of its own (`RightPane`).
  const asideFloat = asideLayout.folded ? asidePeek : undefined;
  return { sidebarColumnPhase, setSidebarColumnPhase, sidebarWidth, asideWidth, asideLayout, asideFloat };
}
