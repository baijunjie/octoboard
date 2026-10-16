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

**With no console at all the content panel holds none of those panes**: a single **welcome screen** fills it instead,
offering the one thing there is to do, and the top bar carries no sidebar toggle (see
`docs/product/welcome-screen.md`). Everything this doc says of the three panes is what the window looks like once a
console exists.

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
right-to-left language), where the top bar meets the rail, is rounded. No line runs along its edges: it is set apart
from the chrome by the contrast between its surfaces and the chrome and by a soft shadow along its edges, and it reaches
the window's other side and its bottom (or the connection banner) with no gap. Inside it a thin line runs down the
inner edge of the docked sidebar and of the docked right pane, setting each apart from the terminal's area. The
terminal's area is the colour of the terminal's own background. The sidebar and the right pane share the same background — a touch greyer than the terminal
in the light appearance, a touch lighter in the dark one — whether docked, floating or shown as drawers. A report page
keeps its own light surface inside the right pane (see "A report page stays on a light surface in both appearances" in
`docs/product/appearance.md`).

**Every docked pane's header inside the content panel is 56 px high** — the sidebar's console header and a focus mode's
header (see `docs/product/sidebar.md` and `docs/product/focus-mode.md`), the archive view's header (see "The archive
view" in `docs/product/sidebar.md`), the project pane's header and the report panel's bar — so the lines under them
run on as one line across the panel. The top bar's 40 px is a tier of its own and no part of this. A floating pane or
an overlay drawer is a card inset from the panel's edges (see "A hidden pane floats in on hover" and "Below 1148 px:
the sidebar and the right pane become drawers" below), and its headers are **47 px**
— shorter by that inset and by the card's own 1 px top border — so the line under a card's header stays level with the
line under the headers docked beside it; the text in a card's header therefore sits 4.5 px lower than theirs.

On the screens shown before there is anything to show — while connecting to the daemon, and the screens for a daemon
that could not start or a UI that failed — there is no rail: under the top bar, the content panel spans the window.

## What an open dialog, menu or popover covers

**A dialog** — Settings, the file viewer, a console, project or session dialog, a confirmation — is drawn over a dimmed
overlay that covers the whole window: the top bar, the rail and the content panel are dimmed behind it and none of
them can be used while it is open, and a press anywhere on the dimmed area dismisses the dialog.

**Where keyboard focus starts in a confirmation.** A confirmation opens with keyboard focus on its confirm button, so
Enter confirms at once. Three kinds start elsewhere:

- **A destructive confirmation** — one whose confirm button is drawn in the danger colour, such as quitting with
  sessions running, removing a project, deleting a session or removing an account — opens with focus on the dialog
  itself, so Enter does nothing until the user moves focus onto a control.
- **A confirmation that asks for a word to be typed** opens with focus in its text field.
- **A question the user did not ask for** — a folder trust prompt (see "The trust dialog" in
  `docs/product/folder-trust.md`) or a request for a console session (see "The confirmation dialog" in
  `docs/product/requesting-a-console-session.md`) — opens with focus on the dialog itself as well, and focus is put
  back on the dialog each time it moves on to the next waiting question, so an Enter held past one answer does not
  answer the next.

**A held Enter or Space does not confirm.** Every confirmation and every form dialog ignores the auto-repeats of a
held Enter or Space, so the press that opened it (a menu item acts as the key goes down) cannot go on to confirm it; a
fresh press still does. Where text is being typed the repeats stay: Space in any text field, Enter in a multi-line one.

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

The strip's Retry is also reachable by keyboard with a modal dialog open. For how focus moves to Retry and back,
where it goes when Retry disappears, and the restriction while a menu is open, see "Moving focus between regions with
F6" in `docs/product/moving-focus-between-regions.md`.

## The top bar

The top bar is on every screen, the same height everywhere. On the screens shown before there is anything to
show it is there but empty.

It runs, left to right:

- **Back** and **Forward**, which move through the places the window has shown (see
  `docs/product/navigation-history.md`); each is disabled while there is nowhere to go in its direction.
- The sidebar toggle ("Show sessions" / "Hide sessions"), there only while there is a sidebar — so not while the
  window shows the welcome screen (see `docs/product/welcome-screen.md`).
