# Window layout

The window is made of **window chrome** and a **content panel**. The chrome is a **top bar** across the window's whole
width and, under it, a narrow **rail** down the window's left edge (its right edge under a right-to-left language, see
"Right-to-left layout" below). Everything else is the content panel, which holds up to three panes, left to right: the
**sidebar**, showing one console's console sessions, projects and sessions (see `docs/product/sidebar.md`), the
selected session's **terminal**, which the archive view covers while it is open (see "The archive view" in
`docs/product/sidebar.md`), and the **right pane**, which shows a console session's **report panel** (see
`docs/product/report-panel.md`) or a project's **project pane** (see `docs/product/project-pane.md`), and is not there
at all while it shows neither (see "What the right pane shows" below). The connection banner, while the daemon
connection is down, is a full-width strip along the window's bottom edge, under the rail and the content panel (see
"Losing the daemon connection" in `docs/product/application-lifecycle.md`). Toasts float over the bottom right, above
the banner while it is shown (see `docs/product/toasts.md`).

How the three panes are arranged depends on one width, **1148 px**. At that width and above they sit side by
side in a row, the layout the macOS window is always in; a pane the user has hidden there can still float in over
the terminal for a moment. Below it the sidebar and the right pane become overlay drawers over the terminal; only
a client with no window minimum of its own, such as the UI opened in a plain browser, can get there.

Everything this doc says of the right pane — its toggle, width, hiding, floating in and drawer — is the same whichever
of the two it shows.

For the window's light/dark appearance see `docs/product/appearance.md`; for the settings the rail opens see
`docs/product/settings.md`; for the top bar's Back and Forward see `docs/product/navigation-history.md`; for moving
keyboard focus between the window's regions see `docs/product/moving-focus-between-regions.md`; for how the macOS
window's size and position are remembered see `docs/product/window-size-and-position.md`.

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

## What an open dialog, menu or popover covers

**A dialog** — Settings, the file viewer, a console, project or session dialog, a confirmation — is drawn over a dimmed
overlay that covers the whole window: the top bar, the rail and the content panel are dimmed behind it and none of
them can be used while it is open, and a press anywhere on the dimmed area dismisses the dialog.

**A menu or a popover** — a row's, a header's or an avatar's action menu, the project filter's popover, a worktree or
branch selector's list of choices, a selector's list inside a dialog — takes the window around it the same way: while
one is open, nothing outside it can be used, and a press outside dismisses it. It has no dimmed overlay of its own, so
nothing behind it is dimmed; what it shares with a dialog is that the rest of the window is out of reach until it is
gone.

**The connection banner's strip is the one part of the window that stays usable.** While the daemon connection is down
the strip is drawn above a dialog's overlay and above an open menu or popover, so it is not dimmed along with
everything else — it looks exactly as it does with nothing open — and a press where it is drawn lands on the strip
rather than on what the strip is drawn over (for what its button then does, see "Losing the daemon connection" in
`docs/product/application-lifecycle.md`). **That press dismisses nothing**: the dialog, menu or popover it was drawn
over stays open, where a press anywhere else outside a menu or popover — in the terminal, say — closes it. Retry
pressed with a selector's list open inside a dialog leaves both the list and the dialog open. Toasts float above in
the same way (see `docs/product/toasts.md`), and nothing else does: everything else stays unusable, dimmed as well
behind a dialog, and `Escape` still closes what is open.

Because the strip is stacked above a dialog as well as above its overlay, dialogs give way to it rather than running
under it and losing their bottom edge and any press landing there. While the strip is up, a dialog that fills the
window — Settings and the file viewer in the narrow layout, the file viewer at 1148 px and wider too — keeps the same
margin from the strip's top edge that it would have kept from the window's bottom edge, 16 px in the narrow layout and
40 px at 1148 px and wider, whatever the strip's height; its margin above is the same either way. A dialog with room
to spare, a confirmation say, centres in the space above the strip rather than in the whole window, so it sits half
the strip's height higher than it otherwise would. With the strip gone, every dialog sits exactly where it would with
no connection banner at all.

