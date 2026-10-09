# Sidebar

The sidebar is the left-hand pane of the window's content panel (its width, hiding it and the drawer it becomes in a
narrow window are in `docs/product/window-layout.md`). It shows **one console at a time**, the one picked on the rail:
a header naming that console at its top, then its console sessions, then its projects with their sessions. A **focus
mode** gives the whole sidebar over to one project or one console session (see `docs/product/focus-mode.md`), and a
project's, a console session's or a console's full archive opens in the **archive view**, over the terminal.
What consoles, projects and sessions are is in `docs/product/consoles-and-projects.md` and
`docs/product/sessions.md`.

Archived sessions are never listed among a project's or a console's sessions. A project's are reached through its
"View archive" submenu, through its focus mode, and through the archive view; the archived sessions bound to a console
session through its focus mode and the archive view; a console's archived console sessions through the console
sessions section's own menu and the archive view.

## The console switcher

The consoles are switched from the top of the rail, at the window's left edge (see "The rail" in
`docs/product/window-layout.md`): one avatar per console (see "Avatar" in `docs/product/consoles-and-projects.md`), in
the order the consoles were created, followed by **New console**. The console the sidebar shows is the **current
console**; its avatar has a tile behind it and is announced as the current one. (A sidebar floating in to preview
another console is the one exception, see "Previewing a console from the rail" below.) Pressing another avatar makes
that console the current one, its view fading in (see "How the sidebar's view changes" below); pressing it with the
mouse leaves keyboard focus where it was. Each avatar's tooltip is its console's name (when it opens is in "Tooltips
on icon-only controls" in `docs/product/labels-and-tooltips.md`). A console created from this window — by the rail's
New console or the sidebar's own button when there is no console — becomes the current one as soon as it appears; one
created from another client is only added to the rail.

An avatar carries a badge at its bottom corner while something is going on in its console, taken from all its
sessions, its console sessions included, so a raised hand in a console that is not shown is in sight too:

| Badge | Shown when |
|---|---|
| A waving hand | at least one session is waiting for the user |
| The *working* glyph | otherwise, at least one session is working |
| A green speech bubble | otherwise, at least one session is running and awaiting instructions |
| none | no session has a running process |

The name assistive technology announces for an avatar is the console's name together with what its badge means.

**Right-clicking** (or Control-clicking) an avatar opens that console's action menu at the pointer: **Add project**,
**Edit console** and **Delete console**, as in the sidebar's header below. It does not switch to the console.

**The current console is remembered per client**, in that client's own browser storage, as the panes' widths are
(see "Resizing the sidebar and the right pane" in `docs/product/window-layout.md`). A remembered console that no
longer exists falls back to the first console. Switching console leaves focus mode; the selected session stays
selected. Selecting a session anywhere switches the sidebar to that session's console (see "Selecting a session"
below).

### Previewing a console from the rail

While the docked sidebar is hidden, at 1148 px and wider, resting the mouse on a console's avatar floats the sidebar
in showing **that** console — when it comes up, how it looks and when it goes away are in "A hidden pane floats in on
hover" in `docs/product/window-layout.md`. For any console other than the current one this is a **preview**:

- It shows the console's own view — the header, its console sessions and its projects — never a focus mode, which
  belongs to the current console only. Resting on the current console's avatar shows the sidebar as it is, focus mode
  included. Moving from one avatar to another fades the new console's view in, as switching console does.
- **Hovering changes nothing**: the current console, its tile on the rail, the remembered console and the window's
  history all stay as they were.
- **A press anywhere inside the floating sidebar**, with any mouse button, makes the previewed console the current
  one, and the press then goes on to do what it does there — select a row, open a menu, start a dialog. Pressing the
  avatar itself makes it current too, as always. In the window's history a press that makes the console current and
  then selects a session counts as one visit (see "What counts as a place" in
  `docs/product/navigation-history.md`).
- Showing the sidebar for good with its toggle while it previews a console shows the current console, not the one the
  card was previewing. The card goes away and the column eases open; see "Hiding the sidebar and the right pane"
  and "A hidden pane floats in on hover" in `docs/product/window-layout.md`.

## The console header

