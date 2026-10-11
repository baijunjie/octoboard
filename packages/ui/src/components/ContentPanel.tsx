import React from "react";

/**
 * The window's content panel: everything under the top bar and past the left rail. It is opaque
 * whatever the window behind it is (on macOS a translucent native material, `style.css`), since text
 * that is read or typed into must never sit on the material, and `--panel` (the main area's colour,
 * the terminal's own) is what it is painted with. Its start edge and its top carry a 1px border, and
 * the corner between them is rounded, which is what sets it apart from the chrome around it; it is
 * flush with the window's end and bottom edges. `overflow-hidden` clips what it holds to that
 * corner; the panes that float over the content are fixed overlays, which it does not clip.
 *
 * `-mt-px` and `-ms-px` let the panel's borders overlap the chrome's last pixel — the top bar's
 * bottom row and the rail's end column — so the content starts exactly `--top-chrome-height` down
 * and `--rail-width` in, which is what the pane widths, the drawers and the floating cards' gap
 * count on. Without the top one the gap above a floating card would read 1px narrower than the gap
 * beside it. A `bare` panel is for the screens that have no rail (starting up, failed to start): it
 * spans the window, with a top border only — it sits under the top bar all the same, so it keeps the
 * top overlap — and holds a column.
 */
export function ContentPanel({ children, bare = false }: { children: React.ReactNode; bare?: boolean }): React.ReactElement {
  return (
    <div
      className={`-mt-px flex min-h-0 min-w-0 flex-1 overflow-hidden border-t border-separator bg-panel ${bare ? "flex-col" : "-ms-px rounded-ss-xl border-s"}`}
    >
      {children}
    </div>
  );
}
