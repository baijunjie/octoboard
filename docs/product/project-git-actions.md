# Project Git actions

The actions under **Git** in a project's action menu (see "Project rows" in `docs/product/sidebar-project-rows.md`):
**Pull current branch**, **Push current branch**, **Switch branch** and **Restore file changes**. The branch badge these
actions update, the checks against the remote and the automatic pulling of repositories are in
`docs/product/project-git-status.md`.

## Pulling the current branch

**Pull current branch**, under **Git** in the project's action menu, pulls that project's current branch right now and
always tells the user what it did, or why it could not. It never pushes. It is offered only while the project's status
says the directory is a git repository, so not before its first status has arrived (see "What one check does" in
`docs/product/project-git-status.md`). In every placement of that menu it is disabled while the status says `HEAD` is
detached, with "Not on a branch" under its name, or that the branch has no upstream, with "No upstream" under its name;
assistive technology is given the full reason as its description: that the project is not on a branch, so there is
nothing to pull, or that the branch has no upstream to pull from. A status with no branch read disables nothing. A
status can be out of date, so the refusals below still answer a pull chosen on one.

Choosing it closes the menu at once and runs that project's check right away (see "What one check does" in
`docs/product/project-git-status.md`), whose fetch must succeed. Then, from what the check read:

- a branch that is not behind its upstream, whether or not it is ahead, is left as it is, and the window that asked
  shows a notice that the current branch is already up to date;
- a branch that is behind and has no commits of its own is fast-forwarded to its upstream, and the window that asked
  shows a notice that the current branch was pulled;
- a branch with no commit yet is fast-forwarded to its upstream if `git` allows it, and refused if not;
- a branch that is both ahead and behind has diverged: nothing changes, and the window that asked asks the user to
  confirm a merge commit (see "Confirming a merge" below).

It pulls whether or not **Automatically pull repositories** is on; choosing it is the consent the switch would otherwise
give. Unlike the automatic pull, which only fast-forwards and passes over a branch it cannot fast-forward without a
word (see "Automatically pulling repositories" in `docs/product/project-git-status.md`), it is never silent, and it
merges a diverged branch once the user confirms.

