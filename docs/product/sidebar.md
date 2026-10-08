# Sidebar

The sidebar is the window's left-hand pane (its width, hiding it and the drawer it becomes in a narrow window are in
`docs/product/window-layout.md`). It shows **one console at a time**: a switcher naming that console at its top, then
its console sessions, then its projects with their sessions. A **focus mode** gives the whole sidebar over to one
project or one console session, and a project's, a console session's or a console's full archive opens in the
**archive view**, over the terminal.
What consoles, projects and sessions are is in `docs/product/consoles-and-projects.md` and
`docs/product/sessions.md`.

Archived sessions are never listed among a project's or a console's sessions. A project's are reached through its
"View archive" submenu, through its focus mode, and through the archive view; the archived sessions bound to a console
session through its focus mode and the archive view; a console's archived console sessions through the console
sessions section's own menu and the archive view.

## The console switcher

The top of the sidebar names the console being shown. Pressing it opens a list of every console, in the order they
were created, with the shown one checked, followed by **New console**, which opens the same dialog as the top bar's
New console button (see "The top bar" in `docs/product/window-layout.md`). Choosing a console shows it instead, its
view fading in (see "Focus mode" below for how the sidebar's view changes are animated). A
console created from this window, by either button, is shown as soon as it appears; one created from another client is
only added to the list.

Each console is shown by its avatar (see "Avatar" in `docs/product/consoles-and-projects.md`), here and in the list.

Every console in the list carries a marker of what is going on in it, taken from all its sessions, its console
sessions included:

| Marker | Shown when |
|---|---|
| A waving hand | at least one session is waiting for the user |
| The *working* glyph | otherwise, at least one session is working |
| A green speech bubble | otherwise, at least one session is running and awaiting instructions |
| none | no session has a running process |

The switcher itself carries the waving hand while a session in **another** console is waiting for the user, so a
raised hand elsewhere is not hidden behind the console being shown.

Beside the switcher is the shown console's action menu: **Add project**, **Edit console** and **Delete console** (see
"Associating a project", "Editing a console" and "Deleting a console" in `docs/product/consoles-and-projects.md`;
deleting asks for a typed confirmation first).

**Which console is shown is remembered per client**, in that client's own browser storage, as the panes' widths are
(see "Resizing the sidebar and the report panel" in `docs/product/window-layout.md`). A remembered console that no
longer exists falls back to the first console. Selecting a session anywhere switches the sidebar to that session's
console (see "Selecting a session" below).

With no console at all, the sidebar shows a message saying so and a **New console** button.

## The console sessions section and the project list

Under the switcher comes the **console sessions** section: every console session of the console that is not archived,
each as a session row (see "Session rows" below, which a console session's row otherwise follows exactly — pin, rename,
focus mode, switch account, archive), in the order of "Order of projects and sessions" below. What a console session is,
what selecting one does, and what its row's menu offers are in "Console sessions and project sessions" in
`docs/product/sessions.md`.

The section's own heading carries a **+** button, **New console session**, which opens one with the console's current
console session agent and its account for that agent, and, while the console has an archived console session, the
section's action menu, whose only entry is **Archived console sessions** — a submenu of the console's newest archived
console sessions, each of which can be selected, and the way to the archive view of all of them (see "The archive
view" below). This is the one entry a console session's own row never carries: archived console sessions are reached
from the section, not from a row. With no console session at all, a line says so.

With no agent available on the machine at all, the section instead carries a line saying which agents Octoboard
supports and that one of them has to be on the user's `PATH`, without saying how to install one, and the **+** button
is disabled, since starting one would only be refused. The line wraps onto as many lines as the sidebar's width needs
rather than being cut, since it is an instruction to act on. It shows whatever the section lists — a console session
left interrupted from before an agent went off the `PATH` still has its row — and it takes the place of the "no
console session" line.

Below it, a **Projects** heading and the console's projects (see "Order of projects
and sessions" below). With no project, a message saying the console has none yet and an **Add project** button take
their place.

### Filtering the project list

While the console has projects, a filter button, **Filter projects**, sits among the controls at the end of the
Projects heading (see "Expanding and collapsing the listed projects" below for the order they run in). It opens
a small popover with two controls. A search field filters the project list as it is typed: a project stays listed when
its name contains the keyword, ignoring case and the keyword's surrounding spaces. Below it, the tags in use (see "Tags"
in `docs/product/consoles-and-projects.md`) are shown as toggles, and picking one narrows the list to the projects
carrying it; with several picked, a project must carry all of them, and it must match the keyword too. With no project
carrying a tag yet, a line in place of the toggles says tags are added in a project's settings. `Escape`, from
anywhere in the popover, and Enter in the search field close it and keep the filter; on a tag, Enter or Space picks or
drops that tag and the popover stays open. A picked tag also shows a check mark, so it is not told apart by colour
alone. Opening and closing it with the mouse leaves keyboard focus where it was.

