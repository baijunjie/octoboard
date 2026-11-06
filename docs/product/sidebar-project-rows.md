# Project rows

A project row lists one of the console's projects under the sidebar's Projects heading (see "The console sessions
section and the project list" in `docs/product/sidebar.md`).

A project row shows the project's name and a chevron after the name. Clicking the row, or Enter or Space on it,
collapses or expands it. A project starts collapsed unless it has at least one unarchived session, including an
interrupted session; a project with only archived sessions starts collapsed. This default is applied when the project
first appears in the window, including projects added later. After that, the project expands or collapses on its own
only when whether it has an unarchived session changes: gaining one while it had none — a session opened in it or
reopened from the archive — expands it, and losing its last one — archived or deleted — collapses it. Any other session
change leaves its state, including one set by hand, as it is. This holds in whichever console is shown, and while a
focus mode replaces the list — a console session's focus mode has no fold control of its own — so a session opened there
in a project that had none leaves the project expanded on returning to the list. The state is kept per window across
console switches and entering or leaving focus mode, and is not stored: reloading the window applies the default again.
The chevron always shows while the project is collapsed, and only while the row is hovered or focused while it is
expanded, and while hidden it takes no room, so a long name runs as far as it would in a session row and ends sooner
when the chevron appears. The hover chevron fades and opens out as it appears, and back as it goes, briefly — at once
where the system asks for reduced motion.

After the name, before the status marker below and the row's controls, comes the project's **branch badge** — its
current branch and how far it is from its upstream, when its directory is a git repository, led by a failure mark when
its last check failed (see "The branch badge" in `docs/product/project-git-status.md`). That mark is a button and a tab
stop of its own on the row, and pressing it does not fold the project; the row's own name adds that the Git check failed
while it shows.

A collapsed project shows a **status marker** for its sessions that are not archived, so a waiting session can be found
with its project collapsed: the glyph, at the compact 14 px size, of their top status — the status of the one that
comes first in the status order (see "The top status" in `docs/product/session-statuses.md`). Every status has its own
glyph there, interrupted, starting and switching account included, so a project whose unarchived sessions are all
interrupted shows the pause sign; a project with no unarchived session shows none. The glyphs follow "Status glyphs" in
`docs/product/session-statuses.md`. **The marker carries a tooltip** saying that status in the same words as everywhere
else it is said (see "Tooltips on indicators" in `docs/product/labels-and-tooltips.md`). The row's accessible name says
it too, expanded or collapsed: the project's name followed by its top status, with "pinned" between them for a pinned
project.

At the row's end are its pin button while the project is pinned (see "Pinning" in
`docs/product/order-of-projects-and-sessions.md`), then a **+** button, **New session**, which opens the new-session
dialog for that project (see "The session dialog" in `docs/product/opening-sessions.md`) — or, with the primary modifier
held, opens the session at once without it (see "Opening a session with defaults" in `docs/product/opening-sessions.md`)
— and the project's action menu. The same menu is the focused project's, and a project's heading in a console session's
focus mode, apart from the items those views leave out (see "What both views show" and "A console session's focus mode"
in `docs/product/focus-mode.md`). Choosing an action closes the menu at once; submenu entries open their next menu.
Separators split it into four groups, in this order: the first holds **Pin** / **Unpin**, as in a session's menu, and
the actions for viewing the project inside the app (**Focus mode**, **View archive**); then the **Copy**, **Open in**,
**Open panel** and **Git** submenus, in that order; managing the project itself (**Rename**, **Project settings**); and
**Remove project**. An item a view does not offer is left out without moving the others.

