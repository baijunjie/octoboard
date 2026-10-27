# Which session is working on a branch

Octoboard records, for a local branch of a project's repository, its **responsible session**: the session working on
it now (see "The responsible session" below). The project pane's Git mode marks the branches that have one, and
follows in its branch selector the branch the selected session most recently became responsible for (see "Following the
selected session" in `docs/product/project-pane-git-mode.md`). A branch gets a responsible session only when **a
session's own git reported creating it**, and it moves only by an explicit handover or a handoff; every other branch is
unmarked.
Nothing is inferred from timing, from which session was busy or from where a session works, so a responsible session
is never a guess, and a branch with none — most branches — is no failure.

## The responsible-session badge

In the project pane's Git mode, each branch's row in the branch selector's list carries a small ring when the branch
has a responsible session — the **responsible-session badge** (see "The branch selector" in
`docs/product/project-pane-git-mode.md`).

- The name assistive technology announces for the badge, and its tooltip, are the same words: they say that session
  is working on the branch and name the session. The tooltip sets the session's name apart as a chip, so that where a
  long one ends can be seen; the announced name carries it as plain text, kept apart only in its writing direction from
  the words around it. The badge is no tab stop, so the tooltip shows on pointer hover only, and only while the
  pointer is on the ring itself rather than anywhere on the row; it opens after the same wait as every other tooltip
  in the window (see "Tooltips on indicators" and "How a tooltip behaves" in `docs/product/labels-and-tooltips.md`).
- **A session that still exists is named by its displayed name as it is now** — the one the sidebar shows, task title
  and created name together where it has a task title (see "Displayed names" in `docs/product/session-titles.md`) — so
  that the session named can be found there. An archived session still exists and is named the same way.
- **A session that no longer exists** is named instead by the created name recorded when it became responsible (see
  "The responsible session" below), without a task title, and the wording says that it was working on the branch and
  no longer exists. Deleting an archived session, or removing a project that shares its repository with another
  project, takes the session's record while the branch's record stands. Until the window has its first answer from the
  daemon no session is called gone.
- **A branch the selected session is working on** is marked apart from the rest: its ring is filled and in the accent
  colour where every other badge is a hollow ring in a neutral one, and its wording says the session working on it is
  the selected session as well as naming it. The fill, not the colour, is what tells the two rings apart, so the state
  does not rest on colour. A selected session that no longer exists is named as gone instead, like any other.
- **A branch handed over** carries the badge of the session it was handed to from the move on.
- Beyond that the badge carries nothing its name does not: its presence says a session is working on the branch, the
  name says which.
- **A chosen branch carries no badge**: the mark is on the rows of the list only, never on the selector once a
  branch is chosen.
- A branch with no responsible session carries no mark of any kind, and nothing says why.

## The responsible session

A branch has at most one responsible session at a time; a session can be responsible for several branches.

- **It starts as the session whose own git reported creating the branch**, from the branch's creation on (see "When a
  branch gets a responsible session" below). A branch no session's git reported creating has none, and cannot be handed
  over.
- **It moves only by an explicit handover** of the branch to another session, in one of four ways: the approval of a
  `request_session` call that names the branch, which makes the session the approval starts its responsible
  session (see "Handing branches over" in `docs/product/session-requests.md`); a `new_session` call that names the
  branch, which makes the session it starts, owned by the caller, responsible for it (see "Handing branches to a
  session the caller starts" below); an owner's `handover_branches` call, which moves it between the owner and a
  session of its own, or between two sessions of its own (see "Handing branches between sessions of an owner"
  below); or a handoff of the session to another agent, which makes the receiving session responsible for every
  branch the original was (see `docs/product/handing-off-sessions.md`). Nothing else moves it; handing a session to
  another owner, or a session binding itself to one, leaves the branches it is responsible for where they are. Who was
  responsible for a branch before is not kept.
- **The record holds the session's created name from when it became responsible**, to name a session that no longer
  exists: for the session whose git reported the branch, its created name when the report was taken; for a session a
  handover made responsible, its created name at the handover. Either is cut to its first 80 characters. No task
  title is recorded.
- **It is part of the branch's record** (see "What the record is" below) and follows it: it moves to the new name with
  a rename, goes when the branch is deleted, is not inherited by a branch deleted and created again under the same
  name, and is cleared with the rest when the repository's records are. It can name a session that no longer exists:
  deleting a session leaves it as it is. Archiving or deleting a session does delete the branches it is responsible
  for where git can do so safely, and a deleted branch's record goes with it (see
  `docs/product/branch-and-worktree-cleanup.md`); a branch that stays keeps its record.

## Handing branches to a session the caller starts

A project session that can own sessions — an unbound project session or a lead session (see "The unbound project
session's tools" in `docs/product/hub-orchestration.md`) — can pass `branches` to `new_session`: local branches of its
own project's repository that **the caller itself** is responsible for. The rules are `request_session`'s (see
"Handing branches over" in `docs/product/session-requests.md`): the same input checks; the same refusal for a branch
the caller is not responsible for, made before anything is started, so a refused call starts no session; the move made
right after the new session has started, and only for a branch the caller is still responsible for at that moment;
and a move that cannot be recorded does not fail the call, leaving the caller responsible for every branch it named.
What differs is that the new session is owned by the caller and no user approval is involved.

- **The opening prompt says so.** The brief is preceded by a paragraph naming the caller by id and title, saying that
  it started the new session and handed it those branches, naming each, and that the new session is now responsible
  for them and is to carry on the work on them. The paragraph is written before the move, so it names every branch the
  call named, whatever the move then finds.
- **The result says what became of them.** Where `branches` names at least one branch, the answer adds `branches`, the
  names of the branches moved (none when the move could not be recorded), and a `note`: those handed over, now the new
  session's responsibility and no longer the caller's; those not handed over because by then the caller was no longer
  responsible for them or they were gone, staying as they were; or that the move could not be recorded and the caller
  is still responsible for them.
- **Not for a console session.** A console session is responsible for no branch (see "Project sessions only" below):
  its `new_session` does not offer `branches`, and a call that names a branch in it anyway is refused before anything
  is started.

## Handing branches between sessions of an owner

The `handover_branches` tool, offered to every session that can own sessions, moves the responsibility for branches
without replacing any session (a handoff, by contrast, replaces a session with one of another agent). It takes
`branches` (local branches of the repository, at least one), `from` and `to`; either of `from` and `to` left out is
the caller itself, and the two must differ. No approval is asked: the caller owns the sessions concerned.

- **Who can give and who can take.** `from` is the caller or a session the caller owns directly, an archived one
  included, so that the branches an archived session left behind (named in `branches_left` when it was archived) can
  be recovered. `to` is the caller or a session the caller owns directly that is not archived; an interrupted one can
  take branches. A session owned by anyone else, or the user's own, is refused as in any tool that acts on a session.
  A console session is responsible for no branch, so a console session must name both `from` and `to`, each a project
  session it owns; leaving either out names the console session itself, which is refused.
- **What is checked.** The input is checked as for `request_session`: a list of strings with no blank name, each name
  trimmed and taken once. Each branch must have `from` as its responsible session in the repository, with the same
  refusal as `request_session` gives (naming `from` where it is not the caller). Both sessions' projects must be in the
  repository the branches are in. A refusal changes nothing.
- **A session at work cannot give branches up.** A `from` that is another session is refused while it is working,
  still starting or waiting for the user, because it would go on writing to the worktrees of branches that are no
  longer its own. Both its recorded status and what its agent is showing count, so a prompt the agent has put up is
  refused even before the session's status has caught up with it. Idle, interrupted and archived sessions can give
  branches up. The caller itself is always in a turn when it calls, and may give its own branches away.
- **Nothing else happens to the sessions meanwhile.** While a session other than the caller is being started,
  resumed, handed off, switched to another account or having its owner changed, a handover from or to it is refused,
  changing nothing; and none of that can begin on either of them while the handover is carried out. Messages for a
  `from` other than the caller are queued meanwhile, and delivered once the branches have moved. The checks above are
  made under that protection, so they still hold when the branches move.
- **Nothing in git or on disk changes.** A branch stays checked out wherever it is, with whatever uncommitted changes
  its worktree holds; uncommitted changes do not stop the move. The result carries `from`, `to`, and per branch moved,
  in `branches`: `branch`; `worktree`, the path of the worktree it is checked out in, or `null` when there is none or
  the worktrees could not be read; `worktree_status`, one of `checked_out`, `gone` (git still records a worktree for
  the branch but its directory is gone; `worktree` is that path), `none` (no worktree has it) and `unknown` (the
  worktrees could not be read); and `uncommitted_changes`, `true` or `false` for a checked-out worktree whose changes
  could be read, else `null`. A branch not moved because by then `from` was no longer responsible for it, or it was
  gone, is named in `not_handed_over`. The `note` says what was handed over and that cleanup follows the new
  responsible session. If no branch could be moved, or the move could not be recorded, the call is refused.
- **The sessions concerned are told.** `from` and `to`, each unless it is the caller, are sent a message naming the
  caller as their owner and listing each branch with where it is checked out and whether that worktree holds
  uncommitted changes. `from`'s says it is no longer responsible for the branches, must not touch them or their
  worktrees, and must leave any uncommitted changes there as they are. `to`'s says it is now the session responsible
  for them, the previous one no longer is, and its owner will tell it what to do with them. A session that is
  interrupted or archived, or that a message could not be written to, is not told; the `note` names it and asks the
  caller to tell it once it can take a message.
- **Cleanup follows the new responsible session.** Archiving or deleting a session removes the branches it is
  responsible for (see `docs/product/branch-and-worktree-cleanup.md`), so after the move the branches go with `to` and
  no longer with `from`.

## Reading the branches a session is responsible for

Every session, whatever its kind — a console session, an unbound or a lead project session, or one with the role of a
bound one — has the read-only `get_session_branches` tool. It shows beforehand what `handover_branches`, and the
`branches` of `new_session` and `request_session`, can take — each branch they name must be one the giving session is
responsible for — and which branches and worktrees archiving the session would try to remove (see
`docs/product/branch-and-worktree-cleanup.md`). Nothing in git or on disk changes.

- **Any session can be read.** `session` is optional and names any session on record, of any console or project,
  archived ones included; left out, it is the caller. A session that does not exist is refused.
- **What it answers.** `{ session, branches }`, `session` being the id of the session read and `branches` one entry
  per branch that session is responsible for: `branch`, the branch's name; `project`, the id of the project the
  session is in; and `worktree`, `worktree_status` and `uncommitted_changes` as the result of `handover_branches`
  gives them for each branch moved (see "Handing branches between sessions of an owner" above). When `git` cannot be
  run, or the worktrees cannot be listed, every branch is still listed, with `worktree_status` `unknown`, `worktree`
  `null` and `uncommitted_changes` `null`.
- **An empty list** for a session responsible for no branch, and always for a console session (see "Project sessions
  only" below).

The names alone are also part of every session record the orchestration tools give, as `branches` (see "Session
records" in `docs/product/what-every-session-reads.md`); reading them there runs no `git`, while
`get_session_branches` runs it to find the worktrees, and only when the session is responsible for a branch.

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

Where any of these is missing the session runs as it would without attribution, and without the revert gate, and
nothing says so. Each git and daemon executable is checked at the first launch that uses it; a check that failed is
tried again by a launch at least ten minutes later.

## The session's git hooks

The agent's environment gets `core.hooksPath` through git's own `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_<n>` and
`GIT_CONFIG_VALUE_<n>` variables, in the next free slot after the entries the user's shell exports (which keep their
order and their effect), together with `OCTOBOARD_SESSION_ID` and `OCTOBOARD_DAEMON_PORT`, which say where a report
goes and whom the revert gate asks. Nothing is written to a configuration file or a repository, and nothing reaches a
process outside the agent's tree.

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
- **The revert gate runs after the user's hook.** Once the user's `reference-transaction` hook has passed a
  transaction that moves an existing branch, or the user's `pre-push` hook has passed a push of branches, the
  dispatcher runs the revert gate, which can still refuse it (see `docs/product/revert-gate.md`).
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
  configuration moves the hooks directory and each dispatcher asks git, and a branch creation about 20 ms longer. A
  move of an existing branch and a push also wait on the revert gate (see "What it costs" in
  `docs/product/revert-gate.md`).

## What the record is

- **Nothing is written to the user's repository for the record.** No reference, note or configuration entry carries
  it; it lives in Octoboard's own storage only, and the Git mode stays read-only (see "Reading only" in
  `docs/product/project-pane-git-mode.md`). The record is what decides which branches and worktrees go when a session
  is archived or deleted (see "What is bound to a session" in `docs/product/branch-and-worktree-cleanup.md`).
- **What a record holds**: the branch's birth — the commit and the second it was created at, which recognise the same
  branch across a rename and tell one deleted and created again under the same name apart — and its responsible
  session, with the created name recorded for that session and when it became responsible; and, for the revert gate,
  the branch it was created from, with the commit of its birth standing as its fork point (see "Target branches and
  fork points" in `docs/product/revert-gate.md`). Nothing else about who created the branch is kept.
- **A record lives and dies with its branch.** Deleting the branch — after merging it, say — deletes its record, so
  a merged-and-deleted branch can no longer be traced. A branch deleted and created again under the same name is a new
  branch and does not inherit the old record.
- **When records change.** A record is written when a session's report is taken. Whenever the Git mode lists the
  branches, the records of the repository are brought up to date: the record of a branch that
  is gone is removed, one whose branch was deleted and made again is dropped, and a renamed branch's record moves to
  its new name. Nothing runs on a periodic timer.
- **Removing a project** clears the records of its repository, unless another project still points into that repository
  (see `docs/product/removing-projects.md`). Deleting a console removes every project under it, so it clears theirs the
  same way.
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
