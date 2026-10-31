import React from "react";

/**
 * The window's content panel: everything under the top bar and past the left rail. It is opaque
 * whatever the window behind it is (on macOS a translucent native material, `style.css`), since text
 * that is read or typed into must never sit on the material, and `--panel` (the main area's colour,
 * the terminal's own) is what it is painted with. It has no border: the contrast of what it holds
 * against the chrome around it sets it apart, lifted by a soft shadow (`--panel-shadow`) that a
 * shadow, unlike a border, casts without taking up layout, and the rounded corner between its
 * start edge and its top softens where the two meet; it is flush with the window's end and bottom
 * edges.
 * `overflow-hidden` clips what it holds to that corner; the panes that float over the content are
 * fixed overlays, which it does not clip.
 *
 * The panel must start exactly `--top-chrome-height` down and `--rail-width` in, which is what the
 * pane widths, the drawers and the floating cards' gap count on, so it carries no border: one
 * would push the content 1px in from those lines. A `bare` panel is for the screens that have no
 * rail (starting up, failed to start): it spans the window under the top bar and holds a column.
 * `startsAtEdge` is for a rail that leaves the row below the `phone` breakpoint (`Rail`'s
 * `onDemand`): the panel then starts at the window's edge, where the corner would have nothing to
 * soften, as its end edge has none.
 */
export function ContentPanel({ children, bare = false, startsAtEdge = false }: { children: React.ReactNode; bare?: boolean; startsAtEdge?: boolean }): React.ReactElement {
  return (
    <div
      className={`flex min-h-0 min-w-0 flex-1 overflow-hidden bg-panel ${bare ? "flex-col" : `rounded-ss-xl shadow-(--panel-shadow)${startsAtEdge ? " max-phone:rounded-ss-none" : ""}`}`}
    >
      {children}
    </div>
  );
}