- The selected session's **breadcrumb** — *console › project › session title* for a project session, *console › session
  title* for a console session, which belongs to no project — followed by the session's status icon. The session
  title follows "Displayed names" in `docs/product/session-titles.md`. Nothing in the
  bar reports the session's terminal connection; the terminal itself does (see "Reconnecting" in
  `docs/product/session-terminal.md`). At 1148 px and wider, while the sidebar is shown, the breadcrumb starts where the
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
- At the **end of the bar**, the right pane's toggle, there only while the right pane has an owner (see "What the
  right pane shows" below). The end is the reading direction's, so under a right-to-left language it is at the
  window's left (see "Right-to-left layout" below).

Nothing else is in the bar: the rest of its end is the window's drag area, and the window's other controls are on the
rail (see "The rail" below).

**In the macOS application the top bar is the window's titlebar.** The window has no native titlebar background and
shows no title text of its own (the window is still called Octoboard in the Dock and in Mission Control). The window's
close, minimise and zoom buttons sit inside the bar at its left end, and the bar keeps that space clear for them, so
Back comes after them; in fullscreen, where those buttons are gone, the space goes as well. Under a right-to-left
language the buttons and their clear space stay at the left end, which then holds the bar's end rather than its
start. Dragging any part of the bar that is not a control moves the window, and double-clicking it zooms the window,
as with a native titlebar. Opened in a plain browser, the bar has none of this: it starts at the left edge and does
not move anything.

### The two pane toggles

**Each toggle is there only while its pane exists**: the sidebar toggle while there is a console for the sidebar to
show (see `docs/product/welcome-screen.md`), the right pane's toggle while the right pane has an owner (see "What the
right pane shows" below). A toggle with nothing to control is left out rather than shown disabled.

Both toggles are in the top bar, the sidebar's at its start and the right pane's at its end. What each does depends on
the window's width: see "Hiding the sidebar and the right pane" and "Below 1148 px: the sidebar and the right pane
become drawers" below.

Each toggle's label says what pressing it will do — "Show …" while its pane is hidden, "Hide …" while it is shown —
and its icon shows the pane's current state: a window with that side's pane drawn solid while the pane is shown, and
drawn dashed while it is hidden. Each toggle also tells assistive technology whether its pane is expanded: expanded
while the pane is shown, which below 1148 px means its drawer is open. A hidden pane that is only floating in counts
as hidden for both. Shown and hidden follow the docked state at once, including while a column is
still easing open or closed (see "Hiding the sidebar and the right pane" below): the toggle already reads "Hide …" as
the column opens and "Show …" as it closes.

The right pane's controls are named for what it shows. The toggle reads "Show report" / "Hide report" for a report
panel and "Show project pane" / "Hide project pane" for a project pane; likewise its resize handle is "Resize report
panel" or "Resize project pane", and the dimmed area beside its open drawer is "Close report" or "Close project pane".

## The rail

The rail runs from under the top bar to the window's bottom (or the connection banner). Its two ends hold:

- **At the top, the consoles**: one avatar per console, in the order the consoles were created, the current console
  marked by a tile behind its avatar, then **New console**, which opens the new-console dialog (see "Consoles" in
  `docs/product/consoles-and-projects.md`). Choosing a console, what each avatar's badge says and what a right-click
  on one offers are in "The console switcher" in `docs/product/sidebar.md`. While the docked sidebar is hidden, resting
  the mouse on an avatar floats the sidebar in showing that console (see "A hidden pane floats in on hover" below).
  When there are more consoles than fit, the avatars scroll, with no scrollbar drawn — so the fade at the column's
  edge is the only sign there are more (see "The edge that hides content fades" in
  `docs/product/scrolling-surfaces.md`); New console stays below them.
- **At the bottom**, top to bottom:
  - the **waiting count**, a raised hand with the number of sessions waiting for the user in a badge at its top
    right, its top left under a right-to-left language (see "Right-to-left layout" below), shown only while at least
    one is (99+ beyond 99); pressing it goes to the next waiting session (see "The raised hand" in
    `docs/product/sessions.md`);
  - **Turn on notifications**, a bell with a small dot on it, shown only in a browser whose answer to notifications is
    still undecided; pressing it asks the browser (see "Notifications" in `docs/product/settings.md`);
  - **Settings**, which opens the settings dialog.

The rail is the same at every width; below 1148 px it stays beside the drawers, which open past it.

### Pressing the chrome's controls

