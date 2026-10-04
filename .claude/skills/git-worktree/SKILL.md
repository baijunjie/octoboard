---
name: git-worktree
description: The operating rules for developing on a branch in its own worktree and merging it back to the target branch. Use for starting a new worktree, merging a branch back into the target branch, aligning the target branch with the remote, reverting a commit already on the target branch, and scenarios like "open a worktree", "merge this branch back", "squash the commits then merge", "the target branch has fallen behind the remote and needs aligning", "revert this commit", "pushing the target branch was rejected", "the merge was blocked, what now".
---

# Working a worktree branch

Below, "the branch" means the branch under development in a worktree, and "the target branch" means the branch it merges back into. "Report back" means: stop, explain the situation, and wait for the user's instructions; any situation not covered below, or one you are unsure about, always gets reported back.

Where a branch merges back to and where it was cut from are read from its own git config (see "## Creating" for how they are recorded): if the target branch (`targetBranch`) cannot be read, the fork point (`forkPoint`) is missing, or the named worktree has no branch checked out (a temporary `--detach` worktree), report back in every case — do not guess from the branch name, and do not estimate one yourself to fill the gap.
When no branch is named, use the current worktree's branch; when called from the main working copy, use it if there is exactly one worktree branch, otherwise ask the user.
Aligning with the remote, merging, and pushing the target branch all happen in the working copy that has the target branch checked out (the repository's original directory, called the main working copy below); squashing, rebasing, and verification happen in the branch's own worktree.

## Creating

- The target branch is whichever branch the current working copy had checked out when the worktree was created; it need not be the main branch. If it is currently in detached HEAD with no branch checked out, report back rather than guessing.
- Name the branch with a `feat/`, `fix/`, `refactor/` or `docs/` prefix according to the kind of change; put the worktree at `.worktrees/<short-branch-name>/` inside the repository, where the short name is the branch name with that prefix stripped (`feat/login-retry` goes at `.worktrees/login-retry/`).
- When injecting code only to verify something temporarily, you may open a separate, branchless temporary worktree instead (`git worktree add --detach`); changes made there are never committed, and the worktree is removed as soon as verification is done.
- Before creating the worktree, check whether the repository has a `.githooks/`: if not, treat it as not using the revert gate, and skip this bullet and the pre-merge gate precheck; if it does, install the gate if it is not already installed (its hooks and resident guarded branch configuration live in each clone's own `.git/` and do not travel with the repository):
  - Run this in the root of the main working copy: `git config --get-all revert-gate.branch >/dev/null || sh .githooks/install.sh $(git config core.hooksPath >/dev/null && echo --branches-only)`. If it is already installed this does nothing; if not, it installs from the committed `.githooks/branches`; in a repository where `core.hooksPath` is set (hooks owned by a system such as husky), it only writes the resident guarded branch configuration (the branches always guarded by the gate, usually the main branch).
  - Tell the user if the command actually performed an install; report back on error rather than handling it yourself.
- Once the gate is in place, create in this order:
  1. Align the target branch per "## Aligning with the remote target branch"; being ahead of the remote (unpushed commits) is normal and needs no action. If it cannot be aligned, report back rather than cutting from the old position.
  2. Create the branch and worktree from the aligned target branch.
  3. Record the target branch and fork point: `git config branch.<branch>.targetBranch <target-branch>` and `git config branch.<branch>.forkPoint $(git rev-parse <target-branch>)`. The fork point is read by the pre-merge check; a plain rebase does not update it — the only case where it is updated is covered in "## Aligning with the remote target branch".

## Aligning with the remote target branch

- Align in three places: before cutting, before merging locally, and after a push of the target branch is rejected. In the main working copy, run `git fetch` (if a fast-forward or rebase will be needed and it has uncommitted changes, report back first), then handle the relationship between the target branch and its upstream (`<target-branch>@{u}`):
  - Behind (no unpushed commits locally): fast-forward with `git merge --ff-only <target-branch>@{u}`.
  - Diverged (unpushed commits locally and new commits on the remote): first record the old position with `git rev-parse <target-branch>` (needed below to move other branches), then `git rebase <target-branch>@{u}` to lift the unpushed local commits on top of the latest remote; resolve conflicts the same way as in "Squashing and rebasing".
    - After the rebase, re-run the build, tests and format checks in the main working copy; only passing counts as aligned. If they fail, report back rather than continuing.
    - The target branch's former unpushed commits have now been rewritten, while branches cut from them still carry the pre-rewrite copies — a plain rebase would move those commits a second time, so move them one by one instead:
      - Find such branches: among `git config --get-regexp '^branch\..*\.targetBranch$'`, those whose value equals this target branch; in that branch's own worktree, run `git rebase --onto <target-branch> <old-position>` to move only the branch's own commits — if that worktree has uncommitted changes, ask the user first.
      - After moving, update its fork point to `git merge-base HEAD <target-branch>` (the only case where the fork point is updated this way): the old fork point points at the rewritten, now-stale commits, and leaving it would make the pre-merge check count them.
  - No upstream: if exactly one remote has a same-named branch (`<remote>/<target-branch>`), use it in place of the upstream without setting it as the upstream; if several remotes have one, ask the user which to use, then set the upstream with `git branch -u <remote>/<target-branch>`; if none has one, skip this.

- When alignment is only discovered at merge time, the branch must land on the aligned target branch (if diverged, the `--onto` move above has already carried it); then follow the "whenever a branch has been rebased again" bullet in "## Merging".

## Squashing and rebasing

- Squash before merging: every commit on the target branch must be independently explainable and independently revertible. A branch that did one thing is squashed into a single commit; keep several only when it genuinely contains several independent changes, and merge them into one when the split point falls inside a single file. Commit messages describe the final state, not the process (do not write "fix the previous commit" or "adjusted per feedback"), in whatever language the repository's history uses.
- Before squashing and rebasing, the branch's worktree must be clean (`git status --porcelain` empty, including untracked files); if it is not, report back rather than committing, stashing, or discarding on the user's behalf: uncommitted changes would get folded into the commit by `reset --soft`, wiped out by `reset --hard`, or hidden in a stash by `rebase.autoStash`.
- Always squash on the original base — never reset to `<target-branch>` or any other newer commit, and never open a separate branch and copy the files across: with a different base and unmerged content, the old code would count as a new change and silently revert whatever landed on the target branch after that base. In order:
  1. First record `git rev-parse HEAD` (called the recorded commit below).
  2. Then `git reset --soft $(git merge-base HEAD <target-branch>)` and commit again; the changes are already staged, so no separate `git add` is needed.
  3. After squashing, verify that the squashed commit's parent (the bottommost one, when several are kept) is still the original base and that `git diff <recorded-commit> HEAD` is empty; if either check fails, `git reset --hard <recorded-commit>` and redo it.
- After squashing, `git rebase <target-branch>`, resolving each commit's conflicts once. Resolving a conflict means combining both sides' changes, not always taking your own side; ask the user when unsure how to combine them. Any further edits needed after the rebase get folded back into the original commit, not stacked as new ones.
- A branch only you use (no one else's commits on the remote; ask the user when unsure) that has been pushed before must be pushed again with `--force-with-lease` only, after squashing and rebasing; confirm with the user before pushing.
- A branch that has been pushed but is not exclusively yours (the remote has someone else's commits): do not squash, rebase, or force-push it — report back and let the user decide.

## Reverting and undoing

- The repository has a revert gate installed (`.githooks/revert-gate.py`): when a guarded branch moves (each branch's target branch as recorded in `targetBranch`, plus the gate's resident guarded branches), local merge / commit / reset and push are all checked automatically, and updates that rewrite published history or revert changes already on that branch are refused.
- When the gate reports that existing changes were reverted, a squash or conflict resolution went wrong — fix it where the error happened: if it has not yet landed on the target branch, go back to the branch, restore the reverted changes, and fold them back into the original commit (report back first if that means going back to before the squash and redoing it); if a rebase was blocked while aligning with the remote, `git rebase --abort` and rebase again; if it has already landed on the target branch, add a commit in the main working copy to restore it rather than resetting the target branch.
- When a reset is blocked, the index (and, with `--hard`, the working tree too) has already been changed to the target position's content: recover with `git reset --merge` after `--hard`, or `git reset` after `--mixed`.
- To genuinely drop a commit, ask the user first; once they agree, either use `git revert` in the main working copy, or add a `Reverts: <sha>` line to the commit that does the reverting — either way declares to the gate that the revert is intentional. Never use reset or amend on the target branch to remove an already-pushed commit.

## Merging

- Order: align with the remote → squash → rebase → verify → confirm with the user → gate precheck → `merge --ff-only` → clean up. Verification means running the build, tests, and format checks in the branch's worktree; if it fails, report back and do not merge.
- Projects that have moved to merging through PRs (as the user has said, or as the repository's docs establish): still align, squash, rebase, and verify, up through confirming with the user; do not raise a PR within this flow, do not merge locally, and do not run the precheck — raising the PR and the cleanup afterward belong to the `git-pr` skill, or to the user's instructions if it is not installed.
- Before merging locally you must confirm: every earlier step is done (for history that cannot be squashed, see the "pushed but not exclusively yours" bullet in "Squashing and rebasing"); the main working copy is still on the target branch (switch back if not; report back if uncommitted changes prevent switching back); the user has confirmed (show them the final commit list and the verification results); and the gate precheck passes (skip this when the repository has no `.githooks/`; when the gate blocks a merge, the main working copy has already been written to the branch's content and needs `git reset --merge` to recover, so precheck first). The gate precheck runs the command below in the main working copy, and counts as passing only when the exit code is 0:

  ```bash
  b=$(git config --get-all revert-gate.branch | head -n1); s=$(git show "refs/heads/$b:.githooks/revert-gate.py" 2>/dev/null) || { echo "gate not ready" >&2; (exit 3); } && python3 -I -c "$s" check <target-branch> <branch>
  ```

  - Exit code 3 (prints "gate not ready"): first install it as in the worktree-creation step; if the first resident guarded branch (the first line of `git config --get-all revert-gate.branch`) has no local branch in this clone, create one with `git branch <that-branch> <remote>/<that-branch>`; re-run, and if it still reports 3, report back rather than treating it as a pass.
  - Exit code 1: it found a problem; handle it per what it reports. If it reports that existing changes were reverted, follow the "when the gate reports that existing changes were reverted" bullet in "## Reverting and undoing" and fix it on the branch; if it reports the target branch as behind or diverged from the remote (as of the last fetch), handle it per "## Aligning with the remote target branch".
  - Exit code 2: the gate itself failed; report back, and do not treat it as a pass.
- Merge locally in the main working copy with `git merge --ff-only <branch>`. A failed fast-forward means the target branch has moved again or the rebase was not clean: go back to the branch, rebase again, follow the next bullet through verification and confirmation, re-run the precheck, and merge.
- Whenever a branch has been rebased again (including after aligning with the remote), the new code it brings in has not been through the earlier verification, so proceed as follows:
  - Re-run verification first (handle a failure as above);
  - once it passes, re-confirm with the user if conflicts were resolved or the commit list changed (e.g. squashed again); otherwise, when the branch's own commit content is unchanged, reuse the original confirmation;
  - then re-run the precheck and merge.
- Once the merge is confirmed to have succeeded (`merge --ff-only` exited 0), go back to the main working copy, then immediately `git worktree remove` that worktree and delete the branch (report back on failure rather than adding `--force`; do not touch the remote branch) — this is part of the merge, no need to ask again. Do not push the target branch automatically; push it only if the user asks.

## Pushing

- When a push of the target branch is rejected (the remote reports non-fast-forward, or pre-push reports the remote has commits you do not have), do not force-push: align per "## Aligning with the remote target branch" and re-push once verification passes.
