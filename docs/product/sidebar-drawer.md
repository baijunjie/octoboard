# The sidebar's drawer

In a window narrower than 1168 px the sidebar is not in the row of panes but an overlay **drawer** over them, and below
782 px the rail comes in with it as part of the same card. The window's layout as a whole, the widths it switches at
and the panes in the row are in `docs/product/window-layout.md`; what the sidebar itself shows is in
`docs/product/sidebar.md`; opening and closing the drawer with a finger is in `docs/product/touch-swipes.md`.

## Below 1168 px: the sidebar becomes a drawer

Below 1168 px wide, the top bar stays as it is, and so does the rail down to 782 px (below that see "Below 782 px: the
rail comes in with the drawer" below). The sidebar leaves the row: the terminal and, while one of its panels is open,
the right pane share the content panel (their widths are in "Pane widths below 1168 px" in
`docs/product/window-layout.md`). The sidebar becomes an **overlay drawer** over them, **closed by default**:

- The drawer has the same rounded corners, hairline edge and light card shadow as the floating panes (see "A hidden
  pane floats in on hover" in `docs/product/window-layout.md`). It keeps an **8 px gap** from the top bar, from the
  window's bottom (or the connection banner when it is shown) and from the rail, and is never drawn over the rail;
  below 782 px the gaps are **4 px**, with the start gap measured from the window's edge instead, the rail being
  inside it. Header heights are in "The window chrome and the content panel" in `docs/product/window-layout.md`.
- From 782 px up to 1168 px it opens by fading in while rising into place by **4% of its own height**, and closes by
  fading out while sinking back, without sliding sideways. Below 782 px it fades in while sliding in from the window's
  start edge by **40 px**, and fades out while sliding back towards it. Either way it moves over **240 ms** with the
  same `cubic-bezier(0.2, 0, 0, 1)` easing as a floating pane, and its edge and shadow move and fade with it. Where the
  system asks for reduced motion it opens and closes at once, with no movement.
  While it is closed, its exit included, it is out of reach: nothing in it takes a press, keyboard focus or the
  attention of assistive technology.
- From 782 px up the drawer is 280 px wide; below 782 px it is **328 px**, the rail's 48 px and the sidebar's 280 px.
  Either way it is capped to the window's width less 64 px, so it never reaches the opposite edge of the content
  panel; below 782 px that leaves at least 60 px of the dimmed area beside its end, there to press to close it. The
  dimmed area behind the open drawer covers the content panel,
  including the gaps — below 782 px its whole width, the rail being in the drawer — leaving the top bar, the rail where
  it is in the row and the banner's Retry usable. It fades in and out with the drawer over the same 240 ms, at once
  where the system asks for reduced motion, and takes no presses while it fades out.
- It is opened and closed with the same toggle that hides and shows the sidebar at 1168 px and wider, at the top
  bar's start (see "The sidebar toggle and the panel toggles" in `docs/product/window-layout.md`), and below 782 px
  with a touch swipe too (see `docs/product/touch-swipes.md`).
- It closes on its toggle, on a press anywhere in the dimmed area beside it, or on `Escape`; Open panel in a project's
  action menu and a right pane floating in close it too (see "What the right pane shows" in
  `docs/product/window-layout.md`), and so does a panel opened below 782 px. Opening the drawer closes a panel floating over the terminal (see "Below 782 px: one panel
  floats over the terminal" in `docs/product/right-pane-panels.md`).
  While it is open `Escape` closes it and **does not reach the running agent**; in a field in the drawer that holds
  text it only clears the field, and the next one closes the drawer.
- **Selecting a session in the drawer closes it below 782 px**, so the session's terminal shows — picked in the
  sidebar or, with the rail in the drawer, by the rail's waiting count or from a console avatar's menu on the rail.
  From 782 px up to 1168 px the drawer stays open beside the terminal.
- A dialog opened from a control pressed in the open drawer, or from a menu opened there, gives keyboard focus back
  to that control in the drawer as it closes, rather than to the terminal behind the drawer. While the drawer is open
  this holds for the rail's controls as well, whether the rail is beside the drawer or, below 782 px, inside it:
  Settings opened from the rail's Settings, by a press or a key, gives focus back to that button as it closes, the
  drawer still open.
- Widening the window to 1168 px or more closes it, and so does the last console going, which leaves no sidebar (see
  `docs/product/welcome-screen.md`).
- Closing the drawer while it holds keyboard focus — below 782 px in its rail too — moves focus to the terminal, and
  so does widening the window past 1168 px while it is open when the sidebar is hidden in the wider layout.

Whether the docked sidebar is hidden plays no part below 1168 px: there, the drawer decides whether it is on screen,
and widening the window again brings back the docked sidebar as the user last left it.

Because the drawer overlays the terminal rather than pushing it aside, **opening or closing it never resizes the
terminal**, so the running agent is never sent a terminal-size change by the drawer.

## Below 782 px: the rail comes in with the drawer

Below 782 px, while there is a console, **the rail is not in the row**: under the top bar the content panel spans the
window's whole width, and its top start corner is square rather than rounded (see "The window chrome and the content
panel" in `docs/product/window-layout.md`). The rail comes in only with the sidebar's drawer:

- **The open drawer is one card holding both**: the rail's column along the card's start side, then the sidebar, with
  a groove between them. The rail lies on the card's own surface, the sidebar's background, rather than on the
  chrome's colour, and holds what it holds at any width (see "The rail" in `docs/product/window-layout.md`). The card
  is drawn as in "Below 1168 px: the sidebar becomes a drawer" above.
- **The rail comes and goes with the drawer**, in the same movement, over the same 240 ms.
- **While the drawer is closed the rail is out of reach**, its exit included: nothing on it takes a press, keyboard
  focus or the attention of assistive technology, and `F6` skips it (see "Moving focus between regions with F6" in
  `docs/product/moving-focus-between-regions.md`). Focus on the rail as it goes — the drawer closing, or the window
  narrowing below 782 px with the drawer closed — is handed to the terminal, as focus in the drawer is.
- **With no console there is no drawer, and the rail stays in the row** at every width, being the way to New console
  and Settings there (see `docs/product/welcome-screen.md`); the content panel then keeps its rounded corner.

Under a right-to-left language the card is at the window's right edge, the rail at its right side (see
`docs/product/right-to-left-layout.md`).
