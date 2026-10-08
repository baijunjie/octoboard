# Catching a branch up with its target branch

## A pushed branch the revert gate guards is caught up by merging, never by rebasing

A branch is guarded by the revert gate when some other branch records it as its own target branch
(`git config branch.<other>.targetBranch`, which is what every worktree cut from it records) or when it is one of
the gate's resident guarded branches — so a topic branch that milestone worktrees were cut from is guarded,
while an ordinary branch nobody cut from is not.

Once such a branch has been pushed, `.githooks/revert-gate.py` refuses any move of it that is not a
fast-forward and drops a commit the remote already has ("must not rewrite published history"), and it refuses it
at the local ref update, not only at push time — so `git rebase origin/main` on it fails outright and there is no
sanctioned rebase path. Getting around the gate is forbidden (the "Development workflow: Git worktree" section of
`CLAUDE.md`), so bring the target branch's new commits in with a merge (`git merge origin/main` for a branch cut
from `main`) and carry on.

Read the gate's own suggestion with that in mind: it prints `git rebase <upstream>`, which replays local unpushed
commits onto *this* branch's remote tip and brings in nothing from the target branch. Squashing and rebasing an
unguarded development branch before it merges back is unaffected and still follows the "Squashing and rebasing"
section of `.claude/skills/git-worktree/SKILL.md`.
