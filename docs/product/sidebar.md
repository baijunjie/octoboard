# Sidebar

The sidebar is the left-hand pane of the window's content panel (its width, hiding it and the drawer it becomes in a
narrow window are in `docs/product/window-layout.md`). It shows **one console at a time**, the one picked on the rail:
a header naming that console at its top, then its console sessions, then its projects with their sessions. A **focus
mode** gives the whole sidebar over to one project or one console session (see `docs/product/focus-mode.md`), and a
project's, a console session's or a console's full archive opens in the **archive view**, over the terminal.
What consoles, projects and sessions are is in `docs/product/consoles.md`, `docs/product/projects.md` and
`docs/product/sessions.md`.

**With no console at all there is no sidebar**: the content panel shows the welcome screen instead, which is where the
first console is made (see `docs/product/welcome-screen.md`).

Archived sessions are never listed among a project's or a console's sessions. A project's are reached through its
"View archive" submenu, through its focus mode, and through the archive view; the archived sessions bound to a console
session through its focus mode and the archive view; a console's archived console sessions through the console's
own menu and the archive view.

## The console switcher

The consoles are switched from the top of the rail, at the window's left edge (see "The rail" in
`docs/product/window-layout.md`): one avatar per console (see "Avatar" in `docs/product/consoles.md`), in
the order the consoles were created, followed by **New console**. The console the sidebar shows is the **current
console**; its avatar has a tile behind it and is announced as the current one. (A sidebar floating in to preview
another console is the one exception, see "Previewing a console from the rail" below.) Pressing another avatar makes
that console the current one, its view fading in (see "How the sidebar's view changes" below); pressing it with the
mouse leaves keyboard focus where it was. A console created from this window — by the rail's New console or the
welcome screen's (see `docs/product/welcome-screen.md`) — becomes the current one as soon as it appears; one created
from another client is only added to the rail.

An avatar carries a badge at its bottom corner while something is going on in its console, taken from all its
sessions, its console sessions included, so a raised hand in a console that is not shown is in sight too:

| Badge | Shown when |
|---|---|
| A waving hand | at least one session is waiting for the user |
| A disc filled with the theme background, with a green rim and a blue centre sending a fading ripple outward | otherwise, at least one session is working |
| A green speech bubble | otherwise, at least one session is running and awaiting instructions, or still starting |
| none | no session has a running process or is starting |

A session *running in background* counts as working here (see "Running in background" in `docs/product/sessions.md`).
The centre and ripple use the same blue as a working session's glyph. The ripple stands still where the system asks
for reduced motion, leaving the theme background disc, green rim and blue centre.

The name assistive technology announces for an avatar is the console's name together with what its badge means, and
**the avatar's tooltip is that same text** — the bare console name while nothing is going on. When that tooltip opens,
and why a mouse user with the docked sidebar hidden does not get to read it, is in "Tooltips on icon-only controls" in
`docs/product/labels-and-tooltips.md`.

**Right-clicking** (or Control-clicking) an avatar opens that console's action menu at the pointer, the same menu as
the sidebar's header carries (see "The console header" below). It does not switch to the console.

**The current console is remembered per client**, in that client's own browser storage, as the panes' widths are
(see "Resizing the sidebar and the right pane" in `docs/product/window-layout.md`). A remembered console that no
longer exists falls back to the first console. Switching console switches the terminal's area and the right pane with
it (see "What switching console shows" below). Selecting a session anywhere switches the sidebar to that session's
console (see "Selecting a session" below).

### What switching console shows

Making another console the current one — pressing its avatar, a press inside the floating sidebar while it previews
that console (see "Previewing a console from the rail" below), or a console created from this window appearing —
switches the whole window to that console, not only the sidebar:

- **The terminal's area shows the session last selected in that console**, the way Back and Forward show a place: an
  interrupted session is shown without being resumed, and an archived one is shown still archived, not reopened (see
  "Selecting a session" below for what selecting does instead). With no session remembered for the console, or with
  the remembered one no longer existing, nothing is selected, and the terminal's area shows its empty state for that
  console (see "With no session selected" in `docs/product/session-terminal.md`); a console just created always
  shows that.
- **The right pane shows that session's own pane** — a console session's report panel, a project session's project
  pane — and with nothing selected there is no right pane (see "What the right pane shows" in
  `docs/product/window-layout.md`).
- **Focus mode is left**, and an **open archive view closes**.

**The session last selected in each console is remembered per window, in memory only**: whichever way the session
was selected (a row, a menu, Back and Forward, a session this window opened, and so on), it becomes the one shown the
next time its console is switched to. It is not stored, so reloading the window or restarting the application forgets
it, and every client has its own.

Selecting a session of another console — from a menu, the rail's waiting count or the menu bar icon's menu — also
makes its console the current one, but shows **the session chosen**, not the one remembered for that console; Back and
Forward moving to a place in another console likewise show that place's own session, or none.

### Previewing a console from the rail

While the docked sidebar is hidden, at 1168 px and wider, resting the mouse on a console's avatar floats the sidebar
in showing **that** console — when it comes up, how it looks and when it goes away are in "A hidden pane floats in on
hover" in `docs/product/window-layout.md`. The sidebar's toggle in the top bar floats it in as well, but always
showing the **current** console, so that way in is never a preview: a pointer moving from an avatar onto the toggle
puts the floating card back on the current console. For any console other than the current one, resting on its avatar
is a **preview**:

