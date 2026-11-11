# Verification (`pnpm verify`)

Read-only checks for the packages a change affects, run by the pre-push hook (`scripts/hooks/pre-push.mjs`), by hand,
and in CI. Nothing here rewrites files: formatting belongs to the pre-commit hook. The entry point is
`scripts/verify.mjs`; this directory holds its parts and their tests; every `*.test.mjs` under `scripts/` (this
directory, `hooks/`, `lib/`) runs as the `verify` package's tests.

## How it works

1. **Baseline** (`changes.mjs`). The changes are measured from a base commit to the working copy's tree.
   - `--base <ref>` names it explicitly, force-push tips included. The pre-push hook passes the destination's live tip
     (`--base <tip> --head <pushed commit>`); for a branch being created it passes the merge base with the live remote
     target branch (`branch.<name>.targetBranch`, else the remote's HEAD), and with no baseline at all it passes
     `--full`. A tag push is not verified by this hook; release preparation checks the commit locally before tagging.
   - Without `--base`, the base is the fetched upstream of the branch, else of its target branch. With neither, or when
     the base cannot be fetched or read, the scope is every package.
   - `--quick` runs before a local merge, so a branch with no upstream of its own is measured from its merge base with
     its local target branch: the target's own commits, pushed or not, are already checked, and only the branch's
     changes since the fork count. The target is `branch.<name>.targetBranch` (an explicit setting, never replaced
     when that branch is missing), else the local branch the branch tracks, else the local branch the remote's HEAD
     names. A branch tracking the remote branch of its target (`git switch -c topic origin/main`) counts as having no
     upstream of its own; any other upstream is kept. With no local target, or when the checked-out branch is the
     target itself (its merge base with itself is HEAD, which would measure nothing), the order continues with the
     upstream of the target branch, then the merge base with the remote target branch (or remote HEAD), as a new
     branch is at push. Push verification and a run without `--quick` never use the local target. The plan prints
     `verify: measured from the merge base with ...` when a quick run measured from a merge base; with an upstream
     or `--base` it prints nothing.
   - With `--head`, the pushed commit must be the checked-out commit with a clean working tree, so a different checkout
     cannot certify it.
2. **Scope** (`scope.mjs`). Each changed file is matched against every rule in `RULES`; the consumers of all matching
   rules are unioned, so an ordinary UI change does not run Rust or website checks, while a shared input (the protocol,
   an imported module, configuration, dependency files) selects each package that reads it. A file no rule matches
   selects every package, except Markdown, which is read by no check. If nothing checked changed, the mode is `unread`
   and nothing runs. `build` inputs (`builds`) add the UI build. The `menu.*` keys of the UI message catalog count only
   when a changed line is one of them. A cross-package read (a test that reads another package's file, an import across
   packages) is declared in `scope.mjs`; `scope.test.mjs` fails when a new one is not. `--full` selects every package
   and `--release` the shipping application only. `--release-group frontend|native|windows` selects one part of the
   release plan; it requires `--release` and cannot be combined with `--full` or `--quick`. Without a group, `--release`
   still selects every application check.
3. **Steps** (`steps.mjs`). Every non-`unread` plan, including release and quick plans, checks generated identity fields
   against `config/app.json` without writing (`scripts/sync-identity.mjs --check`). A package selects its other steps
   (lint, type check, locales, tests, build, script tests; both Rust crates always run together). Light steps run first
   and heavy ones (UI tests, Rust builds and tests, the website build) last, and a failure stops the run. The website
   build runs before its SEO tests, which read the generated output. `--release` runs the application checks:
   application lint, production UI type check, application UI tests, desktop script tests, rustfmt, clippy, Rust
   tests and the Windows cross-check (`pnpm check:windows`: both crates, all targets, through `cargo xwin`, and a link of the daemon's executable once its check passes). Local
   `release:version` runs the complete plan with `--fresh --head <release-commit>` before tagging, then scoped push
   verification can reuse matching passes. The release groups partition those same commands; each also runs the
   identity check. The optional hosted workflow runs the frontend and native groups independently, with separate Linux
   and Windows jobs (see "Hosted application checks"); the windows group is local only, since its toolchain
   (cargo-xwin, the `x86_64-pc-windows-msvc` target, LLVM's clang, the downloaded MSVC CRT and Windows SDK) is
   not on the hosted macOS runner. A machine missing any of it fails the step before compiling, with a message naming
   each missing piece and how to install it; the check never accepts the SDK license itself (see
   `scripts/lib/windows-toolchain.mjs`).
4. **Reuse** (`stamp.mjs`, `tree.mjs`, `rerun.mjs`). `tree.mjs` fingerprints the working copy (tracked and untracked,
   non-ignored files, taken through a scratch index). A pass is recorded in the common git directory, keyed by that tree
   and the toolchain versions (node and pnpm, plus rustc except for the frontend release group) per step command, so
   every worktree sees it, and a run on identical content runs nothing; `--fresh` ignores earlier passes. A pass is
   recorded only for the content it ran on. Ignored inputs are not covered. When a narrowable UI test step failed, the
   next run re-runs only the failed test files and the changed ones, provided everything else that changed is a test
   file nothing imports.
5. **Slots** (`lock.mjs`). Heavy steps take a machine-wide slot shared by every worktree (first come, first served;
   consecutive heavy steps share one hold). The number of slots follows cores and memory and is overridden by
   `OCTOBOARD_VERIFY_SLOTS`. The UI's gallery sweep takes the same lock.
6. **Quick mode** (`quick.mjs`, `--quick`). The normal plan without builds and without the two full test suites; UI and
   Rust tests narrow to what the changes reach (`vitest related`, and cargo module filters), falling back to a full
   suite where the reach cannot be traced. A file outside the UI package runs the UI tests that read it
   (`UI_READERS`), and Markdown a crate reads runs the reading modules' tests (`RUST_MARKDOWN_READERS`). A run that
   needs one crate keeps `--workspace` and selects that crate's test targets (`CRATES`), so it reuses the workspace
   build. Narrowed steps are never recorded as a pass of the whole.

## Git environment boundary

Run verification and tests with Git fixtures from the intended working tree with no repository-local Git location
variables in their environment. Git exports these variables to hooks, where they can override a child process's
working directory and make a temporary repository command operate on the caller's repository instead.

`environment.mjs` defines this boundary in one place. Before the pre-push hook launches `pnpm verify`, it removes
repository locations from that child's environment: `GIT_DIR`, `GIT_COMMON_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`,
`GIT_OBJECT_DIRECTORY`, `GIT_ALTERNATE_OBJECT_DIRECTORIES`, `GIT_SHALLOW_FILE`, `GIT_GRAFT_FILE`, `GIT_CONFIG`,
`GIT_PREFIX`, `GIT_IMPLICIT_WORK_TREE`, `GIT_INTERNAL_SUPER_PREFIX` and `GIT_QUARANTINE_PATH`. The hook's own environment
stays intact for its operations on the pushed repository.

Configuration and hook policy remain inherited, including `GIT_CONFIG_COUNT`, its `GIT_CONFIG_KEY_n` and
`GIT_CONFIG_VALUE_n` entries, `GIT_CONFIG_PARAMETERS` and `GIT_CONFIG_GLOBAL`. Clearing locations must not remove the
caller's injected hooks or replace the entire Git environment.

The verification entry point checks this boundary before invoking Git. A leaked location produces
`Refusing to run verification with repository-local Git variables: ...` and exit status 2, before repository
discovery, snapshotting or tests. Tests that create temporary Git repositories also assert the boundary at module
startup, so direct `node --test` invocations refuse the same environment before any Git operation. If this refusal
appears, run again from an ordinary working-tree shell with the listed locations cleared; retain the user's
configuration and hook-policy variables. A pre-push invocation prepares its verification child's environment
automatically.

## Hosted application checks

`.github/workflows/quality.yml` runs only on request. Its independent jobs provide hosted checks; release CI
does not call this workflow or wait for its results:

| Job | Runner | Command and scope |
|---|---|---|
| `frontend` | `ubuntu-latest` | `pnpm verify --release --release-group frontend --fresh`: identity, application lint, production UI type check and application UI tests. |
| `native` | `macos-26` | `pnpm verify --release --release-group native --fresh`: identity, desktop script tests, rustfmt, clippy for the workspace and all targets, and both Rust crates' tests. |
| `linux-check` | `ubuntu-latest` | `node scripts/cargo.mjs check --locked`: compile both crates with the Linux prerequisites installed. |
| `windows-check`, `windows-bundle` | `windows-latest` | Native Windows checks and the unsigned installer build (see the [tooling overview](../../docs/structure/tooling.md)); the local release plan's windows group also cross-checks the same crates from the Mac. |

The frontend group does not invoke `rustc`, including when identifying the toolchain for local pass stamps, so it
does not download the pinned Rust toolchain. Desktop script tests stay on macOS because archive tests exercise its
BSD tar; Rust checks stay there for the desktop crate's native frameworks. The separate Linux job detects compilation
failures that the local macOS release checks cannot; it is not a publication gate. Website and gallery-only checks
remain outside the application release plan.

CI uses `--fresh` to run the complete selected checks each time. Rust dependency caches reduce repeated compilation;
they do not carry successful verification stamps or bypass failing tests. Cache profiles, manual main-branch warming,
retention and timing measurements are documented in the [Rust cache module](../../.github/actions/rust-cache/README.md).

## Changing it

- A new package or a new input: add or extend a rule in `RULES`; a rule's `consumers` decide which steps run.
- A new check: a step in `steps.mjs`; keep `quick.mjs` consistent (its tests fail when a UI test reads files or a crate
  has crate-wide tests that its lists do not name). A new release check belongs in the frontend, native or windows
  group; `steps.test.mjs` checks that their combined commands equal the complete release plan.
- A new test target in a Rust crate: name it in `CRATES` of `quick.mjs`, whose tests fail on a `[[test]]`, `[[bench]]`
  or `[[example]]`, a `tests/`, `src/bin`, `examples` or `benches` directory, a second daemon binary, a daemon library,
  or a doctest in the desktop crate. A doctest cannot be selected next to `--lib` and `--bin`, so it needs a change to
  how that crate is narrowed.
- A new `include_str!` of Markdown is named in `DAEMON_READS_MARKDOWN` (or a rule of its own) and in
  `RUST_MARKDOWN_READERS`; the tests fail with the file to name. A file outside the UI package that a UI test reads runs
  the full UI suite in a quick run until `UI_READERS` names its readers; the tests fail when an entry leaves a reader
  out.
- A script that other packages import is named in the matching rule (`scripts/lib/git.mjs`, `scripts/lib/env.mjs`,
  `lock.mjs`).
- A test that creates a temporary Git repository calls `assertRepositoryEnvironmentCleared` at module startup before
  any Git operation. A hook launcher uses `withoutRepositoryEnvironment` for its verification child; changing the
  working directory alone does not isolate a Git fixture.

## External interfaces

- `pnpm verify` with `--dry-run`, `--base <ref>`, `--head <commit>`, `--full`, `--release`,
  `--release-group frontend|native|windows`, `--quick`, `--fresh`.
- `scope.mjs`: `RULES`, `planScope`, `matchingRules`. `steps.mjs`: `checkSteps`. `quick.mjs`: `quickSteps`.
- `changes.mjs`: `chooseBase`, `resolveBase`, `resolveLocalTargetBase`, `remoteTargetBase` (the one
  merge-base-with-the-remote-target function, also used by the pre-push hook), `resolveRemoteTargetBase`,
  `changedFiles`, `requirePushedTree`.
- `lock.mjs`: `acquireSlot`; `stamp.mjs`: `readStamp`, `recordPass`; `rerun.mjs`: the failed-file record; `tree.mjs`:
  `workingTree`.
- `environment.mjs`: `withoutRepositoryEnvironment`, `assertRepositoryEnvironmentCleared`.
