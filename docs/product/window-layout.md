# Window layout

The window is made of **window chrome** and a **content panel**. The chrome is a **top bar** across the window's whole
width and, under it, a narrow **rail** down the window's left edge (its right edge under a right-to-left language, see
"Right-to-left layout" below). Everything else is the content panel, which holds up to three panes, left to right: the
**sidebar**, showing one console's console sessions, projects and sessions (see `docs/product/sidebar.md`), the
selected session's **terminal**, which the archive view covers while it is open (see "The archive view" in
`docs/product/sidebar.md`), and — only while the selected session is a console session — that console session's
**report panel** (see `docs/product/report-panel.md`). The connection banner, while the daemon connection is down, is
a full-width strip along the window's bottom edge, under the rail and the content panel (see "Losing the daemon
connection" in `docs/product/application-lifecycle.md`). Toasts float over the bottom right, above the banner while it
is shown (see `docs/product/toasts.md`).

How the three panes are arranged depends on one width, **1148 px**. At that width and above they sit side by
side in a row, the layout the macOS window is always in; a pane the user has hidden there can still float in over
the terminal for a moment. Below it the sidebar and the report panel become overlay drawers over the terminal; only
a client with no window minimum of its own, such as the UI opened in a plain browser, can get there.

For the window's light/dark appearance see `docs/product/appearance.md`; for the settings the rail opens see
`docs/product/settings.md`; for the top bar's Back and Forward see `docs/product/navigation-history.md`.

## The window chrome and the content panel

The top bar is **40 px** high and the rail **48 px** wide. They carry only icons, the consoles' avatars and the one line
of the breadcrumb.

- **In the macOS application** the top bar and the rail show the system's translucent sidebar material, which follows
  the window's light or dark appearance (see "What follows the choice" in `docs/product/appearance.md`).
- **In a browser** the chrome is an opaque colour of its own, which follows the appearance too.

The **content panel** is always opaque, whatever is behind the window. Its top left corner (top right under a
right-to-left language), where the top bar meets the rail, is rounded, and a thin line runs along its top edge and
along its edge beside the rail; it reaches the window's other side and its bottom (or the connection banner) with no
gap. Inside it the panes are set apart by thin lines too. The terminal's area is the colour of the terminal's own
background, and the sidebar a shade apart from it — a touch greyer in the light appearance, a touch lighter in the
dark one.

On the screens shown before there is anything to show — while connecting to the daemon, and the screens for a daemon
that could not start or a UI that failed — there is no rail: under the top bar, the content panel spans the window.

## The top bar

The top bar is on every screen, the same height everywhere. On the screens shown before there is anything to
show it is there but empty.

It runs, left to right:

- **Back** and **Forward**, which move through the places the window has shown (see
  `docs/product/navigation-history.md`); each is disabled while there is nowhere to go in its direction.
- The sidebar toggle ("Show sessions" / "Hide sessions").
- The selected session's **breadcrumb** — *console › project › session title* for a project session, *console › session
  title* for a console session, which belongs to no project — followed by the session's status icon. At 1148 px and
  wider, while the sidebar is shown, the breadcrumb starts where the terminal's area does, over the sidebar's inner
  edge, and follows that edge while the sidebar is resized and while the docked column eases open or closed (see
  "Hiding the sidebar and the report panel" below). The start of the bar never becomes narrower than the buttons there
  — in the macOS application, that includes the clear space kept for the window buttons. Once the column no longer
  reaches past that start, the breadcrumb stays beside the sidebar toggle, which is also where it sits while the
  sidebar is hidden. Below 1148 px it starts right after the sidebar toggle as well. A trail too long for
  the bar fades out at its right edge, as in "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`, and the status icon always stays visible. With no session selected there is no
  breadcrumb. While the archive view is open (see "The archive view" in `docs/product/sidebar.md`) the breadcrumb shows
  where that is instead, with no status icon: *console › project › Archived sessions*, *console › console session ›
  Archived bound sessions*, or *console › Archived console sessions*.

Nothing else is in the bar: the window's other controls are on the rail (see "The rail" below).