Only the pointer reaches the strip there. `F6` does not move keyboard focus out from under an open dialog or menu, so
a keyboard user has to dismiss it first (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`).

## The top bar

The top bar is on every screen, the same height everywhere. On the screens shown before there is anything to
show it is there but empty.

It runs, left to right:

- **Back** and **Forward**, which move through the places the window has shown (see
  `docs/product/navigation-history.md`); each is disabled while there is nowhere to go in its direction.
- The sidebar toggle ("Show sessions" / "Hide sessions").
- The selected session's **breadcrumb** — *console › project › session title* for a project session, *console › session
  title* for a console session, which belongs to no project — followed by the session's status icon. Nothing in the
  bar reports the session's terminal connection; the terminal itself does (see "The terminal" in
  `docs/product/sessions.md`). At 1148 px and wider, while the sidebar is shown, the breadcrumb starts where the
  terminal's area does, over the sidebar's inner edge, and follows that edge while the sidebar is resized and while the docked column eases open
  or closed (see "Hiding the sidebar and the right pane" below). The start of the bar never becomes narrower than the
  buttons there — in the macOS application, that includes the clear space kept for the window buttons. Once the column no longer
  reaches past that start, the breadcrumb stays beside the sidebar toggle, which is also where it sits while the
  sidebar is hidden. Below 1148 px it starts right after the sidebar toggle as well. A trail too long for the bar
  fades out at its right edge, as in "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`, and the status icon always stays visible. With no session selected there is
  no breadcrumb. While the archive view is open (see "The archive view" in `docs/product/sidebar.md`) the breadcrumb
  shows where that is instead, with no status icon: *console › project › Archived sessions*, *console › console
  session › Archived bound sessions*, or *console › Archived console sessions*.

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
  - **Turn on notifications**, a bell with a small dot on it, shown only in a browser whose answer to notifications is
    still undecided; pressing it asks the browser (see "Notifications" in `docs/product/settings.md`);
  - the right pane's toggle, shown only while the right pane has an owner (see "What the right pane shows" below);
  - **Settings**, which opens the settings dialog.

The rail is the same at every width; below 1148 px it stays beside the drawers, which open past it.

### The two pane toggles

What the sidebar toggle in the top bar and the right pane's toggle on the rail do depends on the window's width: see
"Hiding the sidebar and the right pane" and "Below 1148 px: the sidebar and the right pane become drawers" below.
Each toggle's label says what pressing it will do — "Show …" while its pane is hidden, "Hide …" while it is shown —
and its icon shows the pane's current state: a window with that side's pane drawn solid while the pane is shown, and
drawn dashed while it is hidden. Each toggle also tells assistive technology whether its pane is expanded: expanded
while the pane is shown, which below 1148 px means its drawer is open. A hidden pane that is only floating in counts
as hidden for both. For the sidebar, shown and hidden follow the docked state at once, including while its column is
still easing open or closed (see "Hiding the sidebar and the right pane" below): the toggle already reads "Hide …" as
the column opens and "Show …" as it closes.

The right pane's controls are named for what it shows. The toggle reads "Show report" / "Hide report" for a report
panel and "Show project pane" / "Hide project pane" for a project pane; likewise its resize handle is "Resize report
panel" or "Resize project pane", and the dimmed area beside its open drawer is "Close report" or "Close project pane".

### Pressing the chrome's controls

Every control in the top bar and on the rail has a tooltip (see "Tooltips on icon-only controls" in
`docs/product/labels-and-tooltips.md`); the rail's open toward the content panel. Pressing any of them leaves keyboard
focus where it was, so after a mouse press the terminal keeps receiving keystrokes. Turn on notifications pressed from
the keyboard keeps focus on itself while the browser's prompt is up, so no keystrokes reach the session meanwhile; once
the browser answers and the bell goes away, focus moves to the terminal, or to the top bar's first enabled control when
the terminal cannot take it.

## What the right pane shows

The right pane has one **owner** at a time: a console session, whose report panel it shows, or a project, whose
project pane it shows. With no owner there is no right pane at all, no toggle for it on the rail, and its width goes
to the terminal. The owner is set only by what the user does, never worked out from whatever else is on screen, so a
project pane can be shown with no session selected, or beside the terminal of a session of another project.

Each session has a **pane of its own**: a console session its report panel, a project session its project's project
pane.

- **Selecting a session** puts the session's own pane in the right pane, whatever the session is selected from (see
  "Selecting a session" in `docs/product/sidebar.md`), a session this window has just opened included. Selecting
  another session of the project whose pane is shown leaves the pane as it is.
