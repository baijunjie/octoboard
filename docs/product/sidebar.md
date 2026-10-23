# Sidebar

The sidebar is the left-hand pane of the window's content panel (its width, hiding it and the drawer it becomes in a
narrow window are in `docs/product/window-layout.md`). It shows **one console at a time**, the one picked on the rail
(see `docs/product/console-switcher.md`): a header naming that console at its top, then its console sessions, then its
projects with their sessions. A **focus mode** gives the whole sidebar over to one project or one console session (see
`docs/product/focus-mode.md`), and a project's, a console session's or a console's full archive opens in the **archive
view**, over the terminal (see `docs/product/archive-view.md`).
What consoles, projects and sessions are is in `docs/product/consoles.md`, `docs/product/projects.md` and
`docs/product/sessions.md`. The rows the sidebar lists are in `docs/product/sidebar-project-rows.md` and
`docs/product/sidebar-session-rows.md`, and the order it lists them in, with pinning, in
`docs/product/order-of-projects-and-sessions.md`.

**With no console at all there is no sidebar**: the content panel shows the welcome screen instead, which is where the
first console is made (see `docs/product/welcome-screen.md`).

Archived sessions are never listed among a project's or a console's sessions. A project's are reached through its
"View archive" submenu, through its focus mode, and through the archive view; the archived sessions bound to a console
session through its focus mode and the archive view; a console's archived console sessions through the console's
own menu and the archive view.

## The console header

The top of the sidebar names the console being shown, followed by a **+** button, **New console session**, then its
action menu. The **+** opens the session dialog before launching anything (see "Opening a session" in
`docs/product/sessions.md`), or, with the primary modifier held, opens a console session at once without it (see
"Opening a session with defaults" in `docs/product/sessions.md`). The menu offers **Add project**, **Console settings**,
**View archive** and, after a separator, **Delete console** (see "Associating a project" in
`docs/product/projects.md`, and "Editing a console" and "Deleting a console" in `docs/product/consoles.md`; deleting
asks for a typed confirmation first). **View archive** is a submenu of the console's archive preview
(see "The archive view" in `docs/product/archive-view.md` for what it lists and how each item looks); choosing one
selects it (see "Selecting a session" below). After them, **View all** opens the archive view of all of them whenever
the console has an archived console session (see "The archive view" in `docs/product/archive-view.md`). A console with
no archived console session shows only a disabled item saying so. The console's avatar is not repeated here; the rail
marks it.

## The console sessions section and the project list

Under the header comes the **console sessions** section: every console session of the console that is not archived,
each as a session row (see "Session rows" in `docs/product/sidebar-session-rows.md`, which a console session's row
otherwise follows exactly — pin, rename, colour, focus mode, switch account, hand off, archive), in the order of "Order
of projects and sessions" in `docs/product/order-of-projects-and-sessions.md`. What a console session is, what selecting
one does, and what its row's menu offers are in "Console sessions and project sessions" in `docs/product/sessions.md`.

The console sessions heading appends the number of console sessions listed in parentheses, including interrupted
sessions and excluding archived ones. It shows zero when none are listed.

The section's heading carries no action buttons or menu. Archived console sessions are reached from the console's own menu (see "The console header" above), not from the section or from any
one row. With no console session at all, the section shows an empty row saying so; the whole row is a button that
does what the console header's **+** button does (see "Empty rows" below).

With no agent both added and installed, the section instead carries an empty row, and the console header's **+**
button is disabled, since no session can be opened from the window (see "Opening a session" in
`docs/product/sessions.md`, which gives the full rule). When an installed agent waits to be added, the row carries the
bot icon and says that no agent is added; otherwise it carries a warning icon and says that no agent was found on the
user's `PATH`. It says that much and no more, and offers nothing to press: the terminal's empty panel, where it would
otherwise offer a new session, says the rest, with **Add agent…** when an agent waits to be added (see "A console with
nothing in it" in `docs/product/session-terminal.md`); the welcome screen's supported-tool list and hint are described in "What the
welcome screen shows" in `docs/product/welcome-screen.md`. The row shows whatever the section lists — a console
session left interrupted from before an agent went off the `PATH` still has its row — and it takes the place of the "no
console session" row.

Below it, a **Projects** heading and the console's projects (see "Order of projects and sessions" in
`docs/product/order-of-projects-and-sessions.md`). With no project, an empty row saying the console has none yet takes
their place; the whole row is a button opening the new-project dialog, as the console's **Add project** does (see "The
console header" above and "Empty rows" below). The terminal's area chooses its own next action from the console's
console sessions and projects (see "A console with nothing in it" in `docs/product/session-terminal.md`).

