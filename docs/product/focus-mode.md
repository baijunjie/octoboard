# Focus mode

Focus mode replaces the whole sidebar — the console header and the console sessions and project lists (see
`docs/product/sidebar.md`) — with one project or one console session. Choosing **Focus mode** from a project's action
menu enters the first, from a console session's action menu the second. The two views are built alike, and differ in
what they list.

## What both views show

Session titles throughout either view follow "Displayed names" in `docs/product/session-titles.md`.

**The header**, in both: a back button, **Leave focus mode**, which returns to the full sidebar; the view's heading; and
a **+** button for a new session. A project's header shows the console's name on a smaller first line above the
project's name, which is the heading. A console session's header shows the console's name alone, on one line, as the
heading; the console session's title is on its own card below. The heading is a heading only: pressing it selects
nothing. The header shows no pin button. A project's header also carries its branch badge, whose failure mark after a
failed check is a tab stop of its own (see "The branch badge" in `docs/product/project-git-status.md`), and, after the
**+**, the project's action menu without its **Focus mode**, **Pin** / **Unpin** and **View archive** items — the view's
own **Archived (n)** list stands in for the last. A console session's header carries nothing else — no colour, lock
button or action menu, and no menu on a right-click: those are on the console session's own card below it (see "A
console session's focus mode" below).

**The list's section headings**, in both: **Sessions (n)** stays at the top of the scrolling list while its
cards move underneath; as **Archived (n)** approaches the top, it pushes the Sessions heading upward out of view
and takes its place. The headings have an
opaque background and stay clear of the edge fade, which begins below the heading. Keyboard focus scrolled into
view stays below both the heading and the fade. The other scrolling rules are in "The edge that hides content
fades" and "Scrolling a surface from the keyboard" in `docs/product/scrolling-surfaces.md`.

The session lock button on live cards, a console session's own card included, follows
"Session lock controls" in `docs/product/sidebar-session-rows.md`.

