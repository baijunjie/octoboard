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

After the name, before the activity marker below and the row's controls, comes the project's **branch badge** — its
current branch and how far it is from its upstream, when its directory is a git repository, led by a failure mark when
its last check failed (see "The branch badge" in `docs/product/project-git-status.md`). That mark is a button and a tab
stop of its own on the row, and pressing it does not fold the project; the row's own name adds that the Git check failed
while it shows.

A collapsed project shows an activity marker taken from its sessions that are not archived, so a waiting session can
be found with its project collapsed. It shows a waving hand while any session is waiting for the user; otherwise the
working glyph while any is working or working in background; otherwise a green speech bubble while any is awaiting
instructions or starting; otherwise nothing. The glyphs follow "Session statuses" in `docs/product/session-statuses.md`,
with background work counted as working. **The marker carries a tooltip** saying which activity it stands for, in the
same words the row's own name folds in beside the project's name (see "Tooltips on indicators" in
`docs/product/labels-and-tooltips.md`).

At the row's end are its pin button while the project is pinned (see "Pinning" in
`docs/product/order-of-projects-and-sessions.md`), then a **+** button, **New session**, which opens the new-session
dialog for that project (see "The session dialog" in `docs/product/opening-sessions.md`) — or, with the primary modifier
held, opens the session at once without it (see "Opening a session with defaults" in `docs/product/opening-sessions.md`)
— and the project's action menu. The same menu is the focused project's, and a project's heading in a console session's
focus mode, apart from the items those views leave out (see "What both views show" and "A console session's focus mode"
in `docs/product/focus-mode.md`). Choosing an action closes the menu at once; submenu entries open their next menu.
Separators split it into four groups, in this order: the first holds **Pin** / **Unpin**, as in a session's menu, and
the actions for viewing the project inside the app (**Focus mode**, **View archive**); then the **Open panel**, **Open
in** and **Git** submenus; managing the project itself (**Rename**, **Copy project name**, **Project settings**); and
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
- **Open panel** — a submenu with **Files**, then **Processes**, then **Terminal**, which open that panel of the
  project's right pane inside the window; an entry whose panel is already on screen for the project is left out, and so
  is the submenu when all are (see "Opening a panel from a project's action menu" in
  `docs/product/right-pane-panels.md`).
- **Open in** — a submenu with **File manager**, then **System terminal**, then the project's repository host when
  available. File manager opens the project's directory on the daemon's host (see "Opening the system file manager"
  below). System terminal opens the operating system's terminal application at that directory (see "Opening a project
  terminal" in `docs/product/project-terminal.md`). Both are offered for every project, including one that is not a
  git repository and one whose git status has not arrived. Neither raises a success toast; a failure is an error alert.
  The repository entry opens the recorded remote's web address in the client's browser and carries a trailing external-link icon.
  Public GitHub, GitLab and Bitbucket hosts carry their brand names and icons; other hosts carry their hostname and a
  globe icon, including self-hosted installations. A missing, invalid, local or unsupported remote, or a client unable
  to open web links, leaves out the repository entry. HTTP and HTTPS remotes retain their web scheme; SSH, SCP-style,
  `git://`, `git+ssh://` and `ssh+git://` remotes use HTTPS. Credentials, query strings, fragments and a trailing `.git`
  are omitted from the opened address, as are transport ports on non-web remotes. Opening raises no success toast; a
  failure is an error alert.
- **Git** — a submenu, only while the project's status says its directory is a git repository, so not before the first
  status and not when the directory is not one (see "What one check does" in `docs/product/project-git-status.md`). It
  holds **Pull current branch**, then **Push current branch**, then **Switch branch**, then, after a separator, the
  destructive **Restore file changes** action. Pull current branch checks the project against its remote and
  fast-forwards it right now, is not waited on, and toasts nothing when it succeeds (see "Pulling one project by hand"
  in `docs/product/project-git-status.md`). Push current branch pushes that branch and is waited on until the push
  finishes; the window then shows a notice that the current branch was pushed, or an error alert if the push was refused
  (see "Pushing the current branch" in `docs/product/project-git-actions.md`). Switch branch opens the third-level local
  branch menu; see "Switching the local branch" in `docs/product/project-git-actions.md`. Restore file changes asks for
  confirmation before discarding tracked changes (see "Restoring file changes" in
  `docs/product/project-git-actions.md`).
- **Rename** — a dialog with the name alone; an empty name is rejected.
- **Copy project name** — copies the project's name exactly to the clipboard. Available in all the project-menu
  placements above, including right-click menus. Where the platform has no clipboard, or it refuses, the name is copied
  through the browser's selection copy instead (see "Copy buttons" in `docs/product/labels-and-tooltips.md`). Success
  raises a notice toast; a copy neither way allows raises an error alert.
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

## Opening the system file manager

**File manager** under **Open in** is available for every project in all the project-menu placements above,
independently of whether its project pane is visible, which mode it shows, and whether the directory is a Git
repository. It opens the project's directory on the machine running the daemon, including when the client is a browser
on another machine. It changes no session selection or pane ownership. Success raises no toast; a refusal raises an
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