- While a keyword is in force it shows as a tag right after the "Projects" label, with a small remove button that drops
  just the keyword and leaves the picked tags. The picked tags are laid out one after another on a row of their own
  under the heading, each with a small remove button that drops just that tag from the filter; clicking a tag, the
  keyword's remove button or a tag's remove button with the mouse leaves keyboard focus where it was, and removing the
  keyword, or the last tag, with the keyboard moves focus to the filter button. A **Clear filter** button appears just
  before the filter button and clears the keyword and the picked tags together; the search field's own clear button
  clears only the keyword.
- The tags offered are the distinct tags of the console's projects, in alphabetical order, so a tag that no project
  carries any more disappears from the choices, and from the filter if it was picked, without emptying the list. The
  pick is only hidden, not forgotten: if a project carries that tag again, it filters again.
- The filtered list keeps the order and the dimming of "Order of projects and sessions" below.
- With no project matching, a single line says so; the heading's Clear filter button clears the filter.
- Each console has its own filter: switching to another console shows that console's own, and switching back, or
  entering and leaving focus mode, keeps it. Filters are not stored, so reloading the window shows every list
  unfiltered.

### Expanding and collapsing the listed projects

One icon-only button follows the filter button. Its name is whichever action it will take, **Collapse all projects**
or **Expand all projects**. The controls at the end of the Projects heading run: Clear filter (only while a filter is
in force), Filter projects, then this button.

- It shows while the console has projects **and** the list shows at least one of them. With a filter matching no
  project it is not there — there is nothing to act on — while the filter button beside it stays, as the way back.
- It offers one action at a time. Pressing it collapses or expands every project the list is showing, and only those,
  so with a filter in force the projects it hides keep whatever expanded or collapsed state they had. What it changes
  is the same state a project row changes (see "Project rows" below). The press flips the button to the other action
  even when a filter kept it from reaching every project of the console, and even when every listed project is already
  in the state it asks for.
- Projects start expanded, and whether a project is collapsed is kept per window and is not stored (see "Project
  rows" below). The button's choice is kept the same way, separately for each console: switching to another console
  shows that console's own, and switching back, or entering and leaving focus mode, keeps it. It is not stored, so
  reloading the window starts each console's button over. On first show, if any project of the console is expanded
  the button offers Collapse all projects; if every project of the console is collapsed it offers Expand all
  projects. The usual start is Collapse all projects.
- Expanding or collapsing a project from its own row does not change the button while the console still has both
  expanded and collapsed projects. The button changes on its own only when every project of the console is expanded,
  when it offers Collapse all projects, or every project of the console is collapsed, when it offers Expand all
  projects. Projects a filter is hiding count, and changing the filter does not itself flip the button. The same
  holds once the button is offering Collapse all projects again: collapsing some projects from their rows leaves it,
  and collapsing the last expanded project of the console makes it offer Expand all projects.
- A mouse press leaves keyboard focus where it was, as the sidebar's other controls do. The one exception: Collapse
  all projects takes away the session rows under the projects it collapses, so when keyboard focus is standing in one
  of those it is handed to this button, its focus ring showing, rather than dropped. Expand all projects does not
  take rows away, so it does not move focus. The project rows themselves stay, so focus already on a project row is
  left where it is.

## Project rows

A project row shows the project's name, a pin glyph when the project is pinned, and a chevron after the name. Clicking
the row, or Enter or Space on it, collapses or expands it. Projects start expanded; whether one is collapsed is kept
per window and is not stored. The chevron always shows while the project is collapsed, and only while the row is
hovered or focused while it is expanded, and while hidden it takes no room, so a long name runs as far as it would in
a session row and ends sooner when the chevron appears.

After the name, before the activity marker below and the row's controls, comes the project's **branch badge** — its
current branch and how far it is from its upstream, when its directory is a git repository (see "The branch badge" in
`docs/product/project-git-status.md`).

A collapsed project shows the same activity marker as the console switcher's list (see "The console switcher" above),
taken from its sessions that are not archived, so a waiting session can be found with its project collapsed.

At the row's end are a **+** button, **New session**, which opens the new-session dialog for that project (see
"Opening a session" in `docs/product/sessions.md`), and the project's action menu:

