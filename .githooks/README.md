# Repository git hooks

Two stages: staged formatting on commit and push verification on push. The revert gate is not one of these hooks: it is
Octoboard's built-in gate, which applies only to git run inside Octoboard project sessions.

## How it works

- `hook.sh` is the one entry point, installed under the names `pre-commit` and `pre-push` in the common hooks directory
  (shared by every worktree). It runs the scripts of the checkout that is committing or pushing:
  `scripts/hooks/format-staged.mjs` for `pre-commit` and `scripts/hooks/pre-push.mjs` for `pre-push`, or, on a branch
  cut before the scripts moved there, the same names directly under `scripts/`. A checkout with neither (a branch older
  than the hook) gets a warning and that stage is skipped; a script that exists and fails fails the hook. The top-level
  `scripts/format-staged.mjs` and `scripts/pre-push.mjs` of this layout are forwarding stubs, for hook copies installed
  before the move, which look only there. Delete the stubs (and their entry in `scripts/verify/scope.mjs`) once every
  clone's installed hooks have been refreshed from the new `hook.sh` (`pnpm hooks:install` on the main branch).
- `install.sh` (`pnpm hooks:install`) copies `hook.sh` into the hooks directory. It overwrites an existing hook only
  when that carries this entry point's marker line, refuses otherwise, and checks everything before writing anything. It
  refuses when `core.hooksPath` is set in a configuration file: that hook system must call
  `sh .githooks/hook.sh <pre-commit|pre-push> "$@"` itself. `GIT_CONFIG_COUNT`/`GIT_CONFIG_PARAMETERS` entries are
  ignored in that check, since tools that launch agent sessions inject them.
- `auto-install.sh` runs on `pnpm install` (the root `prepare` script) and never fails the install. It installs only
  from the main working copy with the main branch checked out (the remote's default branch, `main` without one), because
  a checkout of an older revision would replace the shared hooks with its older `hook.sh`. It does nothing in CI, in
  linked worktrees and other checkouts, or when `core.hooksPath` is set; a hook that cannot be installed is reported as
  a warning. After hook changes land in the main branch, run `pnpm install` there again.

## What the hooks do

- Commit (`scripts/hooks/format-staged.mjs`, `pnpm format:staged`): formats only staged JavaScript, TypeScript, TSX,
  Vue and Rust files from their staged content (ESLint fixing layout only, rustfmt), and three-way merges the result into the
  working file so separate unstaged edits are preserved. A formatting conflict with unstaged changes stops the commit
  without changing the index or the working files; a partial commit (`git commit <paths>`) is refused; a file that
  changed while formatting runs is not overwritten. CSS, JSON, Markdown and other types are outside its scope.
- Push (`scripts/hooks/pre-push.mjs`): runs `pnpm verify` for each pushed branch, measured from the destination's
  advertised commit to the exact pushed commit. How the baseline, scope and reuse work is in
  [`scripts/verify/README.md`](../scripts/verify/README.md).

## External interfaces

- `hook.sh <pre-commit|pre-push>` — the hook entry point.
- `install.sh` — install the hooks into the common hooks directory.
- `auto-install.sh` — the install the root `prepare` script runs.
