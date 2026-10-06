import { useSyncExternalStore } from "react";

import { createPersistedPreference } from "./persistedPreference";

/** The sidebar's width at and above the `docked` breakpoint: what it is until the user resizes
 * it, and what a double-click on its handle returns to. Below the breakpoint the drawer is always
 * this wide. */
export const SIDEBAR_DEFAULT_WIDTH = 280;
export const SIDEBAR_MIN_WIDTH = 200;
export const SIDEBAR_MAX_WIDTH = 480;

/** The panes' floors the sidebar must leave room for (`TerminalPane.tsx`, `ReportPanel.tsx`);
 * the window's 1100px minimum is these two plus the default width. */
const TERMINAL_FLOOR = 520;
const REPORT_PANEL_FLOOR = 300;

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

// The width the user chose, which is never rewritten by the viewport clamp below: a window
// narrowed and widened again gives the chosen width back. `undefined` means the default.
const chosenWidth = createPersistedPreference<number | undefined>(
  "octoboard.sidebarWidth",
  (raw) => {
    const width = Number(raw);
    return Number.isFinite(width) && width > 0 ? clamp(width, SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH) : undefined;
  },
  (width) => (width === undefined ? null : String(width)),
);

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

/** The largest width the viewport allows. Subscribing to this rather than to the viewport itself
 * is what keeps a window resize from re-rendering the caller while the answer is unchanged. */
function maxWidthFor(hasReportPanel: boolean): number {
  const affordable = viewportWidth - TERMINAL_FLOOR - (hasReportPanel ? REPORT_PANEL_FLOOR : 0);
  return clamp(affordable, SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
}

export interface SidebarWidth {
  /** The width to render the sidebar at (docked mode): the chosen width, held back to what the
   * viewport can afford. This is the number anything aligned with the sidebar's edge (such as a
   * top-bar segment) should follow. */
  width: number;
  /** The largest width the current viewport allows, for the handle's `aria-valuemax`. */
  max: number;
  /** Chooses a width (clamped to the bounds and to `max`). It is persisted unless `persist` is
   * false, for a drag that calls this on every pointer move and then `persist()` once. */
  setWidth: (width: number, options?: { persist?: boolean }) => void;
  /** Writes the chosen width to storage. */
  persist: () => void;
  /** Forgets the chosen width, returning to `SIDEBAR_DEFAULT_WIDTH`. */
  reset: () => void;
}

/**
 * The sidebar's docked-mode width: the user's persisted choice (separate `localStorage` entry,
 * every access guarded) clamped at render time so the terminal's floor, and the report panel's
 * when `hasReportPanel`, still hold at the current viewport width. Never goes below
 * `SIDEBAR_MIN_WIDTH`. State is shared module-wide, so every caller sees the same value.
 */
export function useSidebarWidth(hasReportPanel: boolean): SidebarWidth {
  const chosen = chosenWidth.useValue();
  const max = useSyncExternalStore(subscribeToViewport, () => maxWidthFor(hasReportPanel));
  return {
    width: clamp(chosen ?? SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MIN_WIDTH, max),
    max,
    setWidth: (width, options) =>
      chosenWidth.set(Math.round(clamp(width, SIDEBAR_MIN_WIDTH, max)), options),
    persist: chosenWidth.persist,
    reset: () => chosenWidth.set(undefined),
  };
}