- **Pin** / **Unpin** — see "Order of projects and sessions" below.
- **Rename** — a dialog with the name alone; an empty name is rejected.
- **Project settings** — the project dialog, see "Editing a project" in `docs/product/consoles-and-projects.md`.
- **Focus mode** — see "Focus mode" below.
- **View archive** — a submenu of the project's newest five archived sessions (most recently archived first), each
  with its agent's icon and its title; choosing one selects it (see "Selecting a session" below). After them,
  **View all (n)**, with the number of archived sessions, opens the archive view. A project with no archived session
  shows only a disabled item saying so.
- **Remove project** — see "Removing a project" in `docs/product/consoles-and-projects.md`; it asks for confirmation
  first.

An expanded project lists its sessions that are not archived as session rows. With none, it shows a single line
saying there are no sessions yet, with a **+** button that opens a new session.

## Session rows

A session row shows the session's status glyph (see "Session statuses" in `docs/product/sessions.md`), its agent's
icon, its title, and a pin glyph when it is pinned. A project session bound to a console session also carries that
console session's **binding badge**: a small dot in the console session's colour, whose tooltip names it (see "The
binding badge" below). A console session's own row carries no *binding* badge — what it shows there instead is its own
colour (see "The binding badge" below). Selecting a row
resumes it when it is interrupted; no menu item does that. The row shows its agent as an icon alone; wherever the agent
is named in words, the account the session runs under follows it in parentheses — "Claude Code (Work)" — as the
account's name, "Default" for the agent's default account, or, for a session whose account has since been removed, the
directory it recorded (or the agent's name alone when it recorded none). Its action menu offers:

- **Pin** / **Unpin**.
- **Rename** — see "Renaming a session" in `docs/product/sessions.md`.
- **Focus mode** — a console session's row only: enters that console session's focus mode (see "Focus mode" below).
  A project session's row has no such entry, and neither has an archived console session's.
- **Switch account** — a submenu of the accounts of the session's agent, the default account first, with the one the
  session is on checked and doing nothing; picking another asks for confirmation, then moves the session to it (see
  "Switching a session's account" in `docs/product/sessions.md`). Its last entry, **Manage accounts…**, opens Settings
  at the Agent accounts section. A session whose account has since been removed still shows it, checked and named by
  the directory the session recorded. The entry is present for a console session's row as well as a project session's,
  and only when the session's agent has more than one account to be on; with just the default account it is left out
  rather than shown disabled. A session in the archive has no such entry.
- **Archive** — it asks for confirmation, then archives the session (see "Archiving, interruption and resuming" in
  `docs/product/sessions.md`). For a console session this is the only way to archive it, since it cannot archive
  itself (see "The console session's tools" in `docs/product/hub-orchestration.md`); its confirmation lists the
  interrupted sessions bound to it, under a warning with their number, as the ones archived with it, and the archive is
  refused, with the sessions named, while any session bound to it has a process running.

## The binding badge

A **bound** project session — one carrying the id of the console session it reports to (see "Console sessions and
project sessions" in `docs/product/sessions.md`) — carries a small dot in that console session's colour wherever the
project session is listed: its sidebar row. An unbound session carries no badge. The badge's tooltip names the
owning console session, and the row's own accessible name carries the same fact in words for assistive technology
("Bound to …"). The badge itself still carries no information its tooltip does not, so colour alone never
distinguishes two owners for a user who cannot tell the colours apart. A console session's own row shows the same
colour, decoratively, since the row's own label already names it, as does its focus mode's header and the line in a
project's focus mode that names it. The cards in a focus mode carry no badge: a project's focus mode lists only
unbound sessions, and a console session's lists only the ones bound to it.

**Where the colour comes from.** Octoboard gives each new console session a colour of its own from a fixed palette of
six, taking the first the console's other console sessions that are not archived do not already have, and starting
over from the first once all six are taken. A console session keeps its colour for its lifetime, archiving included,
so with more than six at once — or with one reopened into a palette that filled up meanwhile — two console sessions
of one console can share a colour, which is the other reason nothing is ever told apart by colour alone. Each colour
has its own light and dark value (see "What follows the choice" in `docs/product/appearance.md`).

## Rows, names and keyboard focus

- A name too long for its row fades out where it ends — the row's right edge, its left under a right-to-left
  language — rather than ending in an ellipsis, the full name is then the row's tooltip, and pointing anywhere on the
  row runs the name as a marquee (see "Names too long for their space" in `docs/product/window-layout.md`). When the
  sidebar's content is taller than the sidebar it scrolls, and fades out at whichever end has more of it beyond.
- A row's **+** and action-menu buttons show while the row is hovered, holds keyboard focus, is the selected session,
  or has its menu open; they are still reached with Tab when hidden, and show once one of them has focus.
- Clicking a row deliberately does not move keyboard focus away from the terminal; a row reached with Tab can be
  activated with Enter or Space.
- Each row tells assistive technology what its icons show: a session row its title, agent (Claude Code, Codex or Grok
  Build) and the account it runs under, status, whether it is pinned, and, for a bound project session, the console
  session it reports to (the binding badge's own fact); a project row whether it is pinned and, while it carries an
  activity marker, what that marker means, plus its branch badge's facts (see "The branch badge" in
  `docs/product/project-git-status.md`). A project row also tells it whether it is expanded or collapsed.

## Order of projects and sessions

**Projects** are listed:

1. pinned projects first;
2. then a project with a session waiting for the user;
3. then a project with a session working;
4. then a project with a session running and awaiting instructions;
5. then the **inactive** projects — those with no session whose process is running;
6. within each of those, by name, case-insensitively and in natural order, so that "item 2" comes before "item 10".

Each rule applies only among projects the rules before it leave tied, so pinned projects come first whatever their
sessions are doing, and the unpinned inactive projects end the list.

Inactive projects, pinned ones included, are drawn dimmed, as one group. Pointing at any of them, keyboard focus inside
one, or an open menu in one brings the whole group back to full strength while it lasts.

**Sessions** within a project are listed, by the same scheme:

1. pinned sessions first;
2. then by status: waiting for the user, working, awaiting instructions, interrupted;
3. then the most recently started first.

The console sessions section lists its console sessions by that same session scheme.

The order follows status changes as they happen. A row that moves slides to its new place over about a quarter of a
second, and moves at once where the system asks for reduced motion. The top bar's waiting count walks waiting sessions
in this same order (see "The raised hand" in `docs/product/sessions.md`).

**Pinning** is the user's own mark on a project or a session, set and cleared from its action menu; nothing is pinned
to begin with. Its only effect is the order above. The pin is stored by the daemon with the project or the session,
so every window and every client sees the same pins, and they survive a restart. A pinned session stays pinned when it
is archived and when it is resumed; an archived session's menu offers no Pin or Unpin.

## Selecting a session

Selecting a session shows its terminal; selecting an interrupted one resumes it. Selecting an archived one does not
reopen it: it is shown still archived, and typing into its terminal or pressing Reopen reopens it (see "Archiving,
interruption and resuming" and "The terminal" in `docs/product/sessions.md`). Whatever it is selected from — a sidebar
row, a "View archive" or "Archived console sessions" submenu, focus mode, the archive view, or the top bar's waiting
count — **the sidebar follows it**: it switches to the session's console, and leaves focus mode when the session does
not belong to what focus mode is on (see "Focus mode" below).

A session is selected automatically only when this application is the one that opened it — through the console
sessions section's New console session button or the new-session dialog — and that closes the archive view if it is
open. A session that appears any other way, such as one the console session started or one another client of the
daemon opened, is listed unselected and is put on screen by the user selecting it.

When the selected session stops existing — deleted from the archive, by this client or another, for instance — nothing
is selected in its place, and the terminal's area shows its empty state.

## Focus mode

Focus mode replaces the whole sidebar — the console switcher and the console sessions and project lists — with one
project or one console session. Choosing **Focus mode** from a project's action menu enters the first, from a console
session's action menu the second. The two views are built alike, and differ in what they list.

**The header**, in both: a back button, **Leave focus mode**, which returns to the full sidebar; the console's name
above the name of what is in focus; a **+** button for a new session; and that thing's action menu, without its Focus
mode item. A project's header also carries its branch badge (see "The branch badge" in
`docs/product/project-git-status.md`). A console session's header shows its colour beside its name, and pressing the
name selects the console session, whose terminal and report panel the view has no other row for; its action menu is the
console session row's own (pin, rename, switch account, archive).

**Session cards**, in both: the status glyph with the status in words, a pin glyph when pinned, the session's action
menu (always shown), its title over up to two lines, its agent's icon and name with its account, and how long ago it
was started. Clicking a card selects the session. They are in the order of "Order of projects and sessions" above.

**Archived (n)**, in both: the ten most recently archived sessions of the view, each showing its agent's icon, its
title and how long ago it was archived. Clicking one selects it. Its action menu offers **Rename** and **Delete**
(which asks for confirmation; see "Deleting archived sessions" in `docs/product/sessions.md`). Below them **View all
(n)** opens the archive view. With none archived, a line saying so.

### A project's focus mode

- **Sessions (n)**: the project's **unbound** sessions that are not archived. A session bound to a console session is
  not listed here, and so no card carries a binding badge. With none, a message and a **New session** button take
  their place.
- **One line** under that heading, present only while the project has sessions bound to a console session that are
  not archived: how many sessions in this project are bound to which console sessions — "2 sessions in this project
  are bound to Hub 1 and Hub 2" — with each console session named as a control, after its colour, that enters that
  console session's focus mode. It is one line for the whole project, not one per console session.
- **New session** (the **+** and the empty message's button) opens the new-session dialog with no "Report to" choice:
  the session is always unbound.
- **Archived (n)** is the project's archived sessions with the bound ones included. The archive is not filtered by
  binding; only the list of live sessions is.

### A console session's focus mode

- **Sessions (n)**: the projects of the console that have a session bound to this console session which is not
  archived, in the order of the project list, and under each project's name only the sessions bound to this console
  session, as cards. A session bound to another console session, or to none, is not shown, and neither is a project
  with nothing bound to this one. With none, a message says so. Each project's name carries a pin glyph while it is
  pinned, its own **+**, which opens a new session in that project, and its action menu, which is the project list's
  without its Focus mode item: a project has no focus mode to be entered from here. The project list's filter is not
  part of this view.
- **New session**: the header's **+** opens a menu of the console's projects, so that a project with nothing bound yet
  can be reached; choosing one opens the new-session dialog for it. The dialog has no "Report to" field, and shows a
  line saying the session reports to this console session, which it will be bound to.
- **With no project in the console at all** there is nothing to open a session in: the header's **+** is disabled, and
  in place of the sessions a message says the console has no projects yet, with an **Add project** button.
- **Archived (n)** is this console session's archived bound sessions, and **View all** opens the archive view
  scoped to them (see "The archive view" below).

**The sidebar's view changes are animated**, the incoming view only, over about a fifth of a second: entering a focus
mode slides it in from the sidebar's end edge as it fades in, a level down; returning to the console's view slides
that in from the start edge, a level up; and switching to another console fades its view in with a slight upward
drift, as does going from one focus mode straight to another (following a summary line). The horizontal slides are
mirrored under a right-to-left language, and nothing moves where the system asks for reduced motion.

**Focus mode is remembered per client** together with the console shown. It is left by the back button, by switching
console, by selecting a session that does not belong to what is in focus, when the selected session stops belonging
to it (reopening an archived session of a project makes it live and bound, so the project no longer lists it), and
when what is in focus no longer exists — a project removed, a console session deleted or, as it has no row any more,
archived. An archived console session is forgotten rather than remembered: reopening it does not put the sidebar back
into its focus mode. A session belongs to a
project's focus mode when it is one of the project's unbound sessions or is archived; to a console session's when it
is that console session or is bound to it. A remembered focus mode on something that is not in the console shown is
ignored.

**A keyboard shortcut toggles focus mode**: `Shift+Cmd+F` on a Mac (the macOS application, or a browser on a Mac),
`Ctrl+Shift+F` elsewhere, matched on the physical F key whatever the keyboard layout. The Focus mode items of the
project and console session menus show it.

- Outside focus mode it enters focus mode for the selected session's context, switching the sidebar to that session's
  console first when needed: a project session's project, or, with a console session selected, that console session
  itself. With nothing selected, or an archived console session, it does nothing.
- In either focus mode it leaves focus mode.
- It does nothing while a dialog or a menu is open, and during an input-method composition.
- The key combination never reaches the terminal, so the agent never receives it.

When keyboard focus was in the sidebar, entering or leaving focus mode moves it to somewhere useful in the new view:
on entering, to the back button; on leaving, to the project's or console session's row. Otherwise — the mouse used, or
the shortcut pressed from the terminal — keyboard focus stays where it was, normally on the terminal.

Relative times ("5 minutes ago") are worded in the current language, and anything under a minute reads as now.

## The archive view

**View all (n)** — in a project's "View archive" submenu, in a focus mode's archived list, or in the console sessions
section's "Archived console sessions" submenu — opens the archive view: **every** archived session of that project,
every archived session bound to that console session, or every archived console session of that console.

- It **covers the terminal's area only**: the sidebar and, for a console session, the report panel stay as they are.
  The terminal goes on running beneath it and is not resized.
- Its header says how many archived sessions there are, over the title "Archived sessions" (for console sessions,
  "Archived console sessions"; for a console session's bound sessions, "Archived bound sessions"), with **Delete all**
  while there is at least one, and a **Close** button. Which project, console session or console it is for is shown in
  the top bar's breadcrumb (see "The top bar" in `docs/product/window-layout.md`).
- The list is in the order the sessions were archived, most recent first. It shows 30 rows and adds 30 more each time
  its end is scrolled into view. Each row shows the session's agent's icon, its title, and its agent's name with its
  account and how long ago it was archived. Its **Reopen** and **Delete** buttons show while the row is hovered or holds
  keyboard focus, and are still reached with Tab.
- **Reopen** resumes the session and selects it, which closes the view (see "Selecting a session" above).
- **Delete** asks for confirmation, then deletes that session — for an archived console session, listing the archived
  sessions bound to it that are deleted with it; **Delete all** asks for confirmation, naming how many, then deletes
  every archived session in the view's scope — for a console session's archive, its archived bound sessions and not
  the console session itself; for a console's archived console sessions it also says how many archived sessions
  bound to them are deleted with them. What deleting does and does not remove is in "Deleting archived sessions" in
  `docs/product/sessions.md`.
- With nothing archived — including after everything has been deleted — the view stays open and says there are no
  archived sessions.

**It closes** on its Close button, on `Escape` while keyboard focus is in it, when a session is selected (from it or
from anywhere else, as in "Selecting a session" above), when this window opens a new session, and by itself when its
project, its console session or its console no longer exists. Closing it hands keyboard focus to the terminal.

It takes keyboard focus as it opens, so keystrokes stop reaching the terminal it covers, and it keeps focus after a
deletion. It is a region of its own in the `F6` cycle (see "Moving focus between regions with F6" in
`docs/product/window-layout.md`). Below 1100 px, where the sidebar is a drawer, opening the archive view closes the
sidebar's drawer so that the view is not hidden behind it.