The top of the sidebar names the console being shown, with its action menu beside it: **Add project**, **Edit
console** and **Delete console** (see "Associating a project", "Editing a console" and "Deleting a console" in
`docs/product/consoles-and-projects.md`; deleting asks for a typed confirmation first). The console's avatar is not
repeated here; the rail marks it.

With no console at all, the sidebar shows a message saying so and a **New console** button.

## The console sessions section and the project list

Under the header comes the **console sessions** section: every console session of the console that is not archived,
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
  just the keyword and leaves the picked tags. The picked tags follow it on the heading, in the same light accent tint
  as the tags in a project's Tags field (see "Editing a project" in `docs/product/consoles-and-projects.md`), so they
  are told apart from the keyword, and wrap onto further lines when they do not fit, while the label and the heading's
  controls stay on the first line. Each picked tag has a small remove button that drops just that tag from the filter;
  clicking a tag, the keyword's remove button or a tag's remove button with the mouse leaves keyboard focus where it
  was, and removing the keyword, or the last tag, with the keyboard moves focus to the filter button. A **Clear filter**
  button appears just before the filter button and clears the keyword and the picked tags together; the search field's
  own clear button clears only the keyword.
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

A project row shows the project's name, its pin button when the project is pinned (see "Pinning" below), and a chevron
after the name. Clicking the row, or Enter or Space on it, collapses or expands it. Projects start expanded; whether one
is collapsed is kept per window and is not stored. The chevron always shows while the project is collapsed, and only
while the row is hovered or focused while it is expanded, and while hidden it takes no room, so a long name runs as far
as it would in a session row and ends sooner when the chevron appears. The hover chevron fades and opens out as it
appears, and back as it goes, briefly — at once where the system asks for reduced motion.

After the name, before the activity marker below and the row's controls, comes the project's **branch badge** — its
current branch and how far it is from its upstream, when its directory is a git repository (see "The branch badge" in
`docs/product/project-git-status.md`).

A collapsed project shows an activity marker with the same meanings as a console's badge on the rail (see "The
console switcher" above), taken from its sessions that are not archived, so a waiting session can be found with its
project collapsed.

At the row's end are a **+** button, **New session**, which opens the new-session dialog for that project (see
"Opening a session" in `docs/product/sessions.md`), and the project's action menu:

- **Browse files** — shows the project's files in the right pane, selecting, starting and resuming no session (see
  `docs/product/project-pane.md`).
- **Sync repository** — only while the project's directory is known to be a git repository: checks the project
  against its remote and fast-forwards it right now (see "Syncing one project by hand" in
  `docs/product/project-git-status.md`). It and Browse files come first, set apart from the items below them.
- **Pin** / **Unpin** — see "Pinning" below.
- **Rename** — a dialog with the name alone; an empty name is rejected.
- **Project settings** — the project dialog, see "Editing a project" in `docs/product/consoles-and-projects.md`.
- **Focus mode** — see `docs/product/focus-mode.md`.
- **View archive** — a submenu of the project's newest five archived sessions (most recently archived first), each
  with its agent's icon and its title; choosing one selects it (see "Selecting a session" below). After them,
  **View all (n)**, with the number of archived sessions, opens the archive view. A project with no archived session
  shows only a disabled item saying so.
- **Remove project** — see "Removing a project" in `docs/product/consoles-and-projects.md`; it asks for confirmation
  first.

An expanded project lists its sessions that are not archived as session rows. With none, it shows a single line
saying there are no sessions yet, with a **+** button that opens a new session.

## Session rows