- It shows the console's own view — the header, its console sessions and its projects — never a focus mode, which
  belongs to the current console only. Resting on the current console's avatar shows the sidebar as it is, focus mode
  included. Moving from one avatar to another fades the new console's view in, as switching console does.
- **Hovering changes nothing**: the current console, its tile on the rail, the remembered console and the window's
  history all stay as they were.
- **A press anywhere inside the floating sidebar**, with any mouse button, makes the previewed console the current
  one, and the press then goes on to do what it does there — select a row, open a menu, start a dialog. Pressing the
  avatar itself makes it current too, as always. Such a press closes an open archive view at once, but the terminal's
  area and the right pane change only **once the press has ended**, and to what the press chose itself where it
  chose something: a session the press selected is shown, with its own pane; otherwise the console's remembered
  session is shown as described in "What switching console shows" above. A press that only gave the right pane to
  something, as Browse files does, still brings the remembered session into the terminal's area, and the right pane
  keeps what the press gave it. In the window's history the console change together with the press's selection, or
  with the remembered session shown at its end, counts as one visit (see "What counts as a place" in
  `docs/product/navigation-history.md`).
- Showing the sidebar for good with its toggle while it previews a console shows the current console, not the one the
  card was previewing. The card goes away and the column eases open; see "Hiding the sidebar and the right pane"
  and "A hidden pane floats in on hover" in `docs/product/window-layout.md`.

## The console header

The top of the sidebar names the console being shown, followed by a **+** button, **New console session**, then its
action menu. The **+** opens the session dialog before launching anything (see "Opening a session" in
`docs/product/sessions.md`), or, with the primary modifier held, opens a console session at once without it (see
"Opening a session with defaults" in `docs/product/sessions.md`). The menu offers **Add project**, **Console settings**,
**Archived console sessions** and, after a separator, **Delete console** (see "Associating a project" in
`docs/product/projects.md`, and "Editing a console" and "Deleting a console" in `docs/product/consoles.md`; deleting
asks for a typed confirmation first). **Archived console sessions** is a submenu of the console's newest archived
console sessions, each with its agent's icon and its title; choosing one selects it (see "Selecting a session" below).
After them, **View all** opens the archive view of all of them whenever the console has an archived console session (see
"The archive view" below). A console with no archived console session shows only a disabled item saying so. The
console's avatar is not repeated here; the rail marks it.

## The console sessions section and the project list

Under the header comes the **console sessions** section: every console session of the console that is not archived,
each as a session row (see "Session rows" below, which a console session's row otherwise follows exactly — pin, rename,
colour, focus mode, switch account, hand off, archive), in the order of "Order of projects and sessions" below. What a console session is,
what selecting one does, and what its row's menu offers are in "Console sessions and project sessions" in
`docs/product/sessions.md`.

The console sessions heading appends the number of console sessions listed in parentheses, including interrupted
sessions and excluding archived ones. It shows zero when none are listed.

The section's heading carries no action buttons or menu. Archived console sessions are reached from the console's own menu (see "The console header" above), not from the section or from any
one row. With no console session at all, the section shows an empty row saying so; the whole row is a button that
does what the console header's **+** button does (see "Empty rows" below).

