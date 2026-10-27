# The revert gate

The **revert gate** stops git run inside a project session from quietly undoing changes already on a branch other
work is merged into. The typical cause is a squash that changed the base without merging the content — committing
after `git reset --soft <target branch>`, say — so old code goes back in as if it were new, and `git merge --ff-only`
fast-forwards it all the same because it checks only ancestry. The gate checks by content instead, and refuses the
update or the push before it happens. Ordinary commits, rebases, merges and pushes go through.

It is off by default (opt in per project or application-wide) and runs whenever the project's effective `revert_gate`
git workflow setting is on (see "The switch" below). It needs nothing installed,
in the project or on the machine: it runs from the hook dispatchers every project session's git is already pointed at
(see "The session's git hooks" in `docs/product/branch-responsible-session.md`), as part of Octoboard itself, and
depends on no interpreter such as Python — on a fresh macOS, `/usr/bin/python3` is only a stub that prompts to install
the Command Line Tools.

## Guarded branches

The local updates the gate checks are those of a **guarded branch**:

- **the project's default branch**: the branch a remote's `HEAD` names in the repository (`refs/remotes/<remote>/HEAD`),
  taking `origin` where it has one and otherwise the first remote by name. A repository with no remote, or whose
  remotes record no `HEAD`, has no default branch to guard;
