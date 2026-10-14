# Focus mode

Focus mode replaces the whole sidebar — the console header and the console sessions and project lists (see
`docs/product/sidebar.md`) — with one project or one console session. Choosing **Focus mode** from a project's action
menu enters the first, from a console session's action menu the second. The two views are built alike, and differ in
what they list.

## What both views show

**The header**, in both: a back button, **Leave focus mode**, which returns to the full sidebar; the console's name
above the name of what is in focus; a **+** button for a new session; and that thing's action menu, without its **Focus
mode** and **Pin** / **Unpin** items. The header shows no pin button either. A project's header also carries its branch
badge (see "The branch badge" in `docs/product/project-git-status.md`). A console session's header shows its colour
beside its name, and pressing the name selects the console session, whose terminal and report panel the view has no
other row for.

**Session cards**, in both: the status glyph with the status in words, the pin button when pinned (see "Pinning" in
`docs/product/sidebar.md`), the session's action menu (always shown), its title over up to two lines, its agent's icon
and name with its account, and how long ago it was started. The agent's icon has a tooltip, as on a session row (see
"Session rows" in `docs/product/sidebar.md`). Clicking a card selects the session. They are in the order of "Order of
projects and sessions" in `docs/product/sidebar.md`. While cards move, their backgrounds stay opaque, so text from
other cards, the console-session summary or the archive does not show through them.

A card for a session **bound to a project session the same view lists** is **inset under that session's card** and
follows it, as a session row is under its owner's row (see "Project rows" in `docs/product/sidebar.md`); the cards sit
apart from one another, so the nesting is the inset alone, with no line down it. The owner's name in the card's
accessible name carries the nesting to assistive technology. In a project's combined view, cards also identify the
console session above them (see "A project's focus mode" below and "The binding badge" in
`docs/product/sidebar.md`).

**Archived (n)**, in both, only while the view has archived sessions — with none, the section is not shown at all: the
ten most recent rows of the view's archive, each showing its agent's icon, its title and how long ago it was
archived. Clicking one selects it. Its action menu offers **Rename** and **Delete** (which asks for confirmation; see
"Deleting archived sessions" in `docs/product/sessions.md`). Below them **View all (n)** opens the archive view. An
archived session whose owner is a project session the list holds too is inset under it, with rounded connectors, as
in the archive view (see "The archive view" in `docs/product/sidebar.md`); the ten are counted as rows, so the preview
can end between an owner and the sessions under it but never shows an inset row without the row it is inset under.

Relative times ("5 minutes ago") are worded in the current language, and anything under a minute reads as now.

**Where either view has nothing to list**, a single **compact empty row** takes the list's place, under its
section heading, as in the full sidebar (see "The compact row" in `docs/product/empty-states.md` and "Empty rows" in
`docs/product/sidebar.md`). Where there is one action, clicking anywhere on the row takes it; a row with no action is
plain text. These lists do not carry a separate empty-state heading or button.

## A project's focus mode

The switch below the header chooses whether all of the project's sessions are listed together. It appears only
while the project has live sessions under a console session that is not archived, including a lead session and the
sessions under it; archived bound sessions alone do not make it appear. It is **off by default**: sessions under
console sessions are shown separately as summary chips. Turning it on lists all of the project's live sessions as
cards and hides the summary chips. The choice applies to every project's focus mode in this client and survives
reloading, including while the switch is hidden; it is not a shared application setting and is not exported with
configuration. If client storage is unavailable, the choice lasts for the current page load only.

From the top, below the switch:

- **Sessions (n)**: with the switch off, the project's sessions that are not archived and have **no console session
  above them** (see "Lead sessions" in `docs/product/hub-orchestration.md`) — the unbound ones, each with the sessions
  bound to it inset under it. A **lead session and the sessions under it** are left out of the list together, as is
  any session bound to a console session. With the switch on, every live session of the project is listed, with the
  same order and nesting. Each card under a console session, directly or through a lead session, carries that console
  session's colour dot and a tooltip naming it; its accessible name names that console session as well as its
  immediate project-session owner, where it has one. The count is every card listed, the inset ones included. With
  none, a compact row saying there are no sessions yet takes their place; clicking the row opens a **New session**.
