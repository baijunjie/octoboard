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

Above **768 px**, each side docks only when the panes actually shown leave the terminal its **400 px** minimum. The
sidebar has priority; a right pane that cannot fit floats as a group. Each side keeps its own docking preference through
these changes. Widths, capacity thresholds and resizing are in "Docking capacity" and "Pane widths" in
`docs/product/pane-docking.md`.

**At 768 px and below**, including a portrait iPad at exactly 768 px, only one overlay is shown at a time: the sidebar
with the rail or one right-pane panel. Touch swipes open, close and switch them (see `docs/product/touch-swipes.md`).
The fixed viewport boundaries are **1024 px**, **768 px** and **390 px**. At 1024 px and wider the desktop presentation
stays the same; at 768 px and below the single-panel interaction applies; below 390 px compact spacing makes more room
for content. Pane docking above 768 px still depends on the open panes' space needs, rather than another fixed viewport
category. The macOS window's separate minimum remains **1168×600**.

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

At 768 px and below the rail, when it comes in with the floating sidebar, lies on the card's surface instead (see "At
768 px and below: the rail comes in with the sidebar" in `docs/product/sidebar-drawer.md`).

The **content panel** is always opaque, whatever is behind the window. Its top left corner (top right under a
right-to-left language), where the top bar meets the rail, is rounded; at 768 px and below, where the rail is not in the
row while there is a console, the content panel starts at the window's edge and that corner is square. No line runs
along its edges: it is set apart from the chrome by the contrast between its surfaces and the chrome and by a soft
shadow along its edges, and it reaches the window's other side and its bottom (or the connection banner) with no gap.
Inside it no line runs down the inner edge of the docked sidebar either: while the sidebar is docked the terminal's area
casts the same soft shadow as the panel's edges over the docked sidebar, so the terminal reads as the layer above it.
Where the sidebar is closed the terminal's area reaches the content panel's edge and casts no shadow on that side. It
casts none on the right pane's side either: the right pane has no surface of its own, only its panel cards, which lie on
the terminal's area across a gap of its background (see "The panel cards" in `docs/product/right-pane-panels.md`). The
terminal's area is the colour of the terminal's own background. The sidebar and the right pane's cards share the same
background — a touch greyer than the terminal in the light appearance, a touch lighter in the dark one — whether docked
or floating. A report page keeps its own light surface inside its panel (see "A report page stays on a light surface in
both appearances" in `docs/product/appearance.md`).

**Every docked sidebar header inside the content panel is 48 px high** — the sidebar's console header and a focus mode's
header (see `docs/product/sidebar.md` and `docs/product/focus-mode.md`) — and so is the archive view's header (see "The
archive view" in `docs/product/archive-view.md`), so the lines under them run on as one line across the panel. The top
bar's 40 px is a tier of its own and no part of this. A floating sidebar and every card of the right pane are inset **8
px** from the top bar above 768 px, and **4 px** at 768 px and below (see "A hidden pane floats in on hover" in
`docs/product/pane-docking.md`, "When the sidebar cannot dock" in `docs/product/sidebar-drawer.md` and "The panel cards"
in `docs/product/right-pane-panels.md`). Their headers are **40 px** above 768 px, and **44 px** at 768 px and below —
shorter than the docked headers by the inset, the card's edge taking no space — so the line under a card's header stays
level with the line under the headers docked beside it. The text in a card's header therefore sits 4 px lower than
theirs above 768 px, and 2 px lower at 768 px and below.

**The lines that divide a pane's parts are grooves**: the line under every pane header, and the lines that divide
parts and rows elsewhere in the window — under the project pane's filter bar, the Git mode's worktree selector bar and
Compare's From and To row, above a note under a pane's list, between Settings' rows and sections, in the configuration
import's lists, and in the welcome screen's list of agents. On a display of 2× pixel density
or more a groove is two 0.5 px lines, a dark one above a light one, so that it reads as pressed into the surface; below
2× it is a single 1 px dark line. Either way it takes 1 px. In the light appearance, on a surface that is white or
nearly so — a dialog such as Settings, or the archive view — the light line cannot show, and the groove reads as a
single 0.5 px dark line. The borders of controls and of a selected item, the session tree's connectors, the dashed
outline of an empty column in the file viewer and the lines a rendered document draws are not grooves.

**Panel cards take a light card shadow; whatever else floats over the content is raised by a large shadow; what lies
in the layout takes the soft one.** The panel cards — the floating sidebar (see "A hidden pane floats in on hover"
in `docs/product/pane-docking.md` and "When the sidebar cannot dock" in `docs/product/sidebar-drawer.md`), and the
right pane's cards, docked and floating alike (see "The panel cards" in `docs/product/right-pane-panels.md`) — have
the hairline edge and a light card shadow, short and faint. Whatever else floats over the content, such as the
notice floating over the terminal (see "Interrupted and archived sessions" in `docs/product/session-terminal.md`),
casts a large elevation shadow. A card lying in the layout, such as a focus mode's session card (see "What both
views show" in `docs/product/focus-mode.md`), has no outline and is set apart by the same soft shadow as the content
panel's edges.

**The sidebar and the right pane share one horizontal inset**, docked or floating alike: a
header, a bar, a section heading, a notice or a row's content starts **16 px** from the pane's edge, and a row's hover
or selected fill sits **8 px** from it. In the right pane this holds for every panel, measured from the card's edge, and
for the file viewer's header; a report page's own content lines up with it by default (see "What a page may contain, and
what it cannot do" in `docs/product/report-panel.md`).

On the screens shown before there is anything to show — while connecting to the daemon, and the screens for a daemon
that could not start, for a daemon of another version, for no daemon address or key being known, or for a UI that
failed — there is no rail: under the top bar, the content panel spans the window.

The screens for a daemon that could not start, for a daemon of another version and for a UI that failed are each an
empty-state failure of the whole page (see "Failures shown on the screen" in `docs/product/error-alerts.md`), centred
in the content panel: a red circled `!`, a heading that is the page's top-level heading, a line under it in Octoboard's own words and the remedy
after that, then a surface of its own holding what more there is to say — the failure's raw text, or the versions
involved — and a centred row of at most two buttons. No error alert opens over any of them: the page is the failure's
only presentation. None holds an alert, a mark beside its heading or a **Details** button, and nothing on it but the
glyph is drawn in the danger colour.

- **A UI that failed**: headed that an unexpected error occurred, with the line saying that Octoboard hit an error it
  could not recover from, then the remedy to try again and, if the error comes back, to report it. The surface holds
  the stack trace, headed by the error's name and message, the stack following them without repeating that line where
  the stack already begins with it. Its row holds **Try again**, the primary button, then **Report issue**.
- **A daemon that could not start**: headed that the daemon failed to start, with the line saying that the daemon is
  the background process that runs the user's sessions and terminals, then the remedy saying Octoboard cannot work
  without it and that opening Octoboard again tries once more. The surface starts with a sentence saying that if the
  steps above do not resolve it, **Report issue** files an Issue with the steps that led to it; the reason the daemon
  did not start follows (see "Starting up" in `docs/product/application-lifecycle.md`). Its row holds **Report issue**
  alone.
- **A daemon of another version**: headed that Octoboard and its daemon are different versions, with the line saying
  that the window cannot read the daemon it connected to because the two speak different versions of their protocol,
  then the remedy: in the desktop application, to quit Octoboard and open it again, and to reinstall it if this keeps
  happening; in a browser, to reload the page, and if this keeps happening to update Octoboard so that the page and the
  daemon are the same version. The surface starts with the same sentence about **Report issue** as the screen for a
  daemon that could not start, then a line giving the window's version and the daemon's, each said to be unknown where
  it is not known: the window's version is the application's as the desktop shell reports it, so a browser shows it
  unknown, and the daemon's is the release version its greeting gave, unknown for a daemon that sent none (see "A
  daemon of another version" in `docs/product/application-lifecycle.md`). Its row holds **Quit Octoboard** in the desktop application, or
  **Reload** in a browser, as the primary button, then **Report issue**.

On the screen for no daemon address or key being known, the heading is centred, and the rest — its alert, the
explanation and the list in it, then the same sentence about reporting as on the screen for a daemon that could not
start — starts at the reading edge, with **Report issue** centred under it. Its alert is titled that the daemon could
not be connected to, with the detail under the title.

**Report issue** on these four screens is a link in the shape of a button, with the external-link glyph, and opens a
prefilled Issue as the error alert's does (see `docs/product/issue-reports.md`). Where the desktop application cannot
open links it is left out, and its sentence names the issue address to open in the browser instead; on the screen for
a UI that failed, which has no such sentence otherwise, that sentence — that this may be a bug, with the address to
report it at — comes first on the surface, above the stack trace.

Of the four screens only the one for a daemon of another version has a **Quit** button, and only in the desktop
application: the window still quits through `Cmd+Q`, the application menu's Quit, the menu bar icon's Quit Octoboard
and, where closing the window quits, its close button (see "Quitting" in `docs/product/application-lifecycle.md`). As
the screens for a daemon that could not start, for a daemon of another version and for a UI that failed appear,
assistive technology is told their heading and the line under it, once; nothing on them takes keyboard focus by itself.

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

**The rail's account quota panel and Processes popover are the exception**: each floats over the content without
taking the window around it while the mouse has floated it in (opened by a tap or the keyboard it is modal, as a menu
is). While one is floated in the rest of the window stays usable, a press outside it closes it and still does what it
does where it lands, and it closes by itself when a dialog, a menu or another popover opens (see "The panel is not
modal while the mouse has floated it in" in `docs/product/account-quota-panel.md` and "Opening and closing the popover"
in `docs/product/processes-on-the-rail.md`). So is a failure or warning mark's popover while the mouse resting on the
mark has opened it: the rest of the window stays usable, and a press outside closes it and still does what it does where it
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
under it and losing their bottom edge and any press landing there. Settings and the file viewer fill the available area
at 768 px and below, ending directly above the strip. Above 768 px and below 1024 px their outer margin is 16 px. At
1024 px and wider, the file viewer keeps a 40 px margin and Settings at least 48 px, subject to its maximum size. Each
keeps that margin from the strip's top edge just as it would from the window's bottom, whatever the strip's height. A
dialog with room to spare, a confirmation say, centres in the space above the strip rather than in the whole window, so
it sits half the strip's height higher than it otherwise would. With the strip gone, every dialog sits exactly where it
would with no connection banner at all.

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
- The selected session's **breadcrumb** — *console › project › session title* for a project session, *console ›
  session title* for a console session, which belongs to no project — followed by the session's status icon. The
  session title follows "Displayed names" in `docs/product/session-titles.md`. Nothing in the bar reports the
  session's terminal connection; the terminal itself does (see "Reconnecting" in
  `docs/product/session-terminal.md`). While the sidebar is docked and shown, the breadcrumb starts where the
  terminal's area does, over the sidebar's inner edge, and follows that edge while the sidebar is resized and while
  the docked column eases open or closed (see "Hiding the sidebar" in `docs/product/pane-docking.md`). The start of
  the bar never becomes narrower than the buttons there — in the macOS application, that includes the clear space
  kept for the window buttons. Once the column no longer reaches past that start, the breadcrumb stays beside the
  sidebar toggle, which is also where it sits while the sidebar is hidden. While the sidebar floats it starts right
  after the sidebar toggle as well. Its end mirrors this: while the right pane is docked, the breadcrumb's space
  ends where the right pane does, rather than running on over it, and follows the pane's edge while it is resized.
  The end of the bar never becomes narrower than the toggles there, so once the pane no longer reaches past them,
  and while the right pane is not docked — not shown, or a floating group or a mobile panel — the breadcrumb's space
  runs on to the toggles, or to the bar's end where there are none. A trail too long for its space fades out at its
  right edge, as in "Names too long for their space" in `docs/product/labels-and-tooltips.md`, and the status icon
  always stays visible. With no session selected there is no breadcrumb. While the archive view is open (see "The
  archive view" in `docs/product/archive-view.md`) the breadcrumb shows where that is instead, with no status icon:
  *console › project › Archived sessions*, *console › console session › Archived bound sessions*, or *console ›
  Archived console sessions*.
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
has its own docking toggle beside its panel toggles (see `docs/product/right-pane-panels.md`).

The sidebar toggle is at the top bar's start. What it does depends on whether the sidebar can dock: see "Hiding the
sidebar" in `docs/product/pane-docking.md` and "When the sidebar cannot dock" in `docs/product/sidebar-drawer.md`. Its
label says what pressing it will do — "Show sessions" while the sidebar is hidden, "Hide sessions" while it is shown —
and its icon shows the sidebar's current state: a window with that side's pane drawn solid while the sidebar is shown,
and drawn dashed while it is hidden. It also tells assistive technology whether the sidebar is expanded: expanded while
it is shown, which means the floating sidebar is revealed when it cannot dock. Above 768 px, when the sidebar cannot
dock, the label always offers to show sessions: pressing it reveals the sidebar and never hides it or changes the stored
docking choice. When docking is available, a hidden sidebar that is only floating in counts as hidden for both. Shown
and hidden follow the docked state at once, including while the column is still easing open or closed (see "Hiding the
sidebar" in `docs/product/pane-docking.md`): the toggle already reads "Hide sessions" as the column opens and "Show
sessions" as it closes.

The panel toggles are at the top bar's end; their labels, icons and states are in "The panel toggles in the top bar"
in `docs/product/right-pane-panels.md`.

## The rail

The rail runs from under the top bar to the window's bottom (or the connection banner). Its two ends hold:

- **At the top, the consoles**: one avatar per console, in the order the consoles were created, the current console
  marked by a tile behind its avatar, then **New console**, which opens the new-console dialog (see
  `docs/product/consoles.md`). Choosing a console, what each avatar's badge says and what a right-click on one
  offers are in "The console switcher" in `docs/product/console-switcher.md`. While the docked sidebar is hidden,
  resting the mouse on an avatar floats the sidebar in showing that console (see "A hidden pane floats in on hover"
  in `docs/product/pane-docking.md`). When there are more consoles than fit, the avatars scroll, with no scrollbar
  drawn — so the fade at the column's edge is the only sign there are more (see "The edge that hides content fades"
  in `docs/product/scrolling-surfaces.md`); New console stays below them.
- **At the bottom**, top to bottom: - the **waiting count**, a raised hand with the number of sessions waiting for
  the user in a badge at the button's top right, its top left under a right-to-left language (see
  `docs/product/right-to-left-layout.md`), shown only while at least one is (99+ beyond 99); pressing it goes to the
  next waiting session (see "The raised hand" in `docs/product/session-statuses.md`); - **Processes**, the Processes
  panel's glyph with the number of background process trees running across every project and console session in a
  badge placed as the waiting count's, shown only while at least one is (99+ beyond 99); its popover lists where
  they run, each row leading to the Processes panel that lists them (see `docs/product/processes-on-the-rail.md`); -
  **Turn on notifications**, a bell with a small circular badge at the button's top right, shown only in a browser
  whose answer to notifications is still undecided; pressing it asks the browser (see "Notifications" in
  `docs/product/settings.md`); - **Account usage**, a gauge with a small quota indicator for the selected session's
  account (see "The selected session's quota indicator" in `docs/product/account-quotas.md`), whose floating panel
  shows the available agents' accounts and their quotas (see `docs/product/account-quota-panel.md`); - **Settings**,
  which opens the settings dialog.

Above 768 px the rail is the same at every width; while the sidebar floats it stays beside that card, which opens past
it, does not dim it, and leaves it usable while it is open. **At 768 px and below, while there is a console, the rail is
not in the row**: it comes in only as part of the open sidebar overlay's card, and is out of reach while the sidebar
overlay is closed (see "At 768 px and below: the rail comes in with the sidebar" in `docs/product/sidebar-drawer.md`).
With no console it stays in the row at every width.

### Pressing the chrome's controls

Every control in the top bar and on the rail has a tooltip, except the rail's Account usage and Processes buttons (see
"Tooltips on icon-only controls" in `docs/product/labels-and-tooltips.md`); the rail's open toward the content panel.
Pressing any of them leaves keyboard focus where it was, so after a mouse press the terminal keeps receiving
keystrokes. Account usage and Processes pressed from the keyboard are the exception: each moves focus into the panel or
popover it opens (see "Opening the panel" in `docs/product/account-quota-panel.md` and "Opening and closing the
popover" in `docs/product/processes-on-the-rail.md`). Turn on notifications pressed from the keyboard keeps focus on
itself while the browser's prompt is up, so no keystrokes reach the session meanwhile; once the browser answers and the bell goes away,
focus moves to the terminal, or to the top bar's first enabled control when the terminal cannot take it.

## What the right pane shows

The right pane has one **owner** at a time: a console session, whose report panel and Processes panel it holds, or a
project, whose Terminal, Files and Processes panels it holds, together with the selected project session's report panel
while that session has pages (see `docs/product/right-pane-panels.md`). With no owner there is no right pane at all, no
panel toggle in the top bar, and its width goes to the terminal. With an owner, the right pane is on screen while one of
its owner's panels is open. The owner is set only by what the user does, never worked out from whatever else is on
screen, so a project's Files panel can be shown with no session selected, or beside the terminal of a session of another
project.

Each session has a **pane of its own**: a console session its report panel and Processes panel, a project session its
project's Terminal, Files and Processes panels and, while it has pages, its own report panel (see "A project session's
Report panel" in `docs/product/right-pane-panels.md`).

- **Selecting a session** gives the right pane to the session's own owner, whatever the session is selected from (see
  "Selecting a session" in `docs/product/sidebar.md`), a session this window has just opened included. Selecting another
  session of the project that owns the pane leaves the project's panels as they are; only the report panel follows the
  selection, showing the newly selected session's pages, or going when it has none. Whether the pane is then on screen
  is up to which of that owner's panels are open (see "What is remembered" in `docs/product/right-pane-panels.md`).