Every control in the top bar and on the rail has a tooltip (see "Tooltips on icon-only controls" in
`docs/product/labels-and-tooltips.md`); the rail's open toward the content panel. Pressing any of them leaves keyboard
focus where it was, so after a mouse press the terminal keeps receiving keystrokes. Turn on notifications pressed from
the keyboard keeps focus on itself while the browser's prompt is up, so no keystrokes reach the session meanwhile; once
the browser answers and the bell goes away, focus moves to the terminal, or to the top bar's first enabled control when
the terminal cannot take it.

## What the right pane shows

The right pane has one **owner** at a time: a console session, whose report panel it shows, or a project, whose
project pane it shows. With no owner there is no right pane at all, no toggle for it in the top bar, and its width goes
to the terminal. The owner is set only by what the user does, never worked out from whatever else is on screen, so a
project pane can be shown with no session selected, or beside the terminal of a session of another project.

Each session has a **pane of its own**: a console session its report panel, a project session its project's project
pane.

- **Selecting a session** puts the session's own pane in the right pane, whatever the session is selected from (see
  "Selecting a session" in `docs/product/sidebar.md`), a session this window has just opened included. Selecting
  another session of the project whose pane is shown leaves the pane as it is.
- **Browse files**, when offered in a project's action menu (see "Opening a project's files" in
  `docs/product/project-pane.md`),
  gives the right pane to that project and selects, starts and resumes nothing: the selected session's terminal stays,
  and the breadcrumb still names it. It also puts the pane on screen: at 1148 px and wider it shows a hidden pane for
  good, and below that it opens the pane's drawer, closing the sidebar's.
- **Browse files while a console session's report panel is shown** puts the project pane in its place. Selecting the
  console session again brings the report panel back, on its newest page; anything typed into a page's form and not
  submitted is gone (see "Paging through the history" in `docs/product/report-panel.md`).
- **Making another console the current one** — pressing its avatar on the rail, pressing inside the floating sidebar
  while it previews that console, or a console created from this window appearing — puts in the right pane the own
  pane of the session the switch shows, the one last selected in that console; with no session shown there is no
  right pane (see "What switching console shows" in `docs/product/sidebar.md`). A press inside the floating sidebar
  that gives the right pane to something itself, as Browse files does, keeps what it gave it. Only previewing a
  console changes nothing here.
- **Back and Forward** do not record the owner, which is not part of a place (see "What counts as a place" in
  `docs/product/navigation-history.md`). A move to another session puts that session's own pane in the right pane, as
  selecting it would; a move that keeps the selected session leaves the owner as it is. Either way, since the place
  moved to can have another current console than its session's, a project pane of a project of any console other
  than the current one is then closed; when that leaves the right pane with no owner, the selected session's own pane
  comes back: a console session's report panel at once, a project session's project pane only while that session's
  console is the current one.
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
right pane" below). While either docked column is easing closed, it still takes the width it has reached. With
nothing owning the right pane there is no right pane in the row either.

Anything that changes the terminal's size — resizing the window, resizing a pane, hiding or showing a pane, the right
pane coming or going with its owner — resizes the agent's terminal (see "Output, scrollback and size" in
`docs/product/session-terminal.md`).

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
1148 px, where the panes are fixed-width drawers. Each handle is also absent while its column is easing open or
closed; it is there only once the column is fully open.

**Each pane's chosen width is remembered per client**, in that client's own browser storage, as the appearance choice
is (see "Where the choice is stored" in `docs/product/appearance.md`); without that storage a choice lasts until the
page is reloaded.

## Hiding the sidebar and the right pane

At 1148 px and wider, the two pane toggles hide and show the docked panes:

