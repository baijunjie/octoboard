import React, { useEffect, useRef } from "react";

import { useLatest } from "../components/useLatest";
import type { PanePeek } from "./usePaneToggles";

/** How a pane that is a row sibling at and above the `docked` breakpoint is laid out:
 * - `drawer`: a closed-by-default overlay below the breakpoint, a plain row sibling at and above it;
 * - `floating`: the user has hidden the docked pane, which still floats in over the content on
 *   request, so it stays a fixed overlay at every width. */
export type PaneMode = "drawer" | "floating";

/** The DOM ids of the two panes, which the sidebar and aside toggles name in `aria-controls`. */
export const PANE_ID = { sidebar: "sidebar-pane", aside: "aside-pane" } as const;

/** The edge a pane sits on: the reading direction's start or end. */
export type PaneEdge = "start" | "end";

/** The vertical inset and header height shared by both desktop floating cards. */
const FLOATING_INSET =
  "docked:top-[calc(var(--top-chrome-height)+var(--peek-gap))] docked:bottom-[calc(var(--bottom-chrome-height)+var(--peek-gap))] docked:[--pane-header-height:var(--pane-header-height-floating)]";

/** Drawers use the same card geometry as desktop peeks. Scope it below the breakpoint so the
 * docked sidebar can still ease as a full-height column without changing shape. */
const DRAWER_FRAME =
  "max-docked:top-[calc(var(--top-chrome-height)+var(--peek-gap))] max-docked:bottom-[calc(var(--bottom-chrome-height)+var(--peek-gap))] max-docked:[--pane-header-height:var(--pane-header-height-floating)] max-docked:rounded-xl max-docked:border max-docked:overflow-hidden max-docked:pointer-events-auto";

/** The two sides are the reading direction's start and end, so under right-to-left the sidebar is on
 * the right. The classes that differ between them are written out in full: Tailwind emits a utility
 * only for a class name it can find as literal text somewhere in the source, so an interpolated
 * `start-0`/`end-0` would compile to nothing and leave the pane unanchored. `translate-x-*` is
 * physical, so the slide direction under `rtl:` is the opposite one. The start side is anchored
 * past the rail (`--rail-width` in `style.css`), so below the breakpoint it slides that much
 * further, including the inset and shadow, to get off screen entirely. Every overlay is a card
 * inset by `--peek-gap` from the top bar and the bottom chrome, and from the rail (start) or the
 * window's edge (end). Mobile drawers clear the rail and their shadows while sliding away. A card's
 * headers take `--pane-header-height-floating` (`style.css`) so that their bottom border lines up
 * with the docked headers' in the same row; `floatingFrame` sets it for everything inside. */
const SIDE_CLASSES = {
  start: {
    anchor: "start-(--rail-width) max-docked:start-[calc(var(--rail-width)+var(--peek-gap))]",
    closed: "-translate-x-[calc(100%+var(--rail-width)+var(--peek-gap)+2rem)] rtl:translate-x-[calc(100%+var(--rail-width)+var(--peek-gap)+2rem)]",
    floatingFrame: `docked:start-[calc(var(--rail-width)+var(--peek-gap))] ${FLOATING_INSET}`,
  },
  end: {
    anchor: "end-0 max-docked:end-(--peek-gap)",
    closed: "translate-x-[calc(100%+var(--peek-gap)+2rem)] rtl:-translate-x-[calc(100%+var(--peek-gap)+2rem)]",
    floatingFrame: `docked:end-(--peek-gap) ${FLOATING_INSET}`,
  },
};

/**
 * The geometry shared by the sidebar's and the aside's overlay forms (`Sidebar.tsx`,
 * `AsidePane.tsx`): fixed between `style.css`'s `--top-chrome-height` and
 * `--bottom-chrome-height`, with a card inset from the surrounding chrome. Drawers slide from `side`; desktop
 * floating cards fade vertically. The start side is inset from the rail; the end side from the window edge.
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
 * for whether the pane is on screen there. The card fades up from 6px below its resting position
 * and fades down when dismissed, with the console-switch view's easing. It is `invisible` while
 * away, so a faded card stays out of the tab order. Drawers are likewise invisible while away,
 * and their translation clears the card's shadow from the screen edge. A floating start pane is a `RailClip`'s child.
 *
 * Tailwind 4's `translate-x-*` utilities set the CSS `translate` property, not `transform`, so the
 * overlays transition `translate` and `visibility`, and floating cards also transition opacity;
 * visibility stays on until the exit finishes. Every form drops its transition under
 * `prefers-reduced-motion: reduce`.
 *
 * HeroUI's `Drawer` is not used: it is modal (everything outside is inert, focus is trapped, its
 * content is portalled and unmounted while closed). The panes have to be non-modal row siblings
 * that are also drawers and hover-peek panes, with the window chrome (the top bar and the rail)
 * still operable, focus staying on the terminal, and what the aside shows (a report page, a file
 * tree) staying mounted.
 */