- **The sessions elsewhere**, between the live cards and the archive, present only with the switch off and while
  the project has live sessions under a console session that is not archived: one sentence for the whole project
  saying how many of its sessions are under console sessions, naming none of them, and below it a row of chips,
  wrapping onto as many lines as it needs — one chip per console session they are under, in the order the console
  sessions section lists them. A chip shows the console session's colour, its title (faded out when too long, the
  full title then its tooltip) and an inline count badge with the same gray background and contrasting text
  across all chips. Both the total and each chip's count include a lead session **and** the sessions under it, since
  the lead session's team is listed with it in that console session's focus mode. Pressing a chip enters that console
  session's focus mode; assistive technology hears it as entering that focus mode, with the count.
- **Archived (n)** is the project's archived sessions with the bound ones included — those of a lead session that is
  still live among them. The archive is not filtered by binding, regardless of the switch; only the list of live
  sessions changes.

**New session** (the header's **+** and the empty row) opens the new-session dialog with no "Report to"
choice: the session is always unbound.

## A console session's focus mode

- **The switch strip**, right under the header, while the console has more than one console session that is not
  archived: see "The switch strip" below.
- **Sessions (n)**: the projects of the console that have a session bound to this console session which is not archived,
  in the order of the project list, and under each project's name the sessions bound to this console session as cards,
  each **lead session** among them carrying the sessions bound to it inset under it — they run in the lead session's own
  project, so they fall under the same project's name. The count includes them, so it covers sessions this console
  session sees but cannot drive (see "Lead sessions" in `docs/product/hub-orchestration.md`). A session bound to another
  console session, to a project session that is not under this one, or to none, is not shown, and neither is a project
  with nothing under this console session. With none, a compact row saying there are no bound sessions takes their
  place, carrying no action of its own — the header's **+** below is the way to open one. Each project's name carries its pin
  button while it is pinned (see "Pinning" in `docs/product/sidebar.md`), its own **+**, which opens a new session in
  that project, and its action menu, which is the project row's without its Focus mode item — a project has no focus
  mode to be entered from here — and keeps **Pin** / **Unpin**. The project list's filter is not part of this view.
- **New session**: the header's **+** opens a menu of the console's projects, so that a project with nothing bound yet
  can be reached; choosing one opens the new-session dialog for it. The dialog has no "Report to" field, and shows a
  line saying the session reports to this console session, which it will be bound to.
- **With no project in the console at all** there is nothing to open a session in: the header's **+** is disabled, and
  in place of the sessions a compact row says the console has no projects yet; clicking it opens **Add project**.
- **Archived (n)** is the archived sessions under this console session — bound to it, or bound to an archived lead
  session that is bound to it (see "Lead sessions" in `docs/product/hub-orchestration.md`), each of those inset under
  that lead session — and **View all** opens the archive view scoped to them (see "The archive view" in
  `docs/product/sidebar.md`). An archived session of a lead session that is still live is not here: it is in the lead
  session's project's archive alone.

## The switch strip

The switch strip moves between the console's console sessions without leaving focus mode. It is there only in a
console session's focus mode, and only while the console has at least two console sessions that are not archived.

- **One chip per console session of the console that is not archived**, the one in focus included. The order is
  fixed: pinned ones first, then by when they were started, oldest first. Unlike the console sessions section, it does
  not follow their statuses, so a chip stays where it is while what it shows changes; pinning or unpinning one moves
  its chip.
- **A chip** shows the console session's colour (see "The binding badge" in `docs/product/sidebar.md`), its title,
  faded out when too long with the full title then its tooltip, and what is going on in that console session and the
  sessions bound to it: a waving hand while any of them is waiting for the user, otherwise the *working* glyph while
  any of them is working, otherwise nothing — a session merely running is not given a chip's room, so only those two
  markers can be seen here. The marker carries a **tooltip** saying which of the two it is, in the same words the
  chip's own name folds in beside the title (see "Tooltips on indicators" in
  `docs/product/labels-and-tooltips.md`).
- **The console session in focus** has its chip outlined in the accent colour, and is announced as the current one.
- The chips are on **one row**. When they do not fit it scrolls sideways, with no scrollbar and its edges fading where
  more chips lie beyond; the current chip is scrolled to the middle of the row as the view opens and whenever the
  current chip changes or moves.
- For assistive technology the strip is a group named for switching console session, and each chip's name is the
  console session's title together with what is going on in it, in words.

**Pressing a chip** enters that console session's focus mode and selects the console session, showing its terminal and
its report panel; this is a single visit in the window's history (see `docs/product/navigation-history.md`). As
selecting its row does, it resumes the console session when it is interrupted (see "Selecting a session" in
`docs/product/sidebar.md`). Pressing it with the mouse does not move keyboard focus into the sidebar.