**Session cards**, in both: the status glyph with the status in words, the pin button when pinned (see "Pinning" in
`docs/product/order-of-projects-and-sessions.md`), the session's action menu and, just before it, a Delete or Archive
button (both always shown when available; see "Session rows" in `docs/product/sidebar-session-rows.md` for when the
shortcut is there and what it does), its title on one line, its agent's icon
followed by the **model line**, and how long ago it was created at the trailing end of the agent line. The model line
reads "<account> · <model> · <effort>". The account is the one the session runs under, named as in a session row's
agent icon tooltip (see "Session rows" in `docs/product/sidebar-session-rows.md`): the account's name, the default
account's name, or a removed account's recorded directory; a removed account that recorded no directory leaves the line
at "<model> · <effort>". "<model> · <effort>" is the model and reasoning effort the session runs with, as "The model and
effort a session is shown running with" in `docs/product/model-and-reasoning-effort.md` decides them, an unknown one
reading **Default**, and reads **Default model** when neither is known; the agent's name is not shown on the card. The
model line is one line that fades where it is cut off, with its full text as its tooltip while cut (see "Names too long
for their space" in `docs/product/labels-and-tooltips.md`). The card's accessible name ends with "<model> · <effort>"
alone: the account is already named beside the agent in it, as in a session row's accessible name. Resuming or
reopening does not reset the creation time. Resting the mouse on the creation time shows a tooltip writing out the
creation date ("Created …") as an absolute date with year, month, day, hour and minute, formatted in the current
language and the client's local time zone. The time is not a tab stop, so the tooltip does not open on keyboard
focus, and a tap on it is the card's press (see "Tooltips on indicators" in `docs/product/labels-and-tooltips.md`).
The card's accessible description carries that creation date in place of the relative wording, after the context
size while the chip is shown. When the context size is known and above zero, its chip appears immediately
before the creation time on that same agent line and updates while the session runs (see "The context-size chip" in
`docs/product/session-context.md`), set apart from the creation time by spacing alone; without the chip, the trailing
block contains creation time alone. The model line has no adjacent separator. The status
and agent lines share aligned leading icons and text starts. A title with a console-session colour dot shares that
alignment;
a title without one starts at the card’s leading content edge, with no empty icon space. Long titles follow "Names too long
for their space" in `docs/product/labels-and-tooltips.md`. The agent's icon has the same tooltip as on a session row, listing the account,
model and effort under the agent's name (see "Session rows" in `docs/product/sidebar-session-rows.md`). Clicking a card
selects the session. They are in the order of "Order of projects and sessions" in
`docs/product/order-of-projects-and-sessions.md`. Cards have an opaque background distinct from the sidebar's, which
stays opaque while they move, so text from other cards, the console-session summary or the archive does not show through
them. Cards have no outline: each is set apart by the same soft shadow as the content panel's edges (see "The window
chrome and the content panel" in `docs/product/window-layout.md`), and the selected card is outlined in the accent
colour.

A card for a session **bound to a project session the same view lists** is **inset under that session's card** and
follows it, as a session row is under its owner's row (see "Project rows" in `docs/product/sidebar-project-rows.md`). A
connector runs from the owner's card across the gaps between cards, bending smoothly toward the middle of each child
card and stopping at the last child. It follows the inset side under a right-to-left language. The owner's name in the
card's accessible name carries the nesting to assistive technology. In a project's combined view, cards also identify the
console session above them (see "A project's focus mode" below and "The binding badge" in
`docs/product/sidebar-session-rows.md`).

**Archived (n)**, in both, only while the view has archived sessions — with none, the section is not shown at all: the
archive preview described in "The archive view" in `docs/product/archive-view.md`, each row showing its agent's icon
and its title on one compact line, with the relative archive time and its tooltip of the archive and creation dates
at the trailing end as described there; the row's accessible description carries both dates. The heading
counts every archived session in the view's scope, including those the preview leaves out. Clicking a row selects
it while leaving it archived. Its action menu offers **Reopen**, which selects and resumes the session, then **Copy
session ID** (see "Session rows" in `docs/product/sidebar-session-rows.md`), and after a separator **Delete**, disabled
while locked.
A Delete shortcut immediately before the action-menu button appears for an unlocked unused session that is not still
starting, under the same rules as "Session rows" in `docs/product/sidebar-session-rows.md`. It takes the same deletion
action as the menu (see "Deleting archived sessions" in `docs/product/deleting-sessions.md` for the confirmation rules).
The menu and an available shortcut show with the row's other controls as described in "Rows, names and keyboard focus"
in `docs/product/sidebar.md`. Reopening follows "Archiving, interruption and resuming" in
`docs/product/archiving-and-resuming-sessions.md`. Whenever any archived session is left out — one under an archived
owner, or one beyond the first ten — **View all** follows the preview as a row with the same alignment, hover feedback
and keyboard focus treatment as the session rows; it opens the archive view and carries no count. When every archived
session fits in the preview, there is no View all row.

The archive preview, including View all, follows "Items moving within a collection" in
`docs/product/collection-motion.md`.

Relative times ("5 minutes ago") are worded in the current language, and anything under a minute reads as now. While
shown they keep up with the clock by themselves, never more than 30 seconds behind it.

**Where either view has nothing to list**, a single **compact empty row** takes the list's place, under its
section heading, as in the full sidebar (see "The compact row" in `docs/product/empty-states.md` and "Empty rows" in
`docs/product/sidebar.md`). Where there is one action, clicking anywhere on the row takes it; a row with no action is
plain text. These lists do not carry a separate empty-state heading or button.

## A project's focus mode

The switch at the trailing end of the **Sessions (n)** heading chooses whether all of the project's sessions are
listed together. It has no visible text label: its tooltip explains the choice on pointer hover and keyboard focus,
and its accessible name carries the same explanation. It appears only
while the project has live sessions under a console session that is not archived, including a lead session and the
sessions under it; archived bound sessions alone do not make it appear. It is **off by default**: sessions under
console sessions are shown separately as summary chips. Turning it on lists all of the project's live sessions as
cards and hides the summary chips. The choice applies to every project's focus mode in this client and survives
reloading, including while the switch is hidden; it is not a shared application setting and is not exported with
configuration. If client storage is unavailable, the choice lasts for the current page load only.

From the top, below the header:

- **Sessions (n)**: with the switch off, the project's sessions that are not archived and have **no console session
  above them** (see "Lead sessions" in `docs/product/hub-orchestration.md`) — the unbound ones, each with the sessions
  bound to it inset under it. A **lead session and the sessions under it** are left out of the list together, as is
  any session bound to a console session. With the switch on, every live session of the project is listed, with the
  same order and nesting. Each card under a console session, directly or through a lead session, carries that console
  session's colour dot before its title in reading order, staying visible when the title is too long for its space,
  and a tooltip naming it; its accessible name names that console session as well as its immediate project-session
  owner, where it has one. The count is every card listed, the inset ones included. With
  none, a compact row saying there are no sessions yet takes their place; clicking the row opens a **New session**.
- **The sessions elsewhere**, between the live cards and the archive, present only with the switch off and while
  the project has live sessions under a console session that is not archived: one sentence for the whole project
  saying how many of its sessions are under console sessions, naming none of them, and below it a row of chips,
  wrapping onto as many lines as it needs — one chip per console session they are under, in the order the console
  sessions section lists them. A chip shows the console session's colour, its title (faded out when too long, the
  full title then its tooltip) and an inline count badge in the neutral grey tint the neutral badges and chips share,
  the same on every chip. Both the total and each chip's count include a lead session **and** the sessions under it,
  since the lead session's team is listed with it in that console session's focus mode. Pressing a chip enters that console
  session's focus mode; assistive technology hears it as entering that focus mode, with the count.