export function drawerClass(side: PaneEdge, mode: PaneMode, open: boolean, peeking = false): string {
  const { anchor, closed, floatingFrame } = SIDE_CLASSES[side];
  const dockedReset = "docked:static docked:z-auto docked:translate-none docked:rtl:translate-none docked:transition-none";
  const form =
    mode === "drawer"
      ? `transition-[translate,visibility] ${dockedReset}`
      : `transition-[translate,visibility] ${floatingFrame} docked:translate-x-0 docked:rtl:translate-x-0 docked:shadow-xl docked:transition-[translate,opacity,visibility] docked:duration-240 docked:ease-[cubic-bezier(0.2,0,0,1)] ${peeking ? "docked:translate-y-0 docked:opacity-100 docked:pointer-events-auto" : "docked:translate-y-1.5 docked:opacity-0 docked:pointer-events-none docked:invisible"}`;
  const base = "fixed bottom-(--bottom-chrome-height) top-(--top-chrome-height) z-40 duration-200 motion-reduce:transition-none";
  return `${base} ${DRAWER_FRAME} ${anchor} ${open ? "translate-x-0 max-docked:shadow-xl" : `${closed} max-docked:invisible`} ${form}`;
}

/**
 * What a start-side overlay sits in: a zero-width fixed element
 * at the rail's end edge, clipped to the content side of that edge only (`clip-path` reaches past
 * the other three sides, so the pane's shadow is not cut), keeping the card and its shadow off the
 * rail and letting mobile drawers slide behind it. The rail paints nothing of its own on the translucent
 * macOS window, so stacking the pane beneath it would show it through. The wrapper takes no pointer
 * events, the pane (`drawerClass`) does, and the pane stays `fixed` itself:
 * a clip does not make its element a containing block. With the pane docked at the desktop
 * breakpoint (`floating` false), the wrapper is `display: contents` and so is not there. It is always
 * rendered, so docking the pane does not remount what is in it.
 */
export function RailClip({ floating, children }: { floating: boolean; children: React.ReactNode }): React.ReactElement {
  return (
    <div
      className={`contents max-docked:pointer-events-none max-docked:fixed max-docked:inset-y-0 max-docked:start-(--rail-width) max-docked:z-40 max-docked:block max-docked:w-0 max-docked:[clip-path:inset(-100vh_-100vw_-100vh_0)] max-docked:rtl:[clip-path:inset(-100vh_0_-100vh_-100vw)] ${
        floating
          ? "docked:pointer-events-none docked:fixed docked:inset-y-0 docked:start-(--rail-width) docked:z-40 docked:block docked:w-0 docked:[clip-path:inset(-100vh_-100vw_-100vh_0)] docked:rtl:[clip-path:inset(-100vh_0_-100vh_-100vw)]"
          : ""
      }`}
    >
      {children}
    </div>
  );
}

/** How long a mouse pointer has to stay on the aside's hot zone before the aside floats in. */
const PEEK_DWELL_MS = 200;

/** Memoised once read. That holds only while `style.css` declares `--peek-gap` once,
 * unconditionally: overriding it under a media query or a theme would leave this at the value read
 * first, and the halo would stop matching the hot zone and the card without any visible symptom. */
let peekGapPxCache: number | undefined;

/** Reads `--peek-gap` back out of `style.css` (a px length), and throws if its value does not parse
 * as a number. Not a fallback to 0: that is the geometry the halo exists to correct, and nothing
 * would say so. Called on every reveal through the hot zone and every time the floating pane's
 * pointer watch arms, but reads the stylesheet only until it succeeds; a failure is not cached, so
 * the next call retries. The hot zone calls it before revealing, so a missing token floats nothing
 * in instead of leaving a pane up with no watch on the pointer. There the throw is in a `setTimeout`
 * callback, which `ErrorBoundary` does not catch: it is an uncaught console error and nothing
 * floating in. The watch's call is in an effect, and a throw there reaches the `ErrorBoundary`
 * around the whole app: the failure screen. Both entry points import `style.css` before mounting,
 * so only a stylesheet that lost the token can cause it. */
