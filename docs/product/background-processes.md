# Background processes

Octoboard follows the processes started by every agent launch, including servers, previews, watchers and tool helpers.
They belong to that launch even when the agent is at its prompt, the command that started them has exited, or
they have detached into another terminal session. This does not depend on the agent reporting background tasks or
remembering to clean them up. The status an agent reports is separate (see "Working in background" in
`docs/product/session-statuses.md`). The user sees, stops and keeps these processes in the right pane's Processes panel
(see `docs/product/processes-panel.md`); the rail's Processes button counts them across every project and console
session (see `docs/product/processes-on-the-rail.md`).

## Which processes belong to a launch

Whether a process belongs to an agent launch, and so is stopped when that launch ends, is decided by **attribution**
alone: whether that launch started it. A process is the launch's when its chain of parents reaches the agent or another
process of the launch, when its environment carries the launch's tag (`OCTOBOARD_PROCESS_TAG`, which everything the
launch starts inherits; see "The launch environment" in `docs/product/launching-agents.md`), or when it runs in a
terminal session that one of the launch's processes started. A process stays attributed when it is later reparented or
moves into a terminal session of its own. Nothing else enters into it: not a process's name, its kind, the ports it
listens on, or whether other programs are using it. Octoboard keeps no list of services to spare or to stop, and does
not try to judge what a process is.

So everything a launch started is stopped when the session's process ends, by design, a per-user shared service
included: a terminal multiplexer server, a key agent or a build daemon that the agent brought up is stopped with the
launch even when other programs have come to use it since. A shared service that was already running when the agent
connected to it was not started by the launch: it is not the session's, and the session's end does not stop it.

The user's way to spare a process the launch started is to keep it: **Keep** on its card in the Processes panel (see
"Stop and Keep" in `docs/product/processes-panel.md`), or unchecking it in an archive confirmation to keep it running
(see "Processes in an archive confirmation" below). Nothing else exempts a process, and on a Windows daemon host a
process on the agent's own console cannot be kept at all (see "Processes on the agent's own console" below). What
Octoboard keeps without being asked, after a restart that fails or when its saved records cannot be read, and the
processes outside these rules altogether (the daemon's own and its process holders', the user's shell terminals, the
agent's own terminal session), are in "Automatic stopping and its boundaries" below.

Attribution cannot follow everything. Octoboard cannot attribute a program whose environment macOS withholds (a system
binary such as `/bin/sleep` or `/usr/bin/tail`) that a command left running in the background when the shell that
started it exits within one process-table scan, about **2 seconds**: such a process is neither listed nor stopped with
the launch. A limit that concerns kept processes is described with keeping, in "Automatic stopping and its boundaries"
below.

**On a Linux daemon host whose cgroup lets the daemon make groups below it**, each agent launch is also held in a
cgroup v2 group of its own. That is the case where the daemon's cgroup subtree is writable to it — under the example
systemd unit's `Delegate=yes` (see "Running as a systemd user service" in `docs/product/application-lifecycle.md`), or
in a container's writable hierarchy — and normally not for a daemon started from a login session (by
`octoboardd start`, say), whose cgroup systemd does not let the user write. The agent's process is moved into the group
just after it starts, with what it has started by then, and everything a process in the group starts is born in it,
wherever it goes after that: a process in the launch's group is the launch's however it detached and whatever its
environment says. Where no group can be made (no cgroup v2 hierarchy, a read-only one, a cgroup owned by someone else),
the launch has none, which the daemon's log says once, and the rules above apply alone. A process the agent forked and
that left it in the instant before the move is outside the group and is the launch's only by those rules. Shell
terminals get no group; nor does anything the daemon starts other than an agent launch. How the group ends with the
launch is in "Automatic stopping and its boundaries" below.

**On a Windows daemon host** no process's environment is read, so the launch's tag attributes nothing there, and a
process has no terminal session. A process is the launch's when its chain of parents reaches the agent or another
process of the launch, or when it is a member of the launch's job object, which everything the launch starts lands in
wherever it goes (or of the job of a launch whose processes were carried over to it), except for what a Store
PowerShell takes out of it (see "Automatic stopping and its boundaries" below).