- **Archived (n)** counts all of the project's archived sessions, including those under a lead session that is
  still live. The switch changes only the live list; the archive's count, preview and View all
  scope stay the same.

**New session** (the header's **+** and the empty row) opens the new-session dialog with no "Report to"
choice: the session is always unbound. With the primary modifier held, the header's **+** opens the session at once
without the dialog, unbound as well (see "Opening a session with defaults" in `docs/product/sessions.md`). With no agent both
added and installed, the header's **+** is disabled and the empty row is plain text (see "Opening a session" in
`docs/product/sessions.md`).

## A console session's focus mode

- **The switch strip**, right under the header, while the console has more than one console session that is not
  archived: see "The switch strip" below.
- **The console session's own card**, below the switch strip, or right under the header without one. It stays where it
  is while the list below scrolls. It is a session card as in "What both views show" above — status, title, agent,
  creation time and context size — with these differences: its title is preceded by the console session's own colour,
  decoratively, since the title names it; it never shows a pin button, even while the console session is pinned; and
  its action menu is the console session's row's (see "Session rows" in `docs/product/sidebar-session-rows.md`) without
  **Pin** / **Unpin** and **Focus mode**. Its lock button while locked, and its Delete or Archive shortcut, are as on
  any card.
  Pressing it selects the console session, showing its terminal and report panel, which the view has no other row
  for; right-clicking it opens its action menu. It is not counted in **Sessions (n)**.