**`Ctrl+Tab` moves to the next chip and `Ctrl+Shift+Tab` to the previous one**, going round from either end, starting
from the console session in focus. It exists only in the macOS application; a browser keeps the keys for its own
tabs.

- It works only while the sidebar is shown — docked, or as an open drawer below 1148 px; not while it is hidden or
  only floating in (see "Hiding the sidebar and the right pane" in `docs/product/window-layout.md`) — in a console
  session's focus mode, with the strip there. Anywhere else the keys go on to the terminal as usual.
- Where it works, the keys never reach the terminal, so the agent never receives them. It does nothing while a dialog
  or a menu is open, and is left alone during an input-method composition. A key held down moves one chip only.
- It enters the next console session's focus mode and selects it, like pressing its chip, except that **it does not
  resume an interrupted console session**: it only shows it, with its "Not running" card (see "The terminal" in
  `docs/product/sessions.md`), so stepping through the chips never starts a process.
- Wherever keyboard focus was, it goes to the terminal of the console session moved to, an interrupted one's
  included.
- Pressed while focus is inside a report page it is passed to the window (see "Window shortcuts inside a page" in
  `docs/product/report-panel.md`).

## Leaving focus mode, and what is remembered

**Focus mode is remembered per client** together with the console shown. It is left by the back button, by switching
console, by selecting a session that does not belong to what is in focus, when the selected session stops belonging to
it without being selected again, and when what is in focus no longer exists — a project removed, a console session
deleted or, as it has no row any more, archived. An archived console session is forgotten rather than remembered:
reopening it does not put the sidebar back into its focus mode. A remembered focus mode on something that is not in the
console shown is ignored.

**What belongs to what.** Every session of a project belongs to **that project's focus mode**, regardless of its
status, binding or whether its card is shown. The combined-view switch changes only what the list shows: turning
it off keeps project focus mode and the selected session's terminal, even when that session is represented only by
the summary chips. Selecting another session of the same project, reopening a bound session, or its owner becoming
a lead session also keeps project focus mode. Back and Forward use this same project membership rule, independently
of the switch. A session belongs to **a console session's focus mode**
when it is that console session itself, when it is live and under it — bound to it, or bound to a lead session bound to
it (see "Lead sessions" in `docs/product/hub-orchestration.md`) — or when it is archived and that console session's
archive reaches it: bound to it, or bound to a lead session that is archived too. For archived sessions the rule is the
narrower one, so an archived session of a lead session that is **still live** belongs to its project's focus mode only,
and selecting it from the console session's focus mode leaves that focus mode.

## The focus mode shortcut

**A keyboard shortcut toggles focus mode**: `Shift+Cmd+F` on a Mac (the macOS application, or a browser on a Mac),
`Ctrl+Shift+F` elsewhere, matched on the physical F key whatever the keyboard layout. The Focus mode items of the
project and console session menus show it.

- Outside focus mode it enters focus mode for the selected session's context, switching the sidebar to that session's
  console first when needed: a project session's project, or, with a console session selected, that console session
  itself. With nothing selected, or an archived console session, it does nothing.
- In either focus mode it leaves focus mode.
- It does nothing while a dialog or a menu is open, and during an input-method composition.
- The key combination never reaches the terminal, so the agent never receives it.

## Approving a request for a console session

A window that approves a project session's request for a console session (see
`docs/product/requesting-a-console-session.md`) while it is in the focus mode of the requesting session's own project
moves, once the console session has started, into that console session's focus mode with it selected. A window in
another project's focus mode, in a console session's focus mode or not in focus mode stays as it is, and so does every
other connected window.

## Keyboard focus on entering and leaving

**Entering or leaving focus mode by selecting a session hands keyboard focus to that session's terminal**, as
selecting any session does: pressing a switch strip chip, `Ctrl+Tab` or `Ctrl+Shift+Tab` (see "The switch strip"
above), or selecting a session that does not belong to what is in focus. A console session that a chip press resumes
gets it once it is running again.

Where nothing takes keyboard focus that way — focus mode entered or left without a selection (a Focus mode menu item,
a chip in a project's focus mode, the back button, the focus mode shortcut), or a selection whose terminal does not
take it, such as an archived session or one still being resumed — and keyboard focus was in the sidebar, it moves to
somewhere useful in the new view: after going from one console session's focus mode to another's, to the switch
strip's current chip; otherwise, on entering, to the back button, and on leaving, to the row of the project or console
session that was in focus. When it was not in the sidebar — the mouse used, or the shortcut pressed from the terminal —
it stays where it was, normally on the terminal.