## Automatic stopping and its boundaries

When an agent's process ends, Octoboard stops the processes it can attribute to that launch, except those the user chose
to keep and those carried over (below). This applies to an agent exiting on its own, archiving, an interruption and the
daemon shutting down, which quitting the application does (see "Quitting" in `docs/product/application-lifecycle.md`):
of the processes listed in Processes panels, a quit leaves running only those no session owns, the kept ones and those
left behind in a project (see "Processes left behind in a project" below). Cleanup runs before the session is
recorded as having no process, so an archive's worktree removal follows it. A launch's attributed processes that
remain in a deleted working directory for at least **60 seconds** are also stopped while the agent is still running.

A handoff, an account switch, a change of a session's model and a change of its account's sharing **carry the processes
over** instead of stopping them. In a handoff, what the original session's launch started passes to the session that
takes its place when the handoff's archive ends the original's process; in an account switch, a model change or a change
of the account's sharing, it passes to the same session's relaunch (on the new account, with the new model, or on the
changed directory), and is held for it while the session has no process between the two. From then on those processes
are the receiving launch's: they are listed under the receiving session and stopped when its process ends, under the
rules above, and a helper found later carrying the earlier launch's identity is the receiving launch's too. Kept
processes stay kept. Only what is still running once the agent has exited can be carried: what the agent itself ends as
it shuts down is gone, and a restarted session is told so (see "Processes a restart ended" below).

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

Except on a Windows daemon host (below), stopping first sends `SIGTERM` and allows **3 seconds** to exit, then sends
`SIGKILL` to survivors. A process's identity includes its start time: a reused process ID is not permission to stop its
replacement. A process still alive after `SIGKILL`, for example in an uninterruptible sleep, or one the system refuses
to end, such as another user's or one running with more rights than the daemon, may remain; automatic cleanup retries
it after **5 minutes**, or when another launch ends, and a stop naming its tree retries it at once. Cleanup is not
proof that every process has already disappeared, and an automatic stop does not tell the user what it left running (see
the known limit below).

**Where the launch has a cgroup group** (on Linux, see "Which processes belong to a launch" above), ending it does more
once those rounds of signals are through: whatever is still in its group, and in the groups of the launches whose
processes were carried over to it, is killed — what no scan attributed and what sits in the agent's own terminal
session included — and the groups are removed. The processes spared are moved out of the group first: every process the
user kept, with the children it started that are still in the group, goes to the cgroup the group is in, and a process
moved to another session (see "Moving processes between sessions" below) goes to that session's launch's group. A
process is moved out of the group already when it is kept or moved, so what it starts from then on is born outside it;
processes carried over stay in their group, which passes to the receiving launch with them. The kill is skipped, and
the group left to the ordinary cleanup until it is empty and removed, when a process to be spared cannot be moved out
of it, or when the process table could not be read during the rounds of signals; a group whose processes have not all
gone **1 second** after the kill is left in place. Groups an earlier run of the same data directory's daemon left,
which a daemon in the same cgroup finds again at start, are treated as that run's leftovers: what is in them is stopped
as the cleanup after a daemon restart (below) stops what a previous run left, kept processes excepted, and an empty one
is removed.

