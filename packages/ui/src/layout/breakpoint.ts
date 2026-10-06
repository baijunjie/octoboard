import { useEffect, useState } from "react";

/** Memoised once resolved: the value cannot change at runtime, so re-reading it on every call
 * would just be a repeated DOM read for the same answer (see `dockedBreakpointPx`). */
let dockedBreakpointPxCache: number | undefined;

/** Reads the `docked` breakpoint's own width back out of the CSS Tailwind's `@theme` block in
 * `style.css` emits it as, rather than keeping a second literal here — the two would otherwise be
 * free to drift apart. Throws if the stylesheet has not loaded yet (or the variable was renamed),
 * rather than silently falling back to a guessed value this module would then disagree with
 * `style.css` about. Resolved lazily on first use rather than eagerly at module load, so that
 * throw still only ever happens from a call site prepared for it, not as a side effect of
 * importing this module.
 *
 * Reached only from the state initialiser and the effect in `useIsNarrow`, never from its render
 * body: `getComputedStyle` is a side-effecting DOM read, and the throw above would otherwise come
 * out of a render rather than from a call site prepared for it. */
function dockedBreakpointPx(): number {
  if (dockedBreakpointPxCache !== undefined) return dockedBreakpointPxCache;
  const raw = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--breakpoint-docked"));
  if (!Number.isFinite(raw)) throw new Error("--breakpoint-docked is not set; is style.css loaded?");
  dockedBreakpointPxCache = raw;
  return raw;
}

/** `min-width` rather than Tailwind's own `max-width: <breakpoint - 1>px` complement: a query
 * built that way leaves a fractional-width gap matching neither (1099.5px, say), where the
 * `docked:` variants and `useIsNarrow` would disagree about which mode the window is in. */
function dockedQuery(): string {
  return `(min-width: ${dockedBreakpointPx()}px)`;
}

/**
 * Whether the window is currently narrower than the `docked` breakpoint. Every other narrow-mode
 * behaviour (swapping the sidebar's and the report panel's layout) uses the `docked:` Tailwind
 * variant directly, so it tracks the breakpoint with no JavaScript at all and no hydration/resize
 * flicker. This hook exists only for the behaviour that genuinely needs the current mode in JS:
 * closing an open drawer when the window widens past the breakpoint, since an overlay left open
 * there would otherwise cover the row it no longer needs to, and the top bar's panel toggles, which
 * drive a drawer below the breakpoint and show or hide the docked pane above it.
 */
export function useIsNarrow(): boolean {
  const [isNarrow, setIsNarrow] = useState(() => !window.matchMedia(dockedQuery()).matches);

  useEffect(() => {
    const mediaQuery = window.matchMedia(dockedQuery());
    const handleChange = () => setIsNarrow(!mediaQuery.matches);
    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
    // The breakpoint cannot change at runtime, so the query this subscribes to cannot either.
  }, []);

  return isNarrow;
}

/**
 * The geometry shared by the sidebar's and the report panel's narrow-mode drawers (`Sidebar.tsx`,
 * `ReportPanel.tsx`): fixed below `style.css`'s `--top-chrome-height`, sliding in from `side`, and
 * inert again — no fixed positioning, no stacking context, no transition — once `docked:` turns
 * it back into a plain row sibling. `docked:translate-none` rather than `docked:translate-x-0`
 * (which still computes to a `translate` value other than `none`) is what keeps a drawer from
 * becoming a stacking context and a containing block for fixed descendants at or above the
 * breakpoint, where the layout must stay exactly as it was before either component had a
 * `translate` at all.
 *
 * Callers add their own width/flex/border classes on top of this string; the slide direction (left
 * or right) and the open/closed state are this function's job, not theirs, since both drawers
 * share the one rule for what "closed" means on either side.
 *
 * `docked:inset-y-auto` drops the vertical offsets once docked: they would otherwise shift a
 * `relative` drawer down by `--top-chrome-height`.
 *
 * `positioned` makes the docked drawer `relative` rather than `static`, for a drawer whose own
 * children are absolutely positioned against it (the sidebar's resize handle). The two cannot be
 * layered on from outside: both are `docked:` utilities for the same property, so which wins would
 * be down to stylesheet order.
 *
 * Each side's classes are spelled out rather than built by interpolating `side`: Tailwind emits a
 * utility only for a class name it can find as literal text somewhere in the source, so an
 * interpolated `left-0`/`right-0` would compile to nothing and leave the drawer unanchored.
 */
export function drawerClass(side: "left" | "right", open: boolean, positioned = false): string {
  const shared =
    "fixed bottom-0 top-(--top-chrome-height) z-40 transition-transform duration-200 docked:inset-y-auto docked:z-auto docked:translate-none docked:transition-none";
  const docked = positioned ? "docked:relative" : "docked:static";
  return side === "left"
    ? `${shared} ${docked} left-0 ${open ? "translate-x-0" : "-translate-x-full"}`
    : `${shared} ${docked} right-0 ${open ? "translate-x-0" : "translate-x-full"}`;
}
