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
   packages) is declared in `scope.mjs`; `scope.test.mjs` fails when a new one is not. The exception is what the website
   and the docs import from the UI package: `ui-imports.mjs` reads those from their sources (through the UI package's
   `exports` map and the relative imports inside it), so a module they start to reach, such as a provider presentation
   module the agent marks begin to import, is in scope with no edit. The docs package (`apps/docs`) is
   selected by its own files and by the UI's language list and `exports` map, which its locale table is built from
   (verify, ui and docs). The language list also selects the website, the desktop scripts and the daemon. The desktop
   installer test and the daemon's interface language test read it directly. A change to the docs content directory
   also selects the website (verify, web and docs), whose SEO
   test checks that every docs page the website links to exists in every language, and a change to the website's logo
   and wordmark or to the website modules a docs test reads selects the docs in turn (verify, web and docs). `--full`
   selects every package and `--release` the shipping application only. `--release-group frontend|native|windows`
   selects one part of the release plan; it requires `--release` and cannot be combined with `--full` or `--quick`.
   Without a group, `--release` still selects every application check.
3. **Steps** (`steps.mjs`). Every non-`unread` plan, including release and quick plans, checks generated identity fields
   against `config/app.json` without writing (`scripts/sync-identity.mjs --check`), searches the production core for
   provider knowledge (`provider-neutral.mjs`, below) and holds the Rust crates' writes to their registrations
   (`registered-writes.mjs`, below). A package selects its other steps (lint, type check, locales,
   tests, build, script tests; both Rust crates always run together). The daemon package also runs the `node:test`
   files beside the JavaScript it compiles in and an agent's CLI loads in its own process (an extension, a plugin),
   `node --test apps/daemon/src/**/*.test.mjs`: a light step, since each takes about a second and builds nothing, so a
   quick run keeps it; `steps.test.mjs` fails when the daemon tracks a node test the pattern does not match. The docs package's steps are lint, type check,
   build and tests, in that order. Light steps run first and heavy ones (UI tests, Rust builds and tests, the website
   and docs builds) last, and a failure stops the run. The website build runs before its SEO tests, and the docs build
   before the docs tests, which read the generated output. `--release` runs the application checks: application lint,
   production UI type check, application UI tests, desktop script tests, the daemon's node tests, rustfmt, clippy, Rust tests and the Windows
   cross-check (`pnpm check:windows`: both crates, all targets, through `cargo xwin`, and a link of the daemon's
   executable once its check passes, for `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`). Local `release:version` runs the complete plan with `--fresh --head
   <release-commit>` before tagging, then scoped push verification can reuse matching passes. The release groups
   partition those same commands; each also runs the identity, provider-neutral core and registered writes checks. The
   optional hosted workflow runs the frontend and native groups independently, with separate Linux and Windows jobs (see "Hosted
   application checks"); the windows group is local only, since its toolchain (cargo-xwin, the `x86_64-pc-windows-msvc`
   target, LLVM's clang, the downloaded MSVC CRT and Windows SDK) is not on the hosted macOS runner. A machine missing
   any of it fails the step before compiling, with a message naming each missing piece and how to install it; the check
   never accepts the SDK license itself (see `scripts/lib/windows-toolchain.mjs`).
4. **Reuse** (`stamp.mjs`, `tree.mjs`, `rerun.mjs`). `tree.mjs` fingerprints the working copy (tracked and untracked,
   non-ignored files, taken through a scratch index). A pass is recorded in the common git directory, keyed by that tree
   and the toolchain versions (node and pnpm, plus rustc except for the frontend release group) per step command, so
   every worktree sees it, and a run on identical content runs nothing; `--fresh` ignores earlier passes. A pass is
   recorded only for the content it ran on. Ignored inputs are not covered. When a narrowable UI test step failed, the
   next run re-runs only the failed test files and the changed ones, provided everything else that changed is a test
   file nothing imports.
5. **Slots** (`lock.mjs`). Heavy steps take a machine-wide slot shared by every worktree (first come, first served;
   consecutive heavy steps share one hold). The number of slots follows cores and memory and is overridden by
   `OCTOBOARD_VERIFY_SLOTS`. The UI's gallery sweep and the docs' screenshot command take the same lock, and so do the package scripts that run
   tests and Rust checks on their own (`pnpm test:rust`, `clippy:rust`, `check:windows`, and the UI's `test`,
   `test:related` and `test:app`), through `heavy.mjs`. A UI test run that names at most eight existing test files
   (`small-run.mjs`) does not wait: it runs at once, in a group of its own with the stall watch below, and with at
   most four workers. Only flags and the value of a test name filter (`-t`) may come with the files; `--`, a heavy
   flag (`--typecheck`, `--coverage`, `--browser`, `--watch`, `--changed`), short flags run together (`-uw`) or a flag given without `=` right before a file
   (`-u src/a.test.ts`, which vitest may read as the flag's value and then run every test) makes the run queue.
   Such a run's group is recorded in no slot, so one whose `heavy.mjs` is killed outright is left for no waiter
   to end. A heavy step marks its commands as running under its hold
   (`HELD_ENV`), so those scripts run at once inside `pnpm verify` instead of waiting on the slot it holds.
   A slot cannot be held for ever, nor left with work still running (`process-group.mjs`, Unix only). Each of
   `pnpm verify`'s steps, and the command `heavy.mjs` takes a slot for (or runs without one, as a small run), runs in a process group of its own: a
   signal to the runner goes to the whole group, and whatever is left of the group once the command closes is
   ended, so an interrupted run leaves no orphaned `cargo` or test binary. A holder records its command's group in
   the slot, and a waiter taking over the slot of a holder that was killed outright ends what is left of that
   group first. A command whose processes together use under 2 s of CPU for ten minutes is stalled: its processes
   are listed, with the tests the threads of its leaf processes run on macOS (from `sample`; libtest names each
   test's thread after the test), the group is ended and the run fails with exit 124, freeing the slot.
   `OCTOBOARD_HEAVY_STALL_SECONDS` (environment, else `.env.local`) sets another window. The shared compiler
   cache's server (`sccache`), which a build may start inside its group, and the compilers it runs are never
   counted or ended.
6. **Quick mode** (`quick.mjs`, `--quick`). The normal plan without builds and without the two full test suites (nor the
   website and docs tests, which need their builds); UI and Rust tests narrow to what the changes reach (`vitest
   related`, and cargo module filters), falling back to a full suite where the reach cannot be traced. A file outside
   the UI package runs the UI tests that read it (`UI_READERS`), and Markdown a crate reads runs the reading modules'
   tests (`RUST_MARKDOWN_READERS`). A run that needs one crate keeps `--workspace` and selects that crate's test targets
   (`CRATES`), so it reuses the workspace build. Narrowed steps are never recorded as a pass of the whole.

## Provider-neutral core search

`provider-neutral.mjs` is a light step in every non-`unread` plan, including quick and release plans and each release
group. It checks all production source in the daemon and UI, rather than only changed lines, and reports the original
file, line and matching source with exit status 1 for a violation. Run it alone with
`node scripts/verify/provider-neutral.mjs`.

The search covers Rust, JavaScript, TypeScript, TSX and CSS. It looks for the ids of the registered providers, agent
names in wording, agent-specific files, directories, environment variables, CLI flags, URL schemes and hook events. The
registered providers are derived: one descriptor file `apps/daemon/src/providers/<id>.rs` per id, leaving out `mod.rs`,
`layout.rs` and `test_provider.rs`, so a provider is guarded by its id as soon as its descriptor exists. The other
spellings come from `VOCABULARY`, a table with one entry per provider id (product names, quoted forms, dot-directories,
files, environment variables, flags, URL schemes and hook events); a registered provider with no entry is guarded by its
id alone, and an entry applies whether or not its descriptor is present. Descriptors and UI
presentation modules under their respective `src/providers/` directories are outside the core; a daemon
`adapter/<id>/` directory is excluded only when a corresponding descriptor file exists. Shared adapter helpers remain
in scope.

Tests and test fixtures are excluded by their file and directory conventions, as are the UI's root `src/gallery/`,
`src/fixtures/` and `src/website-preview/` directories. Comments are ignored. Rust test-only items and external modules
are excluded only where `cfg(test)` or a necessary `test` conjunct proves they are absent from production; an
`any(test, unix)` item stays in scope. Strings remain searchable, including template strings and Rust raw strings.

There is no list of exceptions to maintain: they are derived per registered id, and the only one is that provider's line
in the adapter registry, `apps/daemon/src/adapter/mod.rs`, which pairs its id with its implementation (with or without
a `cfg` attribute before it). The registry is the permitted place to associate an id with its adapter; a nearby branch or
another line in the same file is not excepted. The vocabulary is a mechanical guard alongside review, not an
interpreter of arbitrary code.

`provider-neutral.test.mjs` covers the scope, test exclusions, the vocabulary, the derived exceptions, a provider added
by a descriptor file alone and a regression that invokes the CLI
on a generic agent-id branch and checks its failing exit status. Run it with
`node --test scripts/verify/provider-neutral.test.mjs`.

## Registered writes

`registered-writes.mjs` guards P9 of `docs/conventions/principles.md` (Octoboard leaves the user's own system as it
was): in the production Rust of both crates, a call that writes is allowed only in a file the crate's registration
names. It is a light step in every non-`unread` plan, checks all of the source rather than the changed lines, and
reports the file, line and source of each unregistered write with exit status 1. Run it alone with
`node scripts/verify/registered-writes.mjs`.

**The vocabulary** (`VOCABULARY`) is what counts as a write: the standard library's and tokio's writing, creating,
renaming, removing, linking, owner and permission functions, called or imported by name; those modules and types
under another name (`use std::fs as f`, `File as F`) and the `fs_err` crate; `File::create` and `DirBuilder`; an open
with a flag that writes set to `true` (`write`, `append`, `create`, `truncate`, `create_new`), and the flags `open`
and `CreateFileW` write with (`O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_TRUNC`, `O_APPEND`, `GENERIC_WRITE`,
`FILE_APPEND_DATA` and their like) wherever they are named, so that an open that only reads is no hit; changes
through a file handle; the C library's and Windows' file calls that change something; opening an SQLite database;
unpacking an archive; the `trash` crate, the daemon's Trash tool and taking that tool from the host tools; the Windows
registry, the credential stores, the macOS login item, the taskbar's jump list and the updater's install. Comments
and strings are blanked first. A `git` run is not in it, whatever its `Access`: what is done in a project's repository
is outside P9.

To these the registration adds the project's own functions that write (`registered function` in a report): every
public function of a `primitive` file, and the entry points other entries name. They count where they are called,
not where a function of the same name is declared.

**The registration** is one file per crate, beside its path helpers: `apps/daemon/src/registered_writes.json` and
`apps/desktop/src-tauri/src/registered_writes.json`. An entry is one file, with what it writes and a kind. A
directory is never an entry, of any kind: a new file under it would be registered unseen. The kinds (`KINDS`), each
saying whose word the writes go by:

| Kind | What it says |
|---|---|
| `primitive` | The helper layer: it writes at the path its caller names. Every function the file declares with any `pub` is a word of the vocabulary, so each caller is registered itself; the entry lists as `readers` the ones that write nothing, and the check fails on a reader that is not a public function of the file or whose body holds a write. A new public function is a word from the moment it is written. |
| `own` | Only inside Octoboard's own directories (`~/.octoboard`, a session's scratch directory, the application's data directory) or its own system resources (the cgroup subtree delegated to the daemon), or a temporary file removed again. |
| `installation` | Octoboard's own installation and presence outside its directories: the server form's executable and what an update puts beside it, the application its updater replaces, the jump list. Removed by an uninstall; not an exception. |
| `exception` | A write into the user's own system that the user approved as an exception to P9. It names its exceptions. |
| `user_path` | At a path the user named for that write, at the user's own action (a save dialog, a project's folder), or an account's own directory, the default account's included, created empty where the agent's client would create it anyway. |

An entry that is not a `primitive` may name `entry_points`: functions of its file that write where their caller
decides (the taking of the Trash for a session's records, the move of a project's folder, the line in a remote
host's authorized keys), which then are words like a primitive's, so that each caller is registered too. An entry
point is also what makes its file count as writing where the vocabulary finds nothing in it, as with a script sent
to a remote host.

An exception is defined once in its crate's registration, with what the user approved, the product doc section that
states it, and either `cleanup`, the item the clean-up before uninstalling clears it through, or `nothing_to_undo`,
why there is none. The check holds both ways: an exception with something to undo whose item is not in the crate's
clean-up list (`EXCEPTIONS` in the daemon's `coordinator/system_changes.rs`, `REGISTERED` in the shell's
`system_changes.rs`) fails, and so does an item of that list that no exception names. The document an exception
names has to be one whose change runs this check: it is listed in `VERIFY_READS_DOCS` of `scope.mjs`, or the check
fails, since a change under `docs/` alone otherwise runs nothing and a renamed section would pass its own push.

A registration that no longer matches the code fails as well: an entry whose file is gone or holds no write any
more, an unknown kind, an exception no entry uses, a section that does not exist, a reader or an entry point the file
does not declare.

**A line that writes no file.** The vocabulary cannot tell a pipe, a terminal or the null device from a file. Such a
line carries `// no-file-write: <why>`, at its end or in the comment lines right above it, and is then no hit; its
file needs no entry for it. One comment acknowledges one line. The check fails on one without a reason and on one
with no hit to acknowledge.

**Adding an entry.** A new write goes behind a file that is already registered, or its file is registered with the
kind that is true of it. A new `exception`, and a new entry of that kind, is a new exception to P9: it needs the
product owner's approval before it is written, and the change check reads a change to either registration that way.

**Tests are outside it** only where `cfg(test)` proves it: an item or an inline module under `#[cfg(test)]` or a
`test` conjunct, and an external module declared under one, with everything below it. No file is left out for its
name, so a test helper compiled into production is checked like any other file, and a bare `#[test]` function outside
a test module counts as production.

**What it cannot see**, which review and the dynamic test below have to:

- Any new write in a file that is registered, of whatever kind: the check holds where a write call may stand, not
  which path it is given. A write to the user's home added to a file registered as `own` passes, and so does one
  added to a file registered for an exception.
- A caller of a registered file's function, unless that function is a word: the public functions of a `primitive`
  file and the `entry_points` are, the other functions of an `own`, `exception` or `user_path` file are not. Such a
  function that takes its target from its caller (a key file written into the directory it is handed, say) can be
  called with any path from an unregistered file.
- A path that is data: the records' paths computed from what an agent wrote, the worktrees `git` lists, a link in
  Octoboard's own directory that leads into the user's.
- What a child process writes: an agent's CLI in a session or in the background, the user's `git`, the login shell
  whose environment is snapshotted, an updater, `systemd-run`. Starting a process is not in the vocabulary.
- What an agent's CLI writes at Octoboard's word (`codex unarchive`, a login, an MCP sign-in): the agent's own, and
  a process like any other to this check.
- What a library writes by itself: the web view, the notification and updater plugins, SQLite's journals, a socket a
  listener binds.
- A write through a name the vocabulary does not have: a function re-exported or imported under another name
  (`use hooks::write_hook_script as write`), an alias of a type or module other than the ones named above, a macro,
  a function passed as a value (the Trash tool is seen where it is taken from something called `…tools` or from
  `get()`, not where it is called, and not when the tools were first put in a variable of another name), an open
  whose writing flag is a variable rather than `true`, a crate added later.
- Code that is not a file of the crate's `src`: what `include!` brings in, `build.rs`, and generated code under
  `OUT_DIR`.
- System state that is not a file: a sleep hold, a transient systemd unit, a privacy grant the system records.
- A write on a remote host other than through the registered entry point: the script it sends is a string.

**The dynamic half** is a daemon test, `coordinator::own_directories_tests` (Unix): it starts the test binary again
with a scratch home holding a shell profile, a git configuration, each agent's directory, a link and a project,
launches a session of the test provider there through the launch a running daemon uses, archives it and waits for
the end to be over, and fails when anything in that home outside `.octoboard` differs afterwards (an entry added or
gone, a file's content, permissions or modification time, a link's target); a second test shows the comparison
catching each of those. It covers the test provider on the host it runs on. The daemon's own start is not part of
it (the helper builds the application state a test runs over), nor are the real providers' adapters or the shell's
system calls.

`registered-writes.test.mjs` covers every word of the vocabulary with an example of each form it takes, lines that
only read, the blanking of comments, strings and test-only items, a planted write in a scratch source tree, a call
of a primitive's public function and of an entry point from an unregistered file, readers that are not, the
acknowledgements, each way a registration can stop matching the code (a directory and a missing registration among
them), the clean-up tie in both directions, the document's consumer and the executable's exit status. Run it with
`node --test scripts/verify/registered-writes.test.mjs`.

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
does not call this workflow or wait for its results, except that its `linux-daemon` job is the reusable workflow
`.github/workflows/linux-daemon.yml`, which release CI also calls for a release with Linux in it and waits for:

| Job | Runner | Command and scope |
|---|---|---|
| `frontend` | `ubuntu-latest` | `pnpm verify --release --release-group frontend --fresh`: identity, provider-neutral core search, registered writes, application lint, production UI type check and application UI tests. |
| `native` | `macos-26` | `pnpm verify --release --release-group native --fresh`: identity, provider-neutral core search, registered writes, desktop script tests, the daemon's node tests, rustfmt, clippy for the workspace and all targets, and both Rust crates' tests. |
| `linux-check` | `ubuntu-latest` | `node scripts/cargo.mjs check --locked`: compile both crates (compilation only) with the Linux prerequisites installed. |
| `linux-daemon` | `ubuntu-24.04`, `ubuntu-24.04-arm` | `.github/workflows/linux-daemon.yml`: clippy on the daemon, its whole test suite on glibc in a privileged container with a writable cgroup v2 hierarchy, and its tests on musl for x86_64 and aarch64, the targets the Linux release links statically. It also gates Linux releases. |
| `windows-check`, `windows-bundle` | `windows-latest` | Native Windows checks and the unsigned installer build (see the [tooling overview](../../docs/structure/tooling.md)); the local release plan's windows group also cross-checks the same crates from the Mac. |

The frontend group does not invoke `rustc`, including when identifying the toolchain for local pass stamps, so it
does not download the pinned Rust toolchain. Desktop script tests stay on macOS because archive tests exercise its
BSD tar; Rust checks stay there for the desktop crate's native frameworks. The separate Linux jobs detect compilation
failures and Linux-only test failures that the local macOS release checks cannot; `linux-check` is not a publication gate,
`linux-daemon` is one for a release with Linux in it. Website and gallery-only checks
remain outside the application release plan.

CI uses `--fresh` to run the complete selected checks each time. Rust dependency caches reduce repeated compilation;
they do not carry successful verification stamps or bypass failing tests. Cache profiles, manual main-branch warming,
retention and timing measurements are documented in the [Rust cache module](../../.github/actions/rust-cache/README.md).

## Changing it

- A new package or a new input: add or extend a rule in `RULES`; a rule's `consumers` decide which steps run. A new
  import of the UI package by the website or the docs needs none: it is derived.
- A product doc a registered exception names (`registered-writes.mjs`): name it in `VERIFY_READS_DOCS` of `scope.mjs`,
  or the check fails; a change to it then selects the verify package, and with it the check.
- A new check: a step in `steps.mjs`; keep `quick.mjs` consistent (its tests fail when a UI test reads files or a crate
  has crate-wide tests that its lists do not name). A new release check belongs in the frontend, native or windows
  group; `steps.test.mjs` checks that their combined commands equal the complete release plan.
- A new test target in a Rust crate: name it in `CRATES` of `quick.mjs`, whose tests fail on a `[[test]]`, `[[bench]]`
  or `[[example]]`, a `tests/`, `src/bin`, `examples` or `benches` directory, a second daemon binary, a daemon library,
  or a doctest in the desktop crate. A doctest cannot be selected next to `--lib` and `--bin`, so it needs a change to
  how that crate is narrowed.
- A desktop test that reads a capability, a permission file or `lib.rs` needs no entry: a change to such a file runs
  the desktop crate in full. One that derives `TS` or reads other modules' sources would be skipped by the module
  filters and goes in `CRATE_WIDE_TESTS.native`; the meta test in `quick.test.mjs` fails when one is not named.
- A new `include_str!` of Markdown is named in `DAEMON_READS_MARKDOWN` (or a rule of its own) and in
  `RUST_MARKDOWN_READERS`; the tests fail with the file to name. A file outside the UI package that a UI test reads runs
  the full UI suite in a quick run until `UI_READERS` names its readers; the tests fail when an entry leaves a reader
  out.
- A script that other packages import or only run, and a test that creates a temporary Git repository: the rules are in
  `docs/conventions/tooling.md`, "Scripts".

## External interfaces

- `pnpm verify` with `--dry-run`, `--base <ref>`, `--head <commit>`, `--full`, `--release`,
  `--release-group frontend|native|windows`, `--quick`, `--fresh`.
- `scope.mjs`: `RULES`, `planScope`, `matchingRules`. `steps.mjs`: `checkSteps`. `quick.mjs`: `quickSteps`.
- `ui-imports.mjs`: `importsFromUi` gives the UI package's files that the sources under a directory import, directly or
  through the package's own relative imports; `scope.mjs` builds the website's and the docs' lists from it. An import
  that lands in the UI package and cannot be resolved to a file there, or an `exports` entry it does not interpret,
  throws, naming the specifier and its importer, rather than leaving the modules behind it out of scope.
- `changes.mjs`: `chooseBase`, `resolveBase`, `resolveLocalTargetBase`, `remoteTargetBase` (the one
  merge-base-with-the-remote-target function, also used by the pre-push hook), `resolveRemoteTargetBase`,
  `changedFiles`, `requirePushedTree`.
- `heavy.mjs`: `node scripts/verify/heavy.mjs <command> [arguments]` runs a command once it holds a slot
  (`OCTOBOARD_HEAVY_SLOT_ROOT` moves the slots, for tests).
- `small-run.mjs`: `smallRun` decides whether a heavy command runs without the slot (`SMALL_RUN_FILES`,
  `SMALL_RUN_WORKERS`).
- `process-group.mjs`: `startOf`; `runInGroup` runs a command in its own group with the stall watch (`stallWatch`, `stallMs`,
  `STALLED_EXIT`); `endLeftovers` ends what is left of a recorded group.
- `lock.mjs`: `acquireSlot` (its release function also has `recordGroup`), `HELD_ENV`; `stamp.mjs`: `readStamp`, `recordPass`; `rerun.mjs`: the failed-file record;
  `tree.mjs`: `workingTree`.
- `environment.mjs`: `withoutRepositoryEnvironment`, `assertRepositoryEnvironmentCleared`.
- `provider-neutral.mjs`: `checkProviderNeutral` searches a repository's production core; `violations` checks one
  source file against the vocabulary and the derived registration exceptions; `providerIds` derives the registered ids
  from the descriptor files; `VOCABULARY` is the per-provider table of spellings. `productionSource` and `testOnly` blank code absent from production.
- `registered-writes.mjs`: `checkRegisteredWrites` checks a repository's crates (`CRATES`) against their
  registrations; `hitsIn` lists one source file's writes by the vocabulary (`VOCABULARY`) and the functions it is
  given, with the acknowledged ones marked; `publicFunctions` and `bodiesOf` read a file's public functions and a
  function's bodies; `productionFiles` lists a crate's production files; `KINDS` names the kinds of entry.