- **Browse files**, the first item of every project's action menu (see "Project rows" in `docs/product/sidebar.md`),
  gives the right pane to that project and selects, starts and resumes nothing: the selected session's terminal stays,
  and the breadcrumb still names it. It also puts the pane on screen: at 1148 px and wider it shows a hidden pane for
  good, and below that it opens the pane's drawer, closing the sidebar's.
- **Browse files while a console session's report panel is shown** puts the project pane in its place. Selecting the
  console session again brings the report panel back, on its newest page; anything typed into a page's form and not
  submitted is gone (see "Paging through the history" in `docs/product/report-panel.md`).
- **Making another console the current one** on the rail — pressing its avatar, or pressing inside the floating
  sidebar while it previews that console (see "Previewing a console from the rail" in `docs/product/sidebar.md`) —
  closes the project pane of a project of the console left behind; only previewing a console changes nothing here. A
  report panel stays, since its console session is still the one selected. When that leaves the right pane with no
  owner, the selected session's own pane comes back: a console session's report panel at once, a project session's
  project pane once that session's console is the current one again, on the rail or by Back and Forward.
- **Back and Forward** do not record the owner, which is not part of a place (see "What counts as a place" in
  `docs/product/navigation-history.md`). A move to another session puts that session's own pane in the right pane, as
  selecting it would; a move that keeps the selected session leaves the owner as it is. Either way the rule of making
  another console the current one then applies, since the place moved to can have another current console than its
  session's.
- **When the owner stops existing** — its project removed, its console session deleted — there is no right pane.
  Nothing takes its place: not another project, and not the selected session's own pane.

When the owner changes while keyboard focus is inside the right pane, focus goes to the terminal.

## The window's minimum size

The macOS window has a **minimum size of 1148×600**. The width is the rail's 48 px plus the three panes' own floors,
with the sidebar at its default width (48 + 280 + 520 + 300), so none of the panes can be squeezed past usability by a
narrower window; the height leaves the terminal about 30 rows.

Nothing enforces a minimum when the UI is opened in a browser, so the narrow layout below applies there.

## Pane widths at 1148 px and wider

| Pane | Default | Range the user can choose |
|---|---|---|
| Sidebar | 280 px | 200–480 px |
| Right pane | 420 px | 300–720 px |
| Terminal | everything left over | never below 520 px (about 53 columns) |

The sidebar and the right pane are drawn at the width the user chose (see "Resizing the sidebar and the right pane"
below), and neither grows past it: width a wider window frees goes to the terminal. The right pane has one width,
whichever owner it has.

**The terminal always keeps its 520 px.** Where the window cannot afford every pane its chosen width, the right pane
gives up width first, down to 300 px, and only then the sidebar, down to 200 px. At the 1148 px minimum with both panes
shown that makes, beside the 48 px rail, the sidebar 280 px, the terminal 520 px and the right pane 300 px. This never
changes the remembered width: when the window widens again, each pane gets its chosen width back.

A pane the user has hidden takes no width at all once it is hidden, and neither does one only floating in: its width
goes to the terminal, and a hidden right pane stops holding back the sidebar's width (see "Hiding the sidebar and the
right pane" below). While the docked sidebar's column is easing closed, it still takes the width it has reached. With
nothing owning the right pane there is no right pane in the row either.

Anything that changes the terminal's size — resizing the window, resizing a pane, hiding or showing a pane, the right
pane coming or going with its owner — resizes the agent's terminal (see "The terminal" in `docs/product/sessions.md`).

