---
name: git-worktree
description: The steps for developing on a branch in its own worktree through to merging it back into the target branch — creating the worktree and recording the target branch, aligning with the remote target branch, squashing commits, rebasing, the pre-merge confirmation, the local merge and the cleanup after it, what to do when a push is rejected, and how to revert a commit or change already on the target branch. Use when the user says "merge this branch back", "squash the commits and merge", "the target branch is behind the remote and needs aligning", "revert this commit", "pushing the target branch was rejected", "the merge was blocked, what now".
---

# Working a worktree branch

Where a branch merges back to and where it was cut from are read from its own git config (see "## Creating" for
how they are recorded): if the target branch (`targetBranch`) cannot be read, report back rather than guessing from
the branch name; if the fork point (`forkPoint`) is missing, or the worktree for the named branch has no branch
checked out (a temporary `--detach` worktree), also report back rather than estimating one.
Below, "the branch" means the branch under development in a worktree, and "the target branch" means the branch it
merges back into. "Report back" on error or when stuck means: stop, tell the user, and wait for instructions; "ask
the user" means a choice or consent is needed from them.
When no branch is named: if called from inside a worktree, it is that worktree's branch; if called from the main
working copy, it is the one worktree branch when there is exactly one, ask the user when there are several, and
report back when there are none.
The operations below happen in two places: aligning with the remote, merging and pushing the target branch happen
in the working copy that has the target branch checked out (the repository's original directory, called the main working copy below), while squashing,
rebasing and verification happen in the branch's own worktree.

## Creating

- The target branch is whichever branch the current working copy had checked out when the worktree was created; it need not be the main branch. If the working copy is in detached HEAD with no branch checked out, report back rather than guessing.
- Name branches with a `feat/`, `fix/`, `refactor/` or `docs/` prefix according to the kind of change; put the worktree in `.worktrees/<short-branch-name>/` inside the repository, where the short name is the branch name with that prefix removed (`feat/login-retry` goes in `.worktrees/login-retry/`).
- When injecting code temporarily just to verify something, you may open a separate worktree with no branch (`git worktree add --detach`); changes in it are never committed and it is removed as soon as the check is done.
- Before creating the worktree, check whether the repository has a `.githooks/`: if not, treat it as not using the revert gate and skip this bullet and the pre-merge gate precheck; if it does, install the gate if it is missing (its hooks and resident guarded branch configuration live in each clone's own `.git/` and do not travel with the repository):
  - Run this in the root of the main working copy: `git config --get-all revert-gate.branch >/dev/null || sh .githooks/install.sh $(git config core.hooksPath >/dev/null && echo --branches-only)`. Already installed does nothing; not yet installed installs it from the committed `.githooks/branches`.
  - In a repository that sets `core.hooksPath`, hooks are owned by a system such as husky, so the command only writes the resident guarded branch configuration (the branches this repository always guards, usually the main branch, listed in `.githooks/branches`).
  - Tell the user if the command actually performed an install; report errors rather than handling them yourself.
- Once the gate is in place, create in this order:
  1. Align the target branch per "## Aligning with the remote target branch". Being ahead of the remote (unpushed commits) is normal and needs no action. If it cannot be aligned, report back rather than cutting from the old position.
  2. Create the branch and worktree from the aligned target branch.
  3. Record the target branch and fork point: `git config branch.<branch>.targetBranch <target-branch>` and `git config branch.<branch>.forkPoint $(git rev-parse <target-branch>)`. The fork point is read by the gate; this file only maintains it. It is updated only after an `--onto` move while aligning with the remote (rule below) — a plain rebase does not update it, since the old fork point is still in the target branch's history.

## Aligning with the remote target branch

- Align in three places: before cutting, before merging locally, and after a push of the target branch is rejected. In the main working copy, run `git fetch` (if a fast-forward or rebase is needed and the working copy has uncommitted changes, report back first), then handle the relationship between the target branch and its upstream (`<target-branch>@{u}`):
  - Behind (no unpushed commits locally): fast-forward with `git merge --ff-only <target-branch>@{u}`.
  - Diverged (unpushed commits locally and new commits on the remote): first record the old position with `git rev-parse <target-branch>` (needed below to move other branches), then `git rebase <target-branch>@{u}` to lift the local unpushed commits on top of the latest remote; resolve conflicts the same way as in "Squashing and rebasing".
    - After the rebase, re-run the build, tests and format checks in the main working copy; only then does it count as aligned. If they fail, report back rather than continuing.
    - The target branch's former unpushed commits have now been rewritten, while branches cut from them still hold the pre-rewrite copies, so a plain rebase would move those commits a second time — move them one by one instead:
      - Find such branches: look among `git config --get-regexp '^branch\..*\.targetBranch$'` for entries whose value is this target branch, and find their worktree paths with `git worktree list`. If such a worktree has uncommitted changes, ask the user first.
      - Move: in that branch's own worktree, run `git rebase --onto <target-branch> <old-position>` to move only the branch's own commits.
      - After moving, update its fork point to `git merge-base HEAD <target-branch>` (the only case where the fork point is updated this way): the old fork point points at the rewritten, now-stale commits, and leaving it would make the revert check count them.
  - No upstream: if exactly one remote has a same-named branch (`<remote>/<target-branch>`), use it in place of the upstream without setting it as the upstream; if several remotes have it, ask the user which one, then set the upstream with `git branch -u <remote>/<target-branch>`; if none has it, skip this.

- **Discovering the need to align only at merge time**: if diverged, the `--onto` move above has already carried this branch along; if only behind, fast-forward, then do a plain `git rebase <target-branch>` on the branch; after that, follow the "whenever a branch has been rebased again" rule in "## Merging".
- **Discovering the need to align only after a push is rejected**: the branch that was merged has already been deleted; any other branches still cut from the target branch are moved with the `--onto` form above.

## Squashing and rebasing

- Commits made during development need no care; squash before merging: every commit on the target branch must stand on its own and be revertible on its own. A branch that did one thing is squashed into one commit; keep several only when it genuinely contains several independent changes, and when the split point falls inside a single file, merge them into one commit instead. Commit messages describe the final state and do not recount the process (no "fix the previous commit", no "adjusted per feedback"), in the language the repository's history uses.
- Always squash on the original base: `git reset --soft $(git merge-base HEAD <target-branch>)`, then commit again. Do not reset to `<target-branch>` or any newer commit, and do not open another branch and copy the files across — with a new base and unmerged content, the old code counts as a new change and silently reverts whatever landed on the target branch after that base. Before squashing, record `git rev-parse HEAD`; afterwards verify that the squashed commit's parent is still the original base and that `git diff <recorded-commit> HEAD` is empty — if either fails, `git reset --hard <recorded-commit>` and redo it.
- Then `git rebase <target-branch>`, resolving each conflict once per commit. Resolving a conflict means combining both sides' changes, not always taking your own side; ask the user when unsure how to combine them. Fold any further edits into the original commit instead of stacking new ones on top.
- A branch only you use (no one else's commits on the remote; ask the user when unsure) that has been pushed before needs a force-push with `--force-with-lease` after squashing; confirm with the user before pushing.
- A branch that has been pushed but is not exclusively yours (the remote has someone else's commits): do not squash or force-push it — report back and let the user decide.

## Reverting and undoing

- The repository has a revert gate installed (`.githooks/revert-gate.py`): when a guarded branch moves (each branch's target branch as recorded in `targetBranch`, plus the gate's resident guarded branches), local merge / commit / reset and push are all checked automatically, and updates that rewrite published history or revert changes already on that branch are refused.
- When the gate reports that existing changes were reverted, the squash or a conflict resolution went wrong: reported before merging, go back to the branch and redo it; a rebase blocked while aligning with the remote, `git rebase --abort` and rebase again; reported only when pushing the target branch, add a commit in the main working copy restoring the reverted changes rather than resetting the target branch; reported when pushing the development branch, go back to the branch and re-squash rather than bypassing it with `--no-verify`.
- When a reset is blocked, the index (and the working tree, with `--hard`) has already been changed to the target position's content: recover with `git reset --merge` after `--hard`, or `git reset` after `--mixed`.
- To genuinely drop a commit, ask the user first; once they agree, use `git revert` in the main working copy, or add a `Reverts: <sha>` line to the commit message to declare to the gate that the revert is intentional. Never remove an already-pushed commit from the target branch with reset or amend.

## Merging

Order: align with the remote → squash → rebase → verify → confirm with the user → gate precheck → `merge --ff-only` → clean up. Verification means running the build, tests and format checks in the branch's worktree; if they fail, report back and do not merge.

- Projects that have moved to merging through PRs (as the user has said, or as the repository's docs establish): still align, squash, rebase and verify, up through confirming with the user; do not raise a PR in this flow, do not merge locally, and do not run the precheck — raising the PR and the cleanup afterward belong to the `git-pr` skill, or to the user's instructions if it is not installed.
- Before merging locally you must confirm: every earlier step is done (for history that cannot be squashed, see the "pushed but not exclusively yours" bullet in "Squashing and rebasing"); the main working copy is still on the target branch (switch back if not; report back if uncommitted changes block switching back); the user has confirmed (show them the final commit list and the verification results); and the gate precheck passes (skip this when the repository has no `.githooks/`; when the gate blocks a merge, the main working copy has already been written to the branch's content and needs `git reset --merge` to recover, so precheck first). The gate precheck runs the command below in the main working copy, and counts as passing only on exit code 0:

  ```bash
  b=$(git config --get-all revert-gate.branch | head -n1); s=$(git show "refs/heads/$b:.githooks/revert-gate.py" 2>/dev/null) || { echo "gate not ready" >&2; (exit 3); } && python3 -I -c "$s" check <target-branch> <branch>
  ```

  - Exit code 3 ("gate not ready"): try in order — (1) install the gate as in the creation step, and re-run; (2) if it still reports 3, and the first resident guarded branch (the first line of `git config --get-all revert-gate.branch`) has no local branch in this clone (in a fresh clone it may exist only on the remote), create one with `git branch <that-branch> <remote>/<that-branch>` and re-run; (3) if it still reports that, report back (this clone may not have the gate installed, or the gate script may not be committed on the main branch yet) rather than treating it as a pass.
  - Exit code 1: a problem was found; follow what it tells you. If it reports that existing changes were reverted, follow the "when the gate reports that existing changes were reverted" bullet in "## Reverting and undoing" and go back to the branch to re-squash; if it reports the target branch as behind or diverged from the remote (as of the last fetch), handle that per "## Aligning with the remote target branch".
  - Exit code 2: the gate itself failed; report that too, and do not treat it as a pass.
- Merge locally in the main working copy with `git merge --ff-only <branch>`. A failed fast-forward means the target branch moved again or the rebase was not clean: go back to the branch, rebase again, follow the next bullet through verification and confirmation, re-run the precheck, then merge.
- Whenever a branch has been rebased again (including after aligning with the remote), the code it brings in has not been through the earlier verification, so proceed as follows:
  - Re-run verification first (handle a failure as above);
  - once it passes, re-confirm with the user if any conflicts were resolved or the commit list changed (e.g. squashed again); otherwise, when the branch's own commit content is unchanged, reuse the original confirmation;
  - then re-run the precheck and merge.
- Once the merge is confirmed to have succeeded (`merge --ff-only` exited 0), go back to the main working copy first, then immediately `git worktree remove` that worktree and delete the branch (report back on failure rather than adding `--force`; do not touch the remote branch) — this is part of the merge, no need to ask again. Do not push the target branch automatically; push it only if the user asks.

## Pushing

- When a push of the target branch is rejected (the remote reports non-fast-forward, or pre-push reports the remote has commits you do not), do not force-push: handle it per the "discovering the need to align only after a push is rejected" bullet in "## Aligning with the remote target branch", then push again once aligned and verification passes.
