# Background processes

Octoboard follows the processes started by every agent launch, including servers, previews, watchers and tool helpers.
They belong to that launch even when the agent is awaiting instructions, the command that started them has exited, or
they have detached into another terminal session. This does not depend on the agent reporting background tasks or
remembering to clean them up. The status an agent reports is separate (see "Working in background" in
`docs/product/session-statuses.md`). The user sees, stops and keeps these processes in the right pane's Processes panel
(see `docs/product/processes-panel.md`).

## Automatic stopping and its boundaries

When an agent's process ends, Octoboard stops the processes it can attribute to that launch, except those the user chose
to keep and those carried over (below). This applies to an agent exiting on its own, archiving, an interruption and the
daemon shutting down. Cleanup runs before the session is recorded as having no process, so an archive's worktree
removal follows it. A launch's attributed processes that remain in a deleted working directory for at least **60
seconds** are also stopped while the agent is still running.

A handoff, an account switch, a change of a session's model and a change of its account's sharing **carry the processes
over** instead of stopping them. In a handoff, what the original session's launch started passes to the session that
takes its place when the handoff's archive ends the original's process; in an account switch, a model change or a change
of the account's sharing, it passes to the same session's relaunch (on the new account, with the new model, or on the
changed directory), and is held for it while the session has no process between the two. From then on those processes
are the receiving launch's: they are listed under the receiving session and stopped when its process ends, under the
rules above, and a helper found later carrying the earlier launch's identity is the receiving launch's too. Kept
processes stay kept.

What nothing receives is stopped as usual:

- A handoff whose receiving session does not start, or whose archive of the original fails, leaves the original, and
  what it started, as they were: stopped when the original's process ends.
- A switch, a model change or a sharing change that fails before the session's process has gone down, because it could
  not be stopped or did not stop in time, leaves what it started with that process, stopped when it ends.
- A receiving session that has ended by the time the original's process ends takes nothing, and its processes are
  stopped; a daemon that shuts down or restarts in between stops them, as it stops every launch's.

**A switch, a model change or a sharing change that fails after it ended the session's process** — the conversation
could not be copied, or the relaunch was refused or did not start, or the session was not resumed, say, or, for a switch
and a model change (whose relaunch is watched coming up), the relaunched process ended at once —
keeps what it carried running as if the user had kept it: listed among the project processes of the session's project
(see "Keeping" below), what the relaunch had started in the moment it ran included, even where the session is then
brought back up on the account it had (see "When the switch fails" in `docs/product/automatic-account-switch.md`). A
console session's, which keeps nothing, is stopped, and so is a session's that was archived meanwhile. A failed restart
whose agent had already exited on its own before the restart ended anything keeps nothing either: what it left is
stopped, as for any agent that exits on its own.

Stopping first sends `SIGTERM` and allows **3 seconds** to exit, then sends `SIGKILL` to survivors. A process's identity
includes its start time: a reused process ID is not permission to stop its replacement. A process still alive after
`SIGKILL`, for example in an uninterruptible sleep, may remain; automatic cleanup retries it after **5 minutes**, or
when another launch ends. Cleanup is not proof that every process has already disappeared.

Octoboard cannot attribute a program whose environment macOS withholds (a system binary such as `/bin/sleep` or
`/usr/bin/tail`) that a command left running in the background when the shell that started it exits within one
process-table scan, about **2 seconds**: such a process is neither listed nor stopped with the launch.

After a daemon restart, Octoboard uses its saved process records and launch identities to stop what its previous run
left behind, while preserving kept processes. A terminal session it saved, which reaches the processes in it that it
never saw, is taken back only while a process it saved is still running in it: a session's ID is a process ID, and
once every process of the session has exited it can pass to unrelated processes. Records written before the system
last started name nothing. If those records cannot be read, it does not assume that a process was not kept: it leaves
the processes identified only by their launch tag running and records them as kept.

The daemon, its ancestors and its direct children are protected. The user's shell terminals and the processes in their
terminal sessions are excluded from the background-process lists and cleanup described here; closing those terminals
still follows "Closing a shell terminal" in `docs/product/shell-terminals.md`.

The agent itself and processes in its own terminal session are not offered in these lists. That terminal's remaining
processes are killed outright when the agent exits and cannot be kept through the controls described here.

**Keeping a tree transfers its management to the user.** No session owns it any more: it leaves its session's list, is
no longer stopped automatically, and remains kept across daemon restarts. It stays listed in its project's list among
the **project processes**, the trees of the project no session owns, marked kept: under the project of the session it
was kept from, or, for a tree left behind, the project it was left in. The user can still stop it there, and removing
the project stops it (see "Removing a project" below). A console session belongs to no project and keeps nothing: Keep
is not offered for its trees, and a request to keep one keeps nothing. Processes kept before a record of their project
was kept, or kept because the saved records could not be read after a restart, belong to no project and are listed
nowhere. There is no action to undo Keep. Octoboard follows descendants of kept processes, and processes found in a kept
process's terminal session, as kept too, listed under the same project, with one limit: a helper that both double-forks
away from its parent and detaches into a new terminal session between process-table scans, about **2 seconds** apart,
can escape that relationship. If it still carries the launch's identity, it can be stopped with the launch.

## Moving processes between sessions