The one-minute floor between checks does not apply, and the check does not wait its turn behind the four at once (see
"When a project is checked" in `docs/product/project-git-status.md`). If a check of the project is only waiting its turn
— in a round of checks, or in the pass that turning **Automatically pull repositories** on starts (see "Turning the
switch on" in `docs/product/project-git-status.md`) — that check starts at once as this request's check, with the pull,
in its place. If a check of the project is already running, or that pass is already fast-forwarding it, nothing starts
alongside it: as soon as it ends, one more check runs, with the pull, so the answer always comes from a fetch that
started after the request. However many requests arrive meanwhile, they share that one extra check and its answer, and
it does not run at all if the project has been removed in the meantime. A pull whose check has not ended within ten
minutes, or was abandoned, is refused (`git_pull_failed`); a check still running then goes on and shows its result on
the badge as any check does.

The badge shows the check in flight at once, without the usual 0.4-second wait, and the pull in flight while the
branch moves, as for any check (see "The branch badge" in `docs/product/project-git-status.md`). A fetch that fails
pulls nothing: the pull is refused with the fetch's message, and, as in any check, that message is also recorded on the
badge. No other refusal of a pull is recorded on the badge: the error alert is the report, as for a push, and the badge
keeps what the last check found.

### Confirming a merge

A diverged branch opens a confirmation titled **Pull and merge**. Its first sentence names the project, as a name chip,
and says that its current branch has diverged from its upstream, with the number of commits ahead and behind as the
check's fetch left them ("3 commits ahead and 2 commits behind"); the next says that pulling merges the upstream
changes into it and creates a merge commit. Its buttons are **Cancel** and **Pull and merge**. It is not a destructive
confirmation, since merging loses nothing: no warning icon, and **Pull and merge** is a primary button. Keyboard focus
starts on the dialog itself, not on **Pull and merge**, since it can come up while the user is typing elsewhere (see
"Where keyboard focus starts in a confirmation" in `docs/product/dialogs.md`). **Cancel** changes nothing.

**Pull and merge** fetches nothing again: the user confirmed the counts of the fetch just made. The branch is read again
locally and the rules above apply to what it says now, so a branch that is no longer behind is up to date and one with
no commits of its own left is fast-forwarded, each with its notice. A branch still diverged has its upstream — the
commit the upstream points to as the merge starts — merged in as a merge commit, whatever the user's merge and pull
configuration says: `merge.ff` (`only` included), a `branch.<name>.mergeOptions` of `--squash`, `--no-commit` or
`--ff-only`, `pull.rebase` and `pull.ff` cannot turn it into a refusal, a squash, a rebase, a fast-forward or a merge
left uncommitted. The commit message is the one a plain `git merge --no-edit` of the upstream branch writes: `git`'s own
subject, such as `Merge remote-tracking branch 'origin/feature' into topic`, without the `into` where `git` leaves it
out, as on `main` or `master`, and `Merge branch 'main' into topic` for an upstream that is a branch of the same
repository; one shortlog when `merge.log` asks for it; and the repository's hooks and `commit.cleanup` acting on it as
on any merge. The one difference is that a `--log` in `mergeOptions` alone adds no shortlog. The merge then counts as a
change Octoboard makes to the project (see "What Octoboard never modifies" in `docs/product/launching-agents.md`). Once
merged, the confirmation closes and the window shows the notice that the current branch was pulled. While the merge
runs the confirmation is busy, as any confirmation is (see "A dialog cannot be left while its request runs" in
`docs/product/dialogs.md`), and the badge shows the pull in flight.

`git` merges what it can on its own. A merge that meets conflicts is aborted — only the merge this pull started — so
the branch and the working tree are as they were, and the confirmation turns into a failure modal (see "The failure
modal" in `docs/product/error-alerts.md`): titled that the branch could not be pulled, saying that merging the upstream
into the branch causes conflicts, so the merge was cancelled and nothing changed, with the remedy to merge the upstream
into the branch in the project's terminal, resolve the conflicts there and commit, and the conflicted paths, one per
line, under **Error details**. Like the confirmation, it starts with focus on the dialog itself. A merge already in
progress in the project, someone else's, is never touched: the pull is refused and that merge left as it is.

Any other refusal after **Pull and merge** is an error alert over the confirmation, which stays open for another try.
The merge never runs alongside a check of the same project: it waits up to ten minutes for a check that is running or
waiting its turn, and is refused once that wait runs out.

### When the confirmation opens

The confirmation opens only in the window that chose **Pull current branch**, and comes up after the check, on its own,
so it never replaces or covers what the user is doing there. While any other modal is open in that window — a dialog,
Settings, a question the user did not ask for (a folder trust prompt or a session request), the confirmation the window
asks before quitting or before a launch, or an error alert — it is held, and it opens once all of them have closed.
Several held confirmations open one after another, in the order they came. A diverged pull of a project whose
confirmation is already open changes nothing; one whose confirmation is held replaces it, with the newer counts. A held
confirmation whose project has been removed meanwhile is dropped. An open one whose project is removed closes without a
word; where keyboard focus then has nowhere to return to, it goes where it goes when the last error alert is
acknowledged with nowhere to return to (see "The error alert" in `docs/product/error-alerts.md`).

### When a pull is refused

Nothing is pulled when the pull is refused, and a refusal is an error alert titled that the branch could not be
pulled, worded from the code:

- The project directory is not a git repository: `not_a_git_repository`, by the badge's rule (see "What one check does"
  in `docs/product/project-git-status.md`).
- The user's `git` cannot be found: `binary_not_found` (see "Where `git` is found" in
  `docs/product/project-git-status.md`), with **Download Git**; the pull can be chosen again once Git is installed.
- `HEAD` is detached: `not_on_a_branch`, saying that the project is not on a branch.
- The branch has no upstream, or its upstream branch no longer exists (deleted on the remote and pruned, or never
  fetched): `no_upstream_to_pull`. Its remedy is to push the branch from the project's action menu to record an
  upstream, or to set one in the project's terminal, then retry.