A session row shows the session's status glyph (see "Session statuses" in `docs/product/sessions.md`), its agent's icon,
its title, and its pin button when it is pinned (see "Pinning" below). A project session bound to a console session also
carries that console session's **binding badge**: a small dot in the console session's colour, whose tooltip names it
(see "The binding badge" below); one bound to a project session carries none. A console session's own row carries no
*binding* badge — what it shows there instead is its own colour (see "The binding badge" below). Selecting a row resumes
it when it is interrupted; no menu item does that. The row shows its agent as an icon alone; wherever the agent is named
in words, the account the session runs under follows it in parentheses — "Claude Code (Work)" — as the account's name,
"Default" for the agent's default account, or, for a session whose account has since been removed, the directory it
recorded (or the agent's name alone when it recorded none). The agent's icon has that account, named the same way, as
its tooltip, or the agent's name when there is no account to name; the tooltip shows on pointer hover only, and the icon
adds no tab stop. The agent's icon carries the same tooltip on a focus mode's session cards and archived rows (see
`docs/product/focus-mode.md`) and on the archive view's rows (see "The archive view" below). Its action menu offers:

- **Pin** / **Unpin**.
- **Rename** — see "Renaming a session" in `docs/product/sessions.md`.
- **Focus mode** — a console session's row only: enters that console session's focus mode (see
  `docs/product/focus-mode.md`).
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
  refused, with the sessions named, while any session bound to it has a process running. A project session that has
  started sessions is archived under the same rule, with the same confirmation.

## The binding badge

A project session **bound to a console session** — one carrying the id of the console session it reports to (see
"Console sessions and project sessions" in `docs/product/sessions.md`) — carries a small dot in that console session's
colour wherever the project session is listed: its sidebar row. An unbound session carries no badge, and neither does a
session bound to a project session. The badge's tooltip names the owning console session; the badge is no tab stop,
so the tooltip shows on pointer hover only. The row's own accessible name carries the same fact in words for assistive
technology ("Bound to …"). The badge itself still carries no information its tooltip does not, so colour alone never
distinguishes two owners for a user who cannot tell the colours apart. A console session's own row shows the same
colour, decoratively, since the row's own label already names it, as do its focus mode's header, its chip in a
project's focus mode and its chip in the switch strip (see `docs/product/focus-mode.md`). The cards in a focus mode
carry no badge: a project's focus mode lists only sessions not bound to a console session, and a console session's
lists only the ones bound to it.

**Where the colour comes from.** Octoboard gives each new console session a colour of its own from a fixed palette of
six, taking the first the console's other console sessions that are not archived do not already have, and starting
over from the first once all six are taken. A console session keeps its colour for its lifetime, archiving included,
so with more than six at once — or with one reopened into a palette that filled up meanwhile — two console sessions
of one console can share a colour, which is the other reason nothing is ever told apart by colour alone. Each colour
has its own light and dark value (see "What follows the choice" in `docs/product/appearance.md`).

## Rows, names and keyboard focus

- A name too long for its row fades out where it ends — the row's right edge, its left under a right-to-left language —
  rather than ending in an ellipsis, the full name is then the row's tooltip, and pointing anywhere on the row runs the
  name as a marquee (see "Names too long for their space" in `docs/product/labels-and-tooltips.md`). When the sidebar's
  content is taller than the sidebar it scrolls, and fades out at whichever end has more of it beyond.
- A row's **+** and action-menu buttons show while the row is hovered, holds keyboard focus, is the selected session,
  or has its menu open; they are still reached with Tab when hidden, and show once one of them has focus. They fade
  and open out as they appear, and fade and close up as they go, briefly — at once where the system asks for reduced
  motion.
- **Right-clicking** (or Control-clicking on macOS) a row that has an action menu opens that menu at the pointer, with
  the same entries as its action-menu button. This holds for a project row, a session row (a console session's
  included), the sidebar's header, a console's avatar on the rail (see "The console switcher" above), a focus mode's
  header, a project's heading in a console session's focus mode, a session card and a focus mode's archived rows. The
  right-click neither selects nor activates the row, nor takes keyboard focus; right-clicking another such row while
  a menu is open closes it and opens that row's. It works the
  same in the desktop app and in a browser, where the browser's own menu is not shown over these rows; what a
  right-click shows anywhere else is in "Right-click menus" in `docs/product/window-layout.md`.
- Closing an action menu that was opened with the pointer, by its button or by a right-click, gives keyboard focus back
  to whatever had it before, typically the terminal; one opened from the keyboard gives it back to its button. An
  entry that opens a dialog leaves focus in the dialog.
- Clicking a row deliberately does not move keyboard focus away from the terminal; a row reached with Tab can be
  activated with Enter or Space.
- Each row tells assistive technology what its icons show: a session row its title, agent (Claude Code, Codex or Grok
  Build) and the account it runs under, status, whether it is pinned, and, for a project session bound to a console
  session, the console session it reports to (the binding badge's own fact); a project row whether it is pinned and,
  while it carries an activity marker, what that marker means, plus its branch badge's facts (see "The branch badge"
  in `docs/product/project-git-status.md`). A project row also tells it whether it is expanded or collapsed.

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
second, and moves at once where the system asks for reduced motion. The rail's waiting count walks waiting sessions
in this same order (see "The raised hand" in `docs/product/sessions.md`).

### Pinning

**Pinning** is the user's own mark on a project or a session, set and cleared from its action menu's **Pin** /
**Unpin**, and cleared by its pin button too; nothing is pinned to begin with. Its only effect is the order above. The
pin is stored by the daemon with the project or the session, so every window and every client sees the same pins, and
they survive a restart. A pinned session stays pinned when it is archived and when it is resumed; an archived
session's menu offers no Pin or Unpin. The header of a focus mode offers no Pin or Unpin for what is in focus (see
"What both views show" in `docs/product/focus-mode.md`).

