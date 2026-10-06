# Window layout

The window has a **top bar** across its whole width and, under it, up to three panes, left to right: the
**sidebar** with the console → project → session tree (see "The console → project → session menu" in
`docs/product/sessions.md`), the selected session's **terminal**, and — only while the selected session is a hub
session — that console's **report panel** (see `docs/product/report-panel.md`). The connection banner, while the
daemon connection is down, sits between the top bar and the panes (see "Losing the daemon connection" in
`docs/product/application-lifecycle.md`).

How the three panes are arranged depends on one width, **1100 px**. At that width and above they sit side by
side in a row, the layout the macOS window is always in. Below it the sidebar and the report panel become
overlay drawers over the terminal; only a client with no window minimum of its own, such as the UI opened
in a plain browser, can get there.

For the window's light/dark appearance see `docs/product/appearance.md`; for the settings the top bar opens see
`docs/product/settings.md`.

## The top bar

The top bar is on every screen, the same height everywhere. On the screens shown before there is anything to
show — while connecting to the daemon, and the screens for a daemon that could not start or a UI that failed — it is
there but empty.

It has three parts:

- **Left**: the sidebar toggle ("Show sessions" / "Hide sessions") and **New console**. At 1100 px and wider, while
  the sidebar is shown, this part is exactly as wide as the sidebar, with the sidebar's edge line under it, so the
  two read as one column; it follows the sidebar's width while the sidebar is being resized. Otherwise it is only as
  wide as its controls.
- **Middle**: the selected session's breadcrumb — *console › project › session title* for a project session,
  *console › Hub* for a hub session — followed by the session's status icon. A trail too long for the bar fades out at
  its right edge, the status icon always stays visible, and the full trail is then the breadcrumb's tooltip. With no
  session selected the middle is empty.
- **Right**, left to right:
  - the **waiting count**, a raised hand and the number of sessions waiting for the user, shown only while at least
    one is; pressing it goes to the next waiting session (see "The raised hand" in `docs/product/sessions.md`);
  - the **connection dot**: green for connected, amber for connecting or reconnecting, red for disconnected, with the
    state as its tooltip;
  - the report panel toggle ("Show report" / "Hide report"), shown only while the selected session is a hub session;
  - **Settings**, which opens the settings dialog.

What the two toggles do depends on the window's width: see "Hiding the sidebar and the report panel" and "Below
1100 px: the sidebar and the report panel become drawers" below. Pressing any of the bar's controls leaves keyboard
focus where it was, so the terminal keeps receiving keystrokes.

**In the macOS application the top bar is the window's titlebar.** The window has no native titlebar background and
shows no title text of its own (the window is still called Octoboard in the Dock and in Mission Control). The
window's close, minimise and zoom buttons sit inside the bar at its left end, and the bar keeps that space clear for
them; in fullscreen, where those buttons are gone, the space goes as well. Dragging any part of the bar that is not
a control moves the window, and double-clicking it zooms the window, as with a native titlebar. Opened in a plain
browser, the bar has none of this: it starts at the left edge and does not move anything.

## The window's minimum size

The macOS window has a **minimum size of 1100×600**. The width is the three panes' own floors added up, with the
sidebar at its default width, so none of them can be squeezed past usability by a narrower window; the height leaves
the terminal about 30 rows.

Nothing enforces a minimum when the UI is opened in a browser, so the narrow layout below applies there.

## Pane widths at 1100 px and wider

- The sidebar is 280 px wide by default and can be resized; see "Resizing the sidebar" below.
- The report panel is 420 px wide and does not grow: width a wider window frees goes to the terminal.
- As the window narrows it is the panel that gives up width, down to a floor of 300 px.
- The terminal never goes below 55 columns (520 px).

A pane the user has hidden takes no width at all; its width goes to the terminal (see "Hiding the sidebar and the
report panel" below).

Anything that changes the terminal's size — resizing the window, resizing the sidebar, hiding or showing a pane —
resizes the agent's terminal (see "The terminal" in `docs/product/sessions.md`).

### Resizing the sidebar

The sidebar has a resize handle on its right edge.

- **Dragging** the handle sets the sidebar's width, between **200 and 480 px**.
- **Double-clicking** it returns the sidebar to its default 280 px.
- The handle is also reachable with Tab and is then operated from the keyboard: Left and Right arrows narrow and widen
  the sidebar by 16 px, by 64 px with Shift held; Home sets the minimum, 200 px, and End the widest the window
  currently allows. Pressing the handle with the mouse does not take keyboard focus off the terminal.

**The chosen width is remembered per client**, in that client's own browser storage, as the appearance choice is
(see "Where the choice is stored" in `docs/product/appearance.md`); without that storage a choice lasts until the
page is reloaded.

**The sidebar never squeezes the other panes below their floors.** It is drawn at the chosen width only as far as the
window leaves the terminal its 520 px and, while the report panel is on screen, the panel its 300 px; where the window
cannot afford more, the sidebar is drawn narrower, never under 200 px. This does not change the remembered width: when
the window widens again, or the report panel is hidden or goes away, the sidebar gets its chosen width back.

Below 1100 px the sidebar is a drawer that is always 280 px wide, and there is no handle.

## Hiding the sidebar and the report panel

At 1100 px and wider, the top bar's two toggles hide and show the docked panes:

- The sidebar toggle hides or shows the sidebar.
- The report toggle, there only while a hub session is selected, hides or shows the report panel. A hidden report
  panel stays hidden for every hub session selected afterwards, until it is shown again.

Both panes are shown by default. **Whether each one is hidden is remembered per client**, the same way as the
sidebar's width. Hiding a pane hands its width to the terminal; a hidden report panel also stops holding back the
sidebar's width (see "Resizing the sidebar" above).

Hiding a pane that holds keyboard focus — a tree row reached with Tab, say, or a report page — moves focus to the
terminal.

## Below 1100 px: the sidebar and the report panel become drawers

Below 1100 px wide, the terminal is the only pane in the row and fills the width. The sidebar and the
report panel each become an **overlay drawer** over it, **closed by default**:

- The sidebar drawer slides in from the left and is 280 px wide; the report panel's drawer slides in from
  the right and is 420 px wide, or 92% of the viewport where that is narrower. Both start below the top bar (and the
  connection banner, when it is shown), so the bar stays usable while a drawer is open.
- Each is opened and closed with the same top bar toggle that hides and shows it at 1100 px and wider — the sidebar
  toggle on the left, the report toggle on the right. The report toggle is there only while the selected session is a
  hub session, since no other session has a panel.
- A drawer closes on its toggle, on a press anywhere in the dimmed area beside it, or on `Escape`.
  While a drawer is open `Escape` closes it and **does not reach the running agent**.
- Opening one drawer closes the other; at most one is ever open.
- Widening the window to 1100 px or more closes both, and selecting a session that has no report panel
  closes the report drawer.
- Closing a drawer that holds keyboard focus moves focus to the terminal, and so does widening the window past 1100 px
  while it is open when that pane is hidden in the wider layout.

Whether a docked pane is hidden plays no part below 1100 px: there, the drawers decide what is on screen, and
widening the window again brings back each docked pane as the user last left it.

Because the drawers overlay the terminal rather than pushing it aside, **opening or closing one never
resizes the terminal**, so the running agent is never sent a terminal-size change by a drawer.

The terminal's floor below the breakpoint is 382 px, about 40 columns. A viewport narrower than that
**scrolls horizontally** rather than squeezing the terminal further.
