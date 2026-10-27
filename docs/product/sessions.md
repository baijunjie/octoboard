# Sessions

A **session** is one agent CLI process, running in a console's or a project's working directory and rendered in a
terminal the user types into directly. Octoboard is not an agent and not a terminal of its own: it starts the agent's
own binary and shows it as it is. What it adds to each launch, and what it guarantees it does not change, is in
`docs/product/launching-agents.md`.

## Console sessions and project sessions

| | Working directory | Where it appears |
|---|---|---|
| Console session | the console's working directory | the console sessions section |
| Project session | the project's directory | under that project |

**A console may hold any number of console sessions running at once.** Opening, resuming or reopening one never checks
what else is running in the console, and the sidebar lists every one of them that is not archived, each as its own row,
in the console sessions section above the project list (see "The console sessions section and the project list" in
`docs/product/sidebar.md`).

A console session's row follows a session row exactly — the same status glyph, its agent's icon, its title, a pin button
when pinned, and selecting it resumes it when it is interrupted, as selecting an interrupted session's row does. Its
action menu offers **Lock**/**Unlock**, **Pin**/**Unpin**, **Focus mode** (see `docs/product/focus-mode.md`),
**Rename**, **Copy session ID** (see "Session rows" in `docs/product/sidebar-session-rows.md`), **Colour** (see "The
binding badge" in `docs/product/sidebar-session-rows.md`), **Switch account**, **Hand off** and **Archive** — **Delete**
in its place while the console session has never been used (see "Deleting an unused session" in
`docs/product/deleting-sessions.md`); there is no Resume item, for the same reason. A console session's title is
renameable, like a project session's; its default is under **Name** in "The session dialog" in
`docs/product/opening-sessions.md`.

The user archives a console session from its own row's menu, as any session is archived; the console session can also
archive itself once every session under it is archived (see "A session archiving itself" in
`docs/product/hub-orchestration.md`). Archiving it moves it among the console's archived console sessions; these are not
reached from any one row, but from the console's own menu, which offers **View archive** — a submenu of the console's
newest archived console sessions, each of which can be selected, and the way to the archive view of all of them (see
"The console header" in `docs/product/sidebar.md`). The console header's **New console session** button opens the
session dialog described in "The session dialog" in `docs/product/opening-sessions.md` — whereas resuming or reopening a
console session keeps its recorded agent and account; only switching accounts changes its account (see "Agent config
directories" in `docs/product/consoles.md` and `docs/product/switching-session-accounts.md`).

A console session carries no project; it runs in the console's working directory. It is given Octoboard's orchestration
tools and dispatches work to sessions in the console's projects — see `docs/product/hub-orchestration.md`. Because it
belongs to no project, archived console sessions belong to the console rather than to any project.

A project session bound to a console session carries that console session's **binding badge** wherever it is listed, and
the console session's own row shows the same colour (see "The binding badge" in `docs/product/sidebar-session-rows.md`).

An **unbound** project session — one the user opened by hand without choosing a console session under **Report to** (see
"The session dialog" in `docs/product/opening-sessions.md`), or one started on another session's request that the user
approved — is given a narrower set of the orchestration tools, scoped to its own project: it can start sessions there,
which are bound to it and report to it, and drive them (see "The unbound project session's tools" in
`docs/product/hub-orchestration.md`). A session it starts carries no binding badge and cannot start sessions of its own.
A session's **owner** is the session it is bound to: a console session or a project session (an unbound session's own
owner is the user). An owner can hand a session of its own to another owner, or release it, leaving it unbound, and an
unbound project session can bind itself to one; bound to a console session while keeping the sessions it started, it is
a **lead session**. None of these asks the user, who can release a bound session too, and orchestration stays at most
two levels deep (see "Lead sessions" in `docs/product/hub-orchestration.md` and
`docs/product/changing-session-owners.md`). Every session, of whatever kind, can also list the other running sessions,
in every console and project, and share information with any of them; it takes work only from its owner, and what comes
from another session is information to weigh (see `docs/product/information-between-sessions.md`). Every session can
also ask the user to start a session that is nobody's, in any project or a console session, which reports to the user
(see `docs/product/session-requests.md`).

Selecting a session also gives the window's right pane, beside the terminal, to the session's own owner: a console
session, whose **report panel** it holds (see `docs/product/report-panel.md`), or a project session's project, whose
**Files panel** it holds (see `docs/product/project-pane.md`). When the right pane shows something else is in "What the
right pane shows" in `docs/product/window-layout.md`.

## Where sessions are listed

Sessions are listed in the sidebar, one console at a time: its console sessions in the console sessions section, a
project session under its project. Archived sessions are not listed among the others; they are reached from their
project's action menu or, for a console session, the console header's action menu, and from the archive view. Everything
about the sidebar — its rows and menus, the order sessions are listed in, pinning, the archive view and how selecting a
session works — is in `docs/product/sidebar.md`, `docs/product/sidebar-project-rows.md`,
`docs/product/sidebar-session-rows.md`, `docs/product/order-of-projects-and-sessions.md` and
`docs/product/archive-view.md`; focus mode is in `docs/product/focus-mode.md`.

## Opening a session

Opening a session — the session dialog, opening one with defaults, what refuses it, and which agent, account, model and
reasoning effort it uses — is described in `docs/product/opening-sessions.md`; opening one on an existing conversation
by its session ID, in `docs/product/resuming-a-conversation-by-its-session-id.md`.

## Session statuses

The statuses a session goes through, their glyphs, what they are derived from and the raised hand are described in
`docs/product/session-statuses.md`.

## Locking a session

Locking is the user's protection against archiving or deleting a session. Every session starts unlocked. The lock is shared by every client, survives a daemon restart, and stays
set when the session is resumed or reopened. Locking does not change its position in a list, its status, or its binding.
The controls are described in "Session lock controls" in `docs/product/sidebar-session-rows.md`. Agents have no tool to
lock or unlock a session.

An archive or deletion that includes a locked session is refused with `session_locked`, identifying that session, and
changes nothing. This covers the session itself, an owner's sessions at both levels, deleting archived sessions in bulk,
removing a project, and deleting a console. A locked descendant blocks archiving or deleting its owner, even when the
descendant is already archived. A bulk operation is refused as a whole rather than skipping locks. Unlock the protected
session before retrying. A completion report still reaches its owner but does not archive a protected session or owner
(see "Automatic archiving" in `docs/product/reporting-to-owners.md`).

An archived session cannot be locked: locking one is refused with `session_archived` (reopen it first), decided in the
same step that writes the lock, so a lock never lands on a session archived a moment before. Unlocking an archived
session is allowed, and a lock an archived session already carries keeps protecting it as described above.

While an archive or deletion is in progress, changing any session's lock is refused with `session_lock_busy`; retry
after that operation finishes. A failed lock change leaves the stored lock unchanged.

## Archiving, interruption and resuming

Archiving a session, an interruption, resuming and reopening a dormant session, when its conversation can no longer be
resumed, and how long archived sessions are kept are described in `docs/product/archiving-and-resuming-sessions.md`.

## Renaming a session

A session that is not archived offers **Rename** in its action menu; an archived session's menu has no Rename item. The
rename field contains the created name alone; an empty name is rejected. The daemon's `rename_session` request and an
owner's `rename_session` tool still accept archived sessions. An agent's terminal title remains separate (see "Displayed
names" in `docs/product/session-titles.md`).

## The terminal

The selected session's terminal — what its area shows with no session selected, typing into it, its output and
scrollback, reconnecting and loading, an interrupted session's saved preview, links in its output, and what it offers
while a console has nothing in it — is described in `docs/product/session-terminal.md`; what an archived session shows
in its place, in `docs/product/session-history.md`.