### Resizing the sidebar and the right pane

Each pane has a resize handle on its inner edge: the sidebar's on its right edge, the right pane's on its left. Both
run the pane's full height, from below the top bar to above the connection banner, while it is shown.

- **Dragging** the handle sets the pane's width, within its range in the table above.
- **Double-clicking** it returns the pane to its default width.
- The handle is also reachable with Tab and is then operated from the keyboard: the arrow pointing away from the pane
  widens it and the other narrows it (Right widens the sidebar and Left the right pane; the other way round under a
  right-to-left language, where the panes have swapped sides), by 16 px, by 64 px with Shift held; Home sets the pane's
  minimum, and End the widest the window currently allows. While it has keyboard focus the handle is drawn as a solid
  accent-coloured bar with a focus ring around it. Pressing the handle with the mouse does not take keyboard focus off
  the terminal.

A handle is there only while its pane is shown in the row: not while the pane is hidden or floating in, and not below
1148 px, where the panes are fixed-width drawers. The sidebar's handle is also absent while that column is easing
open or closed; it is there only once the column is fully open. The right pane's handle is there whenever that pane
is shown in the row.

**Each pane's chosen width is remembered per client**, in that client's own browser storage, as the appearance choice
is (see "Where the choice is stored" in `docs/product/appearance.md`); without that storage a choice lasts until the
page is reloaded.

## Hiding the sidebar and the right pane

At 1148 px and wider, the two pane toggles hide and show the docked panes:

- The sidebar toggle, in the top bar, hides or shows the sidebar.
- The right pane's toggle, on the rail and there only while the right pane has an owner, hides or shows the right
  pane. A hidden right pane stays hidden whatever owns it afterwards — a report panel or a project pane, selecting a
  session included — until it is shown again, by its toggle or by Browse files (see "What the right pane shows"
  above).

Both panes are shown by default. **Whether each one is hidden is remembered per client**, the same way as the panes'
widths. Hiding a pane hands its width to the terminal.

Hiding a pane that holds keyboard focus — a sidebar row reached with Tab, say, or a report page or a file tree row —
moves focus to the terminal, or to the top bar when no session is selected.