- Anything else `git` fails at: `git_pull_failed`, with `git`'s own message under **Error details** and the remedy of a
  Git failure (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`). That covers the check's fetch
  failing, a local read of the branch failing, `git` refusing the fast-forward (local changes it would overwrite, say)
  or the merge for a reason other than conflicts (no committer identity, a hook), a merge already in progress in the
  project or one whose state cannot be read, the check being abandoned, and the waits above running out.
- A confirmed merge that meets conflicts: `pull_merge_conflict`, shown as the confirmation's failure modal above rather
  than as an alert.

The detached `HEAD` and the missing upstream are refused before a failed fetch is, since they are what the user has to
fix first. A project that no longer exists is `unknown_project`.

## Pushing the current branch

**Push current branch**, under **Git** in the project's action menu, pushes that project's current branch and only that
branch. It is offered only while the project's status says the directory is a git repository (see "Project rows" in
`docs/product/sidebar-project-rows.md`). In every placement of that menu, it is disabled while a push of the project is
in flight. No separate reason is shown on the disabled item: the branch badge beside the project's name shows the push
(see "The branch badge" in `docs/product/project-git-status.md`). Choosing it closes the menu at once. The push is
waited on until `git push` has exited, including the repository's `pre-push` hook. When the push is accepted, the window
that asked shows a notice saying the current branch was pushed. A refusal is an error alert worded from the code
below. Pushing does not fetch and does not fast-forward, and a pull never pushes (see "Pulling the current branch"
above and "Automatically pulling repositories" in `docs/product/project-git-status.md`). A local branch that is behind
its upstream stays behind; if the remote rejects the update, the push fails and the branch stays where it was.

The push never uses `--force`, and it is never a bare `git push`, which would follow `push.default` and can publish
every branch whose name exists on the remote. The one branch is named explicitly, so only that branch is published. The
user's git configuration and credential helpers are used as they stand. `git` cannot stop to ask for a credential, a
passphrase or a host key — the same refusal as a fetch (see "The environment a check runs `git` in" in
`docs/product/project-git-status.md`). The push, including its hook, is given at most 30 minutes; the local branch reads
still have at most two minutes each. Hooks run as configured in the repository. If a hook refuses the push, its output
is carried as the failure's `detail` and shown under **Error details**, following "Names and diagnostic details" in
`docs/product/error-alerts.md`.

Once the directory is a repository, the branch is read first, locally, and what it says — the branch, its upstream
and the counts — is published to every window showing the project before the push is shown in flight. So the ahead
count the push is drawn on includes commits nothing had re-read yet. A warning the last check left stays, except a
check that found no `git` (see "Following changes to the repository's refs" in `docs/product/project-git-status.md`). The
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
reported by its error alert. Every refusal other than `unknown_project`, `not_a_git_repository`, `binary_not_found`
and a branch read that fails comes after the branch read was published, so the badge already shows that read's branch
and counts:

- The project directory is not a git repository when the push runs: `not_a_git_repository`. The rule is the badge's
  (see "What one check does" in `docs/product/project-git-status.md`): `.git` must be in the project directory
  itself. A subdirectory of a repository is not one.
- The user's `git` cannot be found: `binary_not_found` (see "Where `git` is found" in
  `docs/product/project-git-status.md`). Its alert says Git is not installed or not on `PATH` and offers **Download
  Git** (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`); the push can be chosen again once Git
  is installed.
- A push of the same project is already in flight: `push_in_progress`. The alert says to wait for it to finish,
  without a separate remedy line or an Issue-reporting action. This guard is reached after the branch read, so an
  earlier repository or branch-read failure is reported instead.
- `HEAD` is detached: `not_on_a_branch`.
- The branch has no commit yet: `no_commits_to_push`.
- The upstream names no configured remote of this repository: `upstream_not_a_remote`.
- There is no upstream, and the repository has neither an `origin` nor exactly one remote: `no_push_remote`. No
  remotes at all, and several remotes with no `origin`, are both this refusal.
- A branch or remote name is empty, starts with `-`, or contains `:`, a NUL or a newline: `git_push_failed`. `git`
  is not run. The reason says the name cannot be used in a refspec. The local branch's name is judged before the
  upstream. A remote or remote-branch name is judged only after the upstream has matched a remote, so an upstream
  that matches none is `upstream_not_a_remote` even when its text would not be a usable name.
- The branch cannot be read, or the remotes cannot be read: `git_push_failed`. The reason is `git`'s own message for
  the read that failed, or that it did not finish in time.
- `git push` itself fails, including a remote that rejects the update, a missing credential, or a push that does
  not finish within 30 minutes: `git_push_failed`, with `git`'s own message, or that it did not finish in time.

A project that no longer exists is `unknown_project`.

The push and a check are not ordered. **Pull current branch**, or a scheduled check, that arrives during a push is
handled as it would be with no push in flight: it is not queued behind the push. The badge shows the push in flight
while the check can still update its branch, counts and error. When the push ends, a check or pull still in flight is
shown again. Only one push of the same project may run at once, across windows; another request that reaches the guard
is refused as `push_in_progress`. The pushing state begins after the branch read has been published and the guard
accepts the push, covers the remotes read as well as `git push`, and ends on a refusal too. A push ending after its
project was removed and registered again cannot clear a newer push's in-flight indication.

A successful push does not edit the project's files and does not move the working tree. It updates the remote, and
in the repository it updates the remote-tracking refs `git` updates for that push. When the branch had no upstream,
it also records one. The daemon then re-reads the local branch header and publishes the project's git status, with
no fetch, so the badge's ahead count can drop. Every window showing the project sees that status. This re-read does
not end the badge's indication of an overlapping check or pull. An error the last check left on the badge
stays, unless that re-read fails, in which case the warning becomes the re-read's message and the branch and the counts
stay as they were. A re-read that fails does not turn the notice into an error alert: the push was accepted.
Turning **Automatically pull repositories** on never pushes.

