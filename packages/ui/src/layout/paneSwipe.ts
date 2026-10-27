import type { PanelKind } from "../panels/stackState";

/** The side of the window a swipe moves towards, in the reading direction: `end` is rightwards in
 * left-to-right text and leftwards in right-to-left. */
export type SwipeToward = "start" | "end";

/** What a swipe does, once `resolveSwipe` has looked at what is on screen. */
export type SwipeAction =
  | { type: "open-drawer" }
  | { type: "close-drawer" }
  | { type: "close-panel" }
  | { type: "open-panel"; kind: PanelKind };

export interface SwipeScene {
  /** The sidebar's drawer exists at all (it does not while there is no console). */
  hasDrawer: boolean;
  drawerOpen: boolean;
  /** The kinds the right pane's owner toggles, in the order of its toggles; empty with no owner. */
  panelKinds: readonly PanelKind[];
  /** The panel floating over the terminal now, if one is. */
  panel: PanelKind | undefined;
  /** The panel the owner had open last, if it ever had one. */
  lastPanel: PanelKind | undefined;
}

/**
 * What a swipe does in a window too narrow to dock the right pane, where the sidebar's drawer
 * comes from the start side and the owner's panels from the end side, one on screen at a time.
 * Towards the end it closes the panel floating there, else opens the drawer; towards the start it
 * closes the drawer, else opens the panel the owner had open last (its first, when it had none) or,
 * with one already open, the next in the order of its toggles, the last wrapping to the first. A
 * side with nothing behind it does nothing.
 */
export function resolveSwipe(toward: SwipeToward, scene: SwipeScene): SwipeAction | undefined {
  const { hasDrawer, drawerOpen, panelKinds, panel, lastPanel } = scene;
  if (toward === "end") {
    if (panel !== undefined) return { type: "close-panel" };
    return hasDrawer && !drawerOpen ? { type: "open-drawer" } : undefined;
  }
  if (drawerOpen) return { type: "close-drawer" };
  if (panelKinds.length === 0) return undefined;
  if (panel === undefined) {
    const kind = lastPanel !== undefined && panelKinds.includes(lastPanel) ? lastPanel : panelKinds[0]!;
    return { type: "open-panel", kind };
  }
  const next = panelKinds[(panelKinds.indexOf(panel) + 1) % panelKinds.length]!;
  return next === panel ? undefined : { type: "open-panel", kind: next };
}
