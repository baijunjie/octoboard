---
name: git-worktree
description: The steps for developing on a branch in its own worktree through to merging it back into the target branch — creating the worktree and recording the target branch, aligning with the remote target branch, squashing commits, rebasing, the pre-merge confirmation, the local merge and the cleanup after it, what to do when a push is rejected, and how to revert a commit or change already on the target branch. Use when the user says "merge this branch back", "squash the commits and merge", "the target branch is behind the remote and needs aligning", "revert this commit", "pushing the target branch was rejected", "the merge was blocked, what now".
---

# Working a worktree branch

Where a branch merges back to and where it was cut from are read from its own git config (see "## Creating" for
how they are recorded); if they cannot be read, report back rather than guessing from the branch name.
The operations below happen in two places: aligning with the remote, merging and pushing the target branch happen
in the working copy that has the target branch checked out (the main working copy below), while squashing,
rebasing and verification happen in the branch's own worktree.

## Creating

- Name branches with a `feat/`, `fix/`, `refactor/` or `docs/` prefix according to the kind of change; put the worktree in `.worktrees/<short-branch-name>/` inside the repository, where the short name is the branch name with that prefix removed (`feat/login-retry` goes in `.worktrees/login-retry/`).
- Record the target branch and the branch point as soon as the branch exists: `git config branch.<branch>.worktreeTarget <target-branch>` and `git config branch.<branch>.worktreeBase $(git rev-parse <target-branch>)` — a later session can then still find out where the branch should be merged back. The pre-merge revert check uses the former to recognise the target branch and the latter to extend the check across every commit after the branch point. Do not change the branch point on later rebases; both entries are cleaned up when the branch is deleted.
- Before creating a worktree, run this in the root of the main working copy: `git config --get-all revert-gate.branch >/dev/null || sh .githooks/install.sh $(git config core.hooksPath >/dev/null && echo --branches-only)`. The revert gate's hooks and its resident guarded branch configuration live in each clone's own `.git/` and do not travel with the repository; when this clone has neither, that command installs them from the committed `.githooks/branches`. In a repository that sets `core.hooksPath`, hooks are owned by a system such as husky, so only the resident guarded branch configuration is written. Tell the user if you had to install it; report errors rather than handling them yourself.

## Aligning with the remote target branch

- Before merging locally, first align with the remote target branch, then squash and rebase: run `git fetch` in the main working copy and handle the relationship between the target branch and its upstream (`<target-branch>@{u}`):
  - Behind (no unpushed commits locally): fast-forward with `git merge --ff-only <target-branch>@{u}`.
  - Diverged (unpushed commits locally and new commits on the remote): `git rebase <target-branch>@{u}` to lift the local unpushed commits on top of the latest remote; resolving conflicts means combining both sides' changes.
    - After the rebase, re-run the build, tests and format checks in the main working copy; only then does it count as aligned.
    - The target branch's former unpushed commits have now been rewritten, while branches cut from them still hold the pre-rewrite copies, so a plain rebase would move those commits a second time: for each such branch, go to its own worktree and run `git rebase --onto <target-branch> <old-position>` to move only the branch's own commits. The old position is `<target-branch>@{1}` right after the rebase finishes, but no longer is once the target branch moves again, so record it first with `git rev-parse <target-branch>`.
  - No upstream: use the remote's single same-named branch (`<remote>/<target-branch>`) in place of the upstream; when several remotes have that branch, set the upstream first with `git branch -u <remote>/<target-branch>`; when no remote has it either, skip this.

  When you only discover the need to align at merge time or after a push is rejected, rebase the branch onto the target branch again once aligned (using the `--onto` form above when diverged), then go back to the pre-merge confirmation.

## Squashing and rebasing

