import React, { useEffect, useRef } from "react";

import type { PanePeek } from "./usePaneToggles";

/** How a pane that is a row sibling at and above the `docked` breakpoint is laid out:
 * - `drawer`: a closed-by-default overlay below the breakpoint, a plain row sibling at and above it;
 * - `floating`: the user has hidden the docked pane, which still floats in over the content on
 *   request, so it stays a fixed overlay at every width. */
export type PaneMode = "drawer" | "floating";

/** The DOM ids of the two panes, which the sidebar and report toggles name in `aria-controls`. */
export const PANE_ID = { sidebar: "sidebar-pane", report: "report-pane" } as const;

/** The edge a pane sits on: the reading direction's start or end. */
export type PaneEdge = "start" | "end";

/** The two sides are the reading direction's start and end, so under right-to-left the sidebar is on
 * the right. The classes that differ between them are written out in full: Tailwind emits a utility
 * only for a class name it can find as literal text somewhere in the source, so an interpolated
 * `start-0`/`end-0` would compile to nothing and leave the pane unanchored. `translate-x-*` is
 * physical, so the slide direction under `rtl:` is the opposite one. The start side is anchored
 * past the left rail (`--rail-width` in `style.css`), so it slides that much further to get off
 * screen entirely instead of going over the rail. */
const SIDE_CLASSES = {
  start: {
    anchor: "start-(--rail-width)",
    closed: "-translate-x-[calc(100%+var(--rail-width))] rtl:translate-x-[calc(100%+var(--rail-width))]",
    floatingAway:
      "docked:-translate-x-[calc(100%+var(--rail-width))] docked:rtl:translate-x-[calc(100%+var(--rail-width))] docked:invisible",
  },
  end: {
    anchor: "end-0",
    closed: "translate-x-full rtl:-translate-x-full",
    floatingAway: "docked:translate-x-full docked:rtl:-translate-x-full docked:invisible",
  },
};

/**
 * The geometry shared by the sidebar's and the report panel's overlay forms (`Sidebar.tsx`,
 * `ReportPanel.tsx`): fixed between `style.css`'s `--top-chrome-height` and
 * `--bottom-chrome-height`, sliding in from `side`; the start side's edge is the rail's.
 * `open` is whether the drawer is open below the breakpoint. Callers add their own
 * width/flex/border classes on top of this string; the slide direction and what "closed" means on
 * either side are this function's job, not theirs.
 *
 * In `drawer` mode `docked:` turns the pane back into a plain row sibling — no fixed positioning,
 * no stacking context, no transition. `docked:translate-none` rather than `docked:translate-x-0`
 * (which still computes to a `translate` value other than `none`) is what keeps it from becoming a
 * stacking context and a containing block for fixed descendants at and above the breakpoint,
 * where the layout must stay exactly as it was before the pane had a `translate` at all. The
 * `rtl:` slide classes win over a plain `docked:` one, so the reset has an `rtl:` twin.
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
 * that are also drawers and hover-peek panes, with the window chrome (the top bar and the rail)
 * still operable, focus staying on the terminal, and the report page staying mounted.
 */
export function drawerClass(side: PaneEdge, mode: PaneMode, open: boolean, peeking = false): string {
  const { anchor, closed, floatingAway } = SIDE_CLASSES[side];
  const form =
    mode === "drawer"
      ? "transition-transform docked:static docked:z-auto docked:translate-none docked:rtl:translate-none docked:transition-none"
      : `transition-[translate,visibility] ${peeking ? "docked:translate-x-0 docked:rtl:translate-x-0 docked:shadow-xl" : floatingAway}`;
  const base = "fixed bottom-(--bottom-chrome-height) top-(--top-chrome-height) z-40 duration-200 motion-reduce:transition-none";
  return `${base} ${anchor} ${open ? "translate-x-0" : closed} ${form}`;
}

/** How long a mouse pointer has to stay on the hot zone before the pane floats in. */
const PEEK_DWELL_MS = 200;

