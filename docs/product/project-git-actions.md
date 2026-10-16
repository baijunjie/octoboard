# Project Git actions

Three of the actions under **Git** in a project's action menu (see "Project rows" in `docs/product/sidebar.md`):
**Push current branch**, **Switch branch** and **Restore file changes**. The menu's other action, **Pull current
branch**, and the branch badge these actions update are in `docs/product/project-git-status.md`.

## Pushing the current branch

**Push current branch**, under **Git** in the project's action menu, pushes that project's current branch and only
that branch. It is offered only while the project's status says the directory is a git repository (see "Project
rows" in `docs/product/sidebar.md`). Choosing it closes the menu at once. The push is waited on until `git push` has
exited. When the push is accepted, the window that asked shows a notice toast saying the current branch was pushed.
A refusal is an error toast worded from the code below. Pushing does not fetch and does not fast-forward, and a
pull never pushes (see "Automatically pulling repositories" in `docs/product/project-git-status.md`). A local
branch that is behind its upstream stays behind; if the remote rejects the update, the push fails and the branch
stays where it was.

The push never uses `--force`, and it is never a bare `git push`, which would follow `push.default` and can publish
every branch whose name exists on the remote. The one branch is named explicitly, so only that branch is published.
The user's git configuration and credential helpers are used as they stand. `git` cannot stop to ask for a
credential, a passphrase or a host key — the same refusal as a fetch (see "The environment a check runs `git` in"
in `docs/product/project-git-status.md`) — and the push itself is given at most two minutes.

Once the directory is a repository, the branch is read first, locally, and what it says — the branch, its upstream
and the counts — is published to every window showing the project before the push is shown in flight. So the ahead
count the push is drawn on includes commits nothing had re-read yet. A warning the last check left stays. The
repository's remotes are read next; that read is filtered and non-interactive in the same way, and is given at most 30
seconds. If either read fails, the push fails and `git push` is not run, so a detached `HEAD` whose remotes cannot be
read is reported as that failed read rather than as not being on a branch. A branch read that fails publishes nothing,
and the push is never shown in flight.

What is pushed, when both reads succeed:

- A branch that has an upstream is pushed to that upstream's remote and remote branch. The remote is the longest
  configured remote name that is a prefix of the upstream followed by `/`, and the rest of the upstream — which
  must be non-empty — is the remote branch. A remote named `fork/origin` wins over `fork` for an upstream
  `fork/origin/main`; a remote named `origin` does not match an upstream `origin-fork/main`. The remote branch
  need not have the same name as the local branch. This push does not change the upstream.
- A branch with no upstream is pushed to `origin` when the repository has that remote, otherwise to its only
  remote, and that push records the upstream under the local branch's own name.

Nothing is pushed when the push is refused. A refusal leaves the last check's error on the badge as it was and is
reported by its error toast. Every refusal other than `unknown_project`, `not_a_git_repository` and a branch read that
fails comes after the branch read was published, so the badge already shows that read's branch and counts:

- The project directory is not a git repository when the push runs: `not_a_git_repository`. The rule is the badge's
  (see "What one check does" in `docs/product/project-git-status.md`): `.git` must be in the project directory
  itself. A subdirectory of a repository is not one.
- `HEAD` is detached: `not_on_a_branch`.
- The branch has no commit yet: `no_commits_to_push`.
- The upstream names no configured remote of this repository: `upstream_not_a_remote`.
- There is no upstream, and the repository has neither an `origin` nor exactly one remote: `no_push_remote`. No
  remotes at all, and several remotes with no `origin`, are both this refusal.
- A branch or remote name is empty, starts with `-`, or contains `:`, a NUL or a newline: `git_push_failed`. `git`
  is not run. The reason says the name cannot be used in a refspec. The local branch's name is judged before the
  upstream. A remote or remote-branch name is judged only after the upstream has matched a remote, so an upstream
  that matches none is `upstream_not_a_remote` even when its text would not be a usable name.
- The branch cannot be read, or the remotes cannot be read: `git_push_failed`. The branch's reason is `git`'s own
  message, or that the read did not finish in time. The remotes' reason says they could not be read.
- `git push` itself fails, including a remote that rejects the update, a missing credential, or a push that does
  not finish within two minutes: `git_push_failed`, with `git`'s own message, or that it did not finish in time.

A project that no longer exists is `unknown_project`.

The push and a check are not ordered. **Pull current branch**, or a scheduled check, that arrives during a push is
handled as it would be with no push in flight: it is not queued behind the push. The badge shows the push in flight
while the check can still update its branch, counts and error. When the push ends, a check or pull still in flight is
shown again. A second push of the same project is not refused while one is still running; the pushing state stays until
the last push ends. The pushing state begins once the branch read has been published, covers the remotes read as well
as `git push`, and ends on a refusal too.

