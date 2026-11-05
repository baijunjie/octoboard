# The floating sidebar

When the sidebar cannot fit beside the terminal, it floats instead of taking a column (see "Docking capacity" in
`docs/product/pane-docking.md`). At 768 px and below the rail comes in with it as part of the same card. The layout
thresholds and hover entry areas are in "A hidden pane floats in on hover" in `docs/product/pane-docking.md`; what the
sidebar shows is in `docs/product/sidebar.md`.

## When the sidebar cannot dock

The sidebar starts closed and overlays the terminal, so opening or closing it never resizes the agent terminal. Its
docking preference is kept separately: widening until it can dock restores the sidebar as the user left it. The top bar
stays reachable, and above 768 px the rail stays in the row beside the floating card.

- **Above 768 px**, the top bar's sidebar button reveals the sidebar. Pressing it again keeps the sidebar open; it never
  changes the docking preference at this width. A mouse can also reveal it from the button, blank rail space or console
  avatars, and leaving hides it under the rules in "A hidden pane floats in on hover" in `docs/product/pane-docking.md`.
  Hover-revealed cards have no dimmed background. On a device whose primary input cannot hover, pressing the button
  opens the sidebar with a dimmed area behind it; another press still keeps it open.
- **At 768 px and below**, pressing that button opens or closes the sidebar, and touch swipes do too (see
  `docs/product/touch-swipes.md`). There is no hover reveal. The dimmed area covers the content panel around the card,
  including its gaps, while the top bar and the connection banner's Retry remain reachable.
- **Appearance**: rounded corners, a hairline edge and the light card shadow, inset 8 px from the top bar, the rail and
  the window's bottom or the bottom banners. At 768 px and below the gaps are 4 px, measured from the window's edge
  because the rail is inside the card. Header heights are in "The window chrome and the content panel" in
  `docs/product/window-layout.md`.
- **Motion**: above 768 px it fades in while rising by 4% of its height, and fades out while sinking back. At 768 px and
  below it instead slides 40 px from the start edge as it fades in, and back towards that edge as it fades out. Both
  take 240 ms with `cubic-bezier(0.2, 0, 0, 1)` easing. Reduced motion removes the movement and delay. While closed, its
  exit included, the sidebar takes no presses or keyboard focus and is hidden from assistive technology.
- **Width**: above 768 px, it uses the sidebar's remembered width and can be resized at its inner edge (see "Resizing
  the sidebar and the right pane" in `docs/product/pane-docking.md`). Below that, it is 328 px including the 48 px rail,
  with no width handle. It is capped at the window's width less the rail and two surrounding gaps, so it leaves room
  beside the card to dismiss an explicitly opened overlay.
- **Dismissal**: `Escape` closes it without reaching the running agent; in a text field holding text it clears the field
  first. A press on the dimmed area dismisses an explicitly opened overlay. Revealing the right pane closes it, and
  opening the sidebar hides a floating right pane: only one floating side is revealed at a time.
- **Selection**: at 768 px and below, selecting a session in the sidebar or its rail closes the card so the session's
  terminal shows. Above 768 px, selecting a session leaves the floating sidebar up until its normal dismissal.
- **Focus**: dismissing the sidebar while it holds keyboard focus hands focus to the terminal. A dialog opened from an
  explicitly opened sidebar or its rail can return focus to its opening control when it closes. Widening until the
  sidebar can dock closes the overlay, handing focus to the terminal if the restored docked sidebar is hidden. Deleting
  the last console also closes it because no sidebar remains (see `docs/product/welcome-screen.md`).

## At 768 px and below: the rail comes in with the sidebar

While there is a console, the rail leaves the row at 768 px and below. The content panel spans the window and its top
start corner is square. The floating sidebar is one card: the rail along its start side, then the sidebar, with a groove
between them. Both use the sidebar's surface. The rail retains its controls and contents (see "The rail" in
`docs/product/window-layout.md`).

The rail appears and disappears with the card over the same 240 ms. While closed, its exit included, none of the rail
takes presses, keyboard focus or assistive-technology attention. `F6` skips it. Focus left on the rail when it goes
away, including when narrowing to 768 px or less with the sidebar closed, is handed to the terminal.

With no console there is no sidebar, so the rail stays in the row at every width as the way to New console and
Settings; the content panel keeps its rounded corner (see `docs/product/welcome-screen.md`). Under a right-to-left
language the card and rail are at the window's right side (see `docs/product/right-to-left-layout.md`).