With no agent available on the machine at all, the section instead carries an empty row saying that no agent was found
on the user's `PATH`, and the console header's **+** button is disabled, since starting one would only be refused.
The row says that much and no more: the new-session dialog explains which agents Octoboard supports and the `PATH` requirement (see "Opening
a session" in `docs/product/sessions.md`); the welcome screen's supported-tool list and hint are described in "What
the welcome screen shows" in `docs/product/welcome-screen.md`. The row shows whatever the section lists — a console
session left interrupted from before an agent went off the `PATH` still has its row — and it takes the place of the "no
console session" row.

Below it, a **Projects** heading and the console's projects (see "Order of projects and sessions" below). With no
project, an empty row saying the console has none yet takes their place; the whole row is a button opening the
new-project dialog, as the console's **Add project** does (see "The console header" above and "Empty rows" below). The
terminal's area chooses its own next action from the console's console sessions and projects (see "A console with
nothing in it" in `docs/product/session-terminal.md`).

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
  while the project list scrolls underneath. The keyword, if there is one, comes first, as a small tag in a neutral
  grey; the picked tags follow it, filled with the light accent tint and accent-coloured text of a project's picked
  tags (see "Picked tags" in `docs/product/project-tags.md`), so they are told apart from the keyword.
- The strip only shows the filter: nothing in it is a control. Its tags have no remove buttons, take no keyboard
  focus and are not Tab stops, and neither the keyword nor a single tag can be dropped from here. The way to drop
  the filter from the heading is Clear filter, which drops the keyword and every picked tag together; the popover
  changes them one at a time, and the search field's own clear button clears only the keyword.
- The strip is one label, under the rules of "Names too long for their space" in
  `docs/product/labels-and-tooltips.md`: it never wraps onto a further line and no tag in it is cut or fades on its
  own. When it does not fit it is cut at its end and fades there; it does not scroll sideways. While the mouse
  pointer is anywhere over the strip, the whole strip runs as one marquee, all its tags moving together. While cut,
  it carries the whole filter as its tooltip, drawn as the same tags as the strip — the keyword in its neutral grey,
  then the picked tags in the accent tint — but wrapped onto as many lines as the tooltip's width needs; a single tag
  longer than a line is broken inside its tag. A strip that fits has no tooltip. For assistive technology the keyword and the picked tags are two groups, named **Filter keyword**
  and **Selected tags**.
- Clicking Clear filter with the mouse leaves keyboard focus where it was; pressing it with the keyboard keeps focus
  on it.
- The tags offered are the distinct tags of the console's projects, in alphabetical order, so a tag that no project
  carries any more disappears from the choices, and from the filter if it was picked, without emptying the list. The
  pick is only hidden, not forgotten: if a project carries that tag again, it filters again.
- The filtered list keeps the order and the dimming of "Order of projects and sessions" below.
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
  changes (see "Project rows" below). A press records the opposite action, subject to the all-expanded or
  all-collapsed reconciliation below.
- A project's initial expanded or collapsed state follows "Project rows" below and is kept per window without being
  stored. The button's choice is kept the same way, separately for each console: switching to another console
  shows that console's own, and switching back, or entering and leaving focus mode, keeps it. It is not stored, so
  reloading the window starts each console's button over. On first show, if any project of the console is expanded
  the button offers Collapse all projects; if every project of the console is collapsed it offers to expand
  projects with unarchived sessions.
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
filter (see "Filtering the project list" above) and the one saying no agent was found on the user's `PATH` (see "The
console sessions section and the project list" above). Focus mode's empty lists use the same compact rows (see "What
both views show" in `docs/product/focus-mode.md`). The terminal's empty state is a full panel carrying the next action
for the current scope (see "A console with nothing in it" in `docs/product/session-terminal.md`).

## Project rows

A project row shows the project's name and a chevron after the name. Clicking the row, or Enter or Space on it,
collapses or expands it. A project starts collapsed unless it has at least one unarchived session, including an
interrupted session; a project with only archived sessions starts collapsed. This default is applied once when the
project first appears in the window, including projects added later. After that, session changes do not change its
expanded or collapsed state. The state is kept per window across console switches and entering or leaving focus
mode, and is not stored: reloading the window applies the default again. The
chevron always shows while the project is collapsed, and only while the row is hovered or focused while it is
expanded, and while hidden it takes no room, so a long name runs as far as it would in a session row and ends sooner
when the chevron appears. The hover chevron fades and opens out as it
appears, and back as it goes, briefly — at once where the system asks for reduced motion.

After the name, before the activity marker below and the row's controls, comes the project's **branch badge** — its
current branch and how far it is from its upstream, when its directory is a git repository (see "The branch badge" in
`docs/product/project-git-status.md`).

A collapsed project shows an activity marker with the same meanings as a console's badge on the rail (see "The
console switcher" above), taken from its sessions that are not archived, so a waiting session can be found with its
project collapsed. The three glyphs being alike in size, **the marker carries a tooltip** saying which activity it
stands for, in the same words the row's own name folds in beside the project's name (see "Tooltips on indicators" in
`docs/product/labels-and-tooltips.md`).

At the row's end are its pin button while the project is pinned (see "Pinning" below), then a **+** button, **New
session**, which opens the new-session dialog for that project (see "Opening a session" in
`docs/product/sessions.md`) — or, with the primary modifier held, opens the session at once without it (see "Opening
a session with defaults" in the same doc) — and the project's action menu. The same menu is the focused
project's, and a project's heading in a console session's focus mode, apart from the items those views leave out
(see "What both views show" and "A console session's focus mode" in `docs/product/focus-mode.md`). Choosing an action
closes the menu at once; submenu entries open their next menu. Separators split it into four groups, in this order:
the first holds **Pin** / **Unpin**, as in a session's menu, and the actions for viewing the project inside the app
(**Focus mode**, **Browse files**, **View archive**); then the **Open in** submenu and **Git** submenu; managing the
project itself (**Rename**, **Copy project name**, **Project settings**);
and **Remove project**. An item a view does not offer is left out without moving the others.

- **Pin** / **Unpin** — see "Pinning" below.
- **Focus mode** — see `docs/product/focus-mode.md`.
- **Browse files** — see "Opening a project's files" in `docs/product/project-pane.md` for its visibility and the
  Files mode it opens.
- **View archive** — a submenu of the project's archived parent-session preview (see "The archive view" below), each
  with its agent's icon and its title; choosing one selects it (see "Selecting a session" below). After them,
  **View all**, without a count, opens the archive view whenever the project has an archived session, including
  when all of them fit in the submenu. A project with no archived session shows only a disabled item saying so.
- **Open in** — a submenu with **File manager**, then **Terminal**, then the project's repository host when available.
  File manager opens the project's directory on the daemon's host (see "Opening the system file manager" below).
  Terminal opens the system terminal at that directory (see "Opening a project terminal" in
  `docs/product/project-terminal.md`). Both are offered for every project, including one that is not a git repository
  and one whose git status has not arrived. Neither raises a success toast; a failure is an error alert.
  The repository entry opens the recorded remote's web address in the client's browser and carries a trailing
  external-link icon. Public GitHub, GitLab and
  Bitbucket hosts carry their brand names and icons; other hosts carry their hostname and a globe icon, including
  self-hosted installations. A missing, invalid, local or unsupported remote, or a client unable to open web links,
  leaves out the repository entry. HTTP and HTTPS remotes retain their web scheme; SSH, SCP-style, `git://`,
  `git+ssh://` and `ssh+git://` remotes use HTTPS. Credentials, query strings, fragments and a trailing `.git` are
  omitted from the opened address, as are transport ports on non-web remotes. Opening raises no success toast;
  a failure is an error alert.
- **Git** — a submenu, only while the project's status says its directory is a git repository, so not before the
  first status and not when the directory is not one (see "What one check does" in
  `docs/product/project-git-status.md`). It holds **Pull current branch**, then **Push current branch**, then
  **Switch branch**, then, after a separator, the destructive **Restore file changes** action. Pull current branch
  checks the project against its remote and fast-forwards it right now, is not waited on, and toasts nothing when it
  succeeds (see "Pulling one project by hand" in `docs/product/project-git-status.md`). Push current branch pushes
  that branch and is waited on until the push finishes; the window then shows a notice that the current branch was
  pushed, or an error alert if the push was refused (see "Pushing the current branch" in
  `docs/product/project-git-actions.md`). Switch branch opens the third-level local branch menu; see "Switching the
  local branch" in `docs/product/project-git-actions.md`. Restore file changes asks for confirmation before discarding tracked changes
  (see "Restoring file changes" in `docs/product/project-git-actions.md`).
- **Rename** — a dialog with the name alone; an empty name is rejected.
- **Copy project name** — copies the project's name exactly to the clipboard. Available in all the
  project-menu placements above, including right-click menus, when the platform offers a clipboard; otherwise the
  item is absent. Success raises a notice toast; a refused copy raises an error alert.
- **Project settings** — the project dialog, see "Editing a project" in `docs/product/projects.md`.
- **Remove project** — see `docs/product/removing-projects.md`; it asks for confirmation
  first.

An expanded project lists its sessions that are not archived as session rows, with a connector bending smoothly
toward the middle of each child row and stopping at the last child. With none, it shows an empty row saying
there are no sessions yet; the whole row is a button that opens the new-session dialog for that project, as the row's
**+** button does (see "Empty rows" above).

A session **bound to another of the project's sessions** — one that session started, a lead session's own sessions
included (see "Lead sessions" in `docs/product/hub-orchestration.md`) — is listed **under that session**, one level
deeper, inset with the same rounded connectors toward each child row. Unbound sessions and sessions bound to a console
session stay at the project's own level, so a project's sessions go at most two levels deep. The nesting has no fold
control of its own: the sessions under an owner fold away with their project, like every other session row.

### Opening the system file manager

**File manager** under **Open in** is available for every project in all the project-menu placements above,
independently of whether its project pane is visible, which mode it shows, and whether the directory is a Git repository. It opens
the project's directory on the machine running the daemon, including when the client is a browser on another machine.
It changes no session selection or pane ownership. Success raises no toast; a refusal raises an error alert with the
reason.

On macOS this asks Finder to open the directory through the system directory opener. The request waits for that
launcher to finish, for at most 15 seconds, with at most 16 KiB each of standard output and standard error. It does
not wait for the Finder window to close.

On Linux it uses `xdg-open`, looked up in the user's login-shell environment. The launcher is detached, and the
request succeeds once it has been started. It does not wait for the launcher or file-manager window to close; a later
launcher failure is not reported to the client.

A project that no longer exists is refused with `unknown_project`. Otherwise the request is refused with
`directory_unavailable` when its directory is missing, is not a directory, or has a relative path; when the operating
system is neither macOS nor Linux; when the Linux launcher or its login-shell environment cannot be obtained; or when
the process cannot be started. On macOS the request also fails if the launcher exceeds its time or output bound or
exits unsuccessfully. The refusal carries the reason as `detail`.

## Session rows

A session row shows the session's status glyph (see "Session statuses" in `docs/product/sessions.md`), its agent's icon,
its displayed title (see "Displayed names" in `docs/product/session-titles.md`), and its pin button when it is pinned
(see "Pinning" below). Outside focus mode, every console and project session row, nested sessions included, shows
native token usage immediately after its title when the total is positive (see "Native session usage" in
`docs/product/session-usage.md`). The title clips and fades when space is short, keeping the usage chip visible.
The row's accessible description carries the usage counts. A project session bound to a console session also
carries that console session's **binding badge**: a small dot in the console session's colour, whose tooltip names it
(see "The binding badge" below); one bound to a project session carries none, since its row is listed under that
session instead (see "Project rows" above), with the owner named in the row's own accessible name. A console session's
own row carries no *binding* badge — what it shows there instead is its own colour (see "The binding badge" below).
Selecting a row resumes it when it is interrupted; no menu item does that. The row shows its agent as an icon alone;
wherever the agent is named in visible session metadata, its name appears without an account suffix. The agent's
icon names the agent and the account the session runs under in its tooltip, the account in parentheses after the
agent ("Claude Code (Work)"): the account's name, **Default** for the agent's default account, or, for a removed
account, the directory the session recorded. A removed account that recorded no directory leaves the agent's name
alone. The tooltip shows on pointer hover only, and the icon adds no tab stop. It is also available on focus mode's
session cards and archived rows (see `docs/product/focus-mode.md`), the archive view's rows (see "The archive view" below),
and sessions listed in a confirmation. Session rows, focus mode cards and the archive view's rows retain the account in
their accessible names; confirmations provide the agent and account as hidden accessible text beside the icon. Its
action menu offers:

- **Lock** / **Unlock** — see "Session lock controls" below.
- **Pin** / **Unpin**.
- **Rename** — see "Renaming a session" in `docs/product/sessions.md`.
- **Copy session ID** — immediately after Rename, copies the session's ID exactly to the clipboard. Available for
  project and console sessions wherever their action menus are offered, including right-click menus, focus mode and
  nested sessions. An archived session's menu offers it after Reopen. When the platform offers no clipboard, the item
  is absent. Success raises a notice toast; a refused copy raises an error alert.
- **Colour** — a console session's row only: a submenu of swatches where the user chooses the console session's colour
  (see "The binding badge" below). A project session's row has no such entry, and neither has an archived console
  session's.
- **Focus mode** — a console session's row only: enters that console session's focus mode (see
  `docs/product/focus-mode.md`).
  A project session's row has no such entry, and neither has an archived console session's.
- **Switch account** — a submenu of the accounts of the session's agent, the default account first, with the one the
  session is on checked and doing nothing; picking another asks for confirmation, then moves the session to it (see
  `docs/product/switching-session-accounts.md`). Its last entry, **Manage accounts…**, opens Settings
  at the Agent accounts section. A session whose account has since been removed still shows it, checked and named by
  the directory the session recorded. The entry is present for a console session's row as well as a project session's,
  and only when the session's agent has more than one account to be on; with just the default account it is left out
  rather than shown disabled. A session in the archive has no such entry, and neither has one still starting.
- **Hand off** — opens the handoff dialog, which moves the session's place to a new session of another agent (see
  "Handing off from the window" in `docs/product/handing-off-sessions.md`). Present for a project session's and a
  console session's row; an archived session has no such entry. Disabled while the session is *working* or
  *starting*, and while it is locked.
- **Archive** — archives the session (see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`). A
  session at its prompt or interrupted, with no unarchived session under it, is archived at once, unless a branch or
  worktree bound to it needs the user's word (see "The confirmation" in `docs/product/branch-and-worktree-cleanup.md`);
  any other asks for confirmation first, and so does one still being resumed. When the archived row takes keyboard focus with it, focus
  goes to the terminal. For a console session this is the user's only way to archive it — the console session can
  also archive itself once nothing under it is unarchived (see "A session archiving itself" in
  `docs/product/hub-orchestration.md`); its confirmation lists the
  interrupted sessions under it — bound to it, or to a lead session bound to it (see "Lead sessions" in
  `docs/product/hub-orchestration.md`) — under a warning with their number, as the ones archived with it, and the
  archive is refused, with the sessions named, while any session under it has a process running. A project session that
  has started sessions is archived under the same rule, with the same confirmation while any of them is unarchived.
- **Delete**, in place of Archive for a session that has never been used and is not still starting (what counts as
  unused is in "Deleting an unused session" in `docs/product/deleting-sessions.md`); it stops the session and deletes
  it outright, leaving nothing in the archive. Deletion needs no confirmation and automatically moves its agent records
  to the Trash, as described in that section.

Archive and Delete menu items are disabled while the session is locked.

**The Delete or Archive button.** Just before its action-menu button, an unlocked session row — a console
session's included — carries a **Delete** button while the session is unused and not still starting (see "Deleting an unused session" in
`docs/product/deleting-sessions.md`). Agent records do not affect its availability. Being archived does not qualify
a session for the shortcut. Pressing it takes the same deletion action as the menu, without confirmation and with
automatic cleanup of its agent records; deletion is refused if the session is no longer unused when it takes place.
A row whose menu offers **Archive** instead carries an Archive button while archiving would go through without a
confirmation: the session is at its prompt or interrupted, no unarchived session is bound to it or to a session
bound to it, and it has been used. Pressing Archive does what the menu's **Archive** does — a session still being
resumed asks for confirmation first there too, and so does one with a bound branch or worktree that needs the user's
word — and when the row leaves with keyboard focus, focus goes to the terminal.
When the Archive conditions stop holding — the session starts working or raises a hand, or a session becomes bound
to it — the button goes away.

A pinned session's pin button stays before the shortcut. A mouse press on either shortcut neither selects the row
nor moves keyboard focus. Its tooltip is "Delete" or "Archive", while the name assistive technology announces for it
also names the session ("Delete session …" or "Archive session …"). It shows with the row's other controls (see "Rows,
names and keyboard focus" below). When a shortcut goes away while holding keyboard focus, focus goes to the row.

## The binding badge

A project session **bound to a console session** — one carrying the id of the console session it reports to (see
"Console sessions and project sessions" in `docs/product/sessions.md`) — carries a small dot in that console session's
colour on its sidebar row. A **lead session** carries it like any other session
bound to a console session (see "Lead sessions" in `docs/product/hub-orchestration.md`). On sidebar rows an unbound
session carries no badge, and neither does a session bound to a project session, which is listed under its owner
instead (see "Project
rows" above). The badge's tooltip names the owning console session; the badge is no tab stop, so the tooltip shows on
pointer hover only. The row's own accessible name carries the same fact in words for assistive technology ("Bound
to …"), in the same wording for a project session's owner as for a console session's, which is the only place the
nesting is put into words. The badge itself still carries no information its tooltip does not, so colour alone never
distinguishes two owners for a user who cannot tell the colours apart. A console session's own row shows the same
colour, decoratively, since the row's own label already names it, as do its own card in its focus mode, its chip in
a project's focus mode and its chip in the switch strip. Cards in a project's combined focus view carry the dot for
the console session above them, including cards inset under a lead session; the tooltip and accessible name identify
that console session. The other focus views carry no binding badge on their cards. See "A project's focus mode" in
`docs/product/focus-mode.md` for the switch and the card labels.

**Where the colour comes from.** A console session's colour is one of a fixed palette of 16, one colour per family —
red, orange, amber, yellow, lime, green, teal, cyan, sky, blue, indigo, violet, fuchsia, pink, brown and gray — each a
vivid colour that looks like its name. Some of them lie near the colours that mark a status elsewhere; that is
deliberate, since a console session's colour says nothing about its status, which its status glyph does. No colour is
ever named in the interface. Octoboard gives each new console session a colour picked at random among the palette
colours that none of the console's other console sessions that are not archived holds, or at random among all 16 once
every one is held; an archived console session's colour is free for others. Afterwards only the user changes it (see
**Choosing the colour** below); the console session's agent has no tool for it. A chosen colour counts as held for a
later console session's colour as an assigned one does. A console session keeps its colour, assigned or chosen, when
it is archived and when it is resumed, so with more than 16 at once, with one resumed after its colour has gone to
another, or with a colour chosen that another already has, two console sessions of one console can share a colour,
which is the other reason nothing is ever told apart by colour alone. A colour stored by an earlier version of
Octoboard is moved onto the nearest family of this palette when the daemon opens its store — olive to lime, jade to
green, azure to sky and rose to pink, while teal and violet stay as they are — so such a console session keeps much
the same colour. Each colour has its own light and dark value (see "What follows the choice" in
`docs/product/appearance.md`).

**Choosing the colour.** **Colour** in a console session's action menu — wherever that menu is offered, its sidebar row
and its own card in its focus mode among them, after **Rename** and the optional **Copy session ID** — is a submenu
entry, marked with the same chevron as **Switch account**. Its submenu holds the palette as swatches in two rows of
eight, in the order above. The console session's current colour is selected, marked with a check mark and a ring in its
own colour. Pointing at **Colour** opens the submenu without moving keyboard focus; `ArrowRight` (`ArrowLeft` under a
right-to-left language) or `Enter` on the entry opens it, or moves into it when it is already open, with focus on the
current swatch, or the first one when the current colour is not in the palette. Inside it the arrow keys move focus
between swatches; `ArrowLeft` from a swatch in the first column (`ArrowRight` under a right-to-left language), or
`Escape`, closes the submenu and puts focus back on **Colour** in the menu, which stays open. Clicking a swatch, or
pressing `Enter` or `Space` on the focused one, gives the console session that colour at once and closes the whole menu;
choosing the current colour just closes it. A click outside closes the menu without a change. When the menu closes,
keyboard focus goes where closing it always puts it (see "Rows, names and keyboard focus" below). For assistive
technology the swatches are a list named by the **Colour** entry, and each swatch is announced by the colour description
generated in the interface language; the palette keeps those descriptions distinct in every offered language and both
appearances, except that in Arabic violet and fuchsia share one, and in Korean teal shares one with cyan in the light
appearance and with sky in the dark one. The colour is stored by the daemon with the console session, so every window
and every client sees it and it survives a restart. A change that fails is shown as an error alert, as a failed **Pin**
is (see "Error alerts" in `docs/product/error-alerts.md`). An archived console session's menu has no **Colour** entry,
and a project session has no colour to choose.

## Rows, names and keyboard focus

- A name too long for its row fades out where it ends — the row's right edge, its left under a right-to-left language —
  rather than ending in an ellipsis, the full name is then the label's tooltip, and pointing at the name runs it as a
  marquee (see "Names too long for their space" in `docs/product/labels-and-tooltips.md`). When the sidebar's
  content is taller than the sidebar it scrolls, fading at the end that has more beyond it (see "The edge that hides
  content fades" in `docs/product/scrolling-surfaces.md`).
- A row's **+**, Delete or Archive (see "Session rows" above) and action-menu buttons show while the row is hovered, holds keyboard focus, is the selected session,
  or has its menu open; they are still reached with Tab when hidden, and show once one of them has focus. They fade
  and open out as they appear, and fade and close up as they go, briefly — at once where the system asks for reduced
  motion.
- **Right-clicking** (or Control-clicking on macOS) a row that has an action menu opens that menu at the pointer, with
  the same entries as its action-menu button. This holds for a project row, a session row (a console session's
  included), the sidebar's header, a console's avatar on the rail (see "The console switcher" above), a project's focus
  mode header, a project's heading in a console session's focus mode, a session card (a console session's own card in
  its focus mode included) and a focus mode's archived rows; a console session's focus mode header has no menu to
  open. The right-click neither selects nor activates the row, nor takes keyboard focus; right-clicking another such
  row while a menu is open closes it and opens that row's. It works the
  same in the desktop app and in a browser, where the browser's own menu is not shown over these rows; what a
  right-click shows anywhere else is in "Right-click menus" in `docs/product/window-layout.md`.
- Closing an action menu that was opened with the pointer, by its button or by a right-click, gives keyboard focus back
  to whatever had it before, typically the terminal; one opened from the keyboard gives it back to its button. An
  entry that opens a dialog leaves focus in the dialog.
- Clicking a row deliberately does not move keyboard focus away from the terminal; a row reached with Tab can be
  activated with Enter or Space.
- Each row tells assistive technology what its icons show: a session row its title, agent (Claude Code, Codex or Grok
  Build) and the account it runs under, status, whether it is pinned, and, for a bound project session, the session it
  reports to — the console session the binding badge shows, or the project session its row is listed under, both in the
  same wording (see "The binding badge" above); a project row whether it is pinned and, while it carries an activity
  marker, what that marker means, plus its branch badge's facts (see "The branch badge" in
  `docs/product/project-git-status.md`). A project row also tells it whether it is expanded or collapsed.

## Order of projects and sessions

**Projects** are listed:

1. projects with at least one unarchived session first, including interrupted sessions;
2. within each group, pinned projects first;
3. then a project with a session waiting for the user;
4. then a project with a session working, a session running in background included;
5. then a project with a session running and awaiting instructions, or still starting;
6. then a project whose unarchived sessions are all interrupted;
7. within each of those, by name, case-insensitively and in natural order, so that "item 2" comes before "item 10".

Each rule applies only among projects the rules before it leave tied. A project with an unarchived session comes
before even a pinned project with none.

**Inactive** projects have no unarchived session. They end the list and, pinned ones included, are drawn dimmed as
one group. Projects with only interrupted sessions remain at full strength. Pointing at any inactive project,
keyboard focus inside one, or an open menu in one brings the whole inactive group back to full strength while it lasts.

**Sessions** within a project are listed, by the same scheme:

1. pinned sessions first;
2. then by status: waiting for the user, then awaiting instructions and starting alike, then working and running in
   background alike, then interrupted;
3. then the most recently started first.

The console sessions section lists its console sessions by that same session scheme.

These three rules apply **within one level** of the project's list: the sessions the project lists at its own level are
ranked against each other, and the sessions under one owner against each other, each group following its owner wherever
the rules put it (see "Project rows" above). So a pinned session comes first among the sessions beside it, not first in
the whole project, and a session under an owner is never ranked against that owner. Everything walked in this order
follows the nesting the same way, so a session bound to a project session comes after its owner wherever both appear.

The order follows status changes as they happen. Row movement follows "Items moving within a collection" in
`docs/product/collection-motion.md`. The rail's waiting count walks waiting sessions in this same order (see
"The raised hand" in `docs/product/sessions.md`).

### Pinning

**Pinning** is the user's own mark on a project or a session, set and cleared from its action menu's **Pin** /
**Unpin**, and cleared by its pin button too; nothing is pinned to begin with. Its only effect is the order above. The
pin is stored by the daemon with the project or the session, so every window and every client sees the same pins, and
they survive a restart. A pinned session stays pinned when it is archived and when it is resumed; an archived
session's menu offers no Pin or Unpin. A focus mode offers no Pin or Unpin for what is in focus, neither in a
project's focus mode header nor on a console session's own card in its focus mode (see `docs/product/focus-mode.md`).

**The pin button.** A pinned project or session shows a pin button: on its row in the sidebar, after the row's markers
(a project's branch badge and activity marker, a session's binding badge) and before the buttons that show only on
hover, so that while those are hidden the pin buttons of project and session rows line up in one column at the rows'
end; after the name on a project's heading in a console session's focus mode; and on the status line of its card in a
focus mode, except a console session's own card in its focus mode, which never shows one. It is always
shown while the item is pinned, and is reached with Tab like the row's other buttons. While the pointer is over it or it
holds keyboard focus, its pin glyph turns into the unpin glyph. Its tooltip is "Unpin", while the name assistive
technology announces for it also names the item ("Unpin …"). Pressing it unpins the item, as the menu's Unpin does; a
mouse press neither selects the row nor moves keyboard focus. The button goes away with the pin, so when it held
keyboard focus, focus is handed first, with its focus ring showing, to the row or card it was on — or, on a project's
heading in a console session's focus mode, to that heading's **+** button.

## Session lock controls

Lock controls are offered only for sessions that are not archived. Their action menus start with **Lock** or **Unlock**,
including nested sessions and a console session's own card in its focus mode. Protection and persistence follow "Locking
a session" in `docs/product/sessions.md`.

A locked session has a persistent yellow lock button before its other action controls, ahead of its pin
button when there is one. It remains visible without hover on sidebar rows and focus cards, a console session's own
card in its focus mode included. Pointer hover or visible keyboard focus changes the closed lock
to an open lock. Its tooltip offers unlocking, and its accessible name also identifies the session. Pressing it
unlocks that session; a mouse press neither selects the session nor moves keyboard focus. When the disappearing
button holds keyboard focus, focus goes to its row or card with a visible focus ring.

Archived sessions have no Lock or Unlock action or lock button, in either the full archive or
focus mode's compact preview. A stored lock remains effective: Delete is disabled for a locked archived session,
and the full archive's Delete all button is disabled while a listed session is locked. Reopen that session to reach
its live Unlock control. A lock among descendants outside the list still refuses deletion under the rules of
"Locking a session" in `docs/product/sessions.md`.

## Selecting a session

Selecting a session shows its terminal; selecting an interrupted one resumes it, except by `Ctrl+Tab` or
`Ctrl+Shift+Tab` in a console session's focus mode, which only shows an interrupted console session (see "The switch
strip" in `docs/product/focus-mode.md`). Selecting an archived one does not reopen it: it is shown still archived,
with its saved terminal preview read-only, and typing into its terminal or pressing Reopen reopens it
(see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md` and "Interrupted, archived and dormant
sessions" in `docs/product/session-terminal.md`). Selecting a session also
puts its own pane in the window's right pane: a console session's report panel, a project session's project pane (see
"What the right pane shows" in `docs/product/window-layout.md`). Whatever it is selected from — a sidebar row, a "View
archive" or "Archived console sessions" submenu, focus mode and its switch strip, the archive view, the rail's waiting
count, or the menu bar icon's menu (see "Choosing a session" in `docs/product/menu-bar-icon.md`) — **the sidebar
follows it**: it switches to the session's console, showing the session selected rather than the one remembered for
that console (see "What switching console shows" above), and leaves focus mode when the session does not belong to what
focus mode is on (see "Leaving focus mode, and what is remembered" in `docs/product/focus-mode.md`). Back and Forward
return to earlier selections without resuming anything (see `docs/product/navigation-history.md`).

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

## The archive view

**View all** — in a project's "View archive" submenu, in a focus mode's archived list, or in a console's
"Archived console sessions" submenu — opens the archive view: **every** archived session of that project,
every archived session under that console session (bound to it, or bound to an archived lead session that is bound to
it), or every archived console session of that console. A lead session's own archived sessions are in that console
session's archive only while the lead session is archived too; while the lead session is still live they are in its
project's archive alone. The submenus offer View all whenever their archive is nonempty, so the archive's
reopen and deletion controls remain reachable even when every session fits in the submenu. An archive containing
only children shows View all alone in its submenu; the disabled empty item is shown only when the archive itself
is empty. Focus mode's preview offers View all whenever it omits any archived session (see "What both views show"
in `docs/product/focus-mode.md`).

**Archive previews** show at most five archived parent sessions in those submenus and at most ten in either focus
mode, most recently archived first. A session bound to a project session is a child and is omitted, whether its owner is
live or archived. Unbound sessions and sessions bound directly to a console session are parent sessions; a session
whose owner is no longer known is included as well. Children remain available in the full archive view, whose
scope and nesting are described below.

- It **covers the terminal's area only**: the sidebar and the right pane stay as they are.
  The terminal goes on running beneath it and is not resized.
- Its header says how many archived sessions there are, over the title "Archived sessions" (for console sessions,
  "Archived console sessions"; for a console session's bound sessions, "Archived bound sessions"), with **Delete all**
  while there is at least one, and a **Close** button. Which project, console session or console it is for is shown in
  the top bar's breadcrumb (see "The top bar" in `docs/product/window-layout.md`).
- The list is in the order the sessions were archived, most recent first. It shows 30 rows and adds 30 more each time
  its end is scrolled into view. Each row shows the session's agent's icon, its title, and its agent's name,
  followed by native token usage when its total is positive (see "Native session usage" in
  `docs/product/session-usage.md`). At the trailing end, a metadata block shows only dates: the archive date first,
  when known, then the creation date, each identified by its meaning. These are absolute dates with year, month, day,
  hour and minute, formatted in the current language and the client's local time zone. An unknown archive date is
  omitted; the creation date never stands in for it. Archive previews show no dates: submenu previews keep their compact title-only presentation, and focus
  mode's preview rows are described in "What both views show" in `docs/product/focus-mode.md`. The date block sits
  flush at the row's end in the reading direction, on the right in a left-to-right language. The row's **Reopen** and
  **Delete** buttons show only while the row is hovered or holds keyboard focus, and are still reached with Tab.
  Hidden, they take no room, so a resting row's title runs up to the date block; shown, they appear just before the
  date block (to its left in a left-to-right language), which stays at the row's end, and the title gives up room for
  them, cut off if it must be. On a pane too narrow for one line, the date block moves onto a line of its own, still at
  the row's end; while the row is hovered or focused the buttons join it there, so such a row grows by a line when its
  buttons appear. Reopen and Delete have filled backgrounds distinct from the hovered row; Delete uses a soft danger fill.
- A session whose owner is **a project session listed here too** — an archived session of an archived lead session, or
  of any archived project session that had started sessions — is **inset under that owner**, with rounded connectors
  as in "Project rows" above, and follows it instead of being ranked against the rows beside it; one whose owner is
  not in the list is listed on its own. The row's accessible name also names the owner, in the wording a
  session row uses (see "The binding badge" above). The 30 rows are counted as rows, so an inset row is never shown before the row it is
  inset under.
- **Selecting a row** — clicking it, or `Enter` or `Space` on it — selects its session, which closes the view (see
  "Selecting a session" above): the session is shown still archived, its saved terminal preview read-only behind the
  card that offers Reopen, just as when it is selected from a preview. Selecting it does not reopen it. Pressing the
  row's Reopen or Delete does not select the row. The row of the selected session is marked as selected.
- **Reopen** resumes the session and selects it, which closes the view (see "Selecting a session" above).
- **Delete** deletes that session, with the confirmation rules in "Deleting archived sessions" in
  `docs/product/deleting-sessions.md` — for an archived owner, listing the archived sessions
  under it that are deleted with it; **Delete all** asks for confirmation, naming how many, then deletes every archived
  session in the view's scope — for a console session's archive, the archived sessions under it and not the console
  session itself; for a console's archived console sessions it also says how many archived sessions under them are
  deleted with them. What deleting does and does not remove is in "Deleting archived sessions" in
  `docs/product/deleting-sessions.md`.
- With nothing archived — including after everything has been deleted — the view stays open and says there are no
  archived sessions.

Archive rows, including inset sessions, follow "Items moving within a collection" in
`docs/product/collection-motion.md`.

**It closes** on its Close button, on `Escape` while keyboard focus is in it, when a session is selected (from it or
from anywhere else, as in "Selecting a session" above), when this window opens a new session, when another console is
made the current one (see "What switching console shows" above), and by itself when its
project, its console session or its console no longer exists. Closing it hands keyboard focus to the terminal.

The view slides up from below on opening and slides down on closing, each over 240 ms. Closing takes effect at
once: selection and navigation do not wait for the animation, and the departing view no longer accepts input or
keyboard focus. With reduced motion requested by the system, the view opens and closes without animation.

It takes keyboard focus as it opens, so keystrokes stop reaching the terminal it covers, and it keeps focus after a
deletion. It is a region of its own in the `F6` cycle (see "Moving focus between regions with F6" in
`docs/product/moving-focus-between-regions.md`). Below 1168 px, where the sidebar is a drawer, opening the archive
view closes the sidebar's drawer so that the view is not hidden behind it.