The Projects heading appends the number of projects listed in parentheses, including inactive and collapsed projects.
The count follows the current project filter and shows zero when the console has no projects or none match.

The console sessions and Projects headings stay at the top of the scrolling list while their rows move underneath.
As Projects approaches the top, it pushes the console sessions heading upward out of view and takes its place.
Each heading has an opaque background; the edge fade begins below it, and keyboard focus scrolled into view stays
below both the heading and the fade. Moving focus among the controls of a heading pinned at the top does not scroll
the list (see "Scrolling a surface from the keyboard" in `docs/product/scrolling-surfaces.md`).

### Filtering the project list

While the console has projects, a filter button sits among the controls at the end of the Projects heading (see
"Expanding and collapsing the listed projects" below for the order they run in). With no effective keyword or picked
tag, it shows a funnel, is named **Filter projects**, and opens a small popover with two controls. With a filter in
force and the popover closed, it shows a funnel with a cross, is named **Clear filter**, and clears all stored
conditions — the keyword and every picked tag, including tags no project currently carries — rather than opening the
popover. A search field filters the project list as it is typed: a project stays listed when its name contains the
keyword, ignoring case and the keyword's surrounding spaces. Below it, the tags in use (see
`docs/product/project-tags.md`) are shown as toggles, and picking one narrows the list to the projects carrying it; with
several picked, a project must carry all of them, and it must match the keyword too. With no project carrying a tag yet,
a full empty panel in place of the toggles says there are no tags yet, with a line saying tags are added in a project's
settings (see "The full panel" in `docs/product/empty-states.md`). `Escape`, from anywhere in the popover, and Enter in
the search field close it and keep the filter; on a tag, Enter or Space picks or drops that tag and the popover stays
open. Typing or picking a tag does not close the popover when a filter comes into force. A picked tag also shows a check
mark, so it is not told apart by colour alone. Opening and closing it with the mouse leaves keyboard focus where it was.

- While a filter is in force, a strip in the Projects heading's own line, between "Projects (n)" and the controls at
  the line's end, shows it; with no filter the strip is not there. Being part of the heading, it stays at the top
  while the project list scrolls underneath. The keyword, if there is one, comes first, then the picked tags, each as
  plain text in the heading line's own style, set apart from each other by spacing alone.
- The strip only shows the filter: nothing in it is a control. Its entries have no remove buttons, take no keyboard
  focus and are not Tab stops, and neither the keyword nor a single tag can be dropped from here. The way to drop
  the filter from the heading is Clear filter, which drops the keyword and every picked tag together; the popover
  changes them one at a time, and the search field's own clear button clears only the keyword.
- The strip is one label, under the rules of "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`: it never wraps onto a further line and no entry in it is cut or fades on its
  own. When it does not fit it is cut at its end and fades there; it does not scroll sideways. While the mouse
  pointer is anywhere over the strip, the whole strip runs as one marquee, all its entries moving together. While cut,
  it carries the whole filter as its tooltip, the same entries in the same order as plain text, but wrapped onto as
  many lines as the tooltip's width needs; a single entry longer than a line is broken inside it. A strip that fits has no tooltip. For assistive technology the keyword and the picked tags are two groups, named **Filter keyword**
  and **Selected tags**.
- Clicking Clear filter with the mouse leaves keyboard focus where it was; pressing it with the keyboard keeps focus
  on it.
- The tags offered are the distinct tags of the console's projects, in alphabetical order, so a tag that no project
  carries any more disappears from the choices, and from the filter if it was picked, without emptying the list. The
  pick is only hidden, not forgotten: if a project carries that tag again, it filters again.
- The filtered list keeps the order and the dimming of "Order of projects and sessions" in
  `docs/product/order-of-projects-and-sessions.md`.
- With no project matching, a single empty row says so; the heading's filter button clears the filter.
- Each console has its own filter: switching to another console shows that console's own, and switching back, or
  entering and leaving focus mode, keeps it. Filters are not stored, so reloading the window shows every list
  unfiltered.