function peekGapPx(): number {
  if (peekGapPxCache !== undefined) return peekGapPxCache;
  const raw = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--peek-gap"));
  if (!Number.isFinite(raw)) throw new Error("--peek-gap is not set");
  peekGapPxCache = raw;
  return raw;
}

/** Whether the pointer is off a pane at its resting position. The element under the
 * pointer cannot say, as the pane is still on its way in and the content shows where it is going to
 * be; the offsets are layout positions, which a `translate` does not move. It is a position in
 * the viewport, comparable with `clientX`, only because a floating pane is `position: fixed` with no
 * transformed ancestor, so it has no offset parent.
 *
 * The rectangle counts the card's `gap` (`--peek-gap`) inset on all four sides as part of the pane:
 * the hot zone lies along the window's edge, in the gap the card leaves there, so a pointer resting
 * on the zone is not outside of the pane that floated in for it. That halo is the terminal's, not
 * the pane's, so a pointer in it raises none of the pane's own events, and one coming from the card
 * raises its `pointerleave` (see `usePanePeekPointer`). The top bar is outside it, as the halo's
 * upper edge is the bar's own; the toggle that floated the card in is not (see the hook).
 *
 * A pane still in the row while it eases out (`phase === "closing"`) is not yet `fixed`, so its
 * offsets are relative to its offsetParent (the sidebar's column clip, or `main` for the right
 * pane), which is never transformed; that parent's viewport position is added back. */
function outsidePane(pane: Element | null, event: PointerEvent, gap: number): boolean {
  if (!(pane instanceof HTMLElement)) return true;
  const parent = pane.offsetParent;
  const origin = parent && parent !== document.documentElement && parent !== document.body ? parent.getBoundingClientRect() : null;
  const left = pane.offsetLeft + (origin?.left ?? 0);
  const top = pane.offsetTop + (origin?.top ?? 0);
  const bottom = top + pane.offsetHeight;
  return (
    event.clientX < left - gap ||
    event.clientX > left + pane.offsetWidth + gap ||
    event.clientY < top - gap ||
    event.clientY > bottom + gap
  );
}

/** The floating card's 240ms fade (`drawerClass`) plus a margin before its pointer watch settles. */
const PEEK_SETTLE_MS = 290;

/**
 * Tells a pane's `peek` where the pointer is relative to its card for as long as the card is
 * floating, however it floated in (the aside's hot zone, a toggle, a console's avatar);
 * `AsidePane` and `Sidebar` call it, `pane` naming the one whose `[data-pane]` card it is.
 *
 * The pane's own events do not suffice. A pointer that moves off the card into the content would be
 * a `pointerleave`, but one that floated the card in from the window's edge may never have reached
 * it, and one resting in the gap around the card is not on the pane either, however its events turn
 * out (they depend on where the card is when it stops, and a browser may not re-check what is under
 * a still pointer then). So every pointer move is looked at, and one that lands outside the card's
 * final extent (`outsidePane`) counts as leaving it, whether or not the card is under the pointer
 * at that moment. A move whose target is the pane's own toggle is inside, though the top bar it sits
 * in is not: the pointer is resting on the control that floated the card in, and a drift of it must
 * not send the card away. So is one on an element matching `alsoInside`, for what else keeps the
 * card up from outside it: the sidebar's, the console avatars on the rail, which sit outside the
 * halo and whose own enter and leave events do not cover a pointer moving about on one. A pointer
 * that leaves the window counts as leaving too, as no move follows to say so. `leave` is reported
 * once per excursion: a flag, which a move inside resets, stops every move outside from restarting
 * the dismissal delay, and does not stop a later excursion from being reported.
 *
 * A move inside the extent, the halo included, calls `keep`. This relies on the order of the events
 * a single move across the card's edge raises: the card's `pointerleave` comes before the
 * `pointermove` at the new place (out/leave, then over/enter, then move), so the card's `leave`
 * arms dismissal first and the halo's `keep` cancels it. Nothing re-arms it: a pointer resting
 * in the halo keeps the card up indefinitely. On the outer side, where the hot zone is, that is the
 * point; on the other three it means a pointer 1..`--peek-gap` px from the card does not let it go,
 * which is accepted as the direct consequence of the halo counting as the pane.
 *
 * The entry itself can leave a `pointerleave` behind: the card's edge moves past a pointer
 * resting on the hot zone before getting out of its way, and an engine that re-runs hit-testing
 * during the transition reports an enter and a leave. With the pointer still, no move would cancel
 * the dismissal that arms, so a `keep` after `PEEK_SETTLE_MS` does. No move in that time means the
 * last known place is still inside, so it cancels nothing that should have gone; a leave reported
 * in the meantime cancels it.
 */
