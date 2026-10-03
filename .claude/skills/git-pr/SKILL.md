---
name: git-pr
description: Squash the current branch's changes, rebase them onto the latest target branch, push, and open a PR; once the PR is created, clean up the local branch and worktree. Defaults to the target branch this project agreed on, and another branch can be given at call time. Use when the user says "open a PR", "send a PR", "open a pull request", "push it and PR it to xxx".
---

# Opening a PR

`<branch>` means the branch carrying this PR: the new branch created in step 1 when one was created, otherwise the current branch.

## Target branch

- When a branch is given at call time, PR to that branch; this does not change the project's default or any other
  branch's record.
- When none is given and the current branch records `git config branch.<current-branch>.targetBranch`, PR to the
  branch it records (being on the target branch itself counts as having no record).
- Without that record, and with none given, PR to `main` (this project's default PR target branch, fixed at install
  time).
- `git fetch --all` before doing anything. Then settle `<remote>`: it is the target branch's upstream
  (`git config branch.<target-branch>.remote`); when the branch does not exist locally or has no upstream set, use
  the remote that has a branch of the same name, ask the user when there is more than one, and stop and ask when
  there is none at all; when the upstream points at a remote that no longer exists, treat it as not existing
  locally. `<remote>` below always means that one.
- The squash, the rebase and every comparison are all based on `<remote>/<target-branch>`, never the local target
  branch: the merge happens on the remote.
- After step 1, record the target branch settled on this round into `<branch>`:
  `git config branch.<branch>.targetBranch <target-branch>`. Write it when there is no record; when one already
  exists and differs from this round's, ask the user first and only change it once they agree. The value is the
  branch name without a remote prefix (`main`, not `origin/main`).

## Steps

When an operation is blocked by one of the repository's git hooks, handle it as the message says; do not work
around it. Write commit messages and the PR title and description per "Authoring identity" at the end.

1. **Prepare the branch**: when there are uncommitted changes, first ask the user whether to include them in this
   PR: include them by committing first; for ones not included, ask the user to commit or stash them and continue
   once they have, rather than doing it for them. When there are no commits ahead of `<remote>/<target-branch>`
   (only being behind also counts as none), there is nothing to submit — tell the user and stop. Otherwise, see
   where you are:
   - **On the target branch**: first check that every commit the branch leads by belongs to this PR, and ask the
     user when others are mixed in, leaving the disposition to them rather than discarding anything yourself; then
     `git switch -c` a new branch to carry them (prefix `feat/`, `fix/`, etc. by the kind of change); finally use
     `git branch -f` to put the local target branch back at the remote's position, or those commits will stay
     behind on it.
   - **Not on the target branch**: ask the user, and leave the disposition to them, when `git log
     <remote>/<target-branch>..HEAD` has commits mixed in that do not belong to this PR.
