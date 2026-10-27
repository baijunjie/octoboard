# Window layout

The window is made of **window chrome** and a **content panel**. The chrome is a **top bar** across the window's whole
width and, under it, a narrow **rail** down the window's left edge (its right edge under a right-to-left language, see
`docs/product/right-to-left-layout.md`). Everything else is the content panel, which holds up to three panes, left to
right: the **sidebar**, showing one console's console sessions, projects and sessions (see `docs/product/sidebar.md`),
the selected session's **terminal**, which the archive view covers while it is open (see "The archive view" in
`docs/product/archive-view.md`), and the **right pane**, a stack of panel cards belonging to its owner: a console
session's **report panel** (see `docs/product/report-panel.md`), or a project's **Files panel** (see
`docs/product/project-pane.md`) and its other panels (see `docs/product/right-pane-panels.md`). The right pane is not
there at all while it has no owner or none of its owner's panels is open (see "What the right pane shows" below and
`docs/product/right-pane-panels.md`). The connection banner, while the daemon connection is down, is a full-width strip
along the window's bottom edge, under the rail and the content panel (see "Losing the daemon connection" in
`docs/product/application-lifecycle.md`). In the macOS application the update notice, while there is an update to tell
the user about, is another such strip, directly above the banner when both are shown (see "The update notice" in
`docs/product/in-app-updates.md`); whatever this doc says ends above the connection banner ends above the update notice
as well. Toasts float over the bottom right, above those strips while they are shown (see `docs/product/toasts.md`).

**With no console at all the content panel holds none of those panes**: a single **welcome screen** fills it instead,
offering the one thing there is to do, and the top bar carries no sidebar toggle (see
`docs/product/welcome-screen.md`). Everything this doc says of the three panes is what the window looks like once a
console exists.

How the panes are arranged depends on one width, **1168 px**. At that width and above they sit side by side in a row,
the layout the macOS window is always in; a sidebar the user has hidden there can still float in over the terminal for a
moment. Below it the sidebar becomes an overlay drawer over the terminal, while the right pane stays docked beside the
terminal, its width clamped, down to **782 px**; below that one panel at a time floats over the terminal instead, the
rail leaves the row to come in with the sidebar's drawer (see `docs/product/sidebar-drawer.md`), and a finger swiping
sideways opens and closes the drawer and the panels (see `docs/product/touch-swipes.md`). Only a client with no window
minimum of its own, such as the UI opened in a plain browser, can get below 1168 px. With none of its panels open, the
right pane can float a panel in over the terminal at 782 px and wider.

The right pane's cards and floating behavior are the same whoever owns it.

For the window's light/dark appearance see `docs/product/appearance.md`; for how a dialog is titled, where focus starts
in it and what Enter does there see `docs/product/dialogs.md`; for how the whole window mirrors under a right-to-left
language see `docs/product/right-to-left-layout.md`; for the settings the rail opens see `docs/product/settings.md`; for
the top bar's Back and Forward see `docs/product/navigation-history.md`; for moving keyboard focus between the window's
regions see `docs/product/moving-focus-between-regions.md`; for how the macOS window's size and position are remembered
see `docs/product/window-size-and-position.md`.

## The window chrome and the content panel

The top bar is **40 px** high and the rail **48 px** wide. They carry only icons, the consoles' avatars and the one line
of the breadcrumb.

- **In the macOS application** the top bar and the rail show the system's translucent sidebar material, which follows
  the window's light or dark appearance (see "What follows the choice" in `docs/product/appearance.md`).
- **In a browser** the chrome is an opaque colour of its own, which follows the appearance too.

Below 782 px the rail, when it comes in as part of the sidebar's drawer, lies on the drawer's surface instead (see
"Below 782 px: the rail comes in with the drawer" in `docs/product/sidebar-drawer.md`).

