# Committing alongside other agents

## Stage your paths only right before committing, and check the index holds nothing else

Applies whenever another agent may commit in the same worktree as you — a dispatcher and its subagents, or several
subagents of one round. They all share the worktree's one git index, and this repository leaves no private way around
it: the `pre-commit` hook (`scripts/hooks/format-staged.mjs`) refuses a commit made through a temporary
`GIT_INDEX_FILE`, and `git commit <paths>` is refused, so every commit takes whatever the shared index holds at that
moment.

- Run anything long — checks, builds, a gallery sweep — before you stage, never between `git add` and `git commit`.
  Files left staged go into the next commit anyone makes, and until you commit, nobody else can commit without
  carrying them.
- `git add` only your own paths, then run `git diff --cached --name-only` immediately before `git commit`. If it lists
  a path that is not yours, do not commit it into yours: wait for that agent's commit to land and check again, or
  report it to whoever dispatched you.
