import React, { useEffect } from "react";

import { useLatest } from "../components/useLatest";
import type { PanePeek } from "./usePaneToggles";

/** How a pane that is a row sibling at and above the `docked` breakpoint is laid out:
 * - `drawer`: a closed-by-default overlay below the breakpoint, a plain row sibling at and above it;
 * - `floating`: the user has hidden the docked pane, which still floats in over the content on
 *   request, so it stays a fixed overlay at every width. */
export type PaneMode = "drawer" | "floating";

/** The DOM ids of the two panes: the sidebar toggle names its pane in `aria-controls`, and the
 * aside's toggles are recognised as part of its pointer watch (`data-pane-toggle`). */
export const PANE_ID = { sidebar: "sidebar-pane", aside: "aside-pane" } as const;

/** What a panel card of the right pane looks like, the same as the sidebar's floating card: rounded,
 * clipped to the corner, the sidebar's colour, the `panel-edge` hairline and the `--panel-card-shadow`
 * elevation, kept `--peek-gap` (`--panel-grip-gap` on the terminal side, `--panel-stack-gap` between cards) from
 * everything around it (`AsidePane.tsx`). The sidebar's card
 * spells the same utilities out under `docked:`/`max-docked:` in `drawerClass` below, and
 * Tailwind needs them as literal text, so a change to the look is made in both. */
export const PANEL_CARD_CLASS = "overflow-hidden rounded-xl bg-panel-sidebar panel-edge shadow-(--panel-card-shadow)";

/** The vertical inset and header height of the desktop floating card. */
const FLOATING_INSET =
  "docked:top-[calc(var(--top-chrome-height)+var(--peek-gap))] docked:bottom-[calc(var(--bottom-chrome-height)+var(--peek-gap))] docked:[--pane-header-height:var(--pane-header-height-floating)]";

/** Drawers use the same card geometry as desktop peeks. Scope it below the breakpoint so the
 * docked sidebar can still ease as a full-height column without changing shape. */
const DRAWER_FRAME =
  "max-docked:top-[calc(var(--top-chrome-height)+var(--peek-gap))] max-docked:bottom-[calc(var(--bottom-chrome-height)+var(--peek-gap))] max-docked:[--pane-header-height:var(--pane-header-height-floating)] max-docked:rounded-xl max-docked:overflow-hidden max-docked:pointer-events-auto max-docked:panel-edge max-docked:shadow-(--panel-card-shadow)";

/** The sidebar is on the reading direction's start side, so under right-to-left it is on the right.
 * It is anchored past the rail (`--rail-width` in `style.css`) while the rail is in the row. The
 * overlay is a card inset by `--peek-gap` from the top bar, the bottom chrome and the rail. Below the
 * `phone` breakpoint the rail is not in the row but part of the card: the card starts `--peek-gap`
 * from the window's edge and the sidebar (`Sidebar.tsx`) pads its start by the rail's width, where
 * the rail (`Rail.tsx`) lies over it. A card's headers take
 * `--pane-header-height-floating` (`style.css`) so that their bottom line lines up with the docked
 * headers' in the same row; `FLOATING_INSET` sets it for everything inside. */
const ANCHOR = "start-(--rail-width) max-docked:phone:start-[calc(var(--rail-width)+var(--peek-gap))] max-phone:start-(--peek-gap)";
/** Below the `phone` breakpoint a panel that is away is shifted `--phone-slide` (`style.css`) towards
 * the edge it lives on, the start edge for the sidebar's drawer and its rail, in place of the vertical
 * drift of the wider layouts, so that it slides in from that edge as it fades in and back out to it.
 * Mirrored under right-to-left. Exported for the rail, which comes and goes with the drawer. */
export const PHONE_SLIDE_AWAY = "max-phone:translate-y-0 max-phone:-translate-x-(--phone-slide) max-phone:rtl:translate-x-(--phone-slide)";