The **content panel** is always opaque, whatever is behind the window. Its top left corner (top right under a
right-to-left language), where the top bar meets the rail, is rounded; below 782 px, where the rail is not in the row
while there is a console, the content panel starts at the window's edge and that corner is square. No line runs along
its edges: it is set apart from the chrome by the contrast between its surfaces and the chrome and by a soft shadow
along its edges, and it reaches the window's other side and its bottom (or the connection banner) with no gap. Inside it
no line runs down the inner edge of the docked sidebar either: at 1168 px and wider the terminal's area casts the same
soft shadow as the panel's edges over the docked sidebar, so the terminal reads as the layer above it. Where the sidebar
is closed the terminal's area reaches the content panel's edge and casts no shadow on that side. It casts none on the
right pane's side either: the right pane has no surface of its own, only its panel cards, which lie on the terminal's
area across a gap of its background (see "The panel cards" in `docs/product/right-pane-panels.md`). The terminal's area
is the colour of the terminal's own background. The sidebar and the right pane's cards share the same background — a
touch greyer than the terminal in the light appearance, a touch lighter in the dark one — whether docked, floating or
shown as a drawer. A report page keeps its own light surface inside its panel (see "A report page stays on a light
surface in both appearances" in `docs/product/appearance.md`).

**Every docked sidebar header inside the content panel is 48 px high** — the sidebar's console header and a focus
mode's header (see `docs/product/sidebar.md` and `docs/product/focus-mode.md`) — and so is the archive view's header
(see "The archive view" in `docs/product/archive-view.md`), so the lines under them run on as one line across the panel.
The top bar's 40 px is a tier of its own and no part of this. A floating sidebar, the sidebar's drawer and every card
of the right pane are cards inset 8 px from the top bar (see "A hidden pane floats in on hover" below, "Below 1168 px:
the sidebar becomes a drawer" in `docs/product/sidebar-drawer.md` and "The panel cards" in
`docs/product/right-pane-panels.md`), and their headers are
**40 px** — shorter by that 8 px inset, the card's edge taking no space — so the line under a card's header stays
level with the line under the headers docked beside it; the text in a card's header therefore sits 4 px lower than
theirs.

**The lines that divide a pane's parts are grooves**: the line under every pane header, and the lines that divide
parts and rows elsewhere in the window — under the project pane's filter bar, the Git mode's worktree selector bar and
Compare's From and To row, above a note under a pane's list, between Settings' rows and sections, in the configuration
import's lists, and in the welcome screen's list of agents. On a display of 2× pixel density
or more a groove is two 0.5 px lines, a dark one above a light one, so that it reads as pressed into the surface; below
2× it is a single 1 px dark line. Either way it takes 1 px. In the light appearance, on a surface that is white or
nearly so — a dialog such as Settings, or the archive view — the light line cannot show, and the groove reads as a
single 0.5 px dark line. The borders of controls and of a selected item, the session tree's connectors, the dashed
outline of an empty column in the file viewer and the lines a rendered document draws are not grooves.

**Panel cards take a light card shadow; whatever else floats over the content is raised by a large shadow; what
lies in the layout takes the soft one.** The panel cards — the floating sidebar and the sidebar's drawer (see "A
hidden pane floats in on hover" below and "Below 1168 px: the sidebar becomes a drawer" in
`docs/product/sidebar-drawer.md`), and the right pane's cards,
docked and floating alike (see "The panel cards" in `docs/product/right-pane-panels.md`) — have the hairline edge and a
light card shadow, short and faint. Whatever else floats over the content, such as the notice floating over the
terminal (see "Interrupted, archived and dormant sessions" in `docs/product/session-terminal.md`), casts a large
elevation shadow. A card lying in the layout, such as a focus mode's session card (see "What both views show" in
`docs/product/focus-mode.md`), has no outline and is set apart by the same soft shadow as the content panel's
edges.

**The sidebar and the right pane share one horizontal inset**, docked, floating or as the sidebar's drawer alike: a
header, a bar, a section heading, a notice or a row's content starts **16 px** from the pane's edge, and a row's hover
or selected fill sits **8 px** from it. In the right pane this holds for every panel, measured from the card's edge, and
for the file viewer's header; a report page's own content lines up with it by default (see "What a page may contain, and
what it cannot do" in `docs/product/report-panel.md`).

On the screens shown before there is anything to show — while connecting to the daemon, and the screens for a daemon
that could not start, for no daemon address being known, or for a UI that failed — there is no rail: under the top
bar, the content panel spans the window.

The screens for a daemon that could not start and for a UI that failed are each an empty-state failure of the whole
page (see "Failures shown on the screen" in `docs/product/error-alerts.md`), centred in the content panel: a red
circled `!`, a heading that is the page's top-level heading, followed on its line by an error mark (a red circled `!`
button), one line under it in Octoboard's own words, and a centred row of at most two buttons. The failure's raw text
is in the popover the heading's mark opens, and never on the page; neither screen holds an alert or a **Details**
button, and nothing on it but the glyph and the mark is drawn in the danger colour. **Quit** is there only where the
application can quit itself.

- **A UI that failed**: headed that an unexpected error occurred, its mark named why Octoboard stopped, with the line
  saying to try again and, if the error comes back, to report it. Its row holds **Try again**, the primary button,
  then **Quit**. The mark's popover holds the stack trace, headed by the error's name and message, the stack following
  them without repeating that line where the stack already begins with it.
