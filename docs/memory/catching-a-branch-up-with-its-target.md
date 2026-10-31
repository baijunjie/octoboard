# Catching a branch up with its target branch

## The target branch is the local `main`, which is usually ahead of `origin/main`

Branches are merged back into the local `main` by fast-forward, and `main` is pushed only when the user asks, so
`origin/main` often lags it by many commits. Whenever the question is what has landed — whether a fix is already on
`main`, whether a worktree branch is behind its target, what to rebase or merge it onto — compare against the local
`main` (`git log main`, `git merge-base HEAD main`), never against `origin/main`; reading `origin/main` has led to
reporting a fix as missing when it was already merged.

## A pushed branch the revert gate guards is caught up by merging, never by rebasing

Applies to git run inside an Octoboard project session while Octoboard's `revert_gate` setting is on for the project.
The guarded branches are listed in the "Guarded branches" section of `docs/product/revert-gate.md`; besides `main`, a
topic branch that milestone worktrees were cut from is one, because it is their recorded target branch.

Once such a branch has been pushed, the gate refuses any move of it that is not a fast-forward and drops a commit
already on a remote-tracking branch, and it refuses it at the local ref update, not only at push time — so
`git rebase main` on it fails outright and there is no sanctioned rebase path. Getting around the gate is forbidden
(the "Repository checks" section of `CLAUDE.md`), so bring the target branch's new commits in with a merge
(`git merge main` for a branch cut from `main`) and carry on.

Read the gate's suggestion for dropped published commits with that in mind: its `git rebase <upstream>` replays local
unpushed commits onto *this* branch's remote tip and brings in nothing from the target branch. Rebasing a development
branch that is not guarded onto its target before it merges back is unaffected.

Keep delivery-time squashing separate from catching up with the target: follow "The development rule" in
`docs/product/git-workflow-settings.md` for the squash and its tree comparison first, then apply the catch-up rule
above if alignment is required. A branch being unpushed does not by itself make reset-based squashing safe; before
choosing a procedure, check "Guarded branches" and "A local update of a guarded branch" in `docs/product/revert-gate.md`.
