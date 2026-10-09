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
projects and sessions" in `docs/product/sidebar.md`.

**Archived (n)**, in both, only while the view has archived sessions — with none, the section is not shown at all: the
ten most recently archived sessions of the view, each showing its agent's icon, its title and how long ago it was
archived. Clicking one selects it. Its action menu offers **Rename** and **Delete** (which asks for confirmation; see
"Deleting archived sessions" in `docs/product/sessions.md`). Below them **View all (n)** opens the archive view.

Relative times ("5 minutes ago") are worded in the current language, and anything under a minute reads as now.

## A project's focus mode

From the top:

- **Sessions (n)**: the project's sessions that are not archived and **not bound to a console session** — unbound
  ones, and ones bound to a project session, which are listed like any other. A session bound to a console session is
  not listed here, and so no card carries a binding badge. With none, a message and a **New session** button take
  their place.
- **Archived (n)** is the project's archived sessions with the bound ones included. The archive is not filtered by
  binding; only the list of live sessions is.
- **The sessions bound elsewhere**, last, below the archive, present only while the project has sessions that are not
  archived bound to a console session that is not archived: one sentence for the whole project saying how many of its
  sessions are bound to console sessions, naming none of them, and below it a row of chips, wrapping onto as many
  lines as it needs — one chip per console session they are bound to, in the order the console sessions section lists
  them. A chip shows the console session's colour, its title (faded out when too long, the full title then its
  tooltip) and how many of this project's sessions that are not archived are bound to it. Pressing a chip enters that
  console session's focus mode; assistive technology hears it as entering that focus mode, with the count.

**New session** (the header's **+** and the empty message's button) opens the new-session dialog with no "Report to"
choice: the session is always unbound.

## A console session's focus mode

- **The switch strip**, right under the header, while the console has more than one console session that is not
  archived: see "The switch strip" below.
- **Sessions (n)**: the projects of the console that have a session bound to this console session which is not
  archived, in the order of the project list, and under each project's name only the sessions bound to this console
  session, as cards. A session bound to another console session, to a project session or to none, is not shown, and
  neither is a project with nothing bound to this one. With none, a message says so. Each project's name carries its
  pin button while it is pinned (see "Pinning" in `docs/product/sidebar.md`), its own **+**, which opens a new session
  in that project, and its action menu, which is the project row's without its Focus mode item — a project has no
  focus mode to be entered from here — and keeps **Pin** / **Unpin**. The project list's filter is not part of this
  view.
- **New session**: the header's **+** opens a menu of the console's projects, so that a project with nothing bound yet
  can be reached; choosing one opens the new-session dialog for it. The dialog has no "Report to" field, and shows a
  line saying the session reports to this console session, which it will be bound to.
- **With no project in the console at all** there is nothing to open a session in: the header's **+** is disabled, and
  in place of the sessions a message says the console has no projects yet, with an **Add project** button.
- **Archived (n)** is this console session's archived bound sessions, and **View all** opens the archive view
  scoped to them (see "The archive view" in `docs/product/sidebar.md`).

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
  any of them is working, otherwise nothing.
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
it (reopening an archived session of a project bound to a console session makes it live and bound, so the project no
longer lists it), and when what is in focus no longer exists — a project removed, a console session deleted or, as it
has no row any more, archived. An archived console session is forgotten rather than remembered: reopening it does not
put the sidebar back into its focus mode. A session belongs to a project's focus mode when it is one of the project's
sessions not bound to a console session, or is archived; to a console session's when it is that console session or is
bound to it. A remembered focus mode on something that is not in the console shown is ignored.

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