- **A daemon that could not start**: headed that the daemon failed to start, its mark named why the daemon did not
  start, with the line saying Octoboard cannot work without it and that opening Octoboard again tries once more. Its
  row holds **Quit** alone. The mark's popover holds the daemon's own message, its long lines wrapped.

The error alert shown first, over the screen for a UI that failed, is titled that an unexpected error occurred, and
its body says that Octoboard hit an error it could not recover from, with the same stack trace, headed the same way,
under its **Error details**; the one over the screen for a daemon that could not start is described in "Starting up" in
`docs/product/application-lifecycle.md`.

On the screen for no daemon address being known, the heading and the button under it are centred, and the rest — its
alert, the explanation and the list in it — starts at the reading edge. Its alert is titled that the daemon could not
be connected to, with the detail under the title.

## What an open dialog, menu or popover covers

**A dialog** — Settings, the file viewer, a console, project or session dialog, a confirmation — is drawn over a dimmed
overlay that covers the whole window: the top bar, the rail and the content panel are dimmed behind it and none of
them can be used while it is open. A press on the dimmed area dismisses an ordinary dialog, but never answers or
closes a confirmation. A confirmation's buttons, close button and Escape still work; an error alert requires its
acknowledgement button (see "Error alerts" in `docs/product/error-alerts.md`).

**A menu or a popover** — a row's, a header's or an avatar's action menu, the project filter's popover, a worktree or
branch selector's list of choices, a selector's list inside a dialog — takes the window around it the same way: while
one is open, nothing outside it can be used, and a press outside dismisses it. It has no dimmed overlay of its own, so
nothing behind it is dimmed; what it shares with a dialog is that the rest of the window is out of reach until it is
gone.