/** The drawer below the breakpoint, open or away: it fades in as it rises into its place by 4% of
 * its own height and fades out as it sinks back, over 240ms with the floating card's easing, its
 * edge and shadow (`DRAWER_FRAME`) with it; `visibility` stays on until the exit has finished and
 * then keeps the away drawer out of reach. Below the `phone` breakpoint it slides instead, in from
 * the start edge by `--phone-slide` and back out to it (`PHONE_SLIDE_AWAY`). */
const DRAWER_OPEN = "max-docked:translate-y-0 max-docked:opacity-100";
const DRAWER_AWAY = `max-docked:translate-y-[4%] max-docked:opacity-0 max-docked:invisible ${PHONE_SLIDE_AWAY}`;
const FLOATING_FRAME = `docked:start-[calc(var(--rail-width)+var(--peek-gap))] ${FLOATING_INSET}`;

/**
 * The geometry of the sidebar's overlay forms (`Sidebar.tsx`): fixed between `style.css`'s
 * `--top-chrome-height` and `--bottom-chrome-height`, with a card inset from the surrounding
 * chrome and the rail. Drawers and desktop floating cards alike fade up into place and back down;
 * below the `phone` breakpoint the drawer slides in from the start edge instead.
 * `open` is whether the drawer is open below the breakpoint. Callers add their own
 * width/flex classes on top of this string; the motion, what "closed" means and the card's edge
 * (the `panel-edge` hairline together with the `--panel-card-shadow` elevation) are this
 * function's job, not theirs. A floating card carries the edge while it floats; the drawer carries
 * it at all times below the breakpoint, so it fades with the card, and the fade and `invisible`
 * hide it while the drawer is away.
 *
 * In `drawer` mode `docked:` turns the pane back into a plain row sibling — no fixed positioning,
 * no stacking context, no transition. The drawer's motion classes are all `max-docked:`, and
 * `docked:translate-none` keeps `translate` at `none` at and above the breakpoint, so the pane is
 * no stacking context and no containing block for fixed descendants there, where the layout must
 * stay exactly as it was before the pane had a `translate` at all.
 *
 * In `floating` mode none of the `docked:` resets apply, and `peeking` takes the place of `open`
 * for whether the pane is on screen there. The card fades up from 4% of its height below its
 * resting position and fades down when dismissed, with the console-switch view's easing. It is `invisible` while
 * away, so a faded card stays out of the tab order. Drawers are likewise invisible while away. A
 * floating start pane is a `RailClip`'s child.
 *
 * Tailwind 4's `translate-*` utilities set the CSS `translate` property, not `transform`, so the
 * overlays transition `translate`, `opacity` and `visibility`; visibility stays on until the exit
 * finishes. Every form drops its transition under `prefers-reduced-motion: reduce`.
 *
 * HeroUI's `Drawer` is not used: it is modal (everything outside is inert, focus is trapped, its
 * content is portalled and unmounted while closed). The sidebar has to be a non-modal row sibling
 * that is also a drawer and a hover-peek pane, with the window chrome (the top bar and the rail)
 * still operable, focus staying on the terminal, and what it shows staying mounted.
 */
export function drawerClass(mode: PaneMode, open: boolean, peeking = false): string {
  const dockedReset = "docked:static docked:z-auto docked:translate-none docked:transition-none";
  const form =
    mode === "drawer"
      ? `max-docked:transition-[translate,opacity,visibility] max-docked:duration-240 max-docked:ease-[cubic-bezier(0.2,0,0,1)] max-docked:motion-reduce:transition-none ${dockedReset}`
      : `${FLOATING_FRAME} docked:panel-edge docked:shadow-(--panel-card-shadow) docked:transition-[translate,opacity,visibility] docked:duration-240 docked:ease-[cubic-bezier(0.2,0,0,1)] docked:motion-reduce:transition-none ${peeking ? "docked:translate-y-0 docked:opacity-100 docked:pointer-events-auto" : "docked:translate-y-[4%] docked:opacity-0 docked:pointer-events-none docked:invisible"}`;
  const base = "fixed bottom-(--bottom-chrome-height) top-(--top-chrome-height) z-40 motion-reduce:transition-none";
  return `${base} ${DRAWER_FRAME} ${ANCHOR} ${open ? DRAWER_OPEN : DRAWER_AWAY} ${form}`;
}