### Expanding and collapsing the listed projects

One icon-only button follows the filter button. Its name is whichever action it will take, **Collapse all projects**
or an action to expand projects with unarchived sessions. The Projects heading holds its label and project count,
then the strip of conditions while a filter is in force (see "Filtering the project list" above) and, at its end,
the filter button followed by this button.

- It shows while the console has projects **and** the list shows at least one of them. With a filter matching no
  project it is not there — there is nothing to act on — while the filter button beside it stays, as the way back.
- It offers one action at a time. Collapsing closes every listed project. Expanding opens only listed projects with
  at least one unarchived session, including interrupted sessions; listed projects with none keep their current state.
  Projects hidden by a filter keep their state in either direction. What it changes is the same state a project row
  changes (see "Project rows" in `docs/product/sidebar-project-rows.md`). A press records the opposite action, subject
  to the all-expanded or all-collapsed reconciliation below.
- A project's initial expanded or collapsed state, and when it expands or collapses on its own as its sessions change,
  follow "Project rows" in `docs/product/sidebar-project-rows.md`; the state is kept per window without being stored. The button's choice is kept the same way, separately for each console:
  switching to another console shows that console's own, and switching back, or entering and leaving focus mode, keeps
  it. It is not stored, so reloading the window starts each console's button over. On first show, if any project of the
  console is expanded the button offers Collapse all projects; if every project of the console is collapsed it offers to
  expand projects with unarchived sessions.
- Expanding or collapsing a project from its own row does not change the button while the console still has both
  expanded and collapsed projects. The button changes on its own only when every project of the console is expanded,
  when it offers Collapse all projects, or every project of the console is collapsed, when it offers to expand
  projects with unarchived sessions. Projects a filter is hiding count, and changing the filter does not itself flip
  the button. Once it offers Collapse all projects again, collapsing some projects from their rows leaves that
  action in place; collapsing the last expanded project makes it offer to expand projects with unarchived sessions.
- A mouse press leaves keyboard focus where it was, as the sidebar's other controls do. The one exception: Collapse
  all projects takes away the session rows under the projects it collapses, so when keyboard focus is standing in one
  of those it is handed to this button, its focus ring showing, rather than dropped. Expanding does not
  take rows away, so it does not move focus. The project rows themselves stay, so focus already on a project row is
  left where it is.

## Empty rows

A list in the sidebar with nothing to show puts a single **empty row** in its rows' place — the console sessions
section's, the Projects section's and an expanded project's, each described where its list is — in the shape of "The
compact row" in `docs/product/empty-states.md`: where there is one thing to do about the list being empty the whole row
is the button that does it, and the row takes its accessible name and its tooltip from that action.

The rows with nothing to do about them are not buttons and carry no tooltip: the one saying no project matches the
filter (see "Filtering the project list" above), the one saying no agent is added or none was found on the user's `PATH` (see
"The console sessions section and the project list" above), and a project's row saying it has no sessions yet while no
agent is both added and installed (see "Opening a session" in `docs/product/sessions.md`). Focus mode's empty lists use the same compact rows (see "What
both views show" in `docs/product/focus-mode.md`). The terminal's empty state is a full panel carrying the next action
for the current scope (see "A console with nothing in it" in `docs/product/session-terminal.md`).

## Rows, names and keyboard focus

