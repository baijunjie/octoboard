import React from "react";

import type { PanePeek } from "./usePaneToggles";

/** How a pane that is a row sibling at and above the `docked` breakpoint is laid out:
 * - `drawer`: a closed-by-default overlay below the breakpoint, a plain row sibling at and above it;
 * - `floating`: the user has hidden the docked pane, which still floats in over the content on
 *   request, so it stays a fixed overlay at every width. */
export type PaneMode = "drawer" | "floating";

/** The DOM ids of the two panes, which the top bar's toggles name in `aria-controls`. */
export const PANE_ID = { sidebar: "sidebar-pane", report: "report-pane" } as const;

/** The classes that differ between the two sides, written out in full: Tailwind emits a utility
 * only for a class name it can find as literal text somewhere in the source, so an interpolated
 * `left-0`/`right-0` would compile to nothing and leave the pane unanchored. */
const SIDE_CLASSES = {
  left: {
    anchor: "left-0",
    closed: "-translate-x-full",
    floatingAway: "docked:-translate-x-full docked:invisible",
  },
  right: {
    anchor: "right-0",
    closed: "translate-x-full",
    floatingAway: "docked:translate-x-full docked:invisible",
  },
};

/**
 * The geometry shared by the sidebar's and the report panel's overlay forms (`Sidebar.tsx`,
 * `ReportPanel.tsx`): fixed below `style.css`'s `--top-chrome-height`, sliding in from `side`.
 * `open` is whether the drawer is open below the breakpoint. Callers add their own
 * width/flex/border classes on top of this string; the slide direction and what "closed" means on
 * either side are this function's job, not theirs.
 *
 * In `drawer` mode `docked:` turns the pane back into a plain row sibling — no fixed positioning,
 * no stacking context, no transition. `docked:translate-none` rather than `docked:translate-x-0`
 * (which still computes to a `translate` value other than `none`) is what keeps it from becoming a
 * stacking context and a containing block for fixed descendants at and above the breakpoint,
 * where the layout must stay exactly as it was before the pane had a `translate` at all.
 *
 * In `floating` mode none of the `docked:` resets apply, and `peeking` takes the place of `open`
 * for whether the pane is on screen there. It is `invisible` while away, so a pane that slid out
 * stays out of the tab order, and only a floating pane carries the shadow, which would otherwise
 * bleed in from the screen edge.
 *
 * Tailwind 4's `translate-x-*` utilities set the CSS `translate` property, not `transform`, so the
 * floating form transitions `translate` (the drawer form's `transition-transform` already lists
 * it). Every form drops its transition under `prefers-reduced-motion: reduce`.
 *
 * HeroUI's `Drawer` is not used: it is modal (everything outside is inert, focus is trapped, its
 * content is portalled and unmounted while closed). The panes have to be non-modal row siblings
 * that are also drawers and hover-peek panes, with the top bar still operable, focus staying on the
 * terminal, and the report page staying mounted.
 */
export function drawerClass(side: "left" | "right", mode: PaneMode, open: boolean, peeking = false): string {
  const { anchor, closed, floatingAway } = SIDE_CLASSES[side];
  const form =
    mode === "drawer"
      ? "transition-transform docked:static docked:z-auto docked:translate-none docked:transition-none"
      : `transition-[translate,visibility] ${peeking ? "docked:translate-x-0 docked:shadow-xl" : floatingAway}`;
  const base = "fixed bottom-0 top-(--top-chrome-height) z-40 duration-200 motion-reduce:transition-none";
  return `${base} ${anchor} ${open ? "translate-x-0" : closed} ${form}`;
}

/**
 * The strip along a window edge that floats a hidden docked pane in when a mouse pointer reaches
 * it; rendered by the pane itself, only while it is hidden and not already out. It starts under
 * the top bar so the bar's own controls stay clear, and reacts to a mouse only, as the toggles
 * in the bar do.
 *
 * It is a bare element because it is not a control: it is `aria-hidden`, a mouse-only hover target
 * with no HeroUI equivalent.
 *
 * The left strip is 8px, easy to reach by pushing the pointer to the edge. The right one is 4px,
 * the width of the terminal's padding, so it stays off xterm's scrollbar.
 */
export function PeekHotZone({
  side,
  peek,
}: {
  side: "left" | "right";
  peek: PanePeek;
}): React.ReactElement | null {
  if (peek.active) return null;
  return (
    <div
      aria-hidden="true"
      className={
        side === "left"
          ? "fixed bottom-0 left-0 top-(--top-chrome-height) z-30 hidden w-2 docked:block"
          : "fixed bottom-0 right-0 top-(--top-chrome-height) z-30 hidden w-1 docked:block"
      }
      onPointerEnter={(event) => event.pointerType === "mouse" && peek.reveal(true)}
    />
  );
}