/** Whether the pointer is off a pane at the place the pane is sliding to. The element under the
 * pointer cannot say, as the pane is still on its way in and the content shows where it is going to
 * be; `offsetLeft` is the layout position, which a `translate` does not move. It is a position in
 * the viewport, comparable with `clientX`, only because a floating pane is `position: fixed` with no
 * transformed ancestor, so it has no offset parent. */
function outsidePane(pane: Element | null, event: PointerEvent): boolean {
  if (!(pane instanceof HTMLElement)) return true;
  const { top, bottom } = pane.getBoundingClientRect();
  return (
    event.clientX < pane.offsetLeft ||
    event.clientX > pane.offsetLeft + pane.offsetWidth ||
    event.clientY < top ||
    event.clientY > bottom
  );
}

/**
 * The strip along a window edge that floats a hidden docked pane in when a mouse pointer stays on
 * it; rendered by the pane itself, only while it is hidden and not already out. It starts under
 * the top bar and ends above the connection banner, so their controls stay clear, and on the start
 * side it begins at the rail's edge, not the window's. It reacts to a mouse only, as the toggles do.
 *
 * The start strip lies between the rail and the content, where the pointer passes on its way from
 * a rail control to the terminal, so the pane waits `PEEK_DWELL_MS` before floating in: a pointer
 * crossing the strip does not make a hidden sidebar flicker in, one resting on the edge does. The
 * toggles that float a pane in do so at once, as they are aimed at. Floating in arms no slide-away
 * timer, since the pointer is expected to be on the pane (which may not report entering it while
 * the pointer is still, and slides in under it), whose leaving then starts the timer. A pointer that
 * moves off the strip into the content before the pane has reached it would never leave the pane,
 * so the first pointer move after floating in is looked at: one that lands outside the pane's
 * final extent counts as leaving it, one that lands within it is left to the pane's own events.
 *
 * It is a bare element because it is not a control: it is `aria-hidden`, a mouse-only hover target
 * with no HeroUI equivalent.
 *
 * The strip on the window's physical right is 4px, inside the terminal's horizontal padding, so it
 * stays off xterm's scrollbar, which is on the right whatever the direction; the one on the left is
 * 8px, easy to reach by pushing the pointer to the edge. That is the start strip under left-to-right
 * and the end strip under right-to-left, hence the physical widths.
 */
export function PeekHotZone({
  side,
  peek,
}: {
  side: PaneEdge;
  peek: PanePeek;
}): React.ReactElement | null {
  const dwell = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // The dwell's callback outlives the render that scheduled it, and `peek` is rebuilt every render.
  const latest = useRef(peek);
  latest.current = peek;
  // Stops watching the pointer for its first move after the pane has floated in.
  const unwatch = useRef<(() => void) | undefined>(undefined);
  const watchFirstMove = () => {
    unwatch.current?.();
    const pane = side === "start" ? "sidebar" : "report";
    const onMove = (event: PointerEvent) => {
      unwatch.current?.();
      if (outsidePane(document.querySelector(`[data-pane=${pane}]`), event)) latest.current.leave();
    };
    window.addEventListener("pointermove", onMove, { capture: true });
    unwatch.current = () => {
      window.removeEventListener("pointermove", onMove, { capture: true });
      unwatch.current = undefined;
    };
  };
  useEffect(() => {
    if (!peek.active) unwatch.current?.();
  }, [peek.active]);
  useEffect(
    () => () => {
      clearTimeout(dwell.current);
      unwatch.current?.();
    },
    [],
  );
  if (peek.active) return null;
  return (
    <div
      aria-hidden="true"
      className={
        side === "start"
          ? "fixed bottom-(--bottom-chrome-height) start-(--rail-width) top-(--top-chrome-height) z-30 hidden w-2 rtl:w-1 docked:block"
          : "fixed bottom-(--bottom-chrome-height) end-0 top-(--top-chrome-height) z-30 hidden w-1 rtl:w-2 docked:block"
      }
      onPointerEnter={(event) => {
        if (event.pointerType !== "mouse") return;
        clearTimeout(dwell.current);
        dwell.current = setTimeout(() => {
          latest.current.reveal();
          watchFirstMove();
        }, PEEK_DWELL_MS);
      }}
      onPointerLeave={() => clearTimeout(dwell.current)}
    />
  );
}