**In the macOS application the top bar is the window's titlebar.** The window has no native titlebar background and
shows no title text of its own (the window is still called Octoboard in the Dock and in Mission Control). The window's
close, minimise and zoom buttons sit inside the bar at its left end, and the bar keeps that space clear for them, so
Back comes after them; in fullscreen, where those buttons are gone, the space goes as well. Under a right-to-left
language the buttons and their clear space stay at the left end, which then holds the bar's end rather than its
start. Dragging any part of the bar that is not a control moves the window, and double-clicking it zooms the window,
as with a native titlebar. Opened in a plain browser, the bar has none of this: it starts at the left edge and does
not move anything.

## The rail

The rail runs from under the top bar to the window's bottom (or the connection banner). Its two ends hold:

- **At the top, the consoles**: one avatar per console, in the order the consoles were created, the current console
  marked by a tile behind its avatar, then **New console**, which opens the new-console dialog (see "Consoles" in
  `docs/product/consoles-and-projects.md`). Choosing a console, what each avatar's badge says and what a right-click
  on one offers are in "The console switcher" in `docs/product/sidebar.md`. While the docked sidebar is hidden, resting
  the mouse on an avatar floats the sidebar in showing that console (see "A hidden pane floats in on hover" below).
  When there are more consoles than fit, the avatars scroll, with no scrollbar drawn; New console stays below them.