- The sidebar toggle, in the top bar, hides or shows the sidebar.
- The right pane's toggle, at the top bar's end and there only while the right pane has an owner, hides or shows
  that pane. A hidden right pane stays hidden whatever owns it afterwards — a report panel or a project pane,
  selecting a session included — until it is shown again, by its toggle or by Browse files (see "What the right pane
  shows" above).

Both panes are shown by default. **Whether each one is hidden is remembered per client**, the same way as the panes'
widths. Hiding a pane hands its width to the terminal.

Hiding a pane that holds keyboard focus — a sidebar row reached with Tab, say, or a report page or a file tree row —
moves focus to the terminal, or to the top bar when no session is selected.

At 1148 px and wider, hiding or showing either docked pane eases its column over **200 ms**, the same length as the
overlay drawer's slide. Where the system asks for reduced motion, the column opens or closes at once. Pressing the
toggle again while the column is still moving reverses the ease. Below 1148 px the panes are drawers and slide
laterally instead (see "Below 1148 px: the sidebar and the right pane become drawers" below).

During the ease each pane stays a full-height column, from below the top bar to above the connection banner when that
banner is shown, and does not become the rounded card in "A hidden pane floats in on hover" below. Its contents stay at
the width the open column is drawn at, including where the window is holding that width back (see "Pane widths at
1148 px and wider" above), anchored against the rail for the sidebar and the window's edge for the right pane. The inner
edge covers them as the column closes and uncovers them as it opens, so the text does not reflow. The sidebar's thin
line rides its column's inner edge through the ease and is gone once the column has closed. A hover card comes up only
once its column has closed.

As the sidebar's column closes, the width it gives up goes to the terminal. When the right pane is docked and had been held
below the width the user chose, it can grow into that room as the room appears. Showing the sidebar limits the right
pane again as soon as the sidebar is shown, when the window cannot afford both panes their chosen widths, and the
opening column takes its own width from the terminal as it eases open. A closing right pane keeps the sidebar's
width constrained until that column has closed. Either pane opening takes width from the terminal; either pane
closing returns it. The terminal is resized by these changes, as in
"Pane widths at 1148 px and wider" above.

### A hidden pane floats in on hover

At 1148 px and wider, a hidden pane can be brought up for a moment without showing it for good. This is a **floating
pane**, and both panes float in as the same kind of card: inset from the window chrome and the window's edges, with
rounded corners, a border and a shadow. Below 1148 px the overlay drawers share this card appearance (see "Below
1148 px: the sidebar and the right pane become drawers" below). Docked panes open and close as full-height columns
(see "Hiding the sidebar and the right pane" above).

- **What brings up the sidebar**: the mouse reaching the sidebar's toggle, at once, or resting for **130 ms** on a
  console's avatar on the rail, the current console's or another's. The wait is the avatars' alone, so that a pointer
  crossing the rail on its way elsewhere does not make the card flash; the toggle is aimed at, so it has none.
  - **From the toggle the sidebar shows the current console**, whatever console a card was previewing just before.
  - **From an avatar it shows that console**, which need not be the current one; what that preview is and how a press
    inside it makes the console current is in "Previewing a console from the rail" in `docs/product/sidebar.md`.
  - While the sidebar is out, moving onto another avatar shows that console at once, with no wait, and moving onto the
    toggle goes back to the current console. A pointer that only crosses the avatars brings nothing up, and the
    content panel's edge does not bring the sidebar up at all.
- **What brings up the right pane**: the mouse staying for **200 ms** on a 4 px strip along the window's right edge,
  from below the top bar to above the connection banner, or, at once, the mouse reaching the right pane's toggle.
- Touch and pen bring up neither.
- **How it shows**: both cards fade from transparent to opaque while moving upward **6 px** into place over
  **240 ms**, with the same `cubic-bezier(0.2, 0, 0, 1)` easing as switching consoles. Neither slides horizontally.
  Where the system asks for reduced motion, they appear at once. Each uses the pane's chosen width — the right pane
  at most 92% of the window's width — with a shadow and no dimming behind it. It overlays the terminal, so
  **the terminal is not resized** and the agent is sent no size change.
  - The **sidebar's card** keeps an 8 px gap from the rail, from the top bar and from the window's bottom (or the
    connection banner), never drawn over the rail.
  - The **right pane's card** keeps the same 8 px gap from the top bar, from the window's right edge and from the
    window's bottom (or the connection banner).
  - A card's own headers are 9 px shorter than the docked panes' — the 8 px gap plus the card's own 1 px top border —
    so the line under a header stays level across the panel (see "The window chrome and the content panel" above).
- **How it goes away**: 200 ms after the pointer has left it, the card fades out while moving downward 6 px over
  240 ms, with the same easing as its entrance, or disappears at once where the system asks for reduced motion.
  What counts as having left is the same on
  both sides: the pointer is on none of the card, the 8 px gap around it, or the pane's own toggle in the top bar —
  and, for the sidebar, none of the console avatars on the rail either. **The gap counts as part of the card**: a
  pointer that moves from the card into it, or rests in it — on the edge strip that brought the right pane up, or in
  the gap the sidebar's card leaves below the top bar on the way down from the toggle, say — keeps the card up for as
  long as it stays, and only a pointer that moves past the gap, or out of the window altogether, sends the card away.
  This holds however the card came up. `Escape` starts its exit without that delay and **does not reach the running agent**, except
  that in a field holding text (the project pane's change filter) it only clears the field, and the next one sends
  the pane away; so does `Escape` pressed inside a report page (see "Escape and F6 inside a page" in
  `docs/product/report-panel.md`). A floating pane that holds keyboard focus as it goes away hands focus to the
  terminal. A floating sidebar previewing a
  console that is deleted goes away as if the pointer had left that console's avatar.
- **What keeps it up**, whatever the pointer does: keyboard focus inside it, one of its menus being open, or a dialog
  being open, for as long as it is — such as one opened from the pane, the file viewer a project pane opens among them
  (see "The file viewer" in `docs/product/project-pane.md`). So closing a viewer opened from a floating project pane,
  by the pointer or the keyboard, finds the pane still there to hand focus back to.
- **Only one at a time**: bringing one up sends the other away.
- **Pressing the toggle** while the pane is floating shows it for good, and keyboard focus stays where it was. Until
  the pane is docked the toggle still reads "Show …". The card goes away and its docked column eases open in the row;
  which console the sidebar's column shows is in "Previewing a console from the rail"
  in `docs/product/sidebar.md`.
- The right pane floats in only while it has an owner. There are no floating panes below 1148 px, and narrowing the
  window below it sends one away.

## Below 1148 px: the sidebar and the right pane become drawers

Below 1148 px wide, the top bar and the rail stay as they are, and the terminal is the only pane in the content panel
and fills its width. The sidebar and the right pane each become an **overlay drawer** over it, **closed by
default**:

- Both drawers have the same rounded corners, border and shadow as the floating panes. Each keeps an **8 px gap**
  from the top bar and from the window's bottom (or the connection banner when it is shown). The sidebar drawer
  slides in from behind the rail and out behind it, never drawn over the rail, and keeps an 8 px gap from it when
  open. The right pane's drawer slides in from the window's right edge and keeps an 8 px gap from that edge.
  Their headers are 47 px high (see "The window chrome and the content panel" above).
- The sidebar drawer is 280 px wide; the right pane's drawer is 420 px wide, capped at 92% of the viewport. Both are
  also capped to the space after the rail minus two 8 px gaps, so neither touches the opposite edge of the content
  panel on a narrow screen. The dimmed area behind an open drawer covers the content panel, including the gaps,
  leaving the top bar, the rail and the banner's Retry usable.
- Each is opened and closed with the same toggle that hides and shows it at 1148 px and wider — the sidebar
  toggle at the top bar's start, the right pane's at its end — each there only while its pane exists (see "The two
  pane toggles" above).
  Browse files opens the right pane's drawer too (see "What the right pane shows" above).
- A drawer closes on its toggle, on a press anywhere in the dimmed area beside it, or on `Escape`.
  While a drawer is open `Escape` closes it and **does not reach the running agent**; in a field in the drawer that
  holds text (the project pane's change filter) it only clears the field, and the next one closes the drawer.
- Opening one drawer closes the other; at most one is ever open.
- Widening the window to 1148 px or more closes both. The right pane's drawer stays open when its owner changes,
  showing the new owner's content, and closes when the right pane is left with no owner; the sidebar's drawer closes
  the same way when the last console goes and there is no sidebar left (see `docs/product/welcome-screen.md`).
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
  sidebar toggle are at its right end, in mirrored order, the right pane's toggle at its left end, and the breadcrumb
  runs leftward from them, starting over the terminal's area while the sidebar is shown and following the column's
  inner edge while that column eases, as in "The top bar" above.
- Drawers slide in from their own pane's side: the sidebar's from behind the rail, the right pane's from the window's
  left edge. Floating cards keep their vertical fade and drift in either reading direction. The edge strip that brings up a hidden
  right pane is along the window's left edge too, and 8 px wide there rather than 4 px.
- Each resize handle stays on its pane's inner edge, the sidebar's left edge and the right pane's right one.
  Dragging toward the terminal still widens the pane, and so does the arrow key pointing away from the pane.
- Settings and the other dialogs are mirrored the same way, and toasts sit at the bottom left. So is the file viewer's
  layout: an image change's Before and After images, side by side, have Before on the right.
- A name or breadcrumb too long for its space fades out where it ends in its own direction: a name the user typed
  is laid out in the direction of its own text, so a Latin name in an Arabic window still fades at its right edge.
- Paths always read left to right, in the fields that take one, in the settings list and in a project pane's
  worktree selector, where a path too long for its space fades at its start, so its last folder stays readable.
  Settings' Default clone directory field reads left to right as one control: its folder mark stays beside the path at
  the field's left edge, rather than sitting at the field's leading edge with the path pushed away from it (see
  "Default clone directory" in `docs/product/settings.md`). The Browse button beside the field keeps to the reading
  direction's end, so it is at the field's left.
- **A value named inside a sentence keeps its own order**, wherever that sentence appears — a message, a tooltip, a
  `title`, a toast or an accessible name. A path, a branch name, a commit id, a `git` or operating-system message
  quoted inside localized wording (see "What follows the language" in `docs/product/language.md`) and a name the user
  typed all read in the direction of their own text inside right-to-left prose, and the quotation marks, parentheses
  and full stop the sentence puts around them stay where the sentence puts them.
- The rail's waiting count carries its badge at the hand glyph's top left rather than its top right, so the badge
  points into the content panel and the overflow pill stays clear of the window's edge, the rail being down the
  window's right edge (see "The rail" above).
- Icons that point a direction or show a side are mirrored: Back and Forward, the two pane toggles, whose pane is drawn
  on its own side, the breadcrumb's separators, the report panel's previous-page and next-page buttons, the file
  viewer's Previous file and Next file buttons and its wrap control, focus mode's back button, the directory browser's
  parent-directory entry, an action menu's submenu chevron, which points towards the side the submenu opens on — left
  here — and the chevron of a collapsed project row, which points left. An expanded row's chevron points down, as
  in left-to-right.
- Glyphs that hang their rows or lines off the leading side are mirrored too: the Projects heading's Expand all
  projects / Collapse all projects button, whose own chevrons stay vertical, the Git mode's Group by folder toggle in
  either of its two states, and the "View all" glyph of an archive submenu and of a focus mode's archived list.

Not mirrored:

- **The macOS window buttons** — close, minimise and zoom stay at the window's top left, and the top bar keeps its
  clear space for them there (see "The top bar" above).
- **The notifications bell's dot**, a mark on the glyph itself rather than a decoration of the layout: it stays at the
  glyph's top right, unlike the waiting count's badge above it on the rail.
- **Notation read in a fixed order**: a keyboard shortcut's key caps, as the Focus mode menu items show them (see "The
  focus mode shortcut" in `docs/product/focus-mode.md`), stay in the order the shortcut is written in; the report
  panel's position reads "current / total", so "1 / 12" is page 1 of 12 (see "Paging through the history" in
  `docs/product/report-panel.md`); and the waiting count's overflow label reads "99+".
- **A literal to type, and program text**: the query parameter and the environment-variable names the startup screen
  gives when it has no daemon address to connect to (`?port=`, `VITE_DAEMON_PORT`, `OCTOBOARD_DAEMON_PORT`) and the
  command it names, and the stack trace the screen for a failed UI shows, which also keeps its left alignment and so
  its indentation.
- **The terminal**, which is never mirrored (see "What follows the language" in `docs/product/language.md`).
- **A report page**, which keeps its own direction; only the panel around it is mirrored (see "What follows the
  language" in `docs/product/language.md`).
- **A file's content in the file viewer**: code, a diff — whose split layout keeps the old side on the left — the
  file's name and path, and the paths a change names, read left to right, and a binary change's sizes run from before
  to after left to right; only the viewer around them is mirrored. A file or folder name in the file tree, a name
  and what follows it in the Git mode's change list, a worktree's or a branch's name in the Git mode — in the viewer's
  subtitle too, where a compared change names the branches it is read from (see "Opening a change" in
  `docs/product/project-pane-git-mode.md`) — and the text of the separator a diff shows where it collapsed unchanged
  lines (see "Expanding the collapsed lines" in
  `docs/product/project-pane-git-mode.md`), are laid out in the direction of their own text, as a name the user typed
  is; a commit's id reads left to right.
- **A rendered Markdown document in the file viewer** (see "A Markdown file as a document" in
  `docs/product/project-pane.md`), which is laid out in the direction of its own text, block by block: each paragraph,
  heading, list, quote and table takes the direction of its own first strong character, so a right-to-left passage in
  a left-to-right document reads from its own side, with its markers, quote bar and columns on that side, and an
  English block in an Arabic document reads from the left. Inline code, a fenced block, a diagram and a formula always
  read left to right, and an inline formula is isolated inside its paragraph so it does not reorder the words around
  it. The document as a whole, frame included, follows the window only where it holds no strong directional character
  at all.