## Switching the local branch

**Switch branch**, under **Git** in every placement of the project's action menu, opens a third-level menu of
existing local branches. Remote-only branches are not offered. The list is read afresh when the submenu opens,
in byte order of branch names, with the project's own worktree's current branch checked. Selecting that branch
closes the menu without requesting a switch.

While the list is being read, the submenu shows a loading state. No local branches produces an unavailable empty
item. A failed read shows, in the submenu in place of its items, one alert with its reason and **Try again**, which
reloads without closing the menu; it opens no error alert. Keyboard focus inside the submenu moves to Try again when the
read fails, and back to the submenu when the reload starts. At most 10,000
branches, or as many as 2 MiB of encoded names hold, are listed; a truncated list ends with a note, after the
branches, saying that there are more branches than the list can show and the others are not listed. The note is muted
text with no glyph, lined up with the branches' names; it is reached and read with the arrow keys like the items, but
choosing it does nothing and it is not worded as a failure.

Selecting another branch closes the menus and switches immediately, without confirmation. It changes the project's
own worktree, including a linked worktree, regardless of which worktree the Git pane is reviewing. It never creates
a branch, guesses a remote branch, fetches, pushes or pulls; only in a partial clone may `git` itself fetch objects
the branch needs and the clone lacks, over the user's own ssh (see "The environment a check runs `git` in" in
`docs/product/project-git-status.md`). It does not force the switch or stash or discard local
changes: changes Git can carry safely stay, and a conflict, a branch checked out in another worktree, or another Git
refusal fails with an error alert. Running agent sessions do not prevent a switch; the working tree can change
underneath them. Other Git operations are not serialized with the switch.

The directory must contain its own `.git` entry, as for the badge. A missing project is `unknown_project`, a
directory that is not a repository under that rule is `not_a_git_repository`, a `git` that cannot be found is
`binary_not_found` (see "Where `git` is found" in `docs/product/project-git-status.md`), and an invalid or vanished local
branch or a Git failure is `git_switch_failed`, with the reason as `detail`. With no `git`, the submenu's read of the
branches fails too: its alert says that Git is not installed or not on `PATH`, with the remedy to install it and try
again, and offers **Download Git** before **Try again** (see "Recovery and unexpected errors" in
`docs/product/error-alerts.md`); Try again, still the control focus moves to, then finds a Git installed since. Each Git call uses the filtered,
non-interactive environment described in "The environment a check runs `git` in" in
`docs/product/project-git-status.md` and has at most two minutes.

Success raises no notice. The local branch and upstream counts are read again for the badge in every window,
without fetching; an earlier remote-check warning stays unless that read itself fails. A failed status re-read
records its warning without turning a completed switch into a switch error. The project pane refreshes when the
reported branch changes (see "Files and Git" in `docs/product/project-pane.md`).

## Restoring file changes

**Restore file changes**, under **Git** in every project's action-menu placement, follows Switch branch
after a separator (see "Project rows" in `docs/product/sidebar-project-rows.md`). It opens a destructive confirmation
dialog explaining that staged and unstaged tracked changes will be discarded, staged new files deleted, and untracked
and ignored files kept, and that the discard cannot be undone. Cancelling makes no restore request. Confirming waits for
the restore; success closes the dialog and raises a notice. A failure opens an error alert, leaving the
confirmation available for retry after acknowledgement.

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
- The user's `git` cannot be found: `binary_not_found` (see "Where `git` is found" in
  `docs/product/project-git-status.md`).
- A tracked path blocked by a directory, or an ancestor blocked by a file or symbolic link; or a path in `HEAD`
  removed from the index but still present on disk: `git_restore_failed`. These collisions are refused before
  restoration to preserve untracked or ignored content; move the obstruction aside first.
- On a Windows daemon host, a tracked path Windows cannot name (one with a colon, a reserved device name such as
  `CON`, or a trailing dot), which Git for Windows cannot write to the disk: the restore is refused.
- Git cannot inspect or restore the files: `git_restore_failed`, with the reason as `detail`. Each Git call has at
  most two minutes, 16 MiB of standard output and 64 KiB of standard error; an inspection exceeding a bound fails
  before restoration.

A restore failure does not promise rollback: Git can restore some paths before a later path fails. Running agent
sessions do not prevent restoration, and their Git operations, checks, pulls, pushes and other restores are not
serialized with it. The files can change underneath a running agent.