**On a Windows daemon host** there are no signals. A process that runs on a terminal the daemon started (a pseudo
console, as an agent's own process does) is asked to end through that terminal. Where its agent declares quit keys,
those are pressed first: Codex's are Ctrl-C, then, if it has not quit within **650 milliseconds**, Ctrl-C again, after
which it has up to **6 seconds** to finish quitting. Then the pseudo console is closed (`ClosePseudoConsole`), which asks
every process attached to it to end; an agent that declares no quit keys (Claude Code, Grok Build) gets this at once.
A process still running **3 seconds** after that is terminated. Every other process a cleanup or a stop reaches, the
launch's remaining processes among them, takes no polite request on Windows and is terminated at once, without a grace:
its process ID is opened once, the start time of the process behind that handle is compared with the one recorded, and
it is terminated through the same handle, so a process ID that has passed to another process is taken as the process
having gone, not stopped. A process still running after it was terminated, or one Windows refused to terminate, is
retried as above.

After a daemon restart, Octoboard uses its saved process records and launch identities to stop what its previous run
left behind, while preserving kept processes. It stops nothing until it has read those records back. A launch whose
agent a process holder kept running and the start adopted (see "What a daemon start takes over" in
`docs/product/process-holders.md`) is not left behind: the processes and terminal sessions the records attribute to it
stay that launch's, listed under its session and stopped when its process ends, under the rules above. So does a
launch whose agent a process holder the start cannot connect to may still run, until that holder is connected to or
known to be gone (see "A holder that cannot be connected to" in `docs/product/process-holders.md`). While such a
holder may run a session whose launch nothing records, the records are still read back and this run's own processes
are handled as ever, but what the previous run left, and what descends from it, is kept until no such holder is
left, and stopped then. A terminal session it saved, which reaches the processes in it that it
never saw, is taken back only while a process it saved is still running in it: a session's ID is a process ID, and
once every process of the session has exited it can pass to unrelated processes. Records written before the system
last started name nothing. If those records cannot be read, it does not assume that a process was not kept: it leaves
the processes identified only by their launch tag running and records them as kept. On a Windows daemon host a sign-out
or a shutdown can end the daemon before or without any of the cleanup above: a daemon started on its own is told
nothing, and the desktop application's stop has only the seconds the system allows (see "Crashes and forced
termination" in `docs/product/application-lifecycle.md`); what it leaves is handled by this cleanup at the next start, though the system
has normally ended the user's processes by then, kept ones included.

**On a Windows daemon host, a Store PowerShell leaves the launch's job.** A `pwsh` installed from the Microsoft Store
that one of the agent's own tools or hook commands starts takes itself, and everything it runs, out of the launch's job
object. Such a tree is the launch's only through its chain of parents. Once a process-table scan — one about every
**2 seconds**, and one more just before the agent is ended — has seen that chain reach the agent, the tree is the
launch's like any other, and stays so when the chain later breaks: it is listed under the session, stopped when the
agent's process ends (archiving included), and stopped by the next daemon start's cleanup when the saved records name
it. The kill on an internal failure of the process the agent's terminal runs in (see "Crashes and forced termination"
in `docs/product/application-lifecycle.md`), which reaches only the job's members, does not end it, so it runs on until
that next start. A tree whose chain to the agent is broken before any scan has seen it — the process of the
launch that started it exits first — is not the launch's: it is neither listed under the session nor stopped with
it, and can be left running after the session ends. Which PowerShell runs a tool call is the agent's choice; Octoboard accepts this limit.

**On a Windows daemon host, a process the system will not let the daemon end is left running.** With User Account
Control on, the daemon does not run elevated; a process of the same account an agent started through a User Account
Control prompt runs elevated, and Windows still lets the daemon end it, so the cleanup when its launch ends, a daemon
restart's and **Stop** in the Processes panel end it like any other. A process whose security refuses the daemon that
right is out of its reach: none of those stops ends it, and Stop then tells the user that the process could not be
stopped for lack of permission (see "Stop and Keep" in `docs/product/processes-panel.md`), and a kept process it leaves
running stays kept; the automatic stops do not tell the user (see the next paragraph). Such a process keeps running
until it exits by itself or is ended from a program with the rights to end it. The working directory of a process
running elevated may not be readable, and where it is not, no directory is shown for it; its command is still read.

**An automatic stop does not tell the user what it left running**, a known limit accepted for this release. This holds
for every stop the user did not ask for with **Stop** in the Processes panel: when an agent's process ends, when a
session is archived, when a project is removed or a console deleted, when a working directory is removed, and the
cleanup after a daemon restart. A process such a stop leaves running — one still alive after it was killed, or one the
system refused to end — is recorded only as a warning in the daemon's log, and is tried again only as described above:
after **5 minutes**, or when another launch ends. Only Stop in the Processes panel reports the processes it left running
(see "Process information and actions" below).