- **Open panel** in a project's action menu (see "Opening a panel from a project's action menu" in
  `docs/product/right-pane-panels.md`) gives the right pane to that project and opens the chosen panel (see "Opening and
  closing panels" in `docs/product/right-pane-panels.md`), which puts the pane on screen — at 768 px and below it floats
  the panel over the terminal instead (see "At 768 px and below: one panel floats over the terminal" in
  `docs/product/right-pane-panels.md`); when the sidebar floats automatically it also closes its overlay. It selects,
  starts and resumes no session: the selected session's terminal stays, and the breadcrumb still names it.
- **Open panel while a console session's report panel is shown** puts the project's chosen panel in its place. Selecting
  the console session again brings the report panel back, on its newest page; anything typed into a page's form and not
  submitted is gone (see "Paging through the history" in `docs/product/report-panel.md`).
- **Making another console the current one** — pressing its avatar on the rail, pressing inside the floating sidebar
  while it previews that console, or a console created from this window appearing — gives the right pane to the own
  owner of the session the switch shows, the one last selected in that console; with no session shown there is no right
  pane (see "What switching console shows" in `docs/product/console-switcher.md`). A press inside the floating sidebar
  that gives the right pane to something itself, as Open panel does, keeps what it gave it. Only previewing a console
  changes nothing here.
- **Back and Forward** do not record the owner, which is not part of a place (see "What counts as a place" in
  `docs/product/navigation-history.md`). A move to another session gives the right pane to that session's own owner, as
  selecting it would; a move that keeps the selected session leaves the owner as it is. Either way, since the place
  moved to can have another current console than its session's, a project of any console other than the current one then
  loses the right pane; when that leaves the right pane with no owner, the selected session's own owner comes back: a
  console session at once, a project session's project only while that session's console is the current one.
- **When the owner stops existing** — its project removed, its console session deleted — there is no right pane. Nothing
  takes its place: not another project, and not the selected session's own owner.

When the owner changes while keyboard focus is inside the right pane, focus goes to the terminal.

## The window's minimum size

The macOS window has a **minimum size of 1168×600**. This native limit is separate from the responsive
layout capacities: at this width the sidebar and a right pane of one or two cards can both dock; a right pane of
three or four cards floats while the sidebar is shown. The minimum height leaves the
terminal about 30 rows.

Nothing enforces a minimum when the UI is opened in a browser, so it can also reach the mobile layout.

For pane sizing, docking, hiding and hover reveal, see `docs/product/pane-docking.md`.

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
