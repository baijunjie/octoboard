# Which session is working on a branch

Octoboard records, for a local branch of a project's repository, its **responsible session**: the session working on
it now (see "The responsible session" below). The project pane's Git mode marks the branches that have one, and
follows in its selectors the branch the selected session most recently became responsible for (see "Following the
selected session" in `docs/product/project-pane-git-mode.md`). A branch gets a responsible session only when **a
session's own git reported creating it**, and it moves only by an explicit handover; every other branch is unmarked.
Nothing is inferred from timing, from which session was busy or from where a session works, so a responsible session
is never a guess, and a branch with none — most branches — is no failure.

## The responsible-session badge

In the **Compare** view of the project pane's Git mode, each branch's row in the From and To lists carries a small
ring when the branch has a responsible session — the **responsible-session badge** (see "The branch selectors" in
`docs/product/project-pane-git-mode.md`).

- The name assistive technology announces for the badge, and its tooltip, are the same words: they say that session
  is working on the branch and name it by its title. The tooltip sets the title apart as a chip, so that where a long
  title ends can be seen; the announced name carries it as plain text, kept apart only in its writing direction from
  the words around it. The badge is no tab stop, so the tooltip shows on pointer hover only, and only while the
  pointer is on the ring itself rather than anywhere on the row; it opens after the same wait as every other tooltip
  in the window (see "Tooltips on indicators" and "How a tooltip behaves" in `docs/product/labels-and-tooltips.md`).
- **A session that still exists is named by the title it carries now** — the one the sidebar shows — so that the
  session named can be found there. An archived session still exists and is named the same way.
- **A session that no longer exists** is named instead by the title recorded when it became responsible (see "The
  responsible session" below), and the wording says that it was working on the branch and no longer exists. Deleting
  an archived session, or removing a project that shares its repository with another project, takes the session's
  record while the branch's record stands. Until the window has its first answer from the daemon no session is called
  gone.
- **A branch the selected session is working on** is marked apart from the rest: its ring is filled and in the accent
  colour where every other badge is a hollow ring in a neutral one, and its wording says the session working on it is
  the selected session as well as naming it. The fill, not the colour, is what tells the two rings apart, so the state
  does not rest on colour. A selected session that no longer exists is named as gone instead, like any other.
- **A branch handed over** carries the badge of the session it was handed to from the move on.
- Beyond that the badge carries nothing its name does not: its presence says a session is working on the branch, the
  name says which.
- **A chosen branch carries no badge**: the mark is on the rows of the two lists only, never on a selector once a
  branch is chosen.
- A branch with no responsible session carries no mark of any kind, and nothing says why.

## The responsible session

A branch has at most one responsible session at a time; a session can be responsible for several branches.

- **It starts as the session whose own git reported creating the branch**, from the branch's creation on (see "When a
  branch gets a responsible session" below). A branch no session's git reported creating has none, and cannot be handed
  over.
- **It moves only by an explicit handover** of the branch to another session: the user's approval of a
  `request_session` call that names the branch, which makes the session the approval starts its responsible session
  (see "Handing branches over" in `docs/product/session-requests.md`). Nothing else moves it; handing a session to
  another owner, or a session binding itself to one, leaves the branches it is responsible for where they are. Who
  was responsible for a branch before is not kept.
- **The record holds the session's title from when it became responsible**, to name a session that no longer exists:
  for the session whose git reported the branch, its title when the report was taken; for a session a handover made
  responsible, the title it was started with. Either is cut to its first 80 characters.
- **It is part of the branch's record** (see "What the record is" below) and follows it: it moves to the new name with
  a rename, goes when the branch is deleted, is not inherited by a branch deleted and created again under the same
  name, and is cleared with the rest when the repository's records are. It can name a session that no longer exists:
  deleting a session leaves it as it is.

## When a branch gets a responsible session

Every launch of a project session's agent — a fresh one, a resume or an account switch alike — points the git of
that agent's whole process tree at Octoboard's hook dispatchers (see "The session's git hooks" below). The agent's
shell, the scripts and aliases it runs, the background jobs it starts and its subagents' commands all inherit that. When
one of those gits completes a reference transaction that creates a local branch, git's own `reference-transaction`
hook reports the branch under the session the agent was started for, and Octoboard records that session as the
branch's responsible session — the branch's **attribution**.

- **What ran is never read.** `git branch`, `git checkout -b`, `git switch -c`, `git worktree add -b`,
  `git update-ref`, an alias or a script of the agent's own all count the same: what counts is that a branch which did
  not exist now does.
- **A forced update of an existing branch is no creation** — `git branch -f`, `git checkout -B`, `git switch -C`, or
  `git update-ref` with no expected old value, on a branch that already exists — and leaves whatever record the branch
  has.
- **A copy is no creation**: a branch made by `git branch -c` gets no responsible session.
- **A rename keeps the record**: a branch renamed with `git branch -m` carries its record over to the new name, the
  next time the Git mode lists the branches.
- **Project sessions only.** A console session's agent, which runs in a directory of Octoboard's own (see "Console
  sessions and project sessions" in `docs/product/sessions.md`), is given nothing, and a console session is never
  named.
- **A session that has ended still counts.** A branch created by a background job the session left running, after the
  session's agent stopped or the session was archived, still names the session. A report for a session that has been
  deleted is dropped.
- **Only in the session's own project's repository.** A report counts only for a branch created in the repository the
  session's project is in — its main working copy or any linked worktree of it. A branch the session's git creates in
  any other repository, a clone of the project's included, is ignored.
- **Only while it is still that branch.** In a repository that keeps reflog files, a report counts only while the
  branch's reflog still begins with its creation at the reported commit, within a minute of the report, so a branch
  deleted and made again before the report lands is not given it.
- A creation is recorded once, and a record is replaced only by a later creation of a branch of the same name.

The report is sent in the background once git has finished, so git never waits on it, and its responsible session can
show up on a later listing of the Git mode than the branch itself.

The records belong to the **repository**, not to one project. With two projects in the same repository, a branch
whose responsible session is a session of either project is marked in both projects' Git mode, and the session named
can be one of the other project's.

### What attribution needs

- **git 2.36 or later**, as the user's login-shell `PATH` resolves it at the launch — the environment the agent itself
  runs with (see "The launch environment" in `docs/product/launching-agents.md`). With an older git, nothing is set
  for the session, and no branch it makes gets a responsible session.
- The daemon's own executable found, and Octoboard's data directory writable for the dispatchers (see "The session's
  git hooks" below).
- A `GIT_CONFIG_COUNT` exported by the user's shell that is a number, if there is one at all; one that is not already
  fails every git the user runs, and the session is given nothing.

Where any of these is missing the session runs as it would without attribution, and nothing says so. Each git and
daemon executable is checked at the first launch that uses it; a check that failed is tried again by a launch at least
ten minutes later.

## The session's git hooks

The agent's environment gets `core.hooksPath` through git's own `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_<n>` and
`GIT_CONFIG_VALUE_<n>` variables, in the next free slot after the entries the user's shell exports (which keep their
order and their effect), together with `OCTOBOARD_SESSION_ID` and `OCTOBOARD_DAEMON_PORT`, which say where a report
goes. Nothing is written to a configuration file or a repository, and nothing reaches a process outside the agent's
tree.

- **The dispatchers** live in `~/.octoboard/githooks/<content hash>/`, shared by every session, written once and left
  read-only. It holds a copy of the dispatcher under every hook name git documents: a built-in list, together with
  every name the installed git's own githooks manual page, or its HTML page where no manual page is installed, lists.
  A directory of an earlier content that has not been used for 30 days is removed. Beside them,
  `~/.octoboard/githooks/state/` holds short-lived files of the gits at work, each removed after a day at the latest.
- **The user's own hooks still run.** Each dispatcher runs the hook the user's configuration names, exactly where git
  would have found it without Octoboard — the `core.hooksPath` of any configuration file, include, `-c` option or
  exported entry, or else the repository's own hooks directory — with the same arguments, input and environment. Only
  Octoboard's own `core.hooksPath` entry is taken out, so the hook, and every git it runs, sees the user's
  configuration alone. The hook's outcome is git's: one that aborts a commit or a transaction still aborts it.
- **A dispatcher refuses rather than skipping a hook.** When it cannot tell, from the configuration, where the user's
  hook is, it asks git; if git cannot answer, the dispatcher fails, saying it could not find out where that hook is,
  which fails whatever that hook can stop — a commit, say.
- **The one case where the user's hooks go unrun:** a sandbox an agent runs under that makes the dispatcher directory
  unreadable or not executable leaves git with no hooks at all, so none of the user's hooks run under that sandbox.
  A hook under a name in neither list above is not run in a session either.
- **What a session's git shows of it.** Inside a session, `git config --list` and `git config core.hooksPath` show the
  dispatcher directory, and `git rev-parse --git-path hooks` points there. A tool that installs hooks where git says
  they live fails inside a session, the directory being read-only — `lefthook install`, for one — and `pre-commit
  install` refuses because `core.hooksPath` is set. Run from the user's own terminal, the same tools install as usual,
  and the hooks they install then run in sessions too.
- **What it costs.** Every hook git looks for starts a dispatcher — a `/bin/dash` script where the system has one,
  `/bin/sh` otherwise — about ten for a commit. Measured on a Mac whose endpoint-security software slows every process
  start, a commit with no hooks of the user's took some 50–60 ms longer, about twice that where the user's
  configuration moves the hooks directory and each dispatcher asks git, and a branch creation about 20 ms longer.

## What the record is

- **Nothing is written to the user's repository.** No reference, note or configuration entry carries the record; it
  lives in Octoboard's own storage only, and the Git mode stays read-only (see "Reading only" in
  `docs/product/project-pane-git-mode.md`).
- **What a record holds**: the branch's birth — the commit and the second it was created at, which recognise the same
  branch across a rename and tell one deleted and created again under the same name apart — and its responsible
  session, with the title recorded for that session and when it became responsible. Nothing else about who created
  the branch is kept.
- **A record lives and dies with its branch.** Deleting the branch — after merging it, say — deletes its record, so
  a merged-and-deleted branch can no longer be traced. A branch deleted and created again under the same name is a new
  branch and does not inherit the old record.
- **When records change.** A record is written when a session's report is taken. Whenever the Git mode lists the
  branches, in either of its views, the records of the repository are brought up to date: the record of a branch that
  is gone is removed, one whose branch was deleted and made again is dropped, and a renamed branch's record moves to
  its new name. Nothing runs on a periodic timer.
- **Records from an earlier version.** A database from an earlier version of Octoboard keeps only the records that
  came from a session's git report: any other is deleted when the daemon opens it, and its branch is not attributed
  again. A record kept makes the reporting session the branch's responsible session from the branch's creation, named
  by the title recorded with the report.
- **Removing a project** clears the records of its repository, unless another project still points into that
  repository (see "Removing a project" in `docs/product/consoles-and-projects.md`). Deleting a console removes every
  project under it, so it clears theirs the same way.
- At most **1,000 branches of one repository** carry a record; past that, the records of the branches created longest
  ago are dropped.

## Where a branch has no responsible session

A branch no session's git reported creating has no responsible session; it never carries a wrong one in its place.
That is the case for:

- **a branch made outside a session's agent**: in the user's own terminal, by another application, or by any process
  that is not a descendant of a session's agent;
- **a git that runs without the session's environment**: under `sudo`, `env -i`, in a container or over a remote shell,
  or from a tool that rebuilds the environment before running git;
- **a git told to look elsewhere for hooks**: `git -c core.hooksPath=…`, or a `GIT_CONFIG_*` of its own that overrides
  Octoboard's entry;
- **the receiving repository of a local push**: git clears the `GIT_CONFIG_*` variables for the receiving side of a
  push it makes itself, so a branch `git push ../other` creates in another repository gets no responsible session —
  and that repository runs its own hooks as usual;
- **a copy** (`git branch -c`);
- **a report that cannot reach the daemon**: one made while the daemon is not running or once it has been restarted on
  another port than the session was started with, or from an agent whose sandbox blocks loopback network connections
  or writing outside its workspace (as Codex's can);
- **a repository that keeps no reflog for the branch** — with `core.logAllRefUpdates` off, as a bare repository has it
  by default — where a report cannot be told from one about a later branch of the same name, and is not taken;
- a branch made by a session started while attribution was not available to it (see "What attribution needs" above).

A record can also outlast the branch it was made for, or go early:

- In a repository whose references are kept in **reftable** format there are no reflog files. A record is taken at the
  reported commit and the report's time and cannot be held against the birth of its branch afterwards, so a branch
  deleted and created again under the same name between two listings keeps the first one's record, and its
  responsible session, until the name itself goes; and a rename is not followed, so the record does not move to the
  new name.
- In any repository, a branch deleted and created again **at the same commit within one second** keeps the first one's
  record the same way.
- A record does not outlive its reflog. `git gc` drops reflog entries older than `gc.reflogExpire`, 90 days by default;
  a branch whose first entry has gone that way loses its record, and nothing attributes it again.
