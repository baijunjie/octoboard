import { useState, useSyncExternalStore } from "react";

/** The named breakpoints `style.css` declares in `@theme`: `docked` and `phone`. */
type Breakpoint = "docked" | "phone";

/** Memoised once resolved: the value cannot change at runtime, so re-reading it on every call
 * would just be a repeated DOM read for the same answer (see `breakpointPx`). */
const breakpointPxCache: Partial<Record<Breakpoint, number>> = {};

/** Reads a breakpoint's own width back out of the CSS Tailwind's `@theme` block in `style.css`
 * emits it as, rather than keeping a second literal here — the two would otherwise be free to
 * drift apart. Throws if the stylesheet has not loaded yet (or the variable was renamed), rather
 * than silently falling back to a guessed value this module would then disagree with `style.css`
 * about. Resolved lazily on first use rather than eagerly at module load, so that throw is not a
 * side effect of importing this module.
 *
 * Reached only through the query function `useMediaQuery` is given, which it calls in the
 * subscription and in the snapshot: `useSyncExternalStore` reads the snapshot while rendering, so
 * the first read, and the throw if the stylesheet is missing, happens in the first render. */
function breakpointPx(name: Breakpoint): number {
  const cached = breakpointPxCache[name];
  if (cached !== undefined) return cached;
  const raw = parseFloat(getComputedStyle(document.documentElement).getPropertyValue(`--breakpoint-${name}`));
  if (!Number.isFinite(raw)) throw new Error(`--breakpoint-${name} is not set; is style.css loaded?`);
  breakpointPxCache[name] = raw;
  return raw;
}

/** `min-width` rather than Tailwind's own `max-width: <breakpoint - 1>px` complement: a query
 * built that way leaves a fractional-width gap matching neither (1099.5px, say), where the
 * `docked:` variants and `useIsNarrow` would disagree about which mode the window is in. The
 * `phone` breakpoint's query (`useAsideFloats`) is built the same way. */
export function minWidthQuery(name: Breakpoint): string {
  return `(min-width: ${breakpointPx(name)}px)`;
}

/**
 * Whether the window is currently narrower than the `docked` breakpoint. Every other narrow-mode
 * behaviour (swapping the sidebar's layout) uses the `docked:` Tailwind variant directly, so it
 * tracks the breakpoint with no JavaScript at all and no hydration/resize flicker. This hook exists
 * only for the behaviour that genuinely needs the current mode in JS: closing the sidebar's drawer
 * when the window widens past the breakpoint, since an overlay left open there would otherwise
 * cover the row it no longer needs to; the sidebar's toggle, which drives a drawer below the
 * breakpoint and shows or hides the docked pane above it; the width clamps, which count the
 * sidebar as a column in the row only above it; and the settings dialog's tabs, whose orientation
 * sets react-aria's arrow keys and `aria-orientation`, not only their layout.
 */
export function useIsNarrow(): boolean {
  return !useMediaQuery(() => minWidthQuery("docked"));
}

/**
 * Whether the window matches the media query `query` returns, kept current as the window changes, and right from the
 * first render. `query` is a function so a query built from what the stylesheet sets (`minWidthQuery`) is read only
 * in the subscription and the snapshot, not by this hook's own body. It is the function given as the component mounts
 * that is kept: what the query is built from cannot change at runtime, so neither can the query.
 */
export function useMediaQuery(query: () => string): boolean {
  const [read] = useState(() => query);
  const [store] = useState(() => ({
    subscribe: (onChange: () => void) => {
      const mediaQuery = window.matchMedia(read());
      mediaQuery.addEventListener("change", onChange);
      return () => mediaQuery.removeEventListener("change", onChange);
    },
    snapshot: () => window.matchMedia(read()).matches,
  }));
  return useSyncExternalStore(store.subscribe, store.snapshot);
}
