# Catching a branch up with its target branch

## The target branch is the local `main`, which is usually ahead of `origin/main`

Branches are merged back into the local `main` by fast-forward, and `main` is pushed only when the user asks, so
`origin/main` often lags it by many commits. Whenever the question is what has landed — whether a fix is already on
`main`, whether a worktree branch is behind its target, what to rebase or merge it onto — compare against the local
`main` (`git log main`, `git merge-base HEAD main`), never against `origin/main`; reading `origin/main` has led to
reporting a fix as missing when it was already merged. Aligning `main` itself with its remote still follows the
"Aligning with the remote target branch" section of `.claude/skills/git-worktree/SKILL.md`.

## A pushed branch the revert gate guards is caught up by merging, never by rebasing

Which branches the gate guards is in the "Reverting and undoing" section of `.claude/skills/git-worktree/SKILL.md`:
every branch some worktree records as its target branch is one, so a topic branch that milestone worktrees were cut
from is guarded, while an ordinary branch nobody cut from is not.

Once such a branch has been pushed, `.githooks/revert-gate.py` refuses any move of it that is not a
fast-forward and drops a commit the remote already has ("must not rewrite published history"), and it refuses it
at the local ref update, not only at push time — so `git rebase main` on it fails outright and there is no
sanctioned rebase path. Getting around the gate is forbidden (the "Development workflow: Git worktree" section of
`CLAUDE.md`), so bring the target branch's new commits in with a merge (`git merge main` for a branch cut from
`main`) and carry on.

Read the gate's own suggestion with that in mind: it prints `git rebase <upstream>`, which replays local unpushed
commits onto *this* branch's remote tip and brings in nothing from the target branch. Squashing and rebasing an
unguarded development branch before it merges back is unaffected and still follows the "Squashing and rebasing"
section of `.claude/skills/git-worktree/SKILL.md`.