- **every recorded target branch**: the branch some branch a session created was created from (see "Target branches
  and fork points" below).

Which branch is checked out where plays no part, so rebasing or amending a branch of one's own is not checked, and a
target branch nothing has recorded is not guarded.

## Target branches and fork points

When a session's git creates a branch and Octoboard records its responsible session (see "When a branch gets a
responsible session" in `docs/product/branch-responsible-session.md`), the same record keeps:

- its **target branch**, the local branch it was created from: a local branch start point; the local branch of the
  same name behind a remote-tracking start point (`origin/main` gives `main`, where a local `main` exists); for a
  creation from `HEAD` (`git switch -c`, `git checkout -b`, `git worktree add -b` with no start point), the branch
  `HEAD` was on. A branch created from a tag, a commit, an expression such as `main~2` or a detached `HEAD` has no
  target branch. In a repository whose references are kept in reftable format the start point cannot be read, and the
  branch `HEAD` was on is taken whatever the start point was;
- its **fork point**, the commit it was created at.

Both follow the branch's record: a rename carries them to the new name, a handover leaves them as they are, and they go
when the record goes. A branch no session's git reported creating has neither.

## What it refuses

### A local update of a guarded branch

Any update that moves a guarded branch which already exists is checked, whatever command made it — a commit, a merge
(`--ff-only` included), `git reset`, `git update-ref`, `git branch -f`, a rebase's last step. Creating or deleting a
branch is not checked. The update is refused when:

- **it undoes a recent commit on the branch**: it takes back what one of the branch's recent non-merge commits changed
  — the lines the commit added mostly gone and the lines it removed mostly back, or the file returned to how it was
  before the commit. Recent means the 50 most recent, and, where the branch moves to the tip of another branch with a
  recorded fork point (a fast-forward merge of it, say), every commit since that fork point as well, up to 1,000;
- **it carries a changed base**: where the branch moves to the tip of another branch with a recorded fork point, it
  changes a file that other branch itself never touched since its fork point — what a squash onto a changed base
  drags in, an addition, a deletion or an edit alike. A branch with no recorded fork point is not protected against
  this;
- **it drops published commits**: a move that is not a fast-forward is refused when any commit it drops is already on
  a remote-tracking branch. A move to a remote-tracking tip of the branch that drops no unpushed local commit passes
  outright; one that only replays unpushed local commits onto the remote (`git pull --rebase`, `git rebase
  <branch>@{u}`) passes when it undoes nothing, where what is already undone on the remote does not count against it
  and commits just fetched from the remote count as well.

### A push

A push of a local branch to a branch of the remote is checked, every branch of it. This is what covers a project that
delivers through pull requests: its target branch never moves locally, so a check of local updates alone would never
see the change.

- **a branch with a recorded target branch**, pushed once it has been rebased onto that target's remote-tracking tip
  (the target's upstream, or else its one remote-tracking branch of the same name), is refused when, against that tip,
  it undoes the target's changes as above, counting back to its own fork point. A branch not yet rebased onto that tip
  is not checked, and a force-push after a squash is expected and allowed. A target branch with several remote-tracking
  branches of its name and no upstream cannot be pinned down: the push goes through unchecked, with a line saying so; a
  target on no remote is passed silently. The target's remote state is as of the last fetch;
- **a guarded branch** is refused when the push is not a fast-forward of what the remote has, when it undoes the
  remote's recent commits as above, or when the remote holds commits the clone does not have yet.

A branch with no recorded target branch that is not guarded is pushed unchecked, and so is a deletion.

## A deliberate revert

A commit that undoes another passes when it says so: the message `git revert` writes (`This reverts commit <sha>.`),
or a `Reverts: <sha>` line per undone commit in the message of the commit that undoes it.

`REVERT_GATE_SKIP=1` in git's environment skips the gate for that command.

## What the agent sees

A refusal fails the git command with a message, prefixed `[revert-gate]`, that names the branch, lists each undone
commit with the files that show it and why, gives the likely cause and how to redo the work so it combines both sides,
and how to mark a deliberate revert. For a refused local update it also says how to restore the working copy for what
was stopped part-way — a merge or pull, a reset, a rebase. For a refused push it says to fetch, rebase or add a commit
that brings the changes back, never to force-push or reset.

## The switch

The gate runs whenever the project's effective `revert_gate` setting is on (see "The four settings" in
`docs/product/git-workflow-settings.md`), application-wide with a per-project override. `revert_gate` is off by
default: Octoboard does not assume a project wants it, so a project, or the whole installation, opts in.
The setting is read every time the gate runs, so turning it off or on reaches running sessions at once, without
relaunching them. Where project sessions develop plays no part: the gate runs alike whether they work on the current
branch, on a branch of their own or in a worktree of their own. Where the user turns it on and off is in "Reading and
changing them" in `docs/product/git-workflow-settings.md`.

## Where it runs

- **Only git run inside a project session**, in the repository the session's project is in — its main working copy or
  any linked worktree of it. The user's own terminal, IDE or git client, a console session, and a session's git working
  in any other repository are not checked. A git that does not reach the session's hook dispatchers — the cases listed
  in "Where a branch has no responsible session" in `docs/product/branch-responsible-session.md` for a git run
  without the session's environment or told to look elsewhere for hooks, and a session for which the hooks were not
  set up (see "What attribution needs" there) — is not checked either.
- **After the user's own hook.** The gate is asked only once the hook the user's configuration names for
  `reference-transaction` or `pre-push` has passed; a hook that refuses has the last word.
- **Beside a project's own gate, not instead of it.** Octoboard does not look for a gate a project installed itself;
  such a gate runs as any user hook does. A project that wants only its own turns the built-in one off.
- **Nothing is written** to the repository or its configuration; what the gate needs is kept in Octoboard's own
  storage.

## When it cannot decide

The gate fails open. Git goes through, saying nothing, whenever the daemon cannot be reached, does not answer within
about a second and a half, or answers with anything else; and it goes through when the gate's own check fails, with a
line saying the check failed and the update was allowed. A failing hook fails every reference update git makes, so the
gate refuses only an update it has judged; an error of its own, or a daemon that is down, is no reason to stop the
user's git.

## What it costs

Every move of an existing branch and every push of a branch waits while the dispatcher asks the daemon; the content
check runs only for a guarded branch, or a pushed branch with a recorded target branch. Measured on a clone of some 800
commits, on a machine under load, the round trip to the daemon added roughly 10–80 ms to a branch move, the check of a
guarded branch roughly 150–500 ms more, and a move of a branch that is not guarded cost about 10 ms in all.