- **At the bottom**, top to bottom:
  - the **waiting count**, a raised hand with the number of sessions waiting for the user in a badge at its top
    right, shown only while at least one is (99+ beyond 99); pressing it goes to the next waiting session (see "The
    raised hand" in `docs/product/sessions.md`);
  - the **connection status**, described below;
  - **Turn on notifications**, a bell with a small dot on it, shown only in a browser whose answer to notifications is
    still undecided; pressing it asks the browser (see "Notifications" in `docs/product/settings.md`);
  - the report panel toggle ("Show report" / "Hide report"), shown only while the selected session is a console
    session;
  - **Settings**, which opens the settings dialog.

The rail is the same at every width; below 1148 px it stays beside the drawers, which open past it.

### The two pane toggles

What the sidebar toggle in the top bar and the report toggle on the rail do depends on the window's width: see
"Hiding the sidebar and the report panel" and "Below 1148 px: the sidebar and the report panel become drawers" below.
Each toggle's label says what pressing it will do — "Show …" while its pane is hidden, "Hide …" while it is shown —
and its icon shows the pane's current state: a window with that side's pane drawn solid while the pane is shown, and
drawn dashed while it is hidden. Each toggle also tells assistive technology whether its pane is expanded: expanded
while the pane is shown, which below 1148 px means its drawer is open. A hidden pane that is only floating in counts
as hidden for both. For the sidebar, shown and hidden follow the docked state at once, including while its column is
still easing open or closed (see "Hiding the sidebar and the report panel" below): the toggle already reads "Hide …"
as the column opens and "Show …" as it closes.

### Pressing the chrome's controls

Every control in the top bar and on the rail has a tooltip (see "Tooltips on icon-only controls" in
`docs/product/labels-and-tooltips.md`); the rail's open toward the content panel. Pressing any of them leaves keyboard
focus where it was, so after a mouse press the terminal keeps receiving keystrokes. Turn on notifications pressed from
the keyboard keeps focus on itself while the browser's prompt is up, so no keystrokes reach the session meanwhile; once
the browser answers and the bell goes away, focus moves to the terminal, or to the top bar's first enabled control when
the terminal cannot take it.

### The connection status

One indicator on the rail covers both the connection to the daemon and the selected session's terminal connection.
**While both are healthy it shows nothing.** On trouble it shows an icon, an amber turning arrow while things are still
being retried and a red unplugged cord once they are not; the label below is its tooltip and accessible name:

| Situation | Label |
|---|---|
| The daemon connection is being established | Connecting… |
| The daemon connection dropped and is being retried | Reconnecting… |
| The daemon connection's automatic attempts are spent | Disconnected |
| The daemon is connected; the selected session's terminal dropped and is being retried | the terminal is reconnecting |
| The daemon is connected; the terminal's automatic attempts are spent | Terminal disconnected, with a **Reconnect terminal** button under it, shown as a plug icon |

The daemon's state comes first: while it is not connected, the terminal's state is not shown. A terminal is in trouble
only while its session's process is running — a session that is not running has nothing to connect to — and a first
connection still in progress is not trouble until an attempt has failed. How the terminal reconnects is in "The
terminal" in `docs/product/sessions.md`; the daemon connection's own banner and its Retry are in "Losing the daemon
connection" in `docs/product/application-lifecycle.md`.

The indicator is a Tab stop that shows its label as a tooltip while it has focus. When it, or the Reconnect terminal
button, disappears while it holds focus, because the connection came back, focus goes to the terminal.

## The window's minimum size

The macOS window has a **minimum size of 1148×600**. The width is the rail's 48 px plus the three panes' own floors,
with the sidebar at its default width (48 + 280 + 520 + 300), so none of the panes can be squeezed past usability by a
narrower window; the height leaves the terminal about 30 rows.

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
  rectangle spanning all the connected displays, its size first (never below the 1148×600 minimum) and then its
  position, so a window spanning displays that are all still there comes back unchanged, and one that reached onto a
  display since unplugged is pulled back onto the ones left.
- Otherwise — its display is gone, its top bar is off every display, or pulling it back would leave its top bar off
  every display — the window opens at 1200×760, centred on the main display. A saved maximized state is kept in this
  case too.
- The frame is decided before the window is shown, so the window is never seen moving into place.

The state is kept in `~/Library/Application Support/dev.octoboard.app/window-state.json` (see "Files Octoboard owns"
in `docs/product/application-lifecycle.md`).

## Pane widths at 1148 px and wider

| Pane | Default | Range the user can choose |
|---|---|---|
| Sidebar | 280 px | 200–480 px |
| Report panel | 420 px | 300–720 px |
| Terminal | everything left over | never below 520 px (about 53 columns) |

The sidebar and the report panel are drawn at the width the user chose (see "Resizing the sidebar and the report
panel" below), and neither grows past it: width a wider window frees goes to the terminal.

**The terminal always keeps its 520 px.** Where the window cannot afford every pane its chosen width, the report panel
gives up width first, down to 300 px, and only then the sidebar, down to 200 px. At the 1148 px minimum with both panes
shown that makes, beside the 48 px rail, the sidebar 280 px, the terminal 520 px and the report panel 300 px. This never
changes the remembered width: when the window widens again, each pane gets its chosen width back.

A pane the user has hidden takes no width at all once it is hidden, and neither does one only floating in: its width
goes to the terminal, and a hidden report panel stops holding back the sidebar's width (see "Hiding the sidebar and
the report panel" below). While the docked sidebar's column is easing closed, it still takes the width it has
reached. With no console session selected there is no report panel in the row either.

Anything that changes the terminal's size — resizing the window, resizing a pane, hiding or showing a pane —
resizes the agent's terminal (see "The terminal" in `docs/product/sessions.md`).

### Resizing the sidebar and the report panel

Each pane has a resize handle on its inner edge: the sidebar's on its right edge, the report panel's on its left. Both
run the pane's full height, from below the top bar to above the connection banner, while it is shown.

- **Dragging** the handle sets the pane's width, within its range in the table above.
- **Double-clicking** it returns the pane to its default width.
- The handle is also reachable with Tab and is then operated from the keyboard: the arrow pointing away from the pane
  widens it and the other narrows it (Right widens the sidebar and Left the report panel; the other way round under a
  right-to-left language, where the panes have swapped sides), by 16 px, by 64 px with Shift held; Home sets the pane's
  minimum, and End the widest the window currently allows. While it has keyboard focus the handle is drawn as a solid
  accent-coloured bar with a focus ring around it. Pressing the handle with the mouse does not take keyboard focus off
  the terminal.

A handle is there only while its pane is shown in the row: not while the pane is hidden or floating in, and not below
1148 px, where the panes are fixed-width drawers. The sidebar's handle is also absent while that column is easing
open or closed; it is there only once the column is fully open. The report panel's handle is there whenever that
panel is shown in the row.

**Each pane's chosen width is remembered per client**, in that client's own browser storage, as the appearance choice
is (see "Where the choice is stored" in `docs/product/appearance.md`); without that storage a choice lasts until the
page is reloaded.

## Hiding the sidebar and the report panel

At 1148 px and wider, the two pane toggles hide and show the docked panes:

- The sidebar toggle, in the top bar, hides or shows the sidebar.
- The report toggle, on the rail and there only while a console session is selected, hides or shows the report panel. A
  hidden report panel stays hidden for every console session selected afterwards, until it is shown again.

Both panes are shown by default. **Whether each one is hidden is remembered per client**, the same way as the panes'
widths. Hiding a pane hands its width to the terminal.

Hiding a pane that holds keyboard focus — a sidebar row reached with Tab, say, or a report page — moves focus to the
terminal.

At 1148 px and wider, hiding or showing the docked sidebar eases that column over **200 ms**, the same length as the
overlay drawer's slide. Where the system asks for reduced motion, the column opens or closes at once. Pressing the
toggle again while the column is still moving reverses the ease. Below 1148 px this ease does not apply: the sidebar
is the drawer, and that drawer slides (see "Below 1148 px: the sidebar and the report panel become drawers" below).
Hiding and showing the report panel are immediate. Floating it still slides in and out over 200 ms (see "A hidden pane floats in on hover" below).

During the ease the sidebar stays a full-height column, flush with the rail, from below the top bar to above the
connection banner when that banner is shown, and it does not become the rounded card in "A hidden pane floats in on
hover" below. Its contents stay at the width the open column is drawn at, including where the window is holding that
width back (see "Pane widths at 1148 px and wider" above), and stay against the rail. The inner edge covers them as
the column closes and uncovers them as it opens, so the text does not reflow. The hover card comes up only once the
column has closed.

As the column closes, the width it gives up goes to the terminal. When the report panel is docked and had been held
below the width the user chose, it can grow into that room as the room appears. Showing the sidebar limits the report
panel again as soon as the sidebar is shown, when the window cannot afford both panes their chosen widths, and the
opening column takes its own width from the terminal as it eases open. The terminal is resized by either, as in
"Pane widths at 1148 px and wider" above.

### A hidden pane floats in on hover

At 1148 px and wider, a hidden pane can be brought up for a moment without showing it for good. This is a **floating
pane**. On the sidebar, the rounded corners, the gaps, the shadow and the slide from behind the rail are this card.
They are not how the docked column opens and closes (see "Hiding the sidebar and the report panel" above).

- **What brings up the sidebar**: the mouse resting for **130 ms** on a console's avatar on the rail, the current
  console's or another's. The sidebar then shows the console whose avatar the mouse is on, which need not be the
  current one; what that preview is and how a press inside it makes the console current is in "Previewing a console
  from the rail" in `docs/product/sidebar.md`. While the sidebar is out, moving onto another avatar shows that console
  at once, with no wait. A pointer that only crosses the avatars brings nothing up. Neither the sidebar toggle nor the
  content panel's edge brings the sidebar up.
- **What brings up the report panel**: the mouse staying for **200 ms** on a 4 px strip along the window's right edge,
  from below the top bar to above the connection banner, or, at once, the mouse reaching the report toggle.
- Touch and pen bring up neither.
- **How it shows**: it slides in over the terminal in 200 ms (at once where the system asks for reduced motion), at the
  pane's chosen width — the report panel at most 92% of the window's width — with a shadow and no dimming behind it.
  It overlays the terminal, so **the terminal is not resized** and the agent is sent no size change.
  - The **sidebar** floats as a card: it keeps an 8 px gap from the rail, from the top bar and from the window's
    bottom (or the connection banner), has rounded corners all round and a border. It slides in from under the rail
    and out again behind it, never drawn over the rail.
  - The **report panel** slides in from the window's right edge and runs from the top bar to the window's bottom (or
    the connection banner).
- **How it goes away**: it slides away 200 ms after the pointer has left it. For the sidebar that means the pointer is
  on neither the pane nor a console's avatar; for the report panel, on neither the pane nor the report toggle. Once the
  report panel has come up from its edge strip, the pointer's first move counts as leaving it when it lands outside the
  place the panel is sliding into. `Escape` sends it away at once and **does not reach the running agent**; so does
  `Escape` pressed inside a report page (see "Escape and F6 inside a page" in `docs/product/report-panel.md`). It stays
  up, whatever the pointer does, while it holds keyboard focus or while a menu or a dialog is open, such as one opened
  from it. A floating pane that holds keyboard focus as it goes away hands focus to the terminal. A floating sidebar
  previewing a console that is deleted goes away as if the pointer had left that console's avatar.
- **Only one at a time**: bringing one up sends the other away.
- **Pressing the toggle** while the pane is floating shows it for good, and keyboard focus stays where it was. Until
  the pane is docked the toggle still reads "Show …". The report panel joins the row. The sidebar's card goes away
  and the docked column eases open in the row; which console that column shows is in "Previewing a console from the
  rail" in `docs/product/sidebar.md`.
- The report panel floats in only while a console session is selected. There are no floating panes below 1148 px,
  and narrowing the window below it sends one away.

## Below 1148 px: the sidebar and the report panel become drawers

Below 1148 px wide, the top bar and the rail stay as they are, and the terminal is the only pane in the content panel
and fills its width. The sidebar and the report panel each become an **overlay drawer** over it, **closed by
default**:

- The sidebar drawer slides in from the rail's edge and is 280 px wide; the report panel's drawer slides in from
  the window's right edge and is 420 px wide, or 92% of the viewport where that is narrower. Both run from below the
  top bar to above the connection banner (when it is shown), and the dimmed area beside an open drawer leaves the rail
  out, so the top bar, the rail and the banner's Retry stay usable while a drawer is open.
- Each is opened and closed with the same toggle that hides and shows it at 1148 px and wider — the sidebar
  toggle in the top bar, the report toggle on the rail. The report toggle is there only while the selected session is
  a console session, since no other session has a panel.
- A drawer closes on its toggle, on a press anywhere in the dimmed area beside it, or on `Escape`.
  While a drawer is open `Escape` closes it and **does not reach the running agent**.
- Opening one drawer closes the other; at most one is ever open.
- Widening the window to 1148 px or more closes both, and selecting a session that has no report panel
  closes the report drawer.
- Closing a drawer that holds keyboard focus moves focus to the terminal, and so does widening the window past 1148 px
  while it is open when that pane is hidden in the wider layout.

Whether a docked pane is hidden plays no part below 1148 px: there, the drawers decide what is on screen, and
widening the window again brings back each docked pane as the user last left it.

Because the drawers overlay the terminal rather than pushing it aside, **opening or closing one never
resizes the terminal**, so the running agent is never sent a terminal-size change by a drawer.

The terminal's floor below the breakpoint is 398 px, about 40 columns. Where the content panel is narrower than that,
the terminal's area **scrolls horizontally** rather than squeezing the terminal further.

## Moving focus between regions with F6

`F6` moves keyboard focus to the next region of the window and `Shift+F6` to the previous one, in a cycle, in this
order:

1. the top bar;
2. the rail;
3. the sidebar;
4. the archive view, only while it is open (see "The archive view" in `docs/product/sidebar.md`);
5. the terminal;
6. the report panel;
7. the connection banner, only while it is shown;
8. the toasts, only while at least one toast is shown.

It works wherever focus is, the terminal included: there `Tab` and `Shift+Tab` still go to the agent, and `F6` and
`Shift+F6` never do. It works from inside a report page too (see "Escape and F6 inside a page" in
`docs/product/report-panel.md`). With focus in none of the regions, `F6` goes to the first region shown and
`Shift+F6` to the last.

**A region not on screen is skipped**: a docked pane the user has hidden, a drawer that is closed, a hidden pane that
is only floating in, the report panel when there is none or it has no controls (before its pages have arrived, or
with no pages yet), and the terminal when no session is selected or while the archive view covers it. A pane counts as
shown when it is docked in the row at 1148 px and wider, or its drawer is open below that. A docked sidebar column
that is still easing closed already counts as hidden, and one that is still easing open already counts as shown (see
"The two pane toggles" above).

Where focus lands in each region:

| Region | Lands on |
|---|---|
| Top bar | its first enabled control |
| Rail | the current console's avatar, or the rail's first control when there is no console |
| Sidebar | the selected session's row, or the sidebar's first control when that row is not on screen |
| Archive view | its first control |
| Terminal | the terminal, so typing reaches the agent |
| Report panel | the pager's first enabled button, else the report page itself |
| Connection banner | its Retry button; it has none while the banner only says it is reconnecting, and F6 then skips it |
| Toasts | the newest toast |

Pressing Retry from the keyboard works as with the mouse, and when the banner goes away while Retry holds focus, focus
goes to the terminal.

The control focus lands on shows its focus ring, even when the last input before `F6` was the mouse.

`F6` does nothing while a modal dialog, Settings included, or a menu is open; focus stays where it is.

## Right-click menus

**In the desktop app** a right-click (or Control-click) shows the system's own menu only in an input field, a text area
or editable text, and in the terminal, where it is how text is copied and pasted. Anywhere else it shows nothing, unless
what was clicked offers its own action menu: the sidebar's rows and headers open theirs at the pointer (see "Rows, names
and keyboard focus" in `docs/product/sidebar.md`), and so do the consoles' avatars on the rail (see "The console
switcher" in `docs/product/sidebar.md`). A report page follows the same rule (see "Right-click inside a page"
in `docs/product/report-panel.md`).

**In a browser** the browser's own menu stays everywhere except over the rows, headers and avatars that open an action
menu.

## Right-to-left layout

Under a right-to-left language — Arabic, see "What follows the language" in `docs/product/language.md` — the whole
window is mirrored. Every left and right named in this doc swaps, and so do the sides the other product docs name for
parts of the window:

- The rail is down the window's right edge, the sidebar on the right inside the content panel, beside the rail, and
  the report panel on the left; the content panel's rounded corner is its top right. The top bar's Back, Forward and
  sidebar toggle are at its right end, in mirrored order, and the breadcrumb runs leftward from them, starting over the
  terminal's area while the sidebar is shown and following the column's inner edge while that column eases, as in
  "The top bar" above.
- Drawers and floating panes come in from their own pane's side: the sidebar's from the rail's edge (a floating
  sidebar from under the rail), the report panel's from the window's left edge. The edge strip that brings up a hidden
  report panel is along the window's left edge too, and 8 px wide there rather than 4 px.
