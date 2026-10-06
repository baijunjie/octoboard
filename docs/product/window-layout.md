# Window layout

The window has a **top bar** across its whole width and, under it, up to three panes, left to right: the
**sidebar** with the console → project → session tree (see "The console → project → session menu" in
`docs/product/sessions.md`), the selected session's **terminal**, and — only while the selected session is a hub
session — that console's **report panel** (see `docs/product/report-panel.md`). The connection banner, while the
daemon connection is down, sits between the top bar and the panes (see "Losing the daemon connection" in
`docs/product/application-lifecycle.md`). Toasts float over the bottom right (see "Toasts" below).

How the three panes are arranged depends on one width, **1100 px**. At that width and above they sit side by
side in a row, the layout the macOS window is always in; a pane the user has hidden there can still float in over
the terminal for a moment. Below it the sidebar and the report panel become overlay drawers over the terminal; only
a client with no window minimum of its own, such as the UI opened in a plain browser, can get there.

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
  - the **connection status**, described below;
  - the report panel toggle ("Show report" / "Hide report"), shown only while the selected session is a hub session;
  - **Settings**, which opens the settings dialog.

What the two toggles do depends on the window's width: see "Hiding the sidebar and the report panel" and "Below
1100 px: the sidebar and the report panel become drawers" below. Each toggle's label and icon say what pressing it
will do: "Show …" with an open-pane icon while its pane is hidden, "Hide …" with a close-pane icon while it is shown.
A hidden pane that is only floating in counts as hidden. Pressing any of the bar's controls leaves keyboard focus
where it was, so the terminal keeps receiving keystrokes.

**In the macOS application the top bar is the window's titlebar.** The window has no native titlebar background and
shows no title text of its own (the window is still called Octoboard in the Dock and in Mission Control). The
window's close, minimise and zoom buttons sit inside the bar at its left end, and the bar keeps that space clear for
them; in fullscreen, where those buttons are gone, the space goes as well. Dragging any part of the bar that is not
a control moves the window, and double-clicking it zooms the window, as with a native titlebar. Opened in a plain
browser, the bar has none of this: it starts at the left edge and does not move anything.

### The connection status

One indicator covers both the connection to the daemon and the selected session's terminal connection. **While both
are healthy it shows nothing.** On trouble it shows a labelled chip, amber while things are still being retried and
red once they are not:

| Situation | Chip |
|---|---|
| The daemon connection is being established | Connecting… |
| The daemon connection dropped and is being retried | Reconnecting… |
| The daemon connection's automatic attempts are spent | Disconnected |
| The daemon is connected; the selected session's terminal dropped and is being retried | Terminal reconnecting… |
| The daemon is connected; the terminal's automatic attempts are spent | Terminal disconnected, with a **Reconnect** button beside it |

The daemon's state comes first: while it is not connected, the terminal's state is not shown. A terminal is in trouble
only while its session's process is running — a session that is not running has nothing to connect to — and a first
connection still in progress is not trouble until an attempt has failed. How the terminal reconnects is in "The
terminal" in `docs/product/sessions.md`; the daemon connection's own banner and its Retry are in "Losing the daemon
connection" in `docs/product/application-lifecycle.md`.

## The window's minimum size

The macOS window has a **minimum size of 1100×600**. The width is the three panes' own floors added up, with the
sidebar at its default width, so none of them can be squeezed past usability by a narrower window; the height leaves
the terminal about 30 rows.

Nothing enforces a minimum when the UI is opened in a browser, so the narrow layout below applies there.

## The window's size and position across launches

The macOS window reopens with the **size, position and maximized state it had when the application last quit**. A
browser tab has no window of its own to place, so none of this applies there.

- **First launch**, or a saved state that cannot be read: the window opens at **1200×760**, placed by the system,
  which centres it.
- The state is saved whenever the application quits normally — every way of quitting in "Quitting" in
  `docs/product/application-lifecycle.md`. A crash or a killed process saves nothing, and the next launch uses what the
  last normal quit saved.
- What is kept is the window's normal frame plus whether it was maximized: a window quit maximized reopens maximized,
  on the display its frame is on, and un-maximizes back to that frame. **Native fullscreen and minimized are never
  restored**: a window quit in either state reopens as it was before entering it.
- The saved frame is checked against the displays connected at launch, measured by their usable area (without the
  menu bar and the Dock). It is restored when at least 200×20 points of its top 40 points — the top bar, which is
  where the window is grabbed to move it — lie on one connected display. The restored frame is then held inside the
  rectangle spanning all the connected displays, its size first (never below the 1100×600 minimum) and then its
  position, so a window spanning displays that are all still there comes back unchanged, and one that reached onto a
  display since unplugged is pulled back onto the ones left.
- Otherwise — its display is gone, its top bar is off every display, or pulling it back would leave its top bar off
  every display — the window opens at 1200×760, centred on the main display. A saved maximized state is kept in this
  case too.
- The frame is decided before the window is shown, so the window is never seen moving into place.

The state is kept in `~/Library/Application Support/dev.octoboard.app/window-state.json` (see "Files Octoboard owns"
in `docs/product/application-lifecycle.md`).

## Pane widths at 1100 px and wider

| Pane | Default | Range the user can choose |
|---|---|---|
| Sidebar | 280 px | 200–480 px |
| Report panel | 420 px | 300–720 px |
| Terminal | everything left over | never below 520 px (55 columns) |