**The pin button.** A pinned project or session shows a pin button: after its name on its row in the sidebar and on a
project's heading in a console session's focus mode, and on the status line of its card in a focus mode. It is always
shown while the item is pinned, and is reached with Tab like the row's other buttons. While the pointer is over it or it
holds keyboard focus, its pin glyph turns into the unpin glyph. Its tooltip is "Unpin", while the name assistive
technology announces for it also names the item ("Unpin …"). Pressing it unpins the item, as the menu's Unpin does; a
mouse press neither selects the row nor moves keyboard focus. The button goes away with the pin, so when it held
keyboard focus, focus is handed first, with its focus ring showing, to the row or card it was on — or, on a project's
heading in a console session's focus mode, to that heading's **+** button.

## Selecting a session

Selecting a session shows its terminal; selecting an interrupted one resumes it, except by `Ctrl+Tab` or
`Ctrl+Shift+Tab` in a console session's focus mode, which only shows an interrupted console session (see "The switch
strip" in `docs/product/focus-mode.md`). Selecting an archived one does not reopen it: it is shown still archived,
with the output its last process left on screen read-only, and typing into its terminal or pressing Reopen reopens it
(see "Archiving, interruption and resuming" and "The terminal" in `docs/product/sessions.md`). Selecting a session also
puts its own pane in the window's right pane: a console session's report panel, a project session's project pane (see
"What the right pane shows" in `docs/product/window-layout.md`). Whatever it is selected from — a sidebar row, a "View
archive" or "Archived console sessions" submenu, focus mode and its switch strip, the archive view, the rail's waiting
count, or the menu bar icon's menu (see "Choosing a session" in `docs/product/menu-bar-icon.md`) — **the sidebar
follows it**: it switches to the session's console, and leaves focus mode when the session does not belong to what
focus mode is on (see "Leaving focus mode, and what is remembered" in `docs/product/focus-mode.md`). Back and Forward
return to earlier selections without resuming anything (see `docs/product/navigation-history.md`).

A session is selected automatically only when this application is the one that opened it — through the console
sessions section's New console session button or the new-session dialog — and that closes the archive view if it is
open. A session that appears any other way, such as one the console session started or one another client of the
daemon opened, is listed unselected and is put on screen by the user selecting it.

When the selected session stops existing — deleted from the archive, by this client or another, for instance — nothing
is selected in its place, and the terminal's area shows its empty state.

## How the sidebar's view changes

**The sidebar's view changes are animated**, the incoming view only, over about a fifth of a second: entering a focus
mode slides it in from the sidebar's end edge as it fades in, a level down; returning to the console's view slides
that in from the start edge, a level up; and switching to another console fades its view in with a slight upward
drift, as does going from one focus mode straight to another (following a chip). The horizontal slides are
mirrored under a right-to-left language, and nothing moves where the system asks for reduced motion.

## The archive view

**View all (n)** — in a project's "View archive" submenu, in a focus mode's archived list, or in the console sessions
section's "Archived console sessions" submenu — opens the archive view: **every** archived session of that project,
every archived session bound to that console session, or every archived console session of that console.

- It **covers the terminal's area only**: the sidebar and the right pane stay as they are.
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
`docs/product/moving-focus-between-regions.md`). Below 1148 px, where the sidebar is a drawer, opening the archive
view closes the sidebar's drawer so that the view is not hidden behind it.