An owner moves process trees between itself and its sessions, or from one of its sessions to another, with the
`handover_processes` agent tool, as `handover_branches` moves branches (see "Handing branches between sessions of an
owner" in `docs/product/branch-responsible-session.md`): `from` and `to` are each the caller or a session it owns,
either left out meaning the caller, and they must differ. No approval is asked. The trees keep running; only the session
whose process end stops them changes, and they are listed under it from then on. Each tree named, by the id the
session's process list gives it, must be one `from` holds now. Both sessions must have a process running and neither may
be archived: an archived session's processes were stopped when it was archived, apart from those kept, which are the
user's, and a session with no process has nothing to stop them with. A `from` other than the caller that is working,
starting or waiting for the user is refused, and so is a session being started, resumed, switched, restarted for a model
change or a change of its account's sharing, or handed off; none of that can begin on either session while the trees
move, and messages for a `from` other than the caller are queued meanwhile and delivered once they have moved. A refusal
moves nothing. The sessions concerned other than the caller are told. `get_session` lists a running session's trees in
`processes`, by those ids. A project process, which no session holds, cannot be handed to a session, as a branch no
session is responsible for cannot be handed over.

## Process information and actions

A process tree comprises a top process and its attributed descendants, so one tree may cover several processes. Its
information includes the top process's command and working directory, whether that directory has been deleted, its
age, the tree's resident memory, its process count and the TCP ports it listens on. Command and directory information
may be unavailable. Trees within a session are oldest first. Requests for the same scope within **2 seconds** may share
a daemon answer; a stop or keep invalidates that cached answer.

Stopping a tree applies the stopping rules in "Automatic stopping and its boundaries" above; a kept tree the user stops
is the user's no more, and what is left of it is stopped as a finished launch's. Keeping it transfers its management to
the user under that section's rules. Both actions check what still qualifies when the request arrives:
a tree that has already gone is ignored. If its original top process has exited, the action can still reach its
remembered surviving members and descendants.

## Processes left behind in a project

**Left behind** covers processes of the daemon's user with a deleted working directory under the local project's
directory, including worktrees inside that directory, that belong to no followed launch and have not been kept.
Worktrees outside the project directory are not searched by this rule. Protected daemon processes and the user's
shell terminal sessions do not qualify.

These processes are **only listed**: they are never stopped automatically just because they qualify as leftovers.
The user can stop each tree from the Processes panel, which offers no Keep for it (see "Stop and Keep" in
`docs/product/processes-panel.md`). A project with no running sessions can still show these leftovers.
Listed together with the project's kept trees, they make up the **project processes**, the trees of the project that no
session owns. The daemon's list marks a kept tree as kept, and a tree left behind as not, whether or not its working
directory is gone; the Processes panel shows them alike.

## Removing a project

Removing a project stops its project processes, kept and left behind alike, whether or not its folder goes to the
Trash, before the folder is moved; deleting a console does the same for each of its projects. Both confirmations list
them, read from each project's process list (see `docs/product/removing-projects.md` and "Deleting a console" in
`docs/product/consoles.md`).

## Processes in an archive confirmation

Before the user archives a session, Octoboard inspects its process trees along with its branches and worktrees. Trees
whose top process has run for at least **30 seconds** are listed in the confirmation, even when the agent is awaiting
instructions and reports no background tasks. Archiving an owner includes every session that archive would end, under
"Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`.

The trees are listed in the confirmation's **Background processes** section (see "Archiving, interruption and resuming"
in `docs/product/archiving-and-resuming-sessions.md`), under a line saying that the selected processes will be stopped
and that any can be unchecked to keep it running. Each is a checkbox labelled with the tree's command on one line, which
fades out where it is cut with the whole command as its tooltip, after a glyph: a crossed-out play glyph in the danger
colour while it is checked, a green play glyph while it is not. Under it a **Details** disclosure, collapsed by default,
shows the tree's working directory and its facts, as on a Processes panel card (see "A tree's card" in
`docs/product/processes-panel.md`).

Each checkbox starts **checked, meaning Stop**. Unchecking it means Keep running. A newly listed tree starts checked;
existing choices follow the tree across inspections. **A console session's trees**, which cannot be kept, are listed
apart, after a line saying that a console session's processes are always stopped, each as a plain row with the
crossed-out glyph and its own Details disclosure, and no checkbox. Under the trees, lines say how many processes are
stopped and, while any is unchecked, that the ones kept stay running. Younger trees are not offered, but are still
stopped when the launch ends unless already kept. Keeping a process does not keep its working directory or prevent the
archive's branch and worktree cleanup.

When the archive scope changes, the confirmation inspects the new scope and cannot be confirmed until that inspection
answers. If reinspection fails, previously known processes still in the current scope retain their keep choices;
stale information grants no permission to force branch removal. The selected keeps are applied before archiving starts
and remain in effect even if the archive is subsequently refused, for example because a session is locked.

An archive initiated by an agent tool or an automatic report does not ask the user which processes to keep. It stops
every attributed process that has not already been kept. A handoff's archive of the original asks nothing either, and
stops nothing: the original's processes are carried over to the receiving session (see "Automatic stopping and its
boundaries" above). Restoring an archived conversation does not restart its stopped background processes.
