# Verification (`pnpm verify`)

Read-only checks for the packages a change affects, run by the pre-push hook (`scripts/pre-push.mjs`), by hand, and in
CI. Nothing here rewrites files: formatting belongs to the pre-commit hook. The entry point is `scripts/verify.mjs`;
this directory holds its parts and their tests (`*.test.mjs`, run as the `verify` package's tests).

## How it works

1. **Baseline** (`changes.mjs`). The changes are measured from a base commit to the working copy's tree.
   - `--base <ref>` names it explicitly, force-push tips included. The pre-push hook passes the destination's live tip
     (`--base <tip> --head <pushed commit>`); for a branch being created it passes the merge base with the live remote
     target branch (`branch.<name>.targetBranch`, else the remote's HEAD), and with no baseline at all it passes
     `--full`. A tag push is not verified (release CI checks the tag).
   - Without `--base`, the base is the fetched upstream of the branch, else of its target branch. With neither, or when
     the base cannot be fetched or read, the scope is every package; only for a branch with no upstream or target branch
     does `--quick` instead measure from the merge base with the remote target branch (or remote HEAD), as a new branch
     is at push.
   - With `--head`, the pushed commit must be the checked-out commit with a clean working tree, so a different checkout
     cannot certify it.
2. **Scope** (`scope.mjs`). Each changed file is matched against every rule in `RULES`; the consumers of all matching
   rules are unioned, so an ordinary UI change does not run Rust or website checks, while a shared input (the protocol,
   an imported module, configuration, dependency files) selects each package that reads it. A file no rule matches
   selects every package, except Markdown, which is read by no check. If nothing checked changed, the mode is `unread`
   and nothing runs. `build` inputs (`builds`) add the UI build. The `menu.*` keys of the UI message catalog count only
   when a changed line is one of them. A cross-package read (a test that reads another package's file, an import across
   packages) is declared in `scope.mjs`; `scope.test.mjs` fails when a new one is not. `--full` selects every package
   and `--release` the shipping application only.
3. **Steps** (`steps.mjs`). A package selects its steps (lint, type check, locales, tests, build, script tests; both
   Rust crates always run together). Light steps run first and heavy ones (UI tests, Rust builds and tests, the website
   build) last, and a failure stops the run. The website build runs before its SEO tests, which read the generated
   output. `--release` runs the application checks: application lint, production UI type check, application UI tests,
   desktop script tests, rustfmt, clippy and Rust tests; `.github/workflows/quality.yml` runs it unconditionally before
   a release and on request, plus a Linux `cargo check` of both crates.
4. **Reuse** (`stamp.mjs`, `tree.mjs`, `rerun.mjs`). `tree.mjs` fingerprints the working copy (tracked and untracked,
   non-ignored files, taken through a scratch index). A pass is recorded in the common git directory, keyed by that tree
   and the toolchain versions (node, pnpm, rustc) per step command, so every worktree sees it, and a run on identical
   content runs nothing; `--fresh` ignores earlier passes. A pass is recorded only for the content it ran on. Ignored
   inputs are not covered. When a narrowable UI test step failed, the next run re-runs only the failed test files and
   the changed ones, provided everything else that changed is a test file nothing imports.
5. **Slots** (`lock.mjs`). Heavy steps take a machine-wide slot shared by every worktree (first come, first served;
   consecutive heavy steps share one hold). The number of slots follows cores and memory and is overridden by
   `OCTOBOARD_VERIFY_SLOTS`. The UI's gallery sweep takes the same lock.
6. **Quick mode** (`quick.mjs`, `--quick`). The normal plan without builds and without the two full test suites; UI and
   Rust tests narrow to what the changes reach (`vitest related`, and cargo module filters), falling back to a full
   suite where the reach cannot be traced. Narrowed steps are never recorded as a pass of the whole.

## Changing it

- A new package or a new input: add or extend a rule in `RULES`; a rule's `consumers` decide which steps run.
- A new check: a step in `steps.mjs`; keep `quick.mjs` consistent (its tests fail when a UI test reads files or a crate
  has crate-wide tests that its lists do not name).
- A script that other packages import is named in the matching rule (`scripts/git.mjs`, `scripts/env.mjs`, `lock.mjs`).

## External interfaces

- `pnpm verify` with `--dry-run`, `--base <ref>`, `--head <commit>`, `--full`, `--release`, `--quick`, `--fresh`.
- `scope.mjs`: `RULES`, `planScope`, `matchingRules`. `steps.mjs`: `checkSteps`. `quick.mjs`: `quickSteps`.
- `changes.mjs`: `resolveBase`, `resolveRemoteTargetBase`, `changedFiles`, `requirePushedTree`.
- `lock.mjs`: `acquireSlot`; `stamp.mjs`: `readStamp`, `recordPass`; `rerun.mjs`: the failed-file record; `tree.mjs`:
  `workingTree`.
