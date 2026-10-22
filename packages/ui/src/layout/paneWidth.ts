import { useState, useSyncExternalStore } from "react";

import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { useMediaQuery } from "./breakpoint";

/** The two panes whose docked width the user can change. */
export type PaneSide = "sidebar" | "aside";

/** Per pane, at and above the `docked` breakpoint: the width it has until the user resizes it (what
 * a double-click on its handle returns to) and the bounds of a chosen width. Below the breakpoint
 * each is a fixed-width drawer instead. */
const PANES = {
  sidebar: { key: PREFERENCE_KEYS.sidebarWidth, min: 280, default: 280, max: 480 },
  aside: { key: PREFERENCE_KEYS.asideWidth, min: 320, default: 420, max: 720 },
} satisfies Record<PaneSide, { key: string; min: number; default: number; max: number }>;

/** The terminal pane's floor (the terminal's wrapper in `App.tsx`); the window's 1168px minimum is
 * this plus the rail, the sidebar's default and the aside's minimum. Below the `docked` breakpoint
 * the sidebar is a drawer and the floor drops to `NARROW_TERMINAL_FLOOR`, which the docked aside
 * shares the row with until the window is too narrow for both (`useAsideFloats`). */
const TERMINAL_FLOOR = 520;
const NARROW_TERMINAL_FLOOR = 398;

/** The aside's floor below the `docked` breakpoint while the Terminal panel is in its column: a
 * narrow Shell header reserves room for a normal tab, its focus ring and every action. */
export const NARROW_TERMINAL_PANEL_ASIDE_MIN = 336;

/** The terminal's floor in a window narrower than the `docked` breakpoint or not. */
function terminalFloor(narrow: boolean): number {
  return narrow ? NARROW_TERMINAL_FLOOR : TERMINAL_FLOOR;
}

/** Memoised once resolved: the rail's width cannot change at runtime. */
let railWidthPxCache: number | undefined;

/** The left rail's width, which the panes' row does not have, read back out of `--rail-width` in
 * `style.css` rather than kept as a second literal here (as `breakpoint.ts` does for the
 * breakpoint). Throws if the stylesheet has not loaded yet, rather than guessing a width the
 * layout then disagrees with. Reached only from `usePaneWidth`'s state initialiser and from
 * `useAsideFloats`'s media query (`useMediaQuery`), never from the snapshot read that runs on every render,
 * so that throw comes from a call site prepared for it. */
function railWidthPx(): number {
  if (railWidthPxCache !== undefined) return railWidthPxCache;
  const raw = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--rail-width"));
  if (!Number.isFinite(raw)) throw new Error("--rail-width is not set; is style.css loaded?");
  railWidthPxCache = raw;
  return raw;
}

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

const chosenWidths = { sidebar: createChosenWidth("sidebar"), aside: createChosenWidth("aside") };

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
  aside: boolean;
}

/**
 * The one rule for sharing the row between the panes, so both clamps agree at every viewport width:
 * the terminal always keeps its floor, and when space is short the aside gives up
 * width first, down to its minimum. The sidebar therefore only ever assumes the aside's
 * minimum (not its chosen width), and the aside gets what is left after the sidebar's
 * rendered width. A pane that is not docked is not limited by the viewport (it overlays the
 * terminal). At 1168px with both docked: rail 48, sidebar 280, terminal 520, aside 320.
 */
function maxWidthFor(side: PaneSide, docked: DockedPanes, narrow: boolean, chosenSidebar: number | undefined, railWidth: number, asideMin: number): number {
  const { max } = PANES[side];
  const min = side === "aside" ? asideMin : PANES.sidebar.min;
  if (!docked[side]) return max;
  if (side === "sidebar") {
    const asideReserve = docked.aside ? asideMin : 0;
    return clamp(viewportWidth - railWidth - terminalFloor(narrow) - asideReserve, min, max);
  }
  const sidebar = PANES.sidebar;
  const sidebarWidth = docked.sidebar
    ? clamp(chosenSidebar ?? sidebar.default, sidebar.min, maxWidthFor("sidebar", docked, narrow, chosenSidebar, railWidth, asideMin))
    : 0;
  return clamp(viewportWidth - railWidth - terminalFloor(narrow) - sidebarWidth, min, max);
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
 * pane's minimum. `narrow` is whether the window is below the `docked` breakpoint (`useIsNarrow`).
 * `asideMin` is the column's effective floor; both panes must receive the same value. It changes
 * allocation only, leaving the persisted choice available when the column's contents change.
 * State is shared module-wide, so every caller sees the same value.
 */
export function usePaneWidth(side: PaneSide, docked: DockedPanes, narrow: boolean, asideMin = PANES.aside.min): PaneWidth {
  const chosen = chosenWidths[side].useValue();
  const chosenSidebar = chosenWidths.sidebar.useValue();
  // Subscribing to the answer rather than to the viewport itself is what keeps a window resize
  // from re-rendering the caller while the answer is unchanged.
  const [railWidth] = useState(railWidthPx);
  const max = useSyncExternalStore(subscribeToViewport, () => maxWidthFor(side, docked, narrow, chosenSidebar, railWidth, asideMin));
  const { default: defaultWidth } = PANES[side];
  const min = side === "aside" ? asideMin : PANES.sidebar.min;
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

/** The narrowest window that docks the aside: the rail, the terminal's narrow floor and the aside's
 * largest floor side by side. One width whichever panels are open, so opening the Terminal panel
 * never moves the pane from beside the terminal to over it. */
function asideDocksQuery(): string {
  return `(min-width: ${railWidthPx() + NARROW_TERMINAL_FLOOR + NARROW_TERMINAL_PANEL_ASIDE_MIN}px)`;
}

/**
 * Whether the window is too narrow to dock the aside beside the terminal (a phone): its open panels
 * then float over the agent terminal's area instead of taking room from it, and the terminal keeps
 * the whole width with no floor that would scroll the row. A media query rather than the viewport
 * store above, so a resize re-renders only when the answer changes.
 */
export function useAsideFloats(): boolean {
  return !useMediaQuery(asideDocksQuery);
}