2. **Squash**:
   - First record the fork point: `git config branch.<branch>.forkPoint $(git merge-base HEAD
     <remote>/<target-branch>)` — it can no longer be computed once a rebase has happened; do not overwrite it when
     it is already recorded.
   - When the branch has already been pushed (the remote has a branch of the same name), both this squash and
     step 3's rebase will rewrite already-published history: confirm per step 5 that a force-push is allowed before
     acting, and stop rather than rewriting first when it is not.
   - A branch with only one commit needs no squash, and skips the verification below too. Several commits are
     squashed into one by default; keep several only when they really do contain several mutually independent
     changes.
   - The squash is done on the original base: first record `git rev-parse HEAD` (call it the pre-squash commit),
     then `git reset --soft $(git merge-base HEAD <remote>/<target-branch>)` and commit again; that merge-base is
     the original base, not the recorded fork point. Do not reset to the target branch's latest commit, and do not
     open another branch and copy the files across either — with the base moved and the content not merged, that
     quietly reverts changes on the target branch.
   - After squashing, verify that the squashed commit's parent is still the original base and that `git diff
     <pre-squash-commit> HEAD` is empty; if either fails, `git reset --hard <pre-squash-commit>` and redo it.
3. **Rebase onto the latest target branch**: `git rebase <remote>/<target-branch>`. Resolving conflicts means
   combining both sides' changes, not taking your own side across the board.
   After the rebase, re-run the build, tests and format checks; when they fail and whether this round's changes
   caused it is unclear, stop and ask the user rather than pushing, and fold any further fix into the original
   commit rather than stacking a new one on top.
4. **Pre-push precheck**: when the worktree has no `.githooks/revert-gate.py` (the repository has no revert gate),
   go straight to the manual check below; when it does, confirm the hook entry first, then run the precheck.

   **Hook entry**: confirm this clone has the hook entry installed. It drives step 5's automatic check at push
   time; missing it does not affect the precheck itself: for a repository with `core.hooksPath` set, only report
   it; for one without, install it and tell the user.
   - For a repository with `core.hooksPath` set, check whether the `pre-push` it points at calls
     `.githooks/hook.sh`: if it does, it is already wired up; if not, report it. Do not run `install.sh` on such a
     repository — it exits with an error.
   - For a repository without `core.hooksPath` set, when `$(git rev-parse --git-common-dir)/hooks/pre-push` lacks a
     `# revert-gate hook entry` line, run `sh .githooks/install.sh` at the repository's (or the current worktree's)
     root to install it, and tell the user. Add `--pr-only` when the repository has no `.githooks/branches`, and
     omit it when it does.

   **Precheck**: run the command below; it reads the script committed on `<remote>/<target-branch>`, and the whole
   command's exit code is the precheck's result:

   ```bash
   s=$(git show "<remote>/<target-branch>:.githooks/revert-gate.py" 2>/dev/null) || { echo "gate not ready" >&2; (exit 3); } && python3 -I -c "$s" check-pr <remote>/<target-branch> <branch>
   ```

   | exit code | meaning | what to do |
   |---|---|---|
   | 0 | passed | proceed to step 5 |
   | 1 | a problem was found | follow the precheck's reported reason: not yet rebased goes back to step 3; reverted existing changes are handled under "Reverted existing changes"; any other reason, stop and ask the user |
   | 2 | the gate itself failed | report it, then do the manual check too — do not treat it as passed |
   | 3 or other | the precheck could not run (the gate is not yet merged into the target branch, no `python3`, etc.) | do the manual check — do not treat it as passed |

   **Manual check**: go through the commits in `git log <the fork point recorded for <branch>,
   branch.<branch>.forkPoint>..<remote>/<target-branch>` one by one and confirm their changes are all still present
   on the current branch; when a change has gone missing, handle it under "Reverted existing changes"; once
   checked, tell the user the gate was not used this round.

   **Reverted existing changes** (reported by the precheck, or found missing in the manual check): first judge
   whether the deletion was intentional, and ask the user when that cannot be judged.
   - Caused by a mistake (the squash or a conflict resolution dropped changes): go back to the commit recorded
     before squashing in step 2 (use `git reflog` to find it when the branch was never squashed), then redo the
     squash and the rebase.
   - Genuinely meant to remove content the target branch gained after the fork point: ask the user first; once they
     agree, amend the commit that made the deletion to add one `Reverts: <sha>` line per reverted commit (for a
     branch already pushed, push per step 5), then re-run the precheck; a manual check does not need re-running.
5. **Push**: a branch never pushed before uses `git push -u <remote> <branch>`; one already pushed with history not
   rewritten uses a plain `git push`; one whose published history was rewritten uses `--force-with-lease`, but only
   on a branch used solely by you (nobody else is developing on it; ask when unsure), and only after asking the user
   and getting their agreement first — that question must be asked before step 2 starts rewriting anything, not at
   push time. Stop and ask the user when this is not satisfied.
6. **Open the PR**: base is `<target-branch>`, head is `<branch>`. When the branch already has an open PR, do not
   recreate it — the push has already updated it; when its base does not match `<target-branch>`, stop and ask the
   user.
7. **Clean up locally**: first confirm the PR exists (created or already open) and the branch is pushed to the
   remote; when either is not met, do not clean up — report and stop. When both are met, clean up right away, as
   part of opening the PR, with no need to ask again:
   - the branch has its own dedicated worktree: go back to the main working copy first, then `git worktree remove`;
   - the branch is parked in the main working copy you are currently in: switch back to the target branch (create a
     tracking branch from the remote when it does not exist locally);
   - finally `git branch -D` to delete the local branch (this also deletes that branch's `targetBranch` and
     `forkPoint` records).

   Stop and ask the user when the worktree has uncommitted or untracked files, or when another worktree holds the
   target branch so you cannot switch back; do not add `--force`.

## Conflicts again after the PR is open

After this round's call ends, the target branch moves on and the PR reports a conflict, and the user comes back to
handle it: the branch and the PR are whichever the user names, and ask when they do not. First tell the user the PR
has a conflict, and ask whether to resolve it with merge or rebase (rebase needs a force-push; choosing it counts as
agreeing; the solo-use check follows step 5). While the PR is still unmerged and the remote branch still exists, the
local branch and its records were already cleaned up in step 7; the target branch is the PR's base (unless the user
names another). Rebuild from the remote first: `git fetch`, then `git switch -c <branch> <remote>/<branch>`, record
`targetBranch` again, and compute the fork point fresh with `git merge-base HEAD <remote>/<target-branch>` and
record it. Choosing merge means merging `<remote>/<target-branch>` into the branch and pushing normally; choosing
rebase follows step 3, and the push follows step 5. Resolve conflicts as in step 3; once resolved, re-run step 4's
precheck, and after pushing, clean up locally per step 7.

## Authoring identity

Applies to every commit message (including amended ones) and the PR title and description, in the language the
repository's history uses:

- Leave no AI traces: no `Co-Authored-By`, no "Generated with ...", no mention of AI assistance.
