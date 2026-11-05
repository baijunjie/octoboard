import { useState, useSyncExternalStore } from "react";

/** The named breakpoints `style.css` declares in `@theme`: `phone`, `tablet` and `desktop`. */
type Breakpoint = "phone" | "tablet" | "desktop";

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

/** Inclusive so iPad portrait at exactly 768px shares the single-panel interaction. */
export function maxWidthQuery(name: Breakpoint): string {
  return `(width <= ${breakpointPx(name)}px)`;
}

/** Settings, dialogs and single-panel interaction use the same inclusive tablet boundary.
 * Pane docking above it follows the actual open columns' space requirements. */
export function useIsNarrow(): boolean {
  return useMediaQuery(() => maxWidthQuery("tablet"));
}

/**
 * Whether the window matches the media query `query` returns, kept current as the window changes, and right from the
 * first render. `query` is a function so a query built from what the stylesheet sets (`maxWidthQuery`) is read only
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