- **Sessions (n)**: the projects of the console that have a session bound to this console session which is not archived,
  in the order of the project list, and under each project's name the sessions bound to this console session as cards,
  each **lead session** among them carrying the sessions bound to it inset under it — they run in the lead session's own
  project, so they fall under the same project's name. The count includes them, so it covers sessions this console
  session sees but cannot drive (see "Lead sessions" in `docs/product/hub-orchestration.md`). A session bound to another
  console session, to a project session that is not under this one, or to none, is not shown, and neither is a project
  with nothing under this console session. With none, a compact row saying there are no bound sessions takes their
  place, carrying no action of its own — the header's **+** below is the way to open one. Each project's name carries its pin
  button while it is pinned (see "Pinning" in `docs/product/order-of-projects-and-sessions.md`), its own **+**, which
  opens a new session in that project, and its action menu, which is the project row's without its Focus mode item — a
  project has no focus mode to be entered from here — and keeps **Pin** / **Unpin**. That menu's **View archive** covers
  only the project's share of this console session's archive: the project's archived sessions bound to this console
  session, or bound to an archived lead session that is bound to it, previewed as any archive preview is (see "The
  archive view" in `docs/product/archive-view.md`). The project's sessions bound to another console session or to none,
  and those of a lead session that is still live, are not in it; with none in the share, the submenu shows only the
  disabled item saying there are none. Its **View all**, offered whenever the share is nonempty, opens this console
  session's archive view — the one **View all** under **Archived (n)** below opens — not the project's. The project's
  row in the full sidebar keeps the project's whole archive in its menu. The project list's filter is not part of this
  view.
- **New session**: the header's **+** opens a menu of the console's projects, so that a project with nothing bound yet
  can be reached; choosing one opens the new-session dialog for it. The dialog has no "Report to" field, and shows a
  line saying the session reports to this console session, which it will be bound to. With the primary modifier held,
  this menu and each project's own **+** open the session at once without the dialog, bound to this console session
  all the same (see "Opening a session with defaults" in `docs/product/sessions.md`). With no agent both added and
  installed, the header's **+** and each project's own **+** are disabled (see "Opening a session" in
  `docs/product/sessions.md`).
- **With no project in the console at all** there is nothing to open a session in: the header's **+** is disabled, and
  in place of the sessions a compact row says the console has no projects yet; clicking it opens **Add project**.
- **Archived (n)** counts the archived sessions under this console session — bound to it, or bound to an archived lead
  session that is bound to it (see "Lead sessions" in `docs/product/hub-orchestration.md`) — and **View all** opens
  the archive view scoped to them, including the sessions of archived lead sessions, which the preview leaves out
  (see "The archive view" in `docs/product/archive-view.md`). An archived session of a lead session that is still live
  is not here: it is in the lead session's project's archive alone.

## The switch strip

The switch strip moves between the console's console sessions without leaving focus mode. It is there only in a
console session's focus mode, and only while the console has at least two console sessions that are not archived.

- **One chip per console session of the console that is not archived**, the one in focus included. The order is
  fixed: pinned ones first, then by when they were started, oldest first. It never follows their statuses, whether or
  not the sidebar is ordered by status (see "Order of projects and sessions" in
  `docs/product/order-of-projects-and-sessions.md`), so a chip stays where it is while what it shows changes; pinning or
  unpinning one moves its chip.
- **A chip** shows the console session's colour (see "The binding badge" in `docs/product/sidebar-session-rows.md`), its
  title, faded out when too long with the full title then its tooltip, and what is going on in that console session and
  the sessions bound to it: a waving hand while any of them is waiting for the user, otherwise the *working* glyph while
  any of them is working or working in background, otherwise nothing — a session merely running is not given a chip's room, so only those two
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

**Next console session moves to the next chip and Previous console session to the previous one** — `Ctrl+Tab` and
`Ctrl+Shift+Tab` unless the user has changed them (see `docs/product/keyboard-shortcuts.md`) — going round from either
end, starting from the console session in focus. They exist only in the macOS application; a browser keeps the keys
for its own tabs.

- It works only while the sidebar is shown — docked, or as an open drawer below 1168 px; not while it is hidden or
  only floating in (see "Hiding the sidebar" in `docs/product/window-layout.md`) — in a console
  session's focus mode, with the strip there. Anywhere else the keys go on to the terminal as usual.
- Where it works, the keys never reach the terminal, so the agent never receives them. It does nothing while a dialog
  or a menu is open, and is left alone during an input-method composition. A key held down moves one chip only.
- It enters the next console session's focus mode and selects it, like pressing its chip, except that **it does not
  resume an interrupted console session**: it only shows it, with its "Not running" card (see "Interrupted, archived and
  dormant sessions" in `docs/product/session-terminal.md`), so stepping through the chips never starts a process.
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

## Creating sessions from the keyboard

In either focus mode, **New session** — `Cmd+N` on an Apple keyboard or `Ctrl+N` elsewhere, unless the user has
changed it — takes the header's **+** action, and **New session with defaults** — `Shift+Cmd+N` or `Ctrl+Shift+N`
unless changed — takes its quick form, opening a session with defaults (see "Opening a session with defaults" in
`docs/product/sessions.md`). How either is changed is in `docs/product/keyboard-shortcuts.md`. The header's tooltip
shows New session's shortcut, or New session with defaults' while the primary modifier is held. These creation
shortcuts are available only in focus mode.

In a project's focus mode, the normal shortcut opens its new-session dialog and the quick shortcut creates an
unbound session in that project. In a console session's focus mode, either shortcut opens the header's project menu;
choosing a project opens its dialog or creates with defaults, bound to the console session in focus. Releasing the
shortcut keys before choosing the project does not change which action was requested. A menu opened with the pointer
continues to follow the held modifier instead.

The shortcuts match the physical key whatever the keyboard layout, work across the window including the terminal,
and do nothing while a dialog or menu is open or during input-method composition. Holding the keys does not repeat
creation. The header's existing availability and quick-creation refusals apply to the shortcuts too.

## The focus mode shortcut

**A keyboard shortcut toggles focus mode**: Toggle focus mode, `Shift+Cmd+F` on a Mac (the macOS application, or a
browser on a Mac) and `Ctrl+Shift+F` elsewhere unless the user has changed it (see
`docs/product/keyboard-shortcuts.md`), matched on the physical key whatever the keyboard layout. The Focus mode items
of the project and console session menus show it.

- Outside focus mode it enters focus mode for the selected session's context, switching the sidebar to that session's
  console first when needed: a project session's project, or, with a console session selected, that console session
  itself. With nothing selected, or an archived console session, it does nothing.
- In either focus mode it leaves focus mode.
- It does nothing while a dialog or a menu is open, and during an input-method composition.
- The key combination never reaches the terminal, so the agent never receives it.

## Keyboard focus on entering and leaving

**Entering or leaving focus mode by selecting a session hands keyboard focus to that session's terminal**, as
selecting any session does: pressing a switch strip chip, Next or Previous console session (see "The switch strip"
above), or selecting a session that does not belong to what is in focus. A console session that a chip press resumes
gets it once it is running again.

Where nothing takes keyboard focus that way — focus mode entered or left without a selection (a Focus mode menu item,
a chip in a project's focus mode, the back button, the focus mode shortcut), or a selection whose terminal does not
take it, such as an archived session or one still being resumed — and keyboard focus was in the sidebar, it moves to
somewhere useful in the new view: after going from one console session's focus mode to another's, to the switch
strip's current chip; otherwise, on entering, to the back button, and on leaving, to the row of the project or console
session that was in focus. When it was not in the sidebar — the mouse used, or the shortcut pressed from the terminal —
it stays where it was, normally on the terminal.