At 1148 px and wider, hiding or showing the docked sidebar eases that column over **200 ms**, the same length as the
overlay drawer's slide. Where the system asks for reduced motion, the column opens or closes at once. Pressing the
toggle again while the column is still moving reverses the ease. Below 1148 px this ease does not apply: the sidebar
is the drawer, and that drawer slides (see "Below 1148 px: the sidebar and the right pane become drawers" below).
Hiding and showing the right pane are immediate. Floating it still slides in and out over 200 ms (see "A hidden pane
floats in on hover" below).

During the ease the sidebar stays a full-height column, flush with the rail, from below the top bar to above the
connection banner when that banner is shown, and it does not become the rounded card in "A hidden pane floats in on
hover" below. Its contents stay at the width the open column is drawn at, including where the window is holding that
width back (see "Pane widths at 1148 px and wider" above), and stay against the rail. The inner edge covers them as
the column closes and uncovers them as it opens, so the text does not reflow. The hover card comes up only once the
column has closed.

As the column closes, the width it gives up goes to the terminal. When the right pane is docked and had been held
below the width the user chose, it can grow into that room as the room appears. Showing the sidebar limits the right
pane again as soon as the sidebar is shown, when the window cannot afford both panes their chosen widths, and the
opening column takes its own width from the terminal as it eases open. The terminal is resized by either, as in
"Pane widths at 1148 px and wider" above.

### A hidden pane floats in on hover

At 1148 px and wider, a hidden pane can be brought up for a moment without showing it for good. This is a **floating
pane**. On the sidebar, the rounded corners, the gaps, the shadow and the slide from behind the rail are this card.
They are not how the docked column opens and closes (see "Hiding the sidebar and the right pane" above).

- **What brings up the sidebar**: the mouse resting for **130 ms** on a console's avatar on the rail, the current
  console's or another's. The sidebar then shows the console whose avatar the mouse is on, which need not be the
  current one; what that preview is and how a press inside it makes the console current is in "Previewing a console
  from the rail" in `docs/product/sidebar.md`. While the sidebar is out, moving onto another avatar shows that console
  at once, with no wait. A pointer that only crosses the avatars brings nothing up. Neither the sidebar toggle nor the
  content panel's edge brings the sidebar up.
- **What brings up the right pane**: the mouse staying for **200 ms** on a 4 px strip along the window's right edge,
  from below the top bar to above the connection banner, or, at once, the mouse reaching the right pane's toggle.
- Touch and pen bring up neither.
- **How it shows**: it slides in over the terminal in 200 ms (at once where the system asks for reduced motion), at the
  pane's chosen width — the right pane at most 92% of the window's width — with a shadow and no dimming behind it. It
  overlays the terminal, so **the terminal is not resized** and the agent is sent no size change.
  - The **sidebar** floats as a card: it keeps an 8 px gap from the rail, from the top bar and from the window's
    bottom (or the connection banner), has rounded corners all round and a border. It slides in from under the rail
    and out again behind it, never drawn over the rail.
  - The **right pane** slides in from the window's right edge and runs from the top bar to the window's bottom (or
    the connection banner).
- **How it goes away**: it slides away 200 ms after the pointer has left it. For the sidebar that means the pointer is
  on neither the pane nor a console's avatar; for the right pane, on neither the pane nor its toggle. Once the right
  pane has come up from its edge strip, the pointer's first move counts as leaving it when it lands outside the place
  the pane is sliding into. `Escape` sends it away at once and **does not reach the running agent**, except that in a
  field holding text (the project pane's change filter) it only clears the field, and the next one sends the pane away;
  so does `Escape` pressed inside a report page (see "Escape and F6 inside a page" in `docs/product/report-panel.md`). A
  floating pane that holds keyboard focus as it goes away hands focus to the terminal. A floating sidebar previewing a
  console that is deleted goes away as if the pointer had left that console's avatar.
- **What keeps it up**, whatever the pointer does: keyboard focus inside it, one of its menus being open, or a dialog
  being open, for as long as it is — such as one opened from the pane, the file viewer a project pane opens among them
  (see "The file viewer" in `docs/product/project-pane.md`). So closing a viewer opened from a floating project pane,
  by the pointer or the keyboard, finds the pane still there to hand focus back to.
- **Only one at a time**: bringing one up sends the other away.
- **Pressing the toggle** while the pane is floating shows it for good, and keyboard focus stays where it was. Until
  the pane is docked the toggle still reads "Show …". The right pane joins the row. The sidebar's card goes away and
  the docked column eases open in the row; which console that column shows is in "Previewing a console from the rail"
  in `docs/product/sidebar.md`.
- The right pane floats in only while it has an owner. There are no floating panes below 1148 px, and narrowing the
  window below it sends one away.

## Below 1148 px: the sidebar and the right pane become drawers

Below 1148 px wide, the top bar and the rail stay as they are, and the terminal is the only pane in the content panel
and fills its width. The sidebar and the right pane each become an **overlay drawer** over it, **closed by
default**:

- The sidebar drawer slides in from the rail's edge and is 280 px wide; the right pane's drawer slides in from
  the window's right edge and is 420 px wide, or 92% of the viewport where that is narrower. Both run from below the
  top bar to above the connection banner (when it is shown), and the dimmed area beside an open drawer leaves the rail
  out, so the top bar, the rail and the banner's Retry stay usable while a drawer is open.
- Each is opened and closed with the same toggle that hides and shows it at 1148 px and wider — the sidebar
  toggle in the top bar, the right pane's toggle on the rail, which is there only while the right pane has an owner.
  Browse files opens the right pane's drawer too (see "What the right pane shows" above).
- A drawer closes on its toggle, on a press anywhere in the dimmed area beside it, or on `Escape`.
  While a drawer is open `Escape` closes it and **does not reach the running agent**; in a field in the drawer that
  holds text (the project pane's change filter) it only clears the field, and the next one closes the drawer.
- Opening one drawer closes the other; at most one is ever open.
- Widening the window to 1148 px or more closes both. The right pane's drawer stays open when its owner changes,
  showing the new owner's content, and closes when the right pane is left with no owner.
- Closing a drawer that holds keyboard focus moves focus to the terminal, and so does widening the window past 1148 px
  while it is open when that pane is hidden in the wider layout.

Whether a docked pane is hidden plays no part below 1148 px: there, the drawers decide what is on screen, and
widening the window again brings back each docked pane as the user last left it.

Because the drawers overlay the terminal rather than pushing it aside, **opening or closing one never
resizes the terminal**, so the running agent is never sent a terminal-size change by a drawer.

The terminal's floor below the breakpoint is 398 px, about 40 columns. Where the content panel is narrower than that,
the terminal's area **scrolls horizontally** rather than squeezing the terminal further.

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
  the right pane on the left; the content panel's rounded corner is its top right. The top bar's Back, Forward and
  sidebar toggle are at its right end, in mirrored order, and the breadcrumb runs leftward from them, starting over the
  terminal's area while the sidebar is shown and following the column's inner edge while that column eases, as in
  "The top bar" above.
- Drawers and floating panes come in from their own pane's side: the sidebar's from the rail's edge (a floating
  sidebar from under the rail), the right pane's from the window's left edge. The edge strip that brings up a hidden
  right pane is along the window's left edge too, and 8 px wide there rather than 4 px.
- Each resize handle stays on its pane's inner edge, the sidebar's left edge and the right pane's right one.
  Dragging toward the terminal still widens the pane, and so does the arrow key pointing away from the pane.
- Settings and the other dialogs are mirrored the same way, and toasts sit at the bottom left. So is the file viewer's
  layout: an image change's Before and After images, side by side, have Before on the right.
- A name or breadcrumb too long for its space fades out where it ends in its own direction: a name the user typed
  is laid out in the direction of its own text, so a Latin name in an Arabic window still fades at its right edge.
- Paths always read left to right, in the fields that take one, in the settings list and in a project pane's
  worktree selector, where a path too long for its space fades at its start, so its last folder stays readable.
- Icons that point a direction or show a side are mirrored: Back and Forward, the two pane toggles, whose pane is drawn
  on its own side, the breadcrumb's separators, the report panel's previous-page and next-page buttons, the file
  viewer's Previous file and Next file buttons, focus mode's back button, the directory browser's parent-directory
  entry, and the chevron of a collapsed project row or of a collapsed folder in a project pane's file tree, which points
  left. An expanded row's chevron points down, as in left-to-right.

Not mirrored:

- **The macOS window buttons** — close, minimise and zoom stay at the window's top left, and the top bar keeps its
  clear space for them there (see "The top bar" above).
- **The terminal**, which is never mirrored (see "What follows the language" in `docs/product/language.md`).
- **A report page**, which keeps its own direction; only the panel around it is mirrored (see "What follows the
  language" in `docs/product/language.md`).
- **A file's content in the file viewer**: code, a diff — whose split layout keeps the old side on the left — the
  file's name and path, and the paths a change names, read left to right, and a binary change's sizes run from before
  to after left to right; only the viewer around them is mirrored. A file or folder name in the file tree, a name
  and what follows it in the Git mode's change list, a worktree's or a branch's name in the Git mode, and the text of
  the separator a diff shows where it collapsed unchanged lines (see "Expanding the collapsed lines" in
  `docs/product/project-pane-git-mode.md`), are laid out in the direction of their own text, as a name the user typed
  is; a commit's id reads left to right.
- **A rendered Markdown document in the file viewer** (see "A Markdown file as a document" in
  `docs/product/project-pane.md`), which is laid out in the direction of its own text, block by block: each paragraph,
  heading, list, quote and table takes the direction of its own first strong letter, so a right-to-left passage in a
  left-to-right document reads from its own side, with its quote bar on that side; inline code and a fenced block
  always read left to right, and the frame around the document follows the window.
