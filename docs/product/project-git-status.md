# Project git status

A project whose directory is a git repository carries its **current branch and how far that branch is from its
upstream** in the sidebar. Octoboard checks those projects against their remotes by itself, on an interval, across
all consoles, and checks one project at once when asked to with **Pull current branch**. What a check does beyond
reporting depends on one setting, **Automatically pull repositories**, described in "Automatically pulling
repositories" below, and on whether the check was asked for by hand. Pushing the current branch is not a check
(see "Pushing the current branch" in `docs/product/project-git-actions.md`). Between checks, the branch and the
counts follow changes made in the repository itself, without going to the remote (see "Following changes to the
repository's refs" below).

A project's uncommitted changes are inspected in the project pane's Git mode (see
`docs/product/project-pane-git-mode.md`), which only reads the repository and has no part in these checks. The
separate **Restore file changes** action discards tracked changes after confirmation (see "Restoring file changes"
in `docs/product/project-git-actions.md`).

## The branch badge

At the end of a project's row — after the project's name, before the status marker a collapsed project carries, a
pinned project's pin button and the row's **+** and action-menu buttons (see "Project rows" in
`docs/product/sidebar-project-rows.md`) — sits a badge showing that project's git state. The same badge sits after the
project's name in its focus mode header and in each project group's heading in a console session's focus mode (see "What
both views show" and "A console session's focus mode" in `docs/product/focus-mode.md`). It shows nothing at all while no
status has arrived for the project yet, and nothing when the project's directory is not a git repository.

What it holds, in this order:

- **A glyph for what is going on**: a branch glyph normally; a commit glyph when `HEAD` is detached; a spinning refresh
  glyph while the remote is being checked, the branch is being pulled or the current branch is being pushed.
  Idle and in-flight states differ in shape, not in motion alone, and nothing animates where the system asks for reduced
  motion.
- **A failure mark**, a red circled `!`, when the last check failed, standing in for the branch or commit glyph at the
  badge's start. It is a button of its own — a tab stop of its own in every placement —
  named that the Git check failed, with no tooltip, and it opens a popover under the rules of a failure mark (see
  "Failures, warnings and notes marked at their point" in `docs/product/error-alerts.md`). The popover says that Git
  reported an error the last time the project's repository was checked or pulled, then that Octoboard checks it again on
  its own and that an error that does not go away is to be resolved in the repository with Git, and last, as raw text in
  a block of its own, the message `git` or the operating system itself gave. That message is shown nowhere else: not in
  the badge's tooltip and not in any name. The two sentences are the same for every failure but one: a check that found
  no `git` at all (see "Where `git` is found" below). Its popover words that failure as an error alert does (see "Recovery
  and unexpected errors" in `docs/product/error-alerts.md`) — Git is not installed or not on `PATH`, then the remedy to
  install it where sessions run, make sure it is on `PATH` and try again — shows no raw text, and ends with two
  buttons at its trailing edge: **Download Git**, a link in the shape of a button that opens Git's download page,
  `https://git-scm.com/downloads`, in the browser, then **Try again** (see "Checking again after Git was not found"
  below). The mark's name is the same as for any other failure. A press on the mark on a
  sidebar row never reaches the row, so it does not fold the project. A failed check does not blank the rest of the
  badge: the branch and the counts are still read from the repository and shown beside the mark; a failed read with no
  branch and no counts shows the mark alone.
- **The branch name** — or the short commit id, when `HEAD` is detached. A repository with no commit yet says so in its
  place, and nothing stands there when the branch could not be read at all, the failure mark carrying the reason
  instead.
- **How far the branch is ahead of its upstream**, after an up arrow, and **how far behind**, after a down arrow, each
  shown only while it is not zero and each number written in the current language. A branch with no upstream shows
  neither. During a push, the ahead count's arrow scrolls continuously upward; during a pull, the behind count's
  arrow scrolls continuously downward. Each scroll stays inside a fixed area the size of one count arrow; the other
  arrow and both numbers stay still. A zero count still has no arrow, so the leading refresh glyph remains the in-flight
  indication when the relevant count is zero. Reduced motion keeps the arrows still too.

**While a check is in flight the badge keeps what it was showing** — the branch name (or short commit id) and the
counts — and only its glyph turns into the spinning refresh glyph, which takes the failure mark's place as well; the
whole badge is replaced once the check finishes. The glyph turns only once the check has been running
for about 0.4 seconds, so a check that finishes sooner goes straight from the old state to the new one without
showing as in flight; a check asked for with **Pull current branch** shows as in flight at once. A project waiting
for its turn (see "When a project is checked" below) is not yet in flight. Meanwhile a local re-read can still update
the branch and the counts (see "Following changes to the repository's refs" below). A project with no status yet has
nothing to keep, so once its first check shows as in flight the badge shows the spinning glyph alone, and nothing
before that.

**The failure mark comes back only once nothing is in flight**, and only while the project's status still records a
failure: the check or pull that ran failed in its turn, or a push, or the pass that turning **Automatically pull
repositories** on starts, has ended without clearing the failure the last check recorded, which neither of them does
(see "Turning the switch on" below). A check that succeeds clears it, the one Pull current branch runs included. When
the mark goes away while it or its popover holds keyboard focus, focus goes to the project's row on the sidebar, or to
the project heading in focus mode.

**While a push is in flight the badge describes the push**, including over a check or pull running at the same time. All
three use the same leading refresh glyph; the tooltip, accessible name and animated count arrow follow the push state.
The branch and counts remain visible and can update during the push, and a failed check's mark gives way to the refresh
glyph as during a check. Every window showing the project sees the pushing state, including one that connects while the
push is running. Once the push ends, successfully or otherwise, the badge shows the latest check or pull state, or its
idle glyph if neither is running (see "Pushing the current branch" in `docs/product/project-git-actions.md`).

**As the row gets tight only the branch name gives way**: a branch name is cut at its start, keeping its end in
view and fading at its left edge, while a detached `HEAD`'s short commit id is cut at its end, keeping its beginning
(see "Names too long for their space" in `docs/product/labels-and-tooltips.md`). The state glyph, upstream arrows and
counts remain visible, even when none of the branch name fits. Only the branch name runs as a marquee while the pointer
is over it — a branch name from its end back toward its beginning. The branch and counts read left to right even under
a right-to-left language. The project's name keeps a floor of its own and never vanishes. The text saying that the
repository has no commits yet is cut at its end and follows the same fade and marquee rules, but follows the window's
reading direction. How the badge shares a project's focus mode header with the title is in "What both views show" in
`docs/product/focus-mode.md`.

**One tooltip for the whole badge.** Resting the mouse anywhere on the badge — the state glyph, the branch name or
either count — shows one tooltip, after the window's usual wait and on pointer hover only, since the badge is no tab
stop (see "How a tooltip behaves" in `docs/product/labels-and-tooltips.md`). It says the badge's whole state in one
sentence, the same one assistive technology is given: the branch (or that a check, a pull or a push is in flight, or why
there is no branch) and the ahead and behind counts, each only while it is not zero. A failed check is not part of it:
the failure mark stands outside the tooltip and opens only its own popover. It is one tooltip in all placements, the
sidebar row, the project focus header and the console-session focus project heading, and it shows whether or not the
badge's text is cut — so the branch name can still be read once the row has squeezed the text away. The states with no
tooltip at all are the one saying in visible text that the repository has no commits yet, which a tooltip would only
repeat — that text still runs as a marquee while the pointer is over the text itself (see "Names too long for their
space" in `docs/product/labels-and-tooltips.md`) — and a failed read showing the failure mark alone. In either
focus-mode placement a tap on the badge opens the same sentence in a popover; on the sidebar row a tap is the row's
press (see "Tooltips on indicators" in `docs/product/labels-and-tooltips.md`), except on the failure mark, which answers
its own tap.

**What assistive technology hears.** A project row announces itself as one control, so the badge's facts are folded into
the row's own name instead of being announced separately: the branch (or why there is none), the ahead and behind
counts, each only while it is not zero, and, while the failure mark is shown, that the Git check failed — never Git's
own message. The mark itself is reached on its own, as a button named that the Git check failed. In either focus-mode
placement the badge is not inside such a row and carries its own name, without the failure, beside the mark's. The badge
is deliberately not a live region, so a check or push starting or finishing is not announced.

One outcome is announced: a **Try again** on the mark of a check that found no `git` whose check again finds none (see
"Checking again after Git was not found" below). Assistive technology then hears the mark's name, that the Git check
failed, once, as a status that does not interrupt, in every placement of the badge. It is said only once the window has
the user's attention, a short moment after the answer arrives or after the user comes back to the window, and goes by
the answer to that Try again, not by the statuses broadcast meanwhile. A check that finds `git` says nothing, the badge
then showing the branch, and so does a check of a round, which no one asked for. Each Try again that fails again is
heard again, except that a second failing answer arriving before the first has been said may not be heard. An answer
about another project than the placement now shows, as when focus mode moves to another project, is dropped.

## When a project is checked

**The projects of every console** are checked, whichever console the sidebar shows:

- as soon as the application's window connects to the daemon, including after a lost connection comes back (see
  "Losing the daemon connection" in `docs/product/application-lifecycle.md`);
- every five minutes while the window remains connected;
- again when the daemon sends a fresh snapshot, since a restarted daemon may have lost the statuses it held;
- when a console is added, or a project appears in a console — added, or moved in from another one — which checks
  that console's projects without waiting for the next round;
- and when the user chooses **Pull current branch** for one project (see "Pulling one project by hand" below).

Switching or previewing consoles does not change which projects are checked or restart the five-minute clock.
Adding, moving or removing a project does not restart that clock either. A removed console is left out of later
rounds.

No periodic checks run while no client is connected: the schedule belongs to the application's window, not to the
daemon. Turning **Automatically pull repositories** on is not a check and is not limited this way; see "Turning the
switch on" below.

**At most four projects are checked at once**, across every window and console; the rest of a round wait their
turn, so a console with many projects is worked through four at a time. The daemon will not start a second check of
a project while one is already running or waiting its turn, and skips a project whose last check finished less than a
minute ago — so several open windows, each on its own interval, cannot multiply the work. A project skipped that way
simply keeps the status it already had. Pull current branch is the exception to all three (see "Pulling one project
by hand" below). **The five-minute interval, the one-minute floor and the four at once are
fixed**, not settings.

A status is never stored: after a daemon restart there is none for any project, and a project's badge shows nothing
again until its next check, or until its refs change (see "Following changes to the repository's refs" below).

### What one check does

- A directory that is not a git repository is reported as such, and the badge shows nothing for it. For this badge, for
  the Git actions in its project menu, a directory is a git repository only when it has a `.git` entry of its own that
  is a directory or a regular file, a link to either counting as it; anything else at `.git`, a FIFO or a device say, is
  no repository. A subdirectory of a repository is not one. The project pane's Git mode looks further up the folders
  (see "The repository and its worktrees" in `docs/product/project-pane-git-mode.md`); the badge does not. The project's
  recorded remote URL is cleared, unless the directory cannot be seen at all (see "The project's remote URL" in
  `docs/product/projects.md`).
- Otherwise the repository's remotes are read, and the project's recorded remote URL is made the one that read gives,
  which is how that value is kept true to a repository whose remote changed outside Octoboard (same section). A read
  that could not be made leaves the recorded value alone, skips the fetch below and records `git`'s own message for
  the badge, as a failed fetch does; the branch and the counts are still read. A changed value reaches the open
  windows at once; nothing about the badge changes with it.
- The remote is then fetched — **remote refs only**: nothing in the working tree moves and no branch changes.
  The remote fetched is the current branch's upstream remote; with no upstream and exactly one remote configured, that
  one; with no upstream and several, `git`'s own default. A repository with no remote at all is not fetched, and that
  is not an error.
- Then the branch, its upstream and the ahead/behind counts are read from the repository. A fetch that failed —
  offline, authentication, a host key that has never been accepted — records its message for the badge but does not
  stop this read, so the branch and the counts still show.
- Finally the branch may be pulled; see "Automatically pulling repositories" and "Pulling one project by hand"
  below.

A check that finds no `git` to run (see "Where `git` is found" below) runs none of these steps: it records that failure
for the badge, with no branch and no counts read, so the badge shows the failure mark alone.

### Checking again after Git was not found

**Try again** in the popover of the failure mark of a check that found no `git` (see "The branch badge" above) closes
the popover and has that one project checked at once — what a person does after installing Git, without restarting
anything. It is a check as in "What one check does" above, except that:

- `git` is looked for once more in a fresh capture of the user's environment where the one Octoboard holds finds none
  (see "Where `git` is found" below);
- the one-minute floor does not apply, and the check does not wait its turn behind the four at once (see "When a
  project is checked" above);
- it shows as in flight at once, without the 0.4-second wait;
- it **never fast-forwards**, whether or not **Automatically pull repositories** is on.

If a check of the project is already running, nothing starts alongside it: as soon as it ends, one more check runs —
even when the running one also looked for `git` again, since it may have looked before Git was installed — and that
later check is the one that answers. If a check of the project is only waiting its turn, in a round or in the pass
that turning **Automatically pull repositories** on starts (see "Turning the switch on" below), that check starts at
once in its place and looks for `git` again, fast-forwarding only as the round or the pass would have. Try again and
Pull current branch requests arriving while one check runs share the one check that follows it, which fast-forwards
if a Pull current branch was among them.

Try again is answered once the check that answers it has ended, with the status it ended with; that answer is what
the announcement in "What assistive technology hears" above goes by. An answer that does not come — the check was
abandoned, as when the project is removed first, or has not ended within ten minutes — announces nothing, and a check
still running then goes on and shows its result on the badge as any check does. A request that fails is an error alert
titled that the branch could not be checked.

### Reading the status from a session

Every tool that gives a project with its settings — `get_project`, `list_projects` for the caller's own console,
`add_project` and `update_project` (see "A project's settings" in `docs/product/agent-settings-tools.md`) — carries the
project's **`git_status`**: the status the daemon holds for it, the one the badge shows. Reading it never starts a
check, fetches nothing and runs no `git`, unless the call asks for a refresh (below).

`git_status` is `null` while the daemon holds no status for the project: nothing has checked it or re-read its refs
since the daemon started, for instance before any client has connected. Otherwise it is:

| Field | Contents |
|---|---|
| `repository` | `false` when the directory is not a git repository; the other fields are then empty. |
| `branch` | The current branch, or the short commit id while `detached`. |
| `detached` | Whether the repository is on a detached commit. |
| `upstream` | The branch's upstream, or `null`. |
| `ahead`, `behind` | How many commits the branch is ahead of and behind its upstream. |
| `activity` | `idle`, `checking`, `pulling` or `pushing`. |
| `error` | The message of the last failed check, with any credential in a URL it names removed, as from the remote URL (see "A project's settings" in `docs/product/agent-settings-tools.md`); `null` when none. |
| `checked_seconds_ago` | How many seconds ago the last check completed, counted on the wall clock, so time the machine spent asleep is part of it; `null` when none has completed since the daemon started, the branch and counts then being only what the repository's local refs gave. |

The counts are as fresh as the last check, so they may be stale by `checked_seconds_ago`.

**A refresh.** `list_projects`, for the caller's own console, and `get_project` take an optional `refresh` (`true` or
`false`, `false` when left out; see "Listing a console's projects" in `docs/product/what-every-session-reads.md` and
"A project's settings" in `docs/product/agent-settings-tools.md`). With it, the projects the answer lists are checked
first, as the window's checks do (see "What one check does" above): the same one-minute floor and the same claim on a
check already running or waiting, so a project checked less than a minute ago, or being checked, is not started again,
and with **Automatically pull repositories** on a branch that is behind and has not diverged is fast-forwarded as
in any check (see "Automatically pulling repositories" below). The checks take their turn among the four that may run
at once, and are made whichever console the window shows. The call waits up
to **30 seconds** for each check to complete, a project the floor skipped counting as complete, then reads the
statuses, and its answer carries `git_refresh` — at the top of `list_projects`' answer, in the project's entry for
`get_project`:

- `{"finished": true}` when every check completed in time;
- otherwise `{"finished": false, "unfinished": [...], "note": "…"}`, `unfinished` naming the ids of the projects whose
  check had not completed, which goes on in the background, and the note saying their `git_status` is what the daemon
  held before and to read it again later.

## Following changes to the repository's refs

Between checks, the badge follows what happens in the repository itself: a commit made by an agent session or in a
terminal, a branch switched, created or deleted there, a fetch, pull or push run there, or a commit in another
worktree of the same repository. Octoboard watches each git project's `HEAD`, its local branches and its
remote-tracking branches, packed ones included, and those of a repository that keeps its refs in the reftable format.
When the system reports that it dropped file events, every git project is read again, since any of their refs may
have changed unseen. The working tree and the index are not watched, so editing or staging
files changes nothing here. Once those refs have stayed unchanged for 300 milliseconds, the branch, its upstream and
the ahead/behind counts are read again from the repository — locally, nothing is fetched — and the result reaches
every window showing the project. So a commit raises the ahead count at once, and a push started right after it is
drawn on that count (see "Pushing the current branch" in `docs/product/project-git-actions.md`).

**Git run inside a project session reports its own ref changes.** Under heavy load the system can deliver the watch's
file events late or not at all, so a project session's git does not depend on them: every transaction it completes that
updates `HEAD`, a local branch or a remote-tracking branch — a commit, a merge or fast-forward, a rebase, a branch
switched, created or deleted, a fetch or pull — is reported to the daemon through the session's git hooks (see "The
session's git hooks" in `docs/product/branch-responsible-session.md`), and every project in the repository it ran in
is re-read the same way, locally: the main working copy and each linked worktree that is a project, of any console.
Reports of one repository are gathered: the read comes once they have been quiet for 300 milliseconds, and never later
than 2 seconds after the first, so a long rebase or a script committing in a loop still shows as it goes. At most four
repositories are re-read this way at once. The report is sent in the background once git has finished, so git never
waits on it, and a daemon that is not running or a report that fails changes nothing for git. Only a git that reaches
the session's hooks reports anything (see "What attribution needs" in `docs/product/branch-responsible-session.md`),
and a git older than 2.46 reports no switch to an existing branch that changes no other ref, since it runs no hook for
that. The watch covers those, as it covers git run outside sessions, and the checks still run on their interval.

This is not a check: the five-minute interval and the one-minute floor do not apply to it, it never pulls, even with
**Automatically pull repositories** on, and it leaves the project's recorded remote URL alone. Every project of every
console is watched, whether or not its console is current and whether or not a window is connected. A directory is a
git repository here by the badge's rule (see "What one check does" above). Watching starts when the daemon starts,
when a project is added, and, for a directory that becomes a repository later, at the first check that finds it one;
it ends when the project, or its console, is deleted. A repository whose ref layout changes while watched (converted to
the reftable format, say) is watched anew from its next check or re-read. A project whose refs cannot be watched is
left to the checks and to its sessions' reports (above). A project whose `.git` has gone is not re-read: its badge
keeps what it showed until its next check reports that the directory is not a repository.

The re-read changes only the branch, its upstream and the counts. It also runs while a check, a pull or a push is in
flight, and leaves the badge's in-flight indication alone. A failure the last check left stays on the badge, since a
local read says nothing about the remote — except a check that found no `git` at all: a re-read that succeeds has just
run `git`, so it clears that failure. This holds for every local read of the branch that reaches the badge, the ones a
push, a branch switch or a restore makes included. A re-read that fails puts its own message on the badge as the failure, and
the branch and the counts stay as they were. A project with no status yet — never checked, or not since the daemon
restarted — gets its badge from the re-read.

## The environment a check runs `git` in

Every `git` run for a project's git status — the remotes read, the fetch, the branch read, and a fast-forward, whether
it comes from a check, from Pull current branch, from the pass that turning the switch on starts or from a change to
the repository's refs — is the `git` on the user's `PATH`, started from the user's own login shell environment, but
with that environment **filtered**. Their git configuration files and their credential helpers are used as they
stand.

What the filter takes out are the `GIT_*` variables the user exports in their own shell configuration. Kept are only
those that say which of the user's own material to use and how to reach a remote with it: where git's configuration
files live, the TLS certificates and keys a connection uses along with their formats, the TLS version and cipher list,
and the git protocol proxy. Everything else is dropped — in particular anything that would point a command at another
repository, worktree or index, anything that sets configuration items outright, and the variable that turns TLS
certificate verification off.

Two consequences are worth knowing:

- Certificates and keys an enterprise network needs still reach the fetch, and a push, when they are given through
  those variables alone rather than through a configuration file.
- A setting only a dropped variable carries does not apply. The check then fails, rather than quietly reporting the
  branch and counts of whatever repository such a variable names, and its reason reaches the badge like any other
  failure (see "The branch badge" above). A push fails the same way, as an error alert, rather than
  publishing some other repository (see "Pushing the current branch" in `docs/product/project-git-actions.md`).

A git association's clone, and the check the association form makes of a git URL, run with the same filtered
environment (see "Associating a project" in `docs/product/projects.md`).

`git` is run so that it can never stop to ask for a credential, a passphrase or an unknown host key — each of those
fails immediately instead — and every `git` call of a check is given at most two minutes, so a check cannot hang; a
call that runs out reports only that it did not finish in time. A push's own bounds are in "Pushing the current
branch" in `docs/product/project-git-actions.md`.

**A command that reaches a remote uses the user's own ssh.** The fetch, the push, a git association's clone and the
form's check of a git URL reach the remote, and so, in a partial clone, can the fast-forward and a branch switch, which
fetch the objects they lack. Each runs the ssh program `git` itself would pick: the shell's `GIT_SSH_COMMAND`, else the
`core.sshCommand` setting, else the shell's `GIT_SSH`, else `ssh`; the shell's `GIT_SSH_VARIANT` is kept for a
`GIT_SSH` wrapper. Where that program is `ssh` itself, it is kept from prompting as above. Any other program — `plink`,
a wrapper script — runs as the user configured it, and a prompt it opens is ended only by the command's time limit. A
`core.sshCommand` that cannot be read leaves plain `ssh`. For the clone and the form's check, `core.sshCommand` comes
from the user's system and global configuration only, so one that only a conditional include (`includeIf`) supplies
for the new repository's directory is not used there; `GIT_SSH_COMMAND` or a global `core.sshCommand` is.

A `git` that only reads — the remotes read and the branch read, among others — never starts the file-system monitor the
user's configuration may name (`core.fsmonitor`), and never rewrites the repository's index. A command that writes,
the fetch, the fast-forward, the push and its hooks among them, keeps the user's monitor; only restoring file changes
(see "Restoring file changes" in `docs/product/project-git-actions.md`) and the branch and worktree cleanup turn it off
for their writes too.

### Where `git` is found

Every `git` Octoboard runs for the user's repositories — this page's, the Git actions', the project pane's Git mode,
adding and cloning a project, the branch and worktree cleanup — is the one the `PATH` of a capture of the user's
environment resolves: their login shell's, or on a Windows daemon host the environment built as in "The launch
environment" in `docs/product/launching-agents.md`. Which capture depends on who is asking:

- **Work that repeats or that no person asked for** uses the capture Octoboard already holds, which may be up to an
  hour old, a failed one included, and never captures afresh to find `git`, so a `git` that is missing does not cost a
  login shell on every round. That is the checks of a round, the local re-reads of the branch, the form's check of a
  git URL as it is typed (see "Associating a project" in `docs/product/projects.md`), the removal of branches and
  worktrees that follows an archive or a deletion, the project pane's reads repeated while it is on screen, and every
  tool a session calls.
- **A request a person makes** looks once more where that capture finds no `git`: it takes a fresh capture and uses
  the `git` found there. The fresh capture becomes the one Octoboard holds, so the repeating work finds that `git` from
  then on too. So installing Git, or putting it on `PATH`, and then retrying works without restarting anything. That is
  **Pull current branch** and **Try again** on the branch badge's failure mark (see "Checking again after Git was not
  found" above), **Push current branch**, **Switch branch** — the list of branches read as its submenu opens, its Try
  again, and the switch — and **Restore file changes** (see `docs/product/project-git-actions.md`), adding a single
  directory that does not exist yet, cloning a git URL and **Try again** on the form's failed check of one (see
  "Associating a project" in `docs/product/projects.md`; a clone looks first in the capture a session launch would
  use), the confirmations to archive, delete or hand off sessions and to remove a project as they look up what goes
  along, the **Try again** of the handoff's and the project removal's marks included, and the removal of a project's folder as it works
  that out again, and in the project pane's Git mode **Refresh** and **Try again** (see "The repository and its
  worktrees" in `docs/product/project-pane-git-mode.md`).

A `git` that none of the captures finds is the failure `binary_not_found` naming `git`. It is the error a request is
refused with, the failure the badge records (see "The branch badge" above), the reason the Git mode gives for Git review
being unavailable, and the reason a project removal could not work out what went along with its folder (see "With a
project's folder moved to the Trash" in `docs/product/branch-and-worktree-cleanup.md`). Wherever it is shown, it is
worded as Git not being installed or not on `PATH` (see "Recovery and unexpected errors" in
`docs/product/error-alerts.md`).

## Automatically pulling repositories

Settings' **Git** section (see "Automatically pull repositories" in `docs/product/git-settings.md`) holds the switch **Automatically pull
repositories**, **off** to begin with. It is stored by the daemon, so every window and every client sees the same
value and it survives a restart.

Off and on alike, the check above runs and the branch and the counts stay accurate. The switch governs what happens
beyond reporting: with it on, a branch that is **behind its upstream and has no commits of its own** is pulled —
at the end of each check, and once immediately when the switch is turned on (see "Turning the switch on" below) —
and the badge shows the pull in flight. **Pulling** a branch here always means fast-forwarding it to its upstream,
nothing more. With the switch off, only Pull current branch pulls (see "Pulling one project by hand" below), under
the same rules:

- It **never pushes**, and never merges anything that is not a fast-forward. A push happens only when the user
  chooses **Push current branch**, and it is never part of this pull (see "Pushing the current branch"
  in `docs/product/project-git-actions.md`).
- A branch that is ahead of its upstream, whether or not it is also behind, is left alone. So are a branch with no
  upstream and a detached `HEAD`.
- Whether the project has a running agent session makes no difference: the working tree can move under an agent that
  is working in that project.
- A fast-forward `git` refuses records its message for the badge, as a failed fetch does.

Pulling a branch — with the switch on, or by Pull current branch — moves that branch's working tree. Pushing
the current branch does not: it updates the remote and, in the repository, the remote-tracking refs `git` updates
for that push, and when the branch had no upstream it records one. It does not edit the project's files (see
"Pushing the current branch" in `docs/product/project-git-actions.md`). Switching to a local branch moves the
project's own working tree (see "Switching the local branch" in `docs/product/project-git-actions.md`). Restoring
file changes discards tracked changes without moving the branch (see "Restoring file changes" in
`docs/product/project-git-actions.md`). A check's fetch still updates remote refs only and still does not move the
working tree or push (see "What one check does" above). The guarantee these qualify is in "What Octoboard never
modifies" in `docs/product/launching-agents.md`.

### Pulling one project by hand

A project's action menu offers **Pull current branch** under **Git** while the project's status says its directory is a
git repository — so not before its first status has arrived (see "What one check does" above, and "Project rows" in
`docs/product/sidebar-project-rows.md` for where the menu is). It does not push. Choosing it closes the menu and starts
the check without waiting; a success raises no notice. It runs that project's check right away and then fast-forwards
the branch under exactly the rules above, whether or not **Automatically pull repositories** is on; choosing it is the
consent the switch would otherwise give. The one-minute floor does not apply.

Pull current branch does not wait its turn behind the four at once (see "When a project is checked" above). If a
check of the project is only waiting its turn — in a round of checks, or in the pass that turning **Automatically
pull repositories** on starts (see "Turning the switch on" below) — that check starts at once as this request's
check, with the fast-forward, in its place. If a check of the project is already running, or that pass is already
fast-forwarding it, nothing starts alongside it: as soon as it ends, one more check runs, with the fast-forward, so
the request ends in a fast-forward attempt either way. However many requests arrive meanwhile, they share that one
extra check, and it does not run at all if the project has been removed in the meantime.

The badge shows the check and the pull in flight as for any check, and a failed fetch or a fast-forward `git`
refuses records its message for the badge the same way.

### Turning the switch on

Turning the switch on takes effect at once: the branches already known to be behind their upstream are fast-forwarded
there and then, instead of waiting for the next check. Setting the switch to the value it already has does nothing.

This pass **never goes to the remote** — nothing is fetched. The statuses Octoboard is already holding only pick which
projects are worth visiting: every project that has a status at all is visited, across all consoles, including
projects whose status came only from a local re-read (see "Following changes to the repository's
refs" above). A project with no status is not visited — nothing is known to fast-forward it to.

Before anything moves, each project's repository is read again — locally, still no network — and the same rule applied
to what it says now, because an arbitrary amount of time may have passed since the status was filled in. So a project
whose branch has been switched, whose `HEAD` has been detached, which has commits of its own, or whose directory is no
longer a git repository is left alone; and so is one whose local re-read fails, since nothing is fast-forwarded on a
rule that could not be checked.

The one-minute floor between checks (see "When a project is checked" above) does not apply to this pass, which goes
nowhere near the remote; the four at once does, shared with the checks. A project whose check is in flight or waiting
its turn is skipped and left to that check, which reads the switch itself.

The badge shows the pull in flight exactly as during a check, and a fast-forward `git` refuses records its
message for the badge the same way. A failure the last check left stays on the badge: it stands for that check
against the remote, which a local fast-forward says nothing about. An error this pass records stays until a later
check succeeds, whichever console is current. A status is dropped only when the project or its console is deleted.

Turning the switch **off** undoes nothing and triggers nothing: branches already fast-forwarded stay where they are.