A successful push does not edit the project's files and does not move the working tree. It updates the remote, and
in the repository it updates the remote-tracking refs `git` updates for that push. When the branch had no upstream,
it also records one. The daemon then re-reads the local branch header and publishes the project's git status, with
no fetch, so the badge's ahead count can drop. Every window showing the project sees that status. This re-read does
not end the badge's indication of an overlapping check or pull. An error the last check left on the badge
stays, unless that re-read fails, in which case the warning becomes the re-read's message and the branch and the counts
stay as they were. A re-read that fails does not turn the notice into an error toast: the push was accepted.
Turning **Automatically pull repositories** on never pushes.

## Switching the local branch

**Switch branch**, under **Git** in every placement of the project's action menu, opens a third-level menu of
existing local branches. Remote-only branches are not offered. The list is read afresh when the submenu opens,
in byte order of branch names, with the project's own worktree's current branch checked. Selecting that branch
closes the menu without requesting a switch.

While the list is being read, the submenu shows a loading state. No local branches produces an unavailable empty
item. A failed read shows its reason and **Try again**, which reloads without closing the menu. At most 10,000
branches, or as many as 2 MiB of encoded names hold, are listed; a truncated list carries an incomplete-list notice.

Selecting another branch closes the menus and switches immediately, without confirmation. It changes the project's
own worktree, including a linked worktree, regardless of which worktree the Git pane is reviewing. It never creates
a branch, guesses a remote branch, fetches, pushes or pulls. It does not force the switch or stash or discard local
changes: changes Git can carry safely stay, and a conflict, a branch checked out in another worktree, or another Git
refusal fails with an error toast. Running agent sessions do not prevent a switch; the working tree can change
underneath them. Other Git operations are not serialized with the switch.

The directory must contain its own `.git` entry, as for the badge. A missing project is `unknown_project`, a
directory that is not a repository under that rule is `not_a_git_repository`, and an invalid or vanished local
branch or a Git failure is `git_switch_failed`, with the reason as `detail`. Each Git call uses the filtered,
non-interactive environment described in "The environment a check runs `git` in" in
`docs/product/project-git-status.md` and has at most two minutes.

Success raises no toast. The local branch and upstream counts are read again for the badge in every window,
without fetching; an earlier remote-check warning stays unless that read itself fails. A failed status re-read
records its warning without turning a completed switch into a switch error. The project pane refreshes when the
reported branch changes (see "Files and Git" in `docs/product/project-pane.md`).

## Restoring file changes

**Restore file changes**, under **Git** in every project's action-menu placement, follows Switch branch
after a separator (see "Project rows" in `docs/product/sidebar.md`). It opens a destructive confirmation dialog
explaining that staged and unstaged tracked changes will be discarded, staged new files deleted, and untracked and
ignored files kept, and that the discard cannot be undone. Cancelling makes no restore request. Confirming waits
for the restore; success closes the dialog and raises a notice toast. A failure stays in the dialog as an inline
error, leaving it open.

Restoration covers the project's own worktree: tracked files and the index are restored to the current commit,
`HEAD`, including tracked deletions and renames. Staged additions absent from that commit are deleted. It leaves
untracked and ignored files, submodule worktrees and their gitlinks in the index, commits, the branch, and other
worktrees unchanged. It fetches and pushes nothing. A successful restore re-reads the local branch status for the
badge; a failure publishes no such re-read.

The project directory itself must contain `.git`, as for the branch badge; a project pointing at a repository's
subdirectory is refused. The other refusals are:

- A project that no longer exists: `unknown_project`; a directory that is not a repository under that rule:
  `not_a_git_repository`.
- Detached `HEAD`: `restore_not_on_branch`; a branch with no commit yet: `no_commits_to_restore`.
- A merge, rebase, cherry-pick, revert or bisect in progress: `git_operation_in_progress`. Finish or abort it first.
- A tracked path blocked by a directory, or an ancestor blocked by a file or symbolic link; or a path in `HEAD`
  removed from the index but still present on disk: `git_restore_failed`. These collisions are refused before
  restoration to preserve untracked or ignored content; move the obstruction aside first.
- Git cannot inspect or restore the files: `git_restore_failed`, with the reason as `detail`. Each Git call has at
  most two minutes, 16 MiB of standard output and 64 KiB of standard error; an inspection exceeding a bound fails
  before restoration.

A restore failure does not promise rollback: Git can restore some paths before a later path fails. Running agent
sessions do not prevent restoration, and their Git operations, checks, pulls, pushes and other restores are not
serialized with it. The files can change underneath a running agent.