**The rail's account quota panel is the exception**: it floats over the content without taking the window around it
while the mouse has floated it in (opened by a tap or the keyboard it is modal, as a menu is). While it is floated in
the rest of the window stays usable, a press outside it closes it and still does what it does where it lands, and it
closes by itself when a dialog or a menu opens (see "The panel is not modal while the mouse has floated it in" in
`docs/product/account-quota-panel.md`). So is a failure or warning mark's popover while the mouse resting on the mark
has opened it: the rest of the window stays usable, and a press outside closes it and still does what it does where it
lands. Once the mark is pressed its popover is an ordinary popover (see "Failures, warnings and notes marked at their
point" in `docs/product/error-alerts.md`).

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
window — Settings and the file viewer in the narrow layout, the file viewer at 1168 px and wider too — keeps the same
margin from the strip's top edge that it would have kept from the window's bottom edge, 16 px in the narrow layout and
40 px at 1168 px and wider, whatever the strip's height; its margin above is the same either way. A dialog with room
to spare, a confirmation say, centres in the space above the strip rather than in the whole window, so it sits half
the strip's height higher than it otherwise would. With the strip gone, every dialog sits exactly where it would with
no connection banner at all.

The update notice's strip counts toward the same room: while it is shown, a dialog that fills the window keeps its
margin from the top of the uppermost strip, and a dialog with room to spare centres in the space above both strips. It
is not drawn above a dialog's overlay, though: it stays on screen, dimmed along with everything else, and a press where
it is drawn lands on the overlay, as a press anywhere else outside the dialog does.

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
  title* for a console session, which belongs to no project — followed by the session's status icon. The session title
  follows "Displayed names" in `docs/product/session-titles.md`. Nothing in the bar reports the session's terminal
  connection; the terminal itself does (see "Reconnecting" in `docs/product/session-terminal.md`). At 1168 px and wider,
  while the sidebar is shown, the breadcrumb starts where the terminal's area does, over the sidebar's inner edge, and
  follows that edge while the sidebar is resized and while the docked column eases open or closed (see "Hiding the
  sidebar" below). The start of the bar never becomes narrower than the buttons there — in the macOS application, that
  includes the clear space kept for the window buttons. Once the column no longer reaches past that start, the
  breadcrumb stays beside the sidebar toggle, which is also where it sits while the sidebar is hidden. Below 1168 px it
  starts right after the sidebar toggle as well. Its end mirrors this: while the right pane is docked, the breadcrumb's
  space ends where the right pane does, rather than running on over it, and follows the pane's edge while it is resized.
  The end of the bar never becomes narrower than the toggles there, so once the pane no longer reaches past them, and
  while the right pane is not docked — not shown, or a panel floating over the terminal below 782 px — the breadcrumb's
  space runs on to the toggles, or to the bar's end where there are none. A trail too long for its space fades out at
  its right edge, as in "Names too long for their space" in `docs/product/labels-and-tooltips.md`, and the status icon
  always stays visible. With no session selected there is no breadcrumb. While the archive view is open (see "The
  archive view" in `docs/product/archive-view.md`) the breadcrumb shows where that is instead, with no status icon:
  *console › project › Archived sessions*, *console › console session › Archived bound sessions*, or *console › Archived
  console sessions*.
- At the **end of the bar**, the **panel toggles**: one per kind of panel the right pane's owner has, there only while
  the right pane has an owner (see "What the right pane shows" below), each showing or hiding its panel and so the right
  pane (see "The panel toggles in the top bar" in `docs/product/right-pane-panels.md`). The end is the reading
  direction's, so under a right-to-left language it is at the window's left (see
  `docs/product/right-to-left-layout.md`).

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

### The sidebar toggle and the panel toggles

**Each toggle is there only while what it controls exists**: the sidebar toggle while there is a console for the
sidebar to show (see `docs/product/welcome-screen.md`), the panel toggles while the right pane has an owner (see "What
the right pane shows" below). A toggle with nothing to control is left out rather than shown disabled. The right pane
has no toggle of its own: it is on screen while one of its panels is open (see `docs/product/right-pane-panels.md`).

The sidebar toggle is at the top bar's start. What it does depends on the window's width: see "Hiding the sidebar" below
and "Below 1168 px: the sidebar becomes a drawer" in `docs/product/sidebar-drawer.md`. Its label says what pressing it
will do — "Show sessions" while the sidebar is hidden, "Hide sessions" while it is shown — and its icon shows the
sidebar's current state: a window with that side's pane drawn solid while the sidebar is shown, and drawn dashed while
it is hidden. It also tells assistive technology whether the sidebar is expanded: expanded while it is shown, which
below 1168 px means its drawer is open. A hidden sidebar that is only floating in counts as hidden for both. Shown and
hidden follow the docked state at once, including while the column is still easing open or closed (see "Hiding the
sidebar" below): the toggle already reads "Hide sessions" as the column opens and "Show sessions" as it closes.

The panel toggles are at the top bar's end; their labels, icons and states are in "The panel toggles in the top bar"
in `docs/product/right-pane-panels.md`.

## The rail

The rail runs from under the top bar to the window's bottom (or the connection banner). Its two ends hold:

- **At the top, the consoles**: one avatar per console, in the order the consoles were created, the current console
  marked by a tile behind its avatar, then **New console**, which opens the new-console dialog (see
  `docs/product/consoles.md`). Choosing a console, what each avatar's badge says and what a right-click on one offers
  are in "The console switcher" in `docs/product/console-switcher.md`. While the docked sidebar is hidden, resting the
  mouse on an avatar floats the sidebar in showing that console (see "A hidden pane floats in on hover" below). When
  there are more consoles than fit, the avatars scroll, with no scrollbar drawn — so the fade at the column's edge is
  the only sign there are more (see "The edge that hides content fades" in `docs/product/scrolling-surfaces.md`); New
  console stays below them.
- **At the bottom**, top to bottom:
  - the **waiting count**, a raised hand with the number of sessions waiting for the user in a badge at its top right,
    its top left under a right-to-left language (see `docs/product/right-to-left-layout.md`), shown only while at least
    one is (99+ beyond 99); pressing it goes to the next waiting session (see "The raised hand" in
    `docs/product/session-statuses.md`);
  - **Turn on notifications**, a bell with a small dot on it, shown only in a browser whose answer to notifications is
    still undecided; pressing it asks the browser (see "Notifications" in `docs/product/settings.md`);
  - **Account usage**, a gauge with a small quota indicator for the selected session's account (see "The selected
    session's quota indicator" in `docs/product/account-quotas.md`), whose floating panel shows the available agents'
    accounts and their quotas (see `docs/product/account-quota-panel.md`);
  - **Settings**, which opens the settings dialog.

From 782 px up the rail is the same at every width; below 1168 px it stays beside the sidebar's drawer, which opens
past it, does not dim it, and leaves it usable while it is open. **Below 782 px, while there is a console, the rail is
not in the row**: it comes in only as part of the open drawer's card, and is out of reach while the drawer is closed
(see "Below 782 px: the rail comes in with the drawer" in `docs/product/sidebar-drawer.md`). With no console it stays
in the row at every width.

### Pressing the chrome's controls

Every control in the top bar and on the rail has a tooltip, except the rail's Account usage button (see "Tooltips on
icon-only controls" in `docs/product/labels-and-tooltips.md`); the rail's open toward the content panel. Pressing any of
them leaves keyboard focus where it was, so after a mouse press the terminal keeps receiving keystrokes. Account usage
pressed from the keyboard is the exception: it moves focus into the panel it opens (see "Opening the panel" in
`docs/product/account-quota-panel.md`). Turn on notifications pressed from the keyboard keeps focus on itself while the
browser's prompt is up, so no keystrokes reach the session meanwhile; once the browser answers and the bell goes away,
focus moves to the terminal, or to the top bar's first enabled control when the terminal cannot take it.

## What the right pane shows

The right pane has one **owner** at a time: a console session, whose report panel it holds, or a project, whose Files
and Terminal panels it holds, together with the selected project session's report panel while that session has pages
(see `docs/product/right-pane-panels.md`). With no owner there is no right pane at all, no panel toggle in the top bar,
and its width goes to the terminal. With an owner, the right pane is on screen while one of its owner's panels is open.
The owner is set only by what the user does, never worked out from whatever else is on screen, so a project's Files
panel can be shown with no session selected, or beside the terminal of a session of another project.

Each session has a **pane of its own**: a console session its report panel, a project session its project's Files
and Terminal panels and, while it has pages, its own report panel (see "A project session's Report panel" in
`docs/product/right-pane-panels.md`).

- **Selecting a session** gives the right pane to the session's own owner, whatever the session is selected from (see
  "Selecting a session" in `docs/product/sidebar.md`), a session this window has just opened included. Selecting another
  session of the project that owns the pane leaves the project's panels as they are; only the report panel follows the
  selection, showing the newly selected session's pages, or going when it has none. Whether the pane is then on screen
  is up to which of that owner's panels are open (see "What is remembered" in `docs/product/right-pane-panels.md`).
- **Browse files**, when offered in a project's action menu (see "Opening a project's files" in
  `docs/product/project-pane.md`), gives the right pane to that project and opens its Files panel (see "Opening and
  closing panels" in `docs/product/right-pane-panels.md`), which puts the pane on screen — below 782 px it floats the
  Files panel over the terminal instead (see "Below 782 px: one panel floats over the terminal" in
  `docs/product/right-pane-panels.md`); below 1168 px it also closes the sidebar's drawer. It selects, starts and
  resumes nothing: the selected session's terminal stays, and the breadcrumb still names it.
- **Browse files while a console session's report panel is shown** puts the project's Files panel in its place.
  Selecting the console session again brings the report panel back, on its newest page; anything typed into a page's
  form and not submitted is gone (see "Paging through the history" in `docs/product/report-panel.md`).
- **Making another console the current one** — pressing its avatar on the rail, pressing inside the floating sidebar
  while it previews that console, or a console created from this window appearing — gives the right pane to the own
  owner of the session the switch shows, the one last selected in that console; with no session shown there is no
  right pane (see "What switching console shows" in `docs/product/console-switcher.md`). A press inside the floating
  sidebar that gives the right pane to something itself, as Browse files does, keeps what it gave it. Only previewing a
  console changes nothing here.
- **Back and Forward** do not record the owner, which is not part of a place (see "What counts as a place" in
  `docs/product/navigation-history.md`). A move to another session gives the right pane to that session's own owner,
  as selecting it would; a move that keeps the selected session leaves the owner as it is. Either way, since the place
  moved to can have another current console than its session's, a project of any console other than the current one
  then loses the right pane; when that leaves the right pane with no owner, the selected session's own owner comes
  back: a console session at once, a project session's project only while that session's console is the current one.
- **When the owner stops existing** — its project removed, its console session deleted — there is no right pane.
  Nothing takes its place: not another project, and not the selected session's own owner.

When the owner changes while keyboard focus is inside the right pane, focus goes to the terminal.

## The window's minimum size

The macOS window has a **minimum size of 1168×600**. The width is the rail's 48 px plus the three panes' own floors,
with the sidebar at its default width (48 + 280 + 520 + 320), so none of the panes can be squeezed past usability by a
narrower window; the height leaves the terminal about 30 rows.

Nothing enforces a minimum when the UI is opened in a browser, so the narrow layout below applies there.

## Pane widths at 1168 px and wider

| Pane | Default | Range the user can choose |
|---|---|---|
| Sidebar | 280 px | 280–480 px |
| Right pane | 420 px | 320–720 px |
| Terminal | everything left over | never below 520 px (about 53 columns) |

The right pane's width includes the gaps around its cards (see "The panel cards" in
`docs/product/right-pane-panels.md`). The sidebar and the right pane are drawn at the width the user chose (see
"Resizing the sidebar and the right pane" below), and neither grows past it: width a wider window frees goes to the
terminal. The right pane has one width, whoever owns it.

**The terminal always keeps its 520 px.** Where the window cannot afford every pane its chosen width, the right pane
gives up width first, down to 320 px, and only then the sidebar, down to 280 px. At the 1168 px minimum with both panes
shown that makes, beside the 48 px rail, the sidebar 280 px, the terminal 520 px and the right pane 320 px. This never
changes the remembered width: when the window widens again, each pane gets its chosen width back.

A sidebar the user has hidden takes no width at all once it is hidden, and neither does one only floating in: its
width goes to the terminal. While the sidebar's docked column is easing closed, it still takes the width it has
reached (see "Hiding the sidebar" below). A right pane with none of its panels open, or with no owner, is not in the
row, takes no width and stops holding back the sidebar's width; one only floating a panel in takes none either, and
nor does a panel floating over the terminal below 782 px.

Anything that changes the terminal's size — resizing the window, resizing a pane, hiding or showing the sidebar, the
right pane coming or going as its first panel opens or its last closes or with its owner — resizes the agent's
terminal (see "Output, scrollback and size" in `docs/product/session-terminal.md`). A panel floating over the
terminal below 782 px coming or going does not.

### Resizing the sidebar and the right pane

Each pane has a resize handle on its inner edge, running the pane's full height, from below the top bar to above the
connection banner, while it is shown:

- the **sidebar's** on its right edge, drawn on hover and keyboard focus as a line along the whole edge;
- the **right pane's** in the 8 px gap between the terminal and the cards, drawn as a **grip**: a short rounded bar
  in the middle of the gap's height, light grey while the pointer or keyboard focus is on the handle, and strong only
  while it is pressed or dragged. While a mouse pointer rests on it, a tooltip saying "Resize" opens beside the grip,
  toward the pane. It is named "Resize panel area".

Both work the same way:

- **Dragging** the handle sets the pane's width within its current limits; below 1168 px the right pane's minimum
  follows the panels in its column (see "Pane widths below 1168 px" below).
- **Double-clicking** it returns the pane to its default width.
- The handle is also reachable with Tab and is then operated from the keyboard: the arrow pointing away from the pane
  widens it and the other narrows it (Right widens the sidebar and Left the right pane; the other way round under a
  right-to-left language, where the panes have swapped sides), by 16 px, by 64 px with Shift held; Home sets the pane's
  minimum, and End the widest the window currently allows. While it has keyboard focus the sidebar's handle is drawn as
  a solid accent-coloured bar with a focus ring around it, and the right pane's grip shows with a focus ring around
  the handle. Pressing the handle with the mouse does not take keyboard focus off the terminal, and while it is dragged
  the pointer shows the resize cursor wherever it is.

The sidebar's handle is there only while the sidebar is shown in the row: not while it is hidden or floating in, and
not below 1168 px, where the sidebar is a drawer; it is also absent while its column is easing open or closed. The
right pane's handle is there whenever the right pane is docked, which is at 782 px and wider, and not while a panel
only floats in, nor while a panel floats over the terminal below 782 px, nor while the pane's column is folded away
under a maximized Terminal panel (see "Maximizing the Terminal panel" in
`docs/product/right-pane-panels.md`). Its pointer target is 24 px wide, centred on the 8 px gap.

**Each pane's chosen width is remembered per client**, in that client's own browser storage, as the appearance choice
is (see "Where the choice is stored" in `docs/product/appearance.md`); without that storage a choice lasts until the
page is reloaded. A stored sidebar width below 280 px is used as 280 px.

## Hiding the sidebar

At 1168 px and wider, the sidebar toggle in the top bar hides and shows the docked sidebar. The sidebar is shown by
default, and **whether it is hidden is remembered per client**, the same way as the panes' widths. Hiding it hands
its width to the terminal. The right pane has no such toggle: it shows and hides as its panels open and close, at once
and with no ease (see `docs/product/right-pane-panels.md`).

Hiding the sidebar while it holds keyboard focus — a sidebar row reached with Tab, say — moves focus to the terminal,
or to the top bar when no session is selected.

At 1168 px and wider, hiding or showing the docked sidebar eases its column over **200 ms**. Where the system asks
for reduced motion, the column opens or closes at once. Pressing the toggle again while the column is still moving
reverses the ease. Below 1168 px the sidebar is a drawer and fades into place instead (see "Below 1168 px: the
sidebar becomes a drawer" in `docs/product/sidebar-drawer.md`).

During the ease the sidebar stays a full-height column, from below the top bar to above the connection banner when
that banner is shown, and does not become the rounded card in "A hidden pane floats in on hover" below. Its contents
stay at the width the open column is drawn at, including where the window is holding that width back (see "Pane
widths at 1168 px and wider" above), anchored against the rail. The inner edge covers them as the column closes and
uncovers them as it opens, so the text does not reflow. The shadow the terminal's area casts over the sidebar follows
the column's inner edge through the ease and is gone once the column has closed. A hover card comes up only once the
column has closed.

As the sidebar's column closes, the width it gives up goes to the terminal. When the right pane is docked and had been
held below the width the user chose, it can grow into that room as the room appears. Showing the sidebar limits the
right pane again as soon as the sidebar is shown, when the window cannot afford both panes their chosen widths, and
the opening column takes its own width from the terminal as it eases open. The terminal is not refitted while the
column moves: it keeps its size through the ease and is fitted once the column comes to rest (see "Output, scrollback
and size" in `docs/product/session-terminal.md`).

### A hidden pane floats in on hover

A pane that is not on screen can be brought up for a moment without showing it for good: a hidden sidebar at 1168 px and
wider, and the right pane, at 782 px and wider, while it has an owner and none of its panels is open, or only a
maximized Terminal panel (see "Floating a panel in" in `docs/product/right-pane-panels.md`). This is a **floating
pane**. The sidebar floats in as a card inset from the window chrome and the window's edges, with rounded corners, a
hairline edge and the light card shadow (see "The window chrome and the content panel" above); the right pane floats in
as its panel's own card, which looks the same (see "The panel cards" in `docs/product/right-pane-panels.md`). The
hairline is 0.5 px on a display of 2× pixel density or more and 1 px below that, and it takes no space, so the card is
exactly its box. Below 1168 px the sidebar's drawer shares this card appearance (see "Below 1168 px: the sidebar becomes
a drawer" in `docs/product/sidebar-drawer.md`). The docked sidebar opens and closes as a full-height column (see
"Hiding the sidebar" above).

- **What brings up the sidebar**: the mouse reaching the sidebar's toggle, at once, or resting for **130 ms** on a
  console's avatar on the rail, the current console's or another's. The wait is the avatars' alone, so that a pointer
  crossing the rail on its way elsewhere does not make the card flash; the toggle is aimed at, so it has none.
  - **From the toggle the sidebar shows the current console**, whatever console a card was previewing just before.
  - **From an avatar it shows that console**, which need not be the current one; what that preview is and how a press
    inside it makes the console current is in "Previewing a console from the rail" in
    `docs/product/console-switcher.md`.
  - While the sidebar is out, moving onto another avatar shows that console at once, with no wait, and moving onto the
    toggle goes back to the current console. A pointer that only crosses the avatars brings nothing up, and the
    content panel's edge does not bring the sidebar up at all.
- **What brings up the right pane**: the mouse reaching a panel toggle in the top bar, at once, which floats that
  toggle's panel in (see "Floating a panel in" in `docs/product/right-pane-panels.md`). Nothing along the window's
  edge brings it up.
- Touch and pen bring up neither, and on a device whose primary input cannot hover — a phone, or a tablet's touch
  screen — neither floats in at all, whatever pointer is used: the sidebar is shown with its toggle and a panel
  opened by pressing its toggle.
- **How it shows**: both cards fade from transparent to opaque while rising into place by **4% of the card's own
  height**, over **240 ms**, with the same `cubic-bezier(0.2, 0, 0, 1)` easing as switching consoles. The whole card
  fades and rises together — its edge, fill, shadow and content. Neither slides horizontally.
  Where the system asks for reduced motion, they appear at once. Each uses the pane's chosen width — the right pane
  at most 92% of the window's width — with a shadow and no dimming behind it. It overlays the terminal, so
  **the terminal is not resized** and the agent is sent no size change.
  - The **sidebar's card** keeps an 8 px gap from the rail, from the top bar and from the window's bottom (or the
    connection banner), never drawn over the rail.
  - The **right pane's card** keeps the gaps it keeps docked: 8 px from the top bar, from the window's end edge and
    from the window's bottom (or the connection banner), and 8 px on the terminal's side.
  - A card's own headers are 8 px shorter than the docked sidebar's — the 8 px gap, the card's edge taking no space —
    so the line under a header stays level across the panel (see "The window chrome and the content panel" above).
- **How it goes away**: 200 ms after the pointer has left it, the card fades out while sinking back by 4% of its height
  over 240 ms, with the same easing as its entrance, or disappears at once where the system asks for reduced motion.
  Having left means the pointer is on none of the card, the gap around it, or what brought it up in the top bar — the
  sidebar's toggle, or any panel toggle for the right pane — and, for the sidebar, none of the console avatars on the
  rail either. **The gap counts as part of the card**: the 8 px around the sidebar's card, and the gaps the right pane
  keeps around its card. A pointer that moves from the card into the gap, or rests in it — in the gap the sidebar's card
  leaves below the top bar on the way down from the toggle, say — keeps the card up for as long as it stays, and only a
  pointer that moves past the gap, or out of the window altogether, sends the card away. `Escape` starts its exit
  without that delay and **does not reach the running agent**, except that in a field holding text (the project pane's
  change filter) it only clears the field, and the next one sends the pane away; so does `Escape` pressed inside a
  report page (see "Escape and F6 inside a page" in `docs/product/report-panel.md`). Inside a shell terminal of the
  Terminal panel `Escape` goes to the shell and never sends the pane away (see "The shell terminal" in
  `docs/product/terminal-panel.md`). A floating pane that holds keyboard focus as it goes away hands focus to the
  terminal, except a floating right pane that goes because the pane's column takes its place — a panel docked, a
  maximized Terminal panel restored (see "Maximizing the Terminal panel" in `docs/product/right-pane-panels.md`): focus
  is then not moved. A floating sidebar previewing a console that is deleted goes away as if the pointer had left that
  console's avatar.
- **What keeps it up**, whatever the pointer does: keyboard focus inside it, one of its menus being open, or a dialog
  being open, for as long as it is — such as one opened from the pane, the file viewer the Files panel opens among
  them (see `docs/product/file-viewer.md`). So closing a viewer opened from a floating Files
  panel, by the pointer or the keyboard, finds the pane still there to hand focus back to. For the right pane, focus
  keeps it up only when the keyboard put it there: focus a press put inside it, on a tab or a button, does not, and
  the card goes once the pointer leaves, as it would have before the press.
- **Only one at a time**: bringing one up sends the other away. Floating the right pane in also closes the sidebar's
  drawer, and opening the sidebar's drawer sends a floating right pane away.
- **Pressing the toggle** while the pane is floating shows it for good, and keyboard focus stays where it was. For the
  sidebar, until the pane is docked the toggle still reads "Show sessions"; the card goes away and its docked column
  eases open in the row; which console the column shows is in "Previewing a console from the rail" in
  `docs/product/console-switcher.md`. For the right pane, pressing the floating panel's toggle opens that panel and
  docks the pane at once, with no animation, or below 782 px floats it over the terminal instead (see "Floating a panel
  in" in `docs/product/right-pane-panels.md`).
- There is no floating sidebar below 1168 px, and narrowing the window below it sends one away. There is no floating
  right pane below 782 px, where a pressed panel toggle floats its panel over the terminal instead (see "Below 782
  px: one panel floats over the terminal" in `docs/product/right-pane-panels.md`).

## Pane widths below 1168 px

Below 1168 px the sidebar is not in the row but a drawer over it (see "Below 1168 px: the sidebar becomes a drawer" in
`docs/product/sidebar-drawer.md`).

**The right pane stays docked below 1168 px, down to 782 px**, at the width the user chose, clamped so that the
terminal keeps its **398 px** floor, about 40 columns. The right pane's minimum is **336 px while the Terminal panel
is in its column**, and **320 px otherwise**; a maximized Terminal panel does not raise the Files panel's column
minimum. This effective minimum does not rewrite the remembered width. 782 px is the rail, the terminal's floor and
the larger of those minimums side by side (48 + 398 + 336 px).

**Below 782 px the right pane does not dock**: one panel at a time floats over the terminal, which takes the content
panel's whole width with no minimum, so the row never scrolls sideways (see "Below 782 px: one panel floats over the
terminal" in `docs/product/right-pane-panels.md`). While there is a console the rail is not in the row there either,
so the content panel spans the window's whole width (see "Below 782 px: the rail comes in with the drawer" in
`docs/product/sidebar-drawer.md`).

## Right-click menus

**In the desktop app** a right-click (or Control-click) shows the system's own menu only in an input field, a text area
or editable text outside the terminal. In the terminal it shows no system menu. Anywhere else it shows nothing, unless
what was clicked offers its own action menu: the sidebar's rows and headers open theirs at the pointer (see "Rows, names
and keyboard focus" in `docs/product/sidebar.md`), and so do the consoles' avatars on the rail (see "The console
switcher" in `docs/product/console-switcher.md`). The project pane's file and change rows also open their own menu (see
"A row's actions" in `docs/product/project-pane.md`). A report page follows the same rule (see "Right-click inside a
page" in `docs/product/report-panel.md`).

**The terminal has its own editing menu** in both the desktop app and a browser (see "Copy, paste and selection" in
`docs/product/session-terminal.md`).

**In a browser** the browser's own menu stays everywhere except over the rows, headers and avatars that open an action
menu, and over the terminal.