- **Pin** / **Unpin** — see "Pinning" in `docs/product/order-of-projects-and-sessions.md`.
- **Focus mode** — see `docs/product/focus-mode.md`.
- **View archive** — a submenu of the project's archive preview (see "The archive view" in
  `docs/product/archive-view.md` for what it lists and how each item looks); choosing one selects it (see "Selecting a
  session" in `docs/product/sidebar.md`). After them, **View all**, without a count, opens the archive view whenever the
  project has an archived session, including when all of them fit in the submenu. A project with no archived session
  shows only a disabled item saying so. On a project's heading in a console session's focus mode, the submenu covers
  only the project's share of that console session's archive, and its View all opens the console session's archive view
  instead (see "A console session's focus mode" in `docs/product/focus-mode.md`).
- **Copy** — a submenu with **Copy project name**, which copies the project's name exactly, then **Copy absolute
  path**, which copies its resolved directory path on the daemon's host. Available in all the project-menu placements
  above, including right-click menus. The directory is looked up when the submenu opens; if it cannot be resolved, the
  absolute-path item is disabled with a reason, while copying the name remains available. Reopening the submenu tries
  the lookup again. Where the platform has no clipboard, or it refuses, either value is copied through the browser's
  selection copy instead (see "Copy buttons" in `docs/product/labels-and-tooltips.md`). Success raises a notice; a copy
  neither way allows raises an error alert.
- **Open in** — a submenu with **File manager**, then **System terminal**, then the project's repository host when
  available. File manager opens the project's directory on the daemon's host (see "Opening the system file manager"
  below). System terminal opens the operating system's terminal application at that directory (see "Opening a project
  terminal" in `docs/product/project-terminal.md`). Both are offered for every project, including one that is not a
  git repository and one whose git status has not arrived. Neither raises a success notice; a failure is an error alert.
  The repository entry opens the recorded remote's web address in the client's browser and carries a trailing external-link icon.
  Public GitHub, GitLab and Bitbucket hosts carry their brand names and icons; other hosts carry their hostname and a
  globe icon, including self-hosted installations. A missing, invalid, local or unsupported remote, or a client unable
  to open web links, leaves out the repository entry. HTTP and HTTPS remotes retain their web scheme; SSH, SCP-style,
  `git://`, `git+ssh://` and `ssh+git://` remotes use HTTPS. Credentials, query strings, fragments and a trailing `.git`
  are omitted from the opened address, as are transport ports on non-web remotes. Opening raises no success notice; a
  failure is an error alert.
- **Open panel** — a submenu with **Files**, then **Processes**, then **Terminal**, which open that panel of the
  project's right pane inside the window; an entry whose panel is already on screen for the project is left out, and so
  is the submenu when all are (see "Opening a panel from a project's action menu" in
  `docs/product/right-pane-panels.md`).
- **Git** — a submenu, only while the project's status says its directory is a git repository, so not before the first
  status and not when the directory is not one (see "What one check does" in `docs/product/project-git-status.md`). It
  holds **Pull current branch**, then **Push current branch**, then **Switch branch**, then, after a separator, the
  destructive **Restore file changes** action. Pull current branch checks the project against its remote and
  fast-forwards it right now, is not waited on, and raises no notice when it succeeds (see "Pulling one project by hand"
  in `docs/product/project-git-status.md`). Push current branch pushes that branch and is waited on until the push
  finishes; the window then shows a notice that the current branch was pushed, or an error alert if the push was refused
  (see "Pushing the current branch" in `docs/product/project-git-actions.md`). Switch branch opens the third-level local
  branch menu; see "Switching the local branch" in `docs/product/project-git-actions.md`. Restore file changes asks for
  confirmation before discarding tracked changes (see "Restoring file changes" in
  `docs/product/project-git-actions.md`).
- **Rename** — a dialog with the name alone; an empty name is rejected.
- **Project settings** — the project dialog, see "Editing a project" in `docs/product/projects.md`.
- **Remove project** — see `docs/product/removing-projects.md`; it asks for confirmation first.

An expanded project lists its sessions that are not archived as session rows, with a connector bending smoothly toward
the middle of each child row and stopping at the last child. With none, it shows an empty row saying there are no
sessions yet; the whole row is a button that opens the new-session dialog for that project, as the row's **+** button
does (see "Empty rows" in `docs/product/sidebar.md`). With no agent both added and installed, the row's **+** is
disabled and the empty row is plain text.

A session **bound to another of the project's sessions** — one that session started, a coordinator's own sessions
included (see "Coordinators with a parent session" in `docs/product/hub-orchestration.md`) — is listed **under that
session**, one level deeper, inset with the same rounded connectors toward each child row. Independent project
coordinators and sessions bound to a console session stay at the project's own level, so a project's sessions go at most
two levels deep. The nesting has no fold control of its own: the sessions under an owner fold away with their project,
like every other session row.

## Project menu keyboard shortcuts

A project's action menu offers these letters on its top-level actions, in its row, its focus header and its heading
inside a console session's focus mode, wherever the corresponding item is offered. They act on the project the menu
belongs to under "Menu keyboard shortcuts" in `docs/product/keyboard-shortcuts.md`.

| Action | Letter |
|---|---|
| Pin / Unpin | P |
| Focus mode | F |
| Rename | R |
| Project settings | E |
| Remove project | D |

The focus header leaves out Pin / Unpin and Focus mode; a heading inside a console session's focus mode leaves out
Focus mode. View archive, Copy, Open in, Open panel and Git are submenus and have no temporary letter shortcuts.

## Opening the system file manager

**File manager** under **Open in** is available for every project in all the project-menu placements above,
independently of whether its project pane is visible, which mode it shows, and whether the directory is a Git
repository. It opens the project's directory on the machine running the daemon, including when the client is a browser
on another machine. It changes no session selection or pane ownership. Success raises no notice; a refusal raises an
error alert with the reason.

On macOS this asks Finder to open the directory through the system directory opener. The request waits for that launcher
to finish, for at most 15 seconds, with at most 16 KiB each of standard output and standard error. It does not wait for
the Finder window to close.

On Linux it uses `xdg-open`, looked up in the user's login-shell environment. The launcher is detached, and the request
succeeds once it has been started. It does not wait for the launcher or file-manager window to close; a later launcher
failure is not reported to the client.

A project that no longer exists is refused with `unknown_project`. Otherwise the request is refused with
`directory_unavailable` when its directory is missing, is not a directory, or has a relative path; when the operating
system is neither macOS nor Linux; when the Linux launcher or its login-shell environment cannot be obtained; or when
the process cannot be started. On macOS the request also fails if the launcher exceeds its time or output bound or exits
unsuccessfully. The refusal carries the reason as `detail`.