export function usePanePeekPointer(
  pane: keyof typeof PANE_ID,
  peek: Pick<PanePeek, "active" | "keep" | "leave"> | undefined,
  alsoInside?: string,
): void {
  // The listeners outlive the render that added them, and `peek` is rebuilt every render.
  const latest = useLatest(peek);
  const active = peek?.active ?? false;
  useEffect(() => {
    if (!active) return;
    const gap = peekGapPx();
    const insideSelector = `[data-pane-toggle=${PANE_ID[pane]}]${alsoInside ? `,${alsoInside}` : ""}`;
    // The first `data-pane` match is the card: `PaneResizeHandle` carries the same attribute, but a
    // side's handle is rendered only while that side is docked, never while its card floats, and the
    // pane comes before it in the document either way. The element is looked up on every move but
    // the rectangle is read live, as the connection banner changes the card's bottom while it floats.
    const paneSelector = `[data-pane=${pane}]`;
    let left = false;
    const settle = setTimeout(() => latest.current?.keep(), PEEK_SETTLE_MS);
    const onLeave = () => {
      if (left) return;
      left = true;
      clearTimeout(settle);
      latest.current?.leave();
    };
    const onMove = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Element && target.closest(insideSelector)) {
        left = false;
        latest.current?.keep();
        return;
      }
      if (outsidePane(document.querySelector(paneSelector), event, gap)) {
        onLeave();
      } else {
        left = false;
        latest.current?.keep();
      }
    };
    window.addEventListener("pointermove", onMove, { capture: true });
    document.documentElement.addEventListener("pointerleave", onLeave);
    return () => {
      clearTimeout(settle);
      window.removeEventListener("pointermove", onMove, { capture: true });
      document.documentElement.removeEventListener("pointerleave", onLeave);
    };
  }, [active, pane, alsoInside]);
}

/**
 * The strip along the window's end edge that floats the hidden aside in when a mouse pointer stays
 * on it; rendered by the pane itself (`AsidePane.tsx`), only while it is hidden and not already out.
 * It starts under the top bar and ends above the connection banner, so their controls stay clear. It
 * reacts to a mouse only, as the toggles do.
 *
 * The pane waits `PEEK_DWELL_MS` before floating in, so a pointer passing along the edge does not
 * make it flicker in, one resting on the edge does. The toggle that floats a pane in does so at
 * once, as it is aimed at. Floating in arms no slide-away timer, since the pointer is expected to
 * be on the pane (which may not report entering it while the pointer is still, and slides up
 * under it), whose leaving then starts the timer; where the pointer goes from then on is
 * `usePanePeekPointer`'s to watch, for either way of floating in.
 *
 * It is a bare element because it is not a control: it is `aria-hidden`, a mouse-only hover target
 * with no HeroUI equivalent.
 *
 * The strip is 4px wide along the window's physical right edge under left-to-right, inside the
 * terminal's horizontal padding, so it stays off xterm's scrollbar, which is on the right whatever
 * the direction; under right-to-left it is on the left and 8px, easy to reach by pushing the
 * pointer to the edge. Hence the physical widths.
 */
export function AsidePeekHotZone({ peek }: { peek: PanePeek }): React.ReactElement | null {
  const dwell = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // The dwell's callback outlives the render that scheduled it, and `peek` is rebuilt every render.
  const latest = useLatest(peek);
  useEffect(() => () => clearTimeout(dwell.current), []);
  if (peek.active) return null;
  return (
    <div
      aria-hidden="true"
      // Only the tests look the strip up by this. The zone is the aside's alone, so it names no side
      // parameter.
      data-peek-zone="aside"
      className="fixed bottom-(--bottom-chrome-height) end-0 top-(--top-chrome-height) z-30 hidden w-1 rtl:w-2 docked:block"
      onPointerEnter={(event) => {
        if (event.pointerType !== "mouse") return;
        clearTimeout(dwell.current);
        dwell.current = setTimeout(() => {
          peekGapPx();
          latest.current.reveal();
        }, PEEK_DWELL_MS);
      }}
      onPointerLeave={() => clearTimeout(dwell.current)}
    />
  );
}