- Commits made during development need no care; squash before merging: every commit on the target branch must stand on its own and be revertible on its own. A branch that did one thing is squashed into one commit; keep several only when it genuinely contains several independent changes, and do not force a split when the split point falls inside a single file. Commit messages describe the final state and do not recount the process (no "fix the previous commit", no "adjusted per feedback"), in the language the repository's history uses.
- Always squash on the original base: `git reset --soft $(git merge-base HEAD <target-branch>)`, then commit again. Do not reset to `<target-branch>` or any newer commit, and do not open another branch and copy the files across — with a new base and unmerged content, the old code counts as a new change and silently reverts whatever landed on the target branch after that base. After squashing, verify that the first commit's parent is still the original base and that the working tree is identical to before the squash; if either fails, go back and redo it.
- Then `git rebase <target-branch>`, resolving each conflict once per commit. Resolving a conflict means combining both sides' changes, not always taking your own side — that would revert the corresponding change on the target branch. Fold any further edits into the original commit instead of stacking new ones on top.
- A branch only you use needs a force-push after squashing; confirm with the user before pushing.
- If the branch is itself the target branch of other worktree branches (nesting), wait until those child branches are merged back and deleted before squashing; a branch that has been pushed while serving as a target branch in a nest must not be squashed and force-pushed. With the revert gate installed both are likely to be rejected, but do not rely on the gate — follow this rule yourself.

## The revert check

- The repository has a revert gate installed (`.githooks/revert-gate.py`): when a guarded branch moves (the target branch each branch recorded in `worktreeTarget`, plus the gate's resident guarded branches, usually the main branch), local merge / commit / reset and push are all checked automatically, and updates that rewrite published history or revert changes already on that branch are refused. Follow what it tells you.
- When the gate reports that existing changes were reverted, the squash or a conflict resolution went wrong: reported before merging, go back to the branch and redo it; a rebase blocked while aligning with the remote, `git rebase --abort` and rebase again; reported only at push time, add a commit in the main working copy restoring the reverted changes rather than resetting the target branch.
- When a reset is blocked, the index (and the working tree, with `--hard`) has already been changed to the target position's content: recover with `git reset --merge` after `--hard`, or `git reset` after `--mixed`.
- To genuinely drop a commit, ask the user first; once they agree use `git revert`, or add a `Reverts: <sha>` line to the commit message. Never remove a commit from the target branch with reset or amend.

## Merging

- Before merging locally you must confirm: the feature is verified; history is squashed and rebased onto the latest target branch; the main working copy is still on the target branch; the user has confirmed; and the gate precheck passes (when the gate blocks a merge, the main working copy has already been written to the branch's content and needs `git reset --merge` to recover, so precheck first). The gate precheck runs the command below and counts as passing only on exit code 0:

  ```bash
  b=$(git config --get-all revert-gate.branch | head -n1); s=$(git show "refs/heads/$b:.githooks/revert-gate.py" 2>/dev/null) || { echo "gate not ready" >&2; false; } && python3 -I -c "$s" check <target-branch> <branch>
  ```

  On "gate not ready", install the gate as described in the rule about creating a worktree and re-run; if the first resident guarded branch (the first line of `git config --get-all revert-gate.branch`) has no local branch in this clone (in a fresh clone it may exist only on the remote), create one with `git branch <that-branch> <remote>/<that-branch>` and re-run; if it still reports that, report back (this clone may not have the gate installed, or the gate script may not be committed on the main branch yet) and treat it neither as a pass nor as a detected revert.
  Otherwise exit code 1 means a problem was found; follow what it tells you. If it reports the target branch as behind or diverged from the remote (as of the last fetch), handle that per the alignment rule above.
  Exit code 2 means the gate itself failed: report that too, and do not treat it as a pass.
- Merge locally in the main working copy with `git merge --ff-only <branch>`. A failed fast-forward means the target branch moved again or the rebase was not clean: go back to the branch, rebase again, re-run the precheck and then merge — do not fall back to a plain merge.
- Whenever a branch has been rebased again (including after aligning with the remote), keep the original confirmation if no conflicts were resolved, and re-confirm if any were.
- Code brought in by a rebase has not been through the earlier verification, so after rebasing re-run the build, tests and format checks in the worktree, then do the pre-merge confirmation and merge.
- Once the merge is confirmed to have succeeded (`merge --ff-only` exited 0 and the target branch points at the branch's commit), immediately `git worktree remove` that worktree and delete the branch — this is part of the merge, no need to ask again.

## Pushing

- When a push of the target branch is rejected (the remote reports non-fast-forward, or pre-push reports the remote has commits you do not), do not force-push: `git fetch` first, rebase onto the upstream and verify it as described for the diverged case in the rule beginning "Before merging locally, first align with the remote", then push.
