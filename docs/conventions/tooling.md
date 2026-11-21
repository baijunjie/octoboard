# Tooling conventions

Rules for the repository's scripts, CI workflows, dependencies, single sources of configuration, and formatting. Where
files and workflows are described is [`../structure/tooling.md`](../structure/tooling.md); how and when checks are run,
and that nothing bypasses them, is in `CLAUDE.md`, "Repository checks".

## General principles

- **One implementation per concern, in `scripts/lib/`:** scripts run `git` only through `scripts/lib/git.mjs` (`git`
  probes and answers null, `gitChecked` and `gitCheckedWith` require an answer, `gitResult` returns the exit status) and
  read env files only through `scripts/lib/env.mjs`, so environment isolation and precedence are implemented once.

## Running tests, lint and checks

- **Cargo runs without the heavy-step slot in three cases only:**
  - a CI runner, which has Node already, runs `node scripts/cargo.mjs <test|clippy|check> [cargo arguments]`, so the
    flags (`--workspace --all-targets -D warnings`, the desktop crate's `TAURI_CONFIG`) come from one place;
  - a container without Node runs plain `cargo` with the same flags `scripts/cargo.mjs` passes; do not install Node only
    to run the wrapper;
  - measuring build or test speed (`cargo test -p octoboardd --no-run`) runs cargo directly, because the slot's wait
    would distort the timing.
- **`scripts/cargo.mjs` is the one place that empties `bundle.externalBin` and `bundle.resources` for checks**, so no
  other script does.
- **A script that extracts an archive on a GitHub Actions Windows job calls System32's `tar.exe` by its full path**
  (`%SystemRoot%\System32\tar.exe`), never a bare `tar`: those jobs run in Git Bash, which puts its own GNU tar first on
  the `PATH` at every start (it reads no zip and takes the `D:` of an absolute path for a remote host), and adding a
  directory with `$GITHUB_PATH` does not get ahead of it. `scripts/fetch-conpty.mjs` does this;
  `apps/desktop/scripts/update-archive.mjs` is its macOS counterpart and names `/usr/bin/tar`.
- **A workflow job that must still run when a job it depends on, directly or through other jobs, was skipped** (an
  unselected platform's, the Windows signing route not taken) **states its own `if`, starting with `!cancelled()` and
  checking the `needs.<job>.result` of each job it relies on**: without a status function in its `if`, GitHub adds
  `success()`, which is false once any job it depends on, however indirectly, was skipped. `release.yml`'s `sign`,
  `publish` and `website` follow it; a job meant to be skipped along with its dependency, such as the later jobs of the
  SignPath chain, needs no `if`.

## Scripts

- **A new repository-level command is a script in `scripts/` run through a root `package.json` script;** code shared by
  scripts goes in `scripts/lib/`. A command for one package lives in that package's `package.json`, invoked with
  `pnpm --filter`.
- **Repository scripts depend on nothing in the product code.** Reads across package boundaries are declared in
  `scripts/verify/scope.mjs`, and a script another package imports is named in the matching rule there; a script others
  only run (`heavy.mjs`) selects the verify package alone.
- **A package that keeps its own copy of another package's logic adds a parity test** that imports or reads the source
  and compares, and lists every module that test reads, transitive imports included, in a relation list of
  `scripts/verify/scope.mjs` (`DOCS_READS_WEB` for `apps/docs/webParity.test.ts`). The cross-package scan in
  `scope.test.mjs` finds only direct reads, from `.rs`, `.ts`, `.tsx`, `.mjs`, `.astro` and `.mdx` files, so a module
  the test reaches through an import is named by hand.
- **A new check or workspace package follows `scripts/verify/README.md`, "Changing it":** a rule in `RULES`, a step in
  `steps.mjs` with `quick.mjs` kept consistent (its tests fail otherwise), a Rust test target named in `CRATES`. A test
  that creates a temporary Git repository calls `assertRepositoryEnvironmentCleared` at module startup, and a hook
  launcher uses `withoutRepositoryEnvironment` for its verification child.
- **A script that drives the UI state gallery in a browser** (the sweep, the comparison, the docs' screenshots) **takes
  the browser, the dev server, frame loading and the run lifecycle from `packages/ui/scripts/gallery-browser.mjs` and
  the heavy-step slot from `scripts/verify/lock.mjs`**, instead of keeping its own copy: a copy drifts and loses the
  handling of interrupts and failed closes.

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
- **`rust-toolchain.toml` pins the one toolchain used locally and in CI, and the workspace `rust-version` matches it.**
  The Tauri CLI version in `.github/signer/` is bumped together with the workspace's `@tauri-apps/cli`.
- **A fact about product identity lives in `config/app.json`** (name, repository, contacts, website,
  `defaultBrowserPort`, license, copyright year), with a check on any copy the toolchain forces elsewhere. `author.name`
  is the only editable author source; after changing it or `copyrightYear`, run `pnpm identity:sync` to regenerate the
  `LICENSE` and `Cargo.toml` author lines (`pnpm identity:check` verifies).
- **`config/launchable-extensions.json` is the one source of which file extensions the system launches rather than
  shows,** read by the daemon, the desktop shell and the UI; none keeps a list of its own.

## Formatting and lint

- **Line width is the formatters' alone.** Where no formatter sets one (comments in any language, Markdown prose), none
  applies: a line is never wrapped or re-wrapped to fit a width, nor reported for its length, and a review does not ask
  for it. A line `cargo fmt` leaves past `max_width` because it cannot break it is not hand-wrapped either; shorten it by
  changing the code, or leave it.
- **A local ESLint plugin lives in `scripts/eslint/<plugin>.mjs` (one rule, or several on one concern), with its
  `node:test` file beside it** (ESLint's
  `RuleTester` bridged to `node:test`), and is loaded as a plugin by `eslint.config.mjs`; `scripts/eslint/` is matched by
  the lint-configuration rule in `scripts/verify/scope.mjs`. Its plugin declares `meta.name` and a `meta.version` taken
  from the file's own content hash: ESLint's cache tells plugins apart only by those, so a rule edit would otherwise
  leave cached lint results standing.
- **A local rule's autofix only removes or rewrites what it has proved changes no behavior.** Where that cannot be
  proved from the syntax, the rule reports without a fix (or offers a suggestion, `hasSuggestions`), or stays silent.
