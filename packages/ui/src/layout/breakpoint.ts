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
 * there would otherwise cover the row it no longer needs to; the sidebar and report toggles, which
 * drive a drawer below the breakpoint and show or hide the docked pane above it; and the settings
 * dialog's tabs, whose orientation sets react-aria's arrow keys and `aria-orientation`, not only
 * their layout.
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
