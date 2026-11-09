import { useState } from "react";

import type { AsideLayout } from "../layout/AsidePane";
import type { DockPhase } from "../layout/paneDockMotion";
import { usePaneWidth } from "../layout/paneWidth";
import type { PaneToggles } from "../layout/usePaneToggles";
import { MIN_PANEL_COLUMN_WIDTH, MIN_SINGLE_COLUMN_ASIDE_WIDTH, MIN_TWO_COLUMN_ASIDE_WIDTH, PANEL_ASIDE_INSET, PANEL_COLUMN_GAP } from "../layout/panelColumnWidth";
import { columnSplit, panelColumns } from "../panels/panelColumns";
import type { PanelKind } from "../layout/panelKind";

/**
 * The widths of the sidebar's column and of the right pane, and how the right pane is laid out:
 * docked, folded with a panel floating in a pane of its own, or floating over the agent terminal.
 * The sidebar column's dock phase is set by the sidebar as its ease runs (`setSidebarColumnPhase`).
 */
export function usePaneWidths({
  noConsoles,
  isNarrow,
  sidebarDocked,
  stackedPanelCount,
  stackedPanelKinds,
  columnShare,
  panelWidthKey,
  maximized = false,
  asideOverlay,
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
  stackedPanelCount: number;
  stackedPanelKinds?: readonly string[];
  columnShare?: number;
  panelWidthKey?: string;
  maximized?: boolean;
  asideOverlay: boolean;
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
  // widen and reflow on the first frame. A sidebar that cannot fit floats and takes no room from the aside beside it.
  const asideMin = !asideFloats && stackedPanelCount > 2 ? MIN_TWO_COLUMN_ASIDE_WIDTH : MIN_SINGLE_COLUMN_ASIDE_WIDTH;
  const sidebarWidth = usePaneWidth("sidebar", {
    sidebar: !noConsoles && !isNarrow && (sidebarDocked || sidebarColumnPhase !== "closed"),
    aside: asideDocked,
  }, asideMin);
  const [allocation, setAllocation] = useState<{
    kinds: readonly string[];
    columns: string[][];
    width: number;
    preferredWidth: number;
    adjustment: number;
    share: number | undefined;
    mobile: boolean;
    key: string | undefined;
    maximized: boolean;
    keepPairColumns: boolean;
    pair: { width: number; kinds: string } | undefined;
  }>();
  const asideWidth = usePaneWidth("aside", {
    sidebar: !noConsoles && !isNarrow && sidebarDocked,
    aside: asideDocked,
  }, asideMin, allocation?.adjustment ?? 0);
  const kinds = stackedPanelKinds ?? Array.from({ length: stackedPanelCount }, (_, index) => String(index));
  const contentWidth = asideWidth.width - PANEL_ASIDE_INSET;
  const columns = panelColumns(kinds, contentWidth, PANEL_COLUMN_GAP);
  const preferredChanged = allocation !== undefined && allocation.preferredWidth !== asideWidth.preferredWidth;
  const changed = allocation !== undefined && allocation.kinds.join("\0") !== kinds.join("\0");
  const keepPairColumns = allocation?.key !== panelWidthKey ? false : allocation && changed && !maximized && !allocation.maximized && allocation.key === panelWidthKey && !asideFloats && !allocation.mobile && allocation.kinds.length === 2 && kinds.length !== 2
    ? allocation.columns.length === 2
    : allocation?.keepPairColumns ?? false;
  // A saved split may be clamped at the column minimum; its total width restores that exact
  // pair until the remaining card is resized. Inverting the saved proportion would widen it.
  const pair = allocation?.key !== panelWidthKey || preferredChanged || kinds.length === 0 ? undefined
    : allocation && changed && !maximized && !allocation.maximized && !asideFloats && !allocation.mobile && allocation.kinds.length === 2
      ? kinds.length === 1 && allocation.columns.length === 2 ? { width: allocation.width, kinds: allocation.kinds.join("\0") } : undefined
      : allocation?.pair;
  let adjustment = preferredChanged ? 0 : allocation?.adjustment ?? 0;
  // Changing the visible cards adds or removes width, including when a new owner has no report.
  // A viewport clamp, a hover preview and maximizing a card leave the chosen allocation
  // available when the full group returns.
  if (allocation && changed && !preferredChanged && !asideFloats && !allocation.mobile && !maximized && !allocation.maximized && kinds.length > 0 && allocation.kinds.length > 0) {
    const oldCount = allocation.columns.length;
    const nextCount = kinds.length > 2 || (kinds.length === 2 && keepPairColumns) ? 2 : kinds.length === 1 || allocation.kinds.length > 2 ? 1 : columns.length;
    if (oldCount !== nextCount) {
      const retained = kinds.find((kind) => allocation.kinds.includes(kind));
      if (retained !== undefined) {
        const oldColumn = allocation.columns.findIndex((column) => column.includes(retained));
        const oldSplit = columnSplit(allocation.width - PANEL_ASIDE_INSET, PANEL_COLUMN_GAP, allocation.share);
        const oldWidth = oldCount === 1 ? allocation.width - PANEL_ASIDE_INSET : oldColumn === 0 ? oldSplit.leading : oldSplit.available - oldSplit.leading;
        const nextColumns = panelColumns(kinds, MIN_TWO_COLUMN_ASIDE_WIDTH - PANEL_ASIDE_INSET, PANEL_COLUMN_GAP);
        const nextColumn = nextColumns.findIndex((column) => column.includes(retained));
        const share = columnShare !== undefined && Number.isFinite(columnShare) && columnShare > 0 && columnShare < 1 ? columnShare : 0.5;
        // At the column floor, two minimum columns preserve the card despite any saved split.
        const desired = nextCount === 1 ? oldWidth + PANEL_ASIDE_INSET : kinds.length === 2 && allocation.kinds.length === 1 && pair !== undefined && pair.kinds === kinds.join("\0") ? pair.width : oldWidth === MIN_PANEL_COLUMN_WIDTH ? MIN_TWO_COLUMN_ASIDE_WIDTH : Math.max(oldWidth + MIN_PANEL_COLUMN_WIDTH, oldWidth / (nextColumn === 0 ? share : 1 - share)) + PANEL_COLUMN_GAP + PANEL_ASIDE_INSET;
        adjustment = desired - asideWidth.preferredWidth;
      }
    }
  }
  // Closing an ordinary single column leaves no width or column history to update. Avoid a
  // render just to record its absence; a saved pair still needs clearing when every card closes.
  const canSkipEmpty = kinds.length === 0 && allocation && allocation.pair === undefined && !allocation.keepPairColumns && allocation.columns.length <= 1
    && allocation.key === panelWidthKey && allocation.mobile === asideFloats && allocation.maximized === maximized && allocation.share === columnShare
    && allocation.preferredWidth === asideWidth.preferredWidth && allocation.width === asideWidth.width && allocation.adjustment === adjustment;
  if (!canSkipEmpty && (!allocation || changed || preferredChanged || allocation.width !== asideWidth.width || allocation.share !== columnShare || allocation.mobile !== asideFloats || allocation.key !== panelWidthKey || allocation.maximized !== maximized || allocation.adjustment !== adjustment)) {
    setAllocation({ kinds, columns, width: asideWidth.width, preferredWidth: asideWidth.preferredWidth, adjustment, share: columnShare, mobile: asideFloats, key: panelWidthKey, maximized, keepPairColumns, pair });
  }
  const asideLayout: AsideLayout = {
    width: asideWidth.width,
    minWidth: asideWidth.min,
    keepMounted: asideOverlay && paneShown,
    peek: paneShown && !asideOverlay ? undefined : asidePeek,
    folded: paneShown && !columnShown && !asideOverlay,
    overlay: asideFloats,
    floatingKind,
  };
  // While the column is folded, a panel floats in a pane of its own (`RightPane`).
  const asideFloat = asideLayout.folded ? asidePeek : undefined;
  return {
    sidebarColumnPhase, setSidebarColumnPhase, sidebarWidth,
    asideWidth: {
      ...asideWidth,
      setWidth: (width: number, options?: { persist?: boolean }) => {
        setAllocation((current) => current && (current.adjustment !== 0 || current.pair !== undefined) ? { ...current, adjustment: 0, pair: undefined } : current);
        asideWidth.setWidth(width, options);
      },
      reset: () => {
        setAllocation((current) => current && (current.adjustment !== 0 || current.pair !== undefined) ? { ...current, adjustment: 0, pair: undefined } : current);
        asideWidth.reset();
      },
    },
    asideLayout, asideFloat,
  };
}