The daemon, its ancestors and its direct children are protected, and so are the running process holders (see
`docs/product/process-holders.md`) and their direct children, the agents, the shell terminals' shells and the Codex
sessions' private app servers a holder starts. The holders are found from their registry files in `~/.octoboard/holders/`, and one counts only while the
process its file names, by process ID and start time, is still running. A protected process is the daemon's own: it is
never a launch's background process, nor kept, nor left behind in a project. Processes in terminal sessions led by
those direct children, including a Codex session's private app server and what runs in its session (see "Codex's
private app server" in `docs/product/per-agent-launch-specifics.md`), are excluded from the background-process lists
and Stop and Keep controls. Their lifecycle remains managed with the agent session. Tool commands that start a
terminal session of their own remain subject to the attribution rules above.

The user's shell terminals and the processes in their terminal sessions are excluded from the background-process lists
and cleanup described here; closing those terminals still follows "Closing a shell terminal" in
`docs/product/shell-terminals.md`.

Except on a Windows daemon host, the agent itself and processes in its own terminal session are not offered in these
lists; that terminal's remaining processes are killed outright when the agent exits and cannot be kept through the
controls described here. On a Windows daemon host, where a process has no terminal session, only the agent itself is
left out: its own internals, the
MCP servers and other helpers the agent's CLI starts as its children, reach the agent through their chain of parents
and are listed among that launch's process trees in the Processes panel, where the user can see and stop them like
any other background process, and keep them unless they are on the agent's own console (see "Processes on the agent's
own console" below). The same goes there for what a Codex session's private app server starts, which is in the
launch's job; the server itself, a direct child of the process holder that runs the session (of the daemon where
holders cannot be set up), is not listed.

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
can escape that relationship. If it still carries the launch's identity, it can be stopped with the launch. On a
Windows daemon host a kept process's descendants are followed by their chain of parents alone: a child whose parent
exits before the next scan has seen it loses that chain, is pulled back to the launch by the job it is still a member
of (see "Which processes belong to a launch" above), and is stopped with the launch instead of being kept.

## Processes on the agent's own console

On a Windows daemon host the agent runs on a pseudo console, and ending its launch closes that console, which ends every
process attached to it whatever keeps it (see "Automatic stopping and its boundaries" above). So a session's tree with a
process attached to the agent's own console cannot be kept:

- **It is listed as not keepable**: in the daemon's process list it carries `keepable: false` and
  `unkeepable_reason: "agent_console"`, the one reason there is; every other tree carries `keepable: true` and no
  reason. Only `keepable` sent as `false` marks a tree not keepable: a tree from a daemon that does not send the field
  can be kept. The Processes panel shows its Keep as unavailable (see "Stop and Keep" in
  `docs/product/processes-panel.md`), and an archive confirmation lists it with no choice (see "Processes in an archive
  confirmation" below).
- **A keep naming it is refused** with `process_shares_agent_console`, whose parameters name the process (`pid`, and
  `name`, empty when unknown): a `keep_processes` keeps none of the trees it names, and an `archive_session` whose
  `keep_process_ids` names it keeps nothing and does not archive. Its message, also the reason shown ahead of the
  choice, says the process runs in the agent's own terminal, so it ends with the session and cannot be kept, and that
  to keep it running it is to be started in a window of its own.
- **A process whose console cannot be told** — it cannot be opened, or the system does not say which console it is
  on — is taken not to be on the agent's console, so its tree stays keepable.

This never applies on macOS or Linux, where what sits in the agent's own terminal session is not listed at all (see
"Automatic stopping and its boundaries" above).

## Processes a restart ended

Carrying processes over does not keep alive what an agent ends itself while it exits: Codex, for one, closes its
background terminals as it shuts down, so a command it left running in one (a dev server, say) does not survive the
restart. So an account switch (the user's, an owner's or Octoboard's automatic one, a resume on the session's own
account after its quota recovered included), a change of a session's model or reasoning effort, and a change of its
account's sharing tell the relaunched session which of the processes it had started the restart ended.