- A name too long for its row fades out where it ends — the row's right edge, its left under a right-to-left language —
  rather than ending in an ellipsis, the full name is then the label's tooltip, and pointing at the name runs it as a
  marquee (see "Names too long for their space" in `docs/product/labels-and-tooltips.md`). When the sidebar's
  content is taller than the sidebar it scrolls, fading at the end that has more beyond it (see "The edge that hides
  content fades" in `docs/product/scrolling-surfaces.md`).
- A row's **+**, Delete or Archive (see "Session rows" in `docs/product/sidebar-session-rows.md`) and action-menu
  buttons show while the row is hovered, holds keyboard focus, is the selected session, or has its menu open; they are
  still reached with Tab when hidden, and show once one of them has focus. They fade and open out as they appear, and
  fade and close up as they go, briefly — at once where the system asks for reduced motion. On a device whose primary input cannot hover — a phone, or a tablet's
  touch screen — they always show.
- **Right-clicking** (or Control-clicking on macOS) a row that has an action menu opens that menu at the pointer, with
  the same entries as its action-menu button. This holds for a project row, a session row (a console session's
  included), the sidebar's header, a console's avatar on the rail (see "The console switcher" in
  `docs/product/console-switcher.md`), a project's focus mode header, a project's heading in a console session's focus
  mode, a session card (a console session's own card in its focus mode included) and a focus mode's archived rows; a
  console session's focus mode header has no menu to open. The right-click neither selects nor activates the row, nor
  takes keyboard focus; right-clicking another such row while a menu is open closes it and opens that row's. It works
  the same in the desktop app and in a browser, where the browser's own menu is not shown over these rows; what a
  right-click shows anywhere else is in "Right-click menus" in `docs/product/window-layout.md`.
- Closing an action menu that was opened with the pointer, by its button or by a right-click, gives keyboard focus back
  to whatever had it before, typically the terminal; one opened from the keyboard gives it back to its button. An
  entry that opens a dialog leaves focus in the dialog.
- Clicking a row deliberately does not move keyboard focus away from the terminal; a row reached with Tab can be
  activated with Enter or Space.
- Each row tells assistive technology what its icons show: a session row its title, agent (Claude Code, Codex or Grok
  Build) and the account it runs under, status, whether it is pinned, and, for a bound project session, the session it
  reports to — the console session the binding badge shows, or the project session its row is listed under, both in the
  same wording (see "The binding badge" in `docs/product/sidebar-session-rows.md`); a project row whether it is pinned
  and, while it carries an activity marker, what that marker means, plus its branch badge's facts (see "The branch
  badge" in `docs/product/project-git-status.md`). A project row also tells it whether it is expanded or collapsed.

## Selecting a session

Selecting a session shows its terminal; selecting an interrupted one resumes it, except by `Ctrl+Tab` or
`Ctrl+Shift+Tab` in a console session's focus mode, which only shows an interrupted console session (see "The switch
strip" in `docs/product/focus-mode.md`). Selecting an archived one does not reopen it: it is shown still archived,
with its history in place of its terminal (see `docs/product/session-history.md`), and pressing Reopen reopens it
(see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md` and "Interrupted, archived and dormant
sessions" in `docs/product/session-terminal.md`). Selecting a session also
gives the window's right pane to its own owner: a console session's report panel, a project session's project's Files
panel (see "What the right pane shows" in `docs/product/window-layout.md`). Whatever it is selected from — a sidebar row, a project or console "View
archive" submenu, focus mode and its switch strip, the archive view, the rail's waiting
count, or the menu bar icon's menu (see "Choosing a session" in `docs/product/menu-bar-icon.md`) — **the sidebar
follows it**: it switches to the session's console, showing the session selected rather than the one remembered for
that console (see "What switching console shows" in `docs/product/console-switcher.md`), and leaves focus mode when the
session does not belong to what focus mode is on (see "Leaving focus mode, and what is remembered" in
`docs/product/focus-mode.md`). Back and Forward return to earlier selections without resuming anything (see
`docs/product/navigation-history.md`).

A session is selected automatically only when this application is the one that opened it through the session dialog,
for either a console or a project session, and that closes the archive view if it is
open. A session that appears any other way, such as one the console session started or one another client of the
daemon opened, is listed unselected and is put on screen by the user selecting it.

When the selected session stops existing — deleted from the archive, by this client or another, for instance — nothing
is selected in its place, and the terminal's area shows its empty state: the panel asking for a session to be selected,
or the next creation action for the current scope (see "With no session selected" in `docs/product/session-terminal.md`). The one exception is the session going with the last console: there is then no terminal at all, and the content panel shows
the welcome screen (see `docs/product/welcome-screen.md`).

## How the sidebar's view changes

**The sidebar's view changes are animated**, the incoming view only, over about a fifth of a second: entering a focus
mode slides it in from the sidebar's end edge as it fades in, a level down; returning to the console's view slides
that in from the start edge, a level up; and switching to another console fades its view in with a slight upward
drift, as does going from one focus mode straight to another (following a chip). The horizontal slides are
mirrored under a right-to-left language, and nothing moves where the system asks for reduced motion.
