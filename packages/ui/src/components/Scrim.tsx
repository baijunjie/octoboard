import React from "react";

/**
 * The dimming layer behind a narrow-mode overlay (the sidebar drawer, the report panel drawer):
 * pressing it closes the overlay. Sits on an ordinary low `z-index`, well under the modal
 * overlay's and the toast stack's (`--z-index-overlay` and up in `Toasts.tsx`), so a dialog opened
 * from inside the drawer, or a toast, still renders above it. Hidden outright at or above the
 * `docked` breakpoint via the variant rather than a width check, matching the drawers themselves.
 *
 * Starts below the top bar and the connection banner (`--top-chrome-height`) rather than at the
 * viewport top, so the bar's toggles and the banner's Retry button stay reachable while a drawer is
 * open instead of being dimmed and swallowing the press into a close instead.
 *
 * A plain `<button>` never takes focus on click in WKWebView, but Chrome still focuses it on
 * mousedown — this one unmounts the instant it is pressed, so without `preventDefault` here a
 * press in the browser build would drop focus to `<body>`, away from the terminal. It is still a
 * tab stop, hence `data-escape-scope`: Escape with the scrim focused has to close the drawer like
 * Escape anywhere else around it.
 */
export function Scrim({ label, onClose }: { label: string; onClose: () => void }): React.ReactElement {
  return (
    <button
      type="button"
      aria-label={label}
      data-escape-scope
      style={{ top: "var(--top-chrome-height)" }}
      className="fixed inset-x-0 bottom-0 z-30 bg-black/40 docked:hidden"
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClose}
    />
  );
}