- Each resize handle stays on its pane's inner edge, the sidebar's left edge and the report panel's right one.
  Dragging toward the terminal still widens the pane, and so does the arrow key pointing away from the pane.
- Settings and the other dialogs are mirrored the same way, and toasts sit at the bottom left.
- A name or breadcrumb too long for its space fades out where it ends in its own direction: a name the user typed
  is laid out in the direction of its own text, so a Latin name in an Arabic window still fades at its right edge.
- Paths always read left to right, in the fields that take one and in the settings list, where a path too long for its
  space fades at its start, so its last folder stays readable.
- Icons that point a direction or show a side are mirrored: Back and Forward, the two pane toggles, whose pane is drawn
  on its own side, the breadcrumb's separators, the report panel's previous-page and next-page buttons, focus mode's
  back button, the directory browser's parent-directory entry, and a collapsed project row's chevron, which points left.
  An expanded row's chevron points down, as in left-to-right.

Not mirrored:

- **The macOS window buttons** — close, minimise and zoom stay at the window's top left, and the top bar keeps its
  clear space for them there (see "The top bar" above).
- **The terminal**, which is never mirrored (see "What follows the language" in `docs/product/language.md`).
- **A report page**, which keeps its own direction; only the panel around it is mirrored (see "What follows the
  language" in `docs/product/language.md`).
