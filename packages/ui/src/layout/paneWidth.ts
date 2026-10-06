import { useSyncExternalStore } from "react";

import { createPersistedPreference } from "./persistedPreference";

/** The two panes whose docked width the user can change. */
export type PaneSide = "sidebar" | "report";

/** Per pane, at and above the `docked` breakpoint: the width it has until the user resizes it (what
 * a double-click on its handle returns to) and the bounds of a chosen width. Below the breakpoint
 * each is a fixed-width drawer instead. */
const PANES = {
  sidebar: { key: "octoboard.sidebarWidth", min: 200, default: 280, max: 480 },
  report: { key: "octoboard.reportWidth", min: 300, default: 420, max: 720 },
} satisfies Record<PaneSide, { key: string; min: number; default: number; max: number }>;

/** The terminal pane's floor (`TerminalPane.tsx`); the window's 1100px minimum is this plus the
 * sidebar's default and the report panel's minimum. */
const TERMINAL_FLOOR = 520;

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

function createChosenWidth(side: PaneSide) {
  const { key, min, max } = PANES[side];
  // The width the user chose, never rewritten by the viewport clamp below: a window narrowed and
  // widened again gives the chosen width back. `undefined` means the default.
  return createPersistedPreference<number | undefined>(
    key,
    (raw) => {
      const width = Number(raw);
      return Number.isFinite(width) && width > 0 ? clamp(width, min, max) : undefined;
    },
    (width) => (width === undefined ? null : String(width)),
  );
}

const chosenWidths = { sidebar: createChosenWidth("sidebar"), report: createChosenWidth("report") };

// The viewport width as of the last `resize`, so reading it never forces a layout.
let viewportWidth = document.documentElement.clientWidth;

function subscribeToViewport(listener: () => void): () => void {
  const onResize = () => {
    viewportWidth = document.documentElement.clientWidth;
    listener();
  };
  onResize();
  window.addEventListener("resize", onResize);
  return () => window.removeEventListener("resize", onResize);
}

/** Which of the two panes take part in the row at the moment; a hidden pane floats over the
 * content instead, so it neither takes space nor leaves any. */
export interface DockedPanes {
  sidebar: boolean;
  report: boolean;
}

/**
 * The one rule for sharing the row between the panes, so both clamps agree at every viewport width:
 * the terminal always keeps `TERMINAL_FLOOR`, and when space is short the report panel gives up
 * width first, down to its minimum. The sidebar therefore only ever assumes the report panel's
 * minimum (not its chosen width), and the report panel gets what is left after the sidebar's
 * rendered width. A pane that is not docked is not limited by the viewport (it overlays the
 * terminal). At 1100px with both docked: sidebar 280, terminal 520, report 300.
 */
function maxWidthFor(side: PaneSide, docked: DockedPanes, chosenSidebar: number | undefined): number {
  const { min, max } = PANES[side];
  if (!docked[side]) return max;
  if (side === "sidebar") {
    const reportReserve = docked.report ? PANES.report.min : 0;
    return clamp(viewportWidth - TERMINAL_FLOOR - reportReserve, min, max);
  }
  const sidebar = PANES.sidebar;
  const sidebarWidth = docked.sidebar
    ? clamp(chosenSidebar ?? sidebar.default, sidebar.min, maxWidthFor("sidebar", docked, chosenSidebar))
    : 0;
  return clamp(viewportWidth - TERMINAL_FLOOR - sidebarWidth, min, max);
}

export interface PaneWidth {
  /** The width to render the pane at: the chosen width, held back to what the viewport can afford
   * while the pane is docked. This is the number anything aligned with the pane's edge (such as a
   * top-bar segment) should follow. */
  width: number;
  /** The pane's smallest width, for the handle's `aria-valuemin`. */
  min: number;
  /** The largest width the current viewport allows, for the handle's `aria-valuemax`. */
  max: number;
  /** Chooses a width (clamped to the bounds and to `max`). It is persisted unless `persist` is
   * false, for a drag that calls this on every pointer move and then `persist()` once. */
  setWidth: (width: number, options?: { persist?: boolean }) => void;
  /** Writes the chosen width to storage. */
  persist: () => void;
  /** Forgets the chosen width, returning to the pane's default. */
  reset: () => void;
}

/**
 * One pane's docked-mode width: the user's persisted choice (a separate `localStorage` entry per
 * side, every access guarded) clamped at render time by the rule at `maxWidthFor`, never below the
 * pane's minimum. State is shared module-wide, so every caller sees the same value.
 */
export function usePaneWidth(side: PaneSide, docked: DockedPanes): PaneWidth {
  const chosen = chosenWidths[side].useValue();
  const chosenSidebar = chosenWidths.sidebar.useValue();
  // Subscribing to the answer rather than to the viewport itself is what keeps a window resize
  // from re-rendering the caller while the answer is unchanged.
  const max = useSyncExternalStore(subscribeToViewport, () => maxWidthFor(side, docked, chosenSidebar));
  const { min, default: defaultWidth } = PANES[side];
  const preference = chosenWidths[side];
  return {
    width: clamp(chosen ?? defaultWidth, min, max),
    min,
    max,
    setWidth: (width, options) => preference.set(Math.round(clamp(width, min, max)), options),
    persist: preference.persist,
    reset: () => preference.set(undefined),
  };
}