/**
 * What a start-side overlay sits in: a zero-width fixed element at the rail's end edge, clipped to
 * the content side of that edge only (`clip-path` reaches past the other three sides, so the pane's
 * shadow is not cut), keeping the card and its shadow off the rail. The rail paints nothing of its
 * own on the translucent macOS window, so stacking the pane beneath it would show it through. The
 * wrapper takes no pointer events, the pane (`drawerClass`) does, and the pane stays `fixed`
 * itself: a clip does not make its element a containing block. With the pane docked at the desktop
 * breakpoint (`floating` false), the wrapper is `display: contents` and so is not there; so it is
 * below the `phone` breakpoint, where the card starts at the window's edge and the rail is part of
 * it, with nothing to keep it off. It is always
 * rendered, so docking the pane does not remount what is in it.
 */
export function RailClip({ floating, children }: { floating: boolean; children: React.ReactNode }): React.ReactElement {
  return (
    <div
      className={`contents max-docked:phone:pointer-events-none max-docked:phone:fixed max-docked:phone:inset-y-0 max-docked:phone:start-(--rail-width) max-docked:phone:z-40 max-docked:phone:block max-docked:phone:w-0 max-docked:phone:[clip-path:inset(-100vh_-100vw_-100vh_0)] max-docked:phone:rtl:[clip-path:inset(-100vh_0_-100vh_-100vw)] ${
        floating
          ? "docked:pointer-events-none docked:fixed docked:inset-y-0 docked:start-(--rail-width) docked:z-40 docked:block docked:w-0 docked:[clip-path:inset(-100vh_-100vw_-100vh_0)] docked:rtl:[clip-path:inset(-100vh_0_-100vh_-100vw)]"
          : ""
      }`}
    >
      {children}
    </div>
  );
}

/** Memoised once read. That holds only while `style.css` declares `--peek-gap` once,
 * unconditionally: overriding it under a media query or a theme would leave this at the value read
 * first, and the halo would stop matching the card without any visible symptom. */
let peekGapPxCache: number | undefined;

/** Reads `--peek-gap` back out of `style.css` (a px length), and throws if its value does not parse
 * as a number. Not a fallback to 0: that is the geometry the halo exists to correct, and nothing
 * would say so. Called every time the sidebar's floating card's pointer watch arms (the aside's
 * needs no halo), but reads the stylesheet only until it succeeds; a failure is not cached, so the
 * next call retries. The watch's call is in an effect, and a throw there reaches the
 * `ErrorBoundary` around the whole app: the failure screen. Both entry points import `style.css`
 * before mounting, so only a stylesheet that lost the token can cause it. */
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
 * The rectangle counts a `gap` (`--peek-gap`) inset on all four sides as part of the pane, for the
 * sidebar's card, which has no room around it of its own: a pointer resting in the gap is not
 * outside of the pane that floated in for it. That halo is the terminal's, not the pane's, so a
 * pointer in it raises none of the pane's own events, and one coming from the card raises its
 * `pointerleave` (see `usePanePeekPointer`). The aside's box already includes the gaps around its
 * cards, so its halo is none (`gap` 0). The top bar is outside it, as the halo's
 * upper edge is the bar's own; the toggle that floated the card in is not (see the hook).
 *
 * A pane still in the row while it eases out (`phase === "closing"`, the sidebar's) is not yet
 * `fixed`, so its offsets are relative to its offsetParent (the app root), which is never
 * transformed; that parent's viewport position is added back. */
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
 * floating, however it floated in (a toggle, a console's avatar);
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
 * in the halo keeps the card up indefinitely, so a pointer 1..`--peek-gap` px from the card does not
 * let it go, which is accepted as the direct consequence of the halo counting as the pane.
 *
 * The entry itself can leave a `pointerleave` behind: the card's edge moves past a pointer
 * resting where the card appears before getting out of its way, and an engine that re-runs hit-testing
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
    // The aside's box includes the gaps around its cards; the sidebar's card has none of its own.
    const gap = pane === "aside" ? 0 : peekGapPx();
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
