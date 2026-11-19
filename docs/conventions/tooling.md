# Tooling conventions

Rules for the repository's scripts, the checks (`pnpm verify`, hooks, CI), dependencies, single sources of
configuration, and formatting. Where files and workflows are described is
[`../structure/tooling.md`](../structure/tooling.md); how tests are written is [`tests.md`](tests.md); how and when
checks are run in a piece of work, and that nothing bypasses them, is in `CLAUDE.md`.

## Running tests, lint and checks

- **Tests, clippy and the Windows cross-check run through the package scripts, not `cargo` or `vitest` directly**
  (`CLAUDE.md`, "Repository checks"). The scripts wait for the machine-wide heavy-step slot (`scripts/verify/heavy.mjs`)
  so sessions testing at once queue; there are three exceptions, where cargo runs without that slot:
  - a CI runner, which has Node already, runs `node scripts/cargo.mjs <test|clippy|check> [cargo arguments]`, so the
    flags (`--workspace --all-targets -D warnings`, the desktop crate's `TAURI_CONFIG`) come from one place;
  - a container without Node runs plain `cargo` with the same flags `scripts/cargo.mjs` passes; do not install Node only
    to run the wrapper;
  - measuring build or test speed (`cargo test -p octoboardd --no-run`) runs cargo directly, because the slot's wait
    would distort the timing.
- **Lint both Rust crates and their tests with warnings denied through `pnpm clippy:rust`**
  (`--workspace --all-targets -D warnings`): a clean plain `cargo clippy` says nothing about test code. `pnpm verify`
  and release verification include it.
- **Run the checks a change calls for with `pnpm verify`** (`--dry-run` prints the plan). `scripts/verify/` owns which
  checks a change selects and their commands; the pre-push hook runs it.
- **The Windows gate is `pnpm check:windows`** (both crates cross-checked with `cargo xwin`, all targets, tests
  included; the daemon's executable is also linked). `scripts/cargo.mjs` is the one place that empties
  `bundle.externalBin` and `bundle.resources` for checks, so no other script does. The hosted `windows-check` job in
  `quality.yml` is the same code compiled natively on a Windows runner; it runs only when started by hand and does not
  gate a release. The daemon tests there run on the ConPTY files `scripts/fetch-conpty.mjs` places beside the test
  binaries; tests sensitive to load are expected to pass on the runner.
- **A script that extracts an archive on a GitHub Actions Windows job calls System32's `tar.exe` by its full path**
  (`%SystemRoot%\System32\tar.exe`), never a bare `tar`: those jobs run in Git Bash, which puts its own GNU tar first on
  the `PATH` at every start (it reads no zip and takes the `D:` of an absolute path for a remote host), and adding a
  directory with `$GITHUB_PATH` does not get ahead of it. `scripts/fetch-conpty.mjs` does this;
  `apps/desktop/scripts/update-archive.mjs` is its macOS counterpart and names `/usr/bin/tar`.
- **A build that embeds the UI (`embedded-ui`) must not have `VITE_DAEMON_KEY` or `VITE_DAEMON_PORT` set;** the daemon's
  `build.rs` refuses it, because those values would put the key in a file the daemon serves.
- UI-only commands (`pnpm compiler:bailouts`, `pnpm bench:wkwebview`) and the lint rules the React Compiler adds are
  stated in [`ui-code.md`](ui-code.md).
- Checks are never bypassed (`--no-verify`, skip variables, hook configuration): `CLAUDE.md`, "Repository checks".

## Scripts

- **A new repository-level command is a script in `scripts/` run through a root `package.json` script;** code shared by
  scripts goes in `scripts/lib/`. A command for one package lives in that package's `package.json`, invoked with
  `pnpm --filter`.
- **Scripts run `git` only through `scripts/lib/git.mjs`** (`git` probes and answers null, `gitChecked` and
  `gitCheckedWith` require an answer, `gitResult` returns the exit status) **and read env files through
  `scripts/lib/env.mjs`:** one implementation of environment isolation and precedence.
- **Repository scripts depend on nothing in the product code.** Reads across package boundaries are declared in
  `scripts/verify/scope.mjs`, and a script another package imports is named in the matching rule there; a script others
  only run (`heavy.mjs`) selects the verify package alone.
- **A new check or workspace package follows `scripts/verify/README.md`, "Changing it":** a rule in `RULES`, a step in
  `steps.mjs` with `quick.mjs` kept consistent (its tests fail otherwise), a Rust test target named in `CRATES`. A test
  that creates a temporary Git repository calls `assertRepositoryEnvironmentCleared` at module startup, and a hook
  launcher uses `withoutRepositoryEnvironment` for its verification child.
- **Hooks are installed by `.githooks/`** (`pre-commit` formats staged files, `commit-msg` rejects AI attribution,
  `pre-push` verifies); when to reinstall them after a hook change is in `CLAUDE.md`, "Repository checks".

## Dependencies and single sources of configuration

- **An npm package whose output ends up in the built UI goes in `dependencies` of `packages/ui`, never
  `devDependencies`,** whether the source imports it, a stylesheet `@import`s it, or a plugin module imports it. Vite
  bundles a package either way, but the third-party licenses list is generated from the production closure of
  `packages/ui` only, so a dev dependency ships unlisted. `cssDependencies.test.ts` catches a stylesheet's package;
  nothing catches a TypeScript import. Build-only tooling (Vite and its plugins, compilers, test tools, `@types/*`)
  stays in `devDependencies`.
- **The root `package.json` holds repository-wide commands and the shared lint dev dependencies;** everything else
  belongs to the package that uses it.
- **A dependency patch goes in `patches/` and is registered in `patchedDependencies` of `pnpm-workspace.yaml`.**
- **The Cargo workspace `version` equals `apps/desktop/package.json` `version`** (the desktop `build.rs` fails
  otherwise). `rust-toolchain.toml` pins the one toolchain used locally and in CI, and the workspace `rust-version`
  matches it. The Tauri CLI version in `.github/signer/` is bumped together with the workspace's `@tauri-apps/cli`.
- **A fact about product identity lives in `config/app.json`** (name, repository, contacts, website,
  `defaultServerPort`, license, copyright year), with a check on any copy the toolchain forces elsewhere. `author.name`
  is the only editable author source; after changing it or `copyrightYear`, run `pnpm identity:sync` to regenerate the
  `LICENSE` and `Cargo.toml` author lines (`pnpm identity:check` verifies).
- **`config/launchable-extensions.json` is the one source of which file extensions the system launches rather than
  shows,** read by the daemon, the desktop shell and the UI; none keeps a list of its own.

## Formatting and lint

- **Formatting runs on staged files in the pre-commit hook (`pnpm format:staged`)** (`CLAUDE.md`, "Repository checks").
  `pnpm format` (`eslint --fix --fix-type layout` and `cargo fmt --all`) is the whole-tree formatter.
- **JavaScript, TypeScript, TSX and Vue layout is whatever `eslint.config.mjs` enforces** (`@stylistic` rules: two-space
  indent, double quotes, semicolons, trailing commas in multi-line literals, `1tbs` braces); `pnpm format` fixes it.
  Unused variables, arguments and caught errors are errors unless prefixed `_`.
- **Rust layout is whatever `cargo fmt` produces under `rustfmt.toml`** (`max_width = 100`); the format check in
  `pnpm verify` holds it. A line rustfmt leaves past 100 because it cannot break it is not hand-wrapped; shorten it by
  changing the code, or leave it.
- **Line width is the formatters' alone.** Where no formatter sets one (comments in any language, Markdown prose), none
  applies: a line is never wrapped or re-wrapped to fit a width, nor reported for its length, and a review does not ask
  for it.
- **Generated and agent-owned paths are outside lint:** `protocol.generated.ts` (written by a daemon test), `.claude/`,
  `.agents/`, `.codex/`, build output and `.worktrees/`. Do not hand-edit the generated file.