The sidebar and the report panel are drawn at the width the user chose (see "Resizing the sidebar and the report
panel" below), and neither grows past it: width a wider window frees goes to the terminal.

**The terminal always keeps its 520 px.** Where the window cannot afford every pane its chosen width, the report panel
gives up width first, down to 300 px, and only then the sidebar, down to 200 px. At the 1100 px minimum with both
panes shown that makes the sidebar 280 px, the terminal 520 px and the report panel 300 px. This never changes the
remembered width: when the window widens again, each pane gets its chosen width back.

A pane the user has hidden takes no width at all, and neither does one only floating in: its width goes to the
terminal, and a hidden report panel stops holding back the sidebar's width (see "Hiding the sidebar and the report
panel" below). With no hub session selected there is no report panel in the row either.

Anything that changes the terminal's size — resizing the window, resizing a pane, hiding or showing a pane —
resizes the agent's terminal (see "The terminal" in `docs/product/sessions.md`).

### Resizing the sidebar and the report panel

Each pane has a resize handle on its inner edge: the sidebar's on its right edge, the report panel's on its left. The
sidebar's handle runs the full height of the window, from the window's top through the top bar's left segment, where
pressing it resizes rather than moving the window; the report panel's starts below the top bar (and the connection
banner, while it is shown).

- **Dragging** the handle sets the pane's width, within its range in the table above.
- **Double-clicking** it returns the pane to its default width.
- The handle is also reachable with Tab and is then operated from the keyboard: the arrow pointing away from the pane
  widens it and the other narrows it (Right widens the sidebar, Left widens the report panel), by 16 px, by 64 px with
  Shift held; Home sets the pane's minimum, and End the widest the window currently allows. Pressing the handle with
  the mouse does not take keyboard focus off the terminal.

A handle is there only while its pane is shown in the row: not while the pane is hidden or floating in, and not below
1100 px, where the panes are fixed-width drawers.

**Each pane's chosen width is remembered per client**, in that client's own browser storage, as the appearance choice
is (see "Where the choice is stored" in `docs/product/appearance.md`); without that storage a choice lasts until the
page is reloaded.

## Hiding the sidebar and the report panel

At 1100 px and wider, the top bar's two toggles hide and show the docked panes:

- The sidebar toggle hides or shows the sidebar.
- The report toggle, there only while a hub session is selected, hides or shows the report panel. A hidden report
  panel stays hidden for every hub session selected afterwards, until it is shown again.

Both panes are shown by default. **Whether each one is hidden is remembered per client**, the same way as the panes'
widths. Hiding a pane hands its width to the terminal.

Hiding a pane that holds keyboard focus — a tree row reached with Tab, say, or a report page — moves focus to the
terminal.

### A hidden pane floats in on hover

At 1100 px and wider, a hidden pane can be brought up for a moment without showing it for good. This is a **floating
pane**:

- **What brings it up**: the mouse reaching the window's edge on the pane's side — an 8 px strip along the left edge
  for the sidebar, a 4 px strip along the right edge for the report panel, both from below the top bar (and the
  connection banner) down — or the mouse resting on the pane's toggle in the top bar. Touch and pen do neither.
- **How it shows**: it slides in over the terminal in 200 ms (at once where the system asks for reduced motion), at
  the pane's chosen width — the report panel at most 92% of the window's width — with a shadow and no dimming behind
  it. It overlays the terminal, so **the terminal is not resized** and the agent is sent no size change.
- **How it goes away**: it slides away 200 ms after the pointer leaves it, or leaves the toggle without moving onto
  it. `Escape` sends it away at once and **does not reach the running agent**; so does `Escape` pressed inside a
  report page (see "Submitting a form back to the hub" in `docs/product/report-panel.md`). It stays up, whatever the
  pointer does, while it holds keyboard focus or one of its menus is open. A floating pane that holds keyboard focus
  as it goes away hands focus to the terminal.
- **Only one at a time**: bringing one up sends the other away.
- **Pressing the toggle** while the pane is floating shows it for good: it joins the row, and keyboard focus stays
  where it was. Until then the toggle still reads "Show …".
- The report panel floats in only while a hub session is selected. There are no floating panes below 1100 px, and
  narrowing the window below it sends one away.

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

## Toasts

Failures and notices that need no answer are shown as **toasts**; which ones are, and how they name the session they
are about, is in "Losing the daemon connection" in `docs/product/application-lifecycle.md`.

- **Always at the window's bottom right**, floating over whatever is there, on the main screen and on the screen
  shown while connecting to the daemon alike, whether or not Settings or another dialog is open. They never move the
  layout. The newest is at the front.
- A toast about a particular session is titled with where that session is — the project it runs in, or the console
  whose hub it is — with the message under it; any other toast is just the message. An error is marked as one; a
  notice is not.
- **A toast dismisses itself**: a notice after about 5 seconds, an error after about 8. The countdown pauses while
  the pointer is over the toasts or keyboard focus is inside them. Each toast also has a close button.
- **An identical toast** — the same kind, message and session — arriving while one is still shown does not stack a
  second copy: the existing one is replaced by a fresh one at the front, its countdown started over.
- **A toast's text can be selected and copied.** A press on a toast still leaves keyboard focus where it was, so
  typing goes on reaching the terminal: a press on the close button or on the toast around its text never takes focus,
  and a plain click on the text, which takes it for a moment, hands it straight back. A drag that leaves text
  selected keeps focus in the toast so `Cmd+C` copies it, and the copy hands focus back. Focus goes back to whatever
  had it before, or to the terminal when that is gone.
- There is no keyboard shortcut to jump to the toasts. **Known limitation**: while a toast is shown, `F6` moves
  keyboard focus onto the toasts, as it cycles through the window's regions.
