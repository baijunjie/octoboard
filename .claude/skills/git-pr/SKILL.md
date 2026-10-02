---
name: git-pr
description: Squash the current branch's changes, push them to the remote, and open a PR; once the PR is created, clean up the local branch and worktree. Defaults to the target branch this project agreed on, and another branch can be given at call time. Use when the user says "open a PR", "send a PR", "open a pull request", "push it and PR it to xxx".
---

# Opening a PR

## Target branch

- When a branch is given at call time, PR to that branch; this applies to this round only.
- When none is given and the current branch records `git config branch.<branch>.worktreeTarget`, PR to the branch it
  records.
- Without that record, and with none given, PR to `main` (this project's default PR target branch, fixed at install
  time).
- `git fetch` the target branch before doing anything; every later comparison and the squash go by the remote's latest
  state. The remote is the target branch's upstream (`git config branch.<target-branch>.remote`); when the branch does
  not exist locally or has no upstream set, use the remote that has a branch of the same name, and ask the user when
  there is more than one. `<remote>` below means that one.

## Steps

1. **Prepare the branch**: when there are uncommitted changes, first ask the user whether to include them in this PR.
   When you are on the target branch itself:
   - first check that every commit the local target branch has beyond the remote belongs to this PR, and ask the user
     when others are mixed in;
   - then `git switch -c` a new branch to carry the local commits (prefix `feat/`, `fix/`, etc. by the kind of change);
   - finally use `git branch -f` to put the local target branch back at the remote's position, or those commits will
     stay behind on it.

   When you are not on the target branch, ask the user when `git log <remote>/<target-branch>..HEAD` has commits mixed
   in that do not belong to this PR (usually unpushed commits on the target branch).
2. **Squash**: several commits are squashed into one by default; keep several only when they really do contain several
   mutually independent changes. The squash is done on the original base:
   `git reset --soft $(git merge-base HEAD <remote>/<target-branch>)`, then commit again.
   Do not reset to the target branch's latest commit — with the base moved and the content not merged, that quietly
   reverts changes on the target branch.
   The commit message's language follows the repository's history.
3. **Push**: when the squash rewrote already-published history, only a branch you alone use may be force-pushed with
   `--force-with-lease`, and ask the user before pushing.
4. **Open the PR**: the title's and description's language follows the repository's history. Do not rebase on your own
   initiative once it is created. Tell the user when the PR conflicts with the target branch, and ask whether to use
   merge or rebase when conflicts need resolving; resolving conflicts means combining both sides' changes, not taking
   your own side across the board — that amounts to reverting the corresponding changes on the target branch.
5. **Clean up locally**: clean up as soon as the PR is confirmed created and the branch is pushed to the remote; this
   is part of opening a PR and needs no further asking —
   - the branch is in a worktree: leave that worktree directory first, then `git worktree remove`;
   - the main working copy is parked on that branch: switch back to the target branch (create a tracking branch from
     the remote when it does not exist locally);
   - finally `git branch -D` to delete the local branch.

   Stop and ask the user when the worktree has uncommitted or untracked files, or when another worktree holds the
   target branch so you cannot switch back; do not add `--force`.

When an operation is blocked by one of the repository's git hooks, handle it as the message says; do not work around it.

## Authoring identity

Applies to the squashed commit message and the PR title and description:

- Leave no AI traces: no `Co-Authored-By`, no "Generated with ...", no mention of AI assistance.
- Write in the voice of a human developer, and make no statement about being an AI.