- **What counts as ended.** Just before ending the session's process, Octoboard lists the process trees the session
  has running, as its Processes panel lists them. Once the process is recorded down, each of those trees no longer
  running among the session's processes is one the restart ended, whatever ended it; a tree whose top process exited
  while another of its processes runs on still counts as running. A tree still exiting at that moment is not named, and
  nothing is named when the process table cannot be read, or when the agent had already exited by itself before the
  restart.
- **How each is named.** By its top process's command, on one line and shortened when long, its working directory and
  its listening ports. At most **8** trees are named, and the rest only counted. The session is asked to start again
  any it still needs.
- **Where it goes.**
  - A session the restart continues (see "Continuing after a switch" in
    `docs/product/switching-session-accounts.md`) has it in the prompt to continue, before the request to continue the
    task: after the opening saying the session was restarted, or, where the relaunch carries a notice of its own
    (Octoboard's automatic switch and its resume after a quota recovery), after that notice.
  - A session Octoboard's automatic switch moved to another account without continuing it has it in the one message
    that switch writes into it, between the switch's notice and the words telling it to carry on (see "When the switch
    succeeds" in `docs/product/automatic-account-switch.md`).
  - Any other relaunched session — one not continued after a switch, a model change or a resume after a quota
    recovery, or one resumed after a change of its account's sharing — is written a message from Octoboard of its own,
    saying the session was restarted and naming the trees, once the restart is over and the session is back up. It
    takes the delivery every message Octoboard sends takes (see "Messages held until a session can take them" in
    `docs/product/message-delivery.md`), so an idle session takes it at once and starts a turn on it; no report is
    synthesised for that turn (see "When a session does not report" in `docs/product/reporting-to-owners.md`). A
    message that cannot be written is dropped, not kept for later.
- **Nothing is said** when no tree ended, or when the restart fails: a session left *interrupted* is told nothing, and
  what was held for its relaunch is kept as "Automatic stopping and its boundaries" above describes.

## Moving processes between sessions

An owner moves process trees between itself and its sessions, or from one of its sessions to another, with the
`transfer_processes` agent tool, as `transfer_branches` moves branches (see "Transferring branches between sessions of
an owner" in `docs/product/branch-responsible-session.md`): `from` and `to` are each the caller or a session it owns,
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
session is responsible for cannot be transferred.

## Process information and actions

A process tree comprises a top process and its attributed descendants, so one tree may cover several processes. Its
information includes the top process's command and working directory, whether that directory has been deleted, its
age, the tree's resident memory, its process count and the TCP ports it listens on. Command and directory information
may be unavailable. Trees within a session are oldest first. Requests for the same scope within **2 seconds** may share
a daemon answer; a stop or keep invalidates that cached answer.

Stopping a tree applies the stopping rules in "Automatic stopping and its boundaries" above. A kept tree the user stops
is the user's no more while the stop runs: what starts from it meanwhile is stopped as a finished launch's. Each of its
kept processes still running once the stop is through — one the system refused to end, or one that outlived being
killed — is kept again under the same project: it stays listed among that project's processes, marked kept, and
automatic cleanup does not retry it. Keeping it transfers its management to the user under that section's rules. Both
actions check what still qualifies when the request arrives: a tree that has already gone is ignored. If its original
top process has exited, the action can still reach its remembered surviving members and descendants.

A stop the user asks for answers which processes are still running once it is through, each with why: **access
denied**, the system refused to end it for lack of permission (another user's process, or one running with more rights
than the daemon), or **other**, refused for another reason or still running after it was killed. A tree with such a
process is not reported as stopped. Only this stop reports them (see the known limit on automatic stops in "Automatic
stopping and its boundaries" above).

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
crossed-out glyph and its own Details disclosure, and no checkbox. **A tree listed as not keepable** (see "Processes on
the agent's own console" above) is such a row too, always stopped, with the reason under its command, and no line of
its own leading it. Beside trees that can be kept, these rows close the list of checkboxes, set in so that their glyph
and command line up with the checkboxes' glyphs and commands; with no tree that can be kept they stand alone. Either
way they come before a console session's trees. A reason this window
does not know reads only that the process cannot be kept. Under the trees, lines say how many processes are
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
