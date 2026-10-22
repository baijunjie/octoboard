import React from "react";

import { useFadeUp } from "../layout/paneDockMotion";

/**
 * The dimming layer behind a narrow-mode overlay (the sidebar drawer, or a panel floating over the
 * terminal in a window too narrow to dock the right pane): pressing it closes the overlay. Sits on
 * an ordinary low `z-index`, well under the modal overlay's and HeroUI's toast region's
 * (`--z-index-overlay` and `--z-index-toast`, overlay + 1), so a dialog opened from inside the
 * drawer, or a toast, still renders above it. Hidden outright at or above the `docked` breakpoint
 * via the variant rather than a width check, matching the drawers themselves.
 *
 * Runs between the top bar and the connection banner (`--top-chrome-height`,
 * `--bottom-chrome-height`) and clear of the left rail (`--rail-width`) rather than the whole
 * viewport, so the bar's toggles, the rail's controls and the banner's Retry button stay reachable
 * while a drawer is open instead of being dimmed and swallowing the press into a close instead.
 *
 * It is a native `<button>` rather than HeroUI's `Button` because it is a full-bleed dimming layer
 * with no button look, which HeroUI's styled `Button` cannot be made into without overriding it
 * into something else.
 *
 * It fades in and out with its overlay while `open` changes, over the same 240ms and easing as the
 * card it dims behind (`useFadeUp`; at once under reduced motion), and takes no presses and no
 * focus while it fades away.
 *
 * A plain `<button>` never takes focus on click in WKWebView, but Chrome still focuses it on
 * mousedown — this one goes out of reach the instant it is pressed, so without `preventDefault` here a
 * press in the browser build would drop focus to `<body>`, away from the terminal. It is still a
 * tab stop, hence `data-escape-scope`: Escape with the scrim focused has to close the drawer like
 * Escape anywhere else around it.
 */
export function Scrim({ open, label, onClose }: { open: boolean; label: string; onClose: () => void }): React.ReactElement | null {
  const fade = useFadeUp(open, undefined);
  if (!fade.rendered) return null;
  return (
    <button
      type="button"
      aria-label={label}
      data-escape-scope
      inert={!open}
      style={{ top: "var(--top-chrome-height)", bottom: "var(--bottom-chrome-height)" }}
      onTransitionEnd={fade.onTransitionEnd}
      className={`fixed start-(--rail-width) end-0 z-30 bg-black/40 docked:hidden transition-opacity duration-240 ease-[cubic-bezier(0.2,0,0,1)] motion-reduce:transition-none ${fade.shown ? "opacity-100" : "pointer-events-none opacity-0"}${fade.preparing ? " transition-none" : ""}`}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClose}
    />
  );
}
