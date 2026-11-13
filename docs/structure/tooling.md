# Tooling and repository configuration

How the repository is built, checked and released, outside the code of any one end. The product ends are described in
[`daemon.md`](daemon.md), [`ui.md`](ui.md), [`desktop.md`](desktop.md) and [`web.md`](web.md).

## Workspaces and toolchains

- **pnpm workspace** (`package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`): the packages are the `apps/*` and
  `packages/*` directories that have a `package.json` (`apps/daemon/`, `apps/web/`, `apps/desktop/`, `packages/ui/`);
  `apps/daemon/`'s only wraps the cargo build that embeds the UI (see [`daemon.md`](daemon.md)). The root `package.json` holds repository-wide verification, formatting, Rust workspace checks, identity
  maintenance and hook installation, with shared lint dev dependencies. Commands that build, type-check, install or
  release one package live in that package's `package.json`, invoked with `pnpm --filter <package> <command>` from the
  root. `patchedDependencies` in `pnpm-workspace.yaml`
  registers the patches in `patches/` (currently one for jsdom's selector engine, used by the UI's tests).
- **Cargo workspace** (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `rustfmt.toml`): members `apps/daemon` and
  `apps/desktop/src-tauri`, built into one `target/`. The workspace `version` is the application's version and must
  equal `version` in `apps/desktop/package.json`; the desktop crate's `build.rs` fails the build when they differ.
  `rust-toolchain.toml` pins the one toolchain used locally and in CI, and the workspace `rust-version` matches it.
- `eslint.config.mjs` — the repository-wide lint and format rules (JavaScript, TypeScript, TSX, Vue), including the UI's
  import-layering blocks, which live in `packages/ui/eslint.layers.mjs` (see [`ui.md`](ui.md)); `pnpm format` runs it
  with `--fix --fix-type layout` and `cargo fmt`.
- `config/app.json` — the single source for product identity, repository and issue-report destinations, author and
  support contacts, website address, the default server port (`defaultServerPort`), license and copyright year. Read by
  the UI (`packages/ui/src/appConfig.ts`), the website (`apps/web/nuxt.config.ts`, `apps/web/seo/downloads.ts`) and
  both Cargo crates' `build.rs` (the daemon's turns the port into `octoboardd start`'s default); the website's
  `install-script.test.ts` pins the install script's and the example systemd unit's port to it; copies the toolchain requires elsewhere are
  checked against it (the UI's `appConfig.test.ts`, the build scripts). `author.name` is the only editable source of
  the author name. After changing it or `copyrightYear`, run `pnpm identity:sync` to regenerate the tracked `LICENSE`
  copyright line and `Cargo.toml` workspace authors; builds and installation do not rewrite these outputs.
- `.env.local.example`, `.env.secret.example` — the templates of the gitignored machine-local and release-credential
  files. `scripts/lib/env.mjs` is the reader; it reads `.env.secret` and `.env.local`.
- `assets/branding/` — brand source artwork that no app serves. `.claude/` holds the agent skills and subagent
  definitions of the repository's workflow.

## Directory map

| Path | Role |
|---|---|
| `scripts/lib/git.mjs`, `scripts/lib/env.mjs` | The one way repository scripts run `git` (and other commands), and the shared env-file reader |
| `scripts/verify.mjs`, [`scripts/verify/`](../../scripts/verify/README.md) | `pnpm verify`: selects and runs the checks a change calls for |
| `scripts/hooks/` | The staged formatter (`format-staged.mjs`), AI attribution check (`commit-msg.mjs`) and push verifier (`pre-push.mjs`), with their tests beside them (see the hooks' README; the pre-push stage takes its baseline from `remoteTargetBase` in `scripts/verify/changes.mjs`); `scripts/pre-push.mjs` and `scripts/format-staged.mjs` are forwarding stubs for hook copies installed before the move |
| `scripts/cargo.mjs` | `pnpm test:rust`, `pnpm clippy:rust`, the Linux CI `cargo check` and `pnpm check:windows` (a `cargo xwin` cross-check for Windows: both crates gate, all targets, tests included, and once the daemon checks clean its executable is also linked with `cargo xwin build`, which a compile check never reaches; the desktop crate is only checked; `pnpm verify --release` runs it before every release tag; the `windows-check` CI job on a Windows runner runs this script with `-p` to check, lint and test the desktop crate natively, and lints and tests the daemon with `cargo` directly): both crates in one cargo invocation, without needing the daemon sidecar (the one place that empties `bundle.externalBin` and `bundle.resources`) |
| `scripts/lib/windows-toolchain.mjs` | The toolchain `pnpm check:windows` needs, checked before it runs and before `release:version` prepares a version: cargo-xwin, the `x86_64-pc-windows-msvc` target, LLVM's clang first on the PATH (Homebrew's is moved there) and the downloaded MSVC CRT and Windows SDK; it names how to install each missing piece and never accepts the SDK license itself |
| `scripts/fetch-conpty.mjs` | `pnpm fetch:conpty`: downloads the pinned NuGet `Microsoft.Windows.Console.ConPTY` package, checks its SHA-256, caches it under `target/conpty/` and takes `runtimes/win-<arch>/native/conpty.dll` and `build/native/runtimes/<arch>/OpenConsole.exe` out of it (the package carries no licence file; the Windows Terminal licence texts the installer ships come from [`apps/desktop/src-tauri/licenses/windows-terminal/`](../../apps/desktop/src-tauri/licenses/windows-terminal/README.md)) and places `conpty.dll` and `OpenConsole.exe` beside the daemon's test binaries and dev build (`--arch`, `--into`), so the Windows daemon tests run on the bundled console host, or with `--installer` lays out what the Windows installer ships in `target/conpty-bundle/` (x64 `conpty.dll` and an `OpenConsole.exe` per architecture); the `windows-check` CI job runs it before the daemon's tests, and `windows-signpath-finish` of `release.yml` runs it with `--installer` |
| `scripts/sync-identity.mjs` | Generates the required literal author fields in `LICENSE` and `Cargo.toml` from `config/app.json`, or checks them without writing |
| [`.githooks/`](../../.githooks/README.md) | The hook entry point and its installers for the commit and push hooks |
| `.github/workflows/` | `quality.yml` (manual hosted checks: `frontend`, macOS `native`, Linux `linux-check` (compiles both crates and runs the daemon's tests), Linux `linux-musl` (the daemon's tests on `x86_64-unknown-linux-musl`, the target of the Linux release), and on `windows-latest` `windows-check` (the desktop crate checked with all targets, linted with `clippy -D warnings` and tested, then the daemon linted and tested on the ConPTY files `fetch-conpty.mjs` places) and `windows-bundle` (builds the unsigned Windows installer)), `release.yml` (next row), `warm-rust-cache.yml` (manual dependency-cache warming on main: the `checks` and `release` profiles on macOS, `linux-check` on Linux, `linux-release` on both Linux architectures (the unsigned Linux archive build) and `windows-release` on Windows, by building the unsigned installer, the profile `release.yml`'s Windows jobs and `windows-bundle` restore; `windows-check` is not warmed), `website.yml` (generate the website, run its tests and lint, deploy it to GitHub Pages) |
| `.github/workflows/release.yml` | On a version tag, or by hand for a tag and a platform selection: `validate` (platforms, `WINDOWS_SIGNING` mode, needed secrets, tag on main, `Cargo.lock`), then in parallel the selected platforms' builds, then `publish`, which waits for every selected platform and only those: a release of some platforms (the tag message's `Platforms:` line, or the `platforms` input of a manual run, which wins; `all` or no selection is macOS, Windows and Linux) builds and publishes just those. `macos` builds, signs and notarizes on macOS. `linux` is a two-runner matrix (x86_64 and aarch64, each on a runner of its own architecture) that builds the daemon's static musl archive with the UI inside it, signs it in a step of its own and uploads `linux-<arch>`. Windows builds by `WINDOWS_SIGNING`: `windows` (one Windows-runner job) for `none` and `command`; for `signpath`, five jobs in a chain, because each SignPath request waits for a manual approval that can outlast one hosted job: `windows-signpath-build` (builds the executables, uploads `unsigned-binaries`), `windows-signpath-sign-binaries` (Linux; submits them to SignPath, waits, uploads `signed-binaries`), `windows-signpath-bundle` (builds the tree again, bundles the installer from the signed executables, uploads `unsigned-installer` for SignPath and `unsigned-nsis` under the bundler's name), `windows-signpath-sign-installer` (Linux; submits and waits, uploads `signed-installer`) and `windows-signpath-finish` (puts the signed installer back under the bundler's name, checks it against freshly fetched ConPTY files, signs the update, uploads `windows-installer`). Each build job signs its update in a step of its own that alone holds the update key. `publish`, holding only the workflow's token and running on Linux when macOS is not selected, runs once every selected platform's last job succeeded (the other Windows route's jobs are skipped) and publishes from the `macos-bundle`, `windows-installer` and `linux-*` artifacts; after it, the `website` job starts `website.yml` so the website's download links read the new manifests |
| [`.github/actions/rust-cache/`](../../.github/actions/rust-cache/README.md) | Shared Rust dependency-cache profiles for checks, universal release builds, Linux compilation, the Linux musl builds and the Windows check; its README holds the save/restore policy, warming procedure, retention and measurement guidance |
| `.github/release-notes.sh`, `.github/dependabot.yml` | Validates a release tag and prints its notes, or the platforms it selects, for `release.yml`; Dependabot's updates of the actions the workflows use |
| `patches/` | The pnpm dependency patches |

## Hosted Windows checks

`quality.yml`'s `windows-check` runs on `windows-latest` (x64, the MSVC host target), whose `TEMP` is an 8.3 short path:
`scripts/cargo.mjs check`, `clippy` and `test` with `-p octoboard`, then clippy and `cargo test -p octoboardd` on the
ConPTY files `fetch-conpty.mjs --arch x64` places. All of it is expected to pass, the daemon's tests included, on the
bundled console host. Some daemon tests are sensitive to load: Grok's three interrupt tests and
`processes::subprocess::tests::cancelling_during_the_grace_kills_the_program_and_keeps_the_reply` once failed under the
full suite on a loaded ARM64 virtual machine and passed run alone; they are expected to pass on the runner.
`windows-bundle` builds the unsigned installer exactly as the release does (`WINDOWS_SIGNING=none`, no update key), so a
green run says `tauri build` passes on Windows. The Windows code-signing modes and the `release.yml` route are described
in the [desktop scripts README](../../apps/desktop/scripts/README.md).

## Dependency direction

- Repository scripts depend on nothing in the product code. The desktop scripts (`apps/desktop/scripts/`) import
  `scripts/lib/env.mjs` and `scripts/lib/git.mjs`, and the UI's gallery sweep imports `scripts/verify/lock.mjs`.
- Reads across package boundaries are declared in `scripts/verify/scope.mjs`, which the verification README explains.

## Where new code goes

- A new repository-level command is a script in `scripts/` run through a root `package.json` script; code shared by
  scripts goes in `scripts/lib/`, and scripts that run `git` use `scripts/lib/git.mjs`, and a script that another package imports is named in the matching `scope.mjs` rule.
- A new check or workspace package follows the verification README's "Changing it".
- A new fact about product identity goes in `config/app.json`, with a check on any copy the toolchain forces elsewhere.
- A dependency patch goes in `patches/` and `patchedDependencies`.

## Shared entry points

| Need | Entry point |
|---|---|
| Run `git` or another command from a script | `scripts/lib/git.mjs` (`git` probes and answers null; `gitChecked` and `gitCheckedWith` require an answer; `gitResult` returns the exit status) |
| Read env files with environment precedence | `scripts/lib/env.mjs` |
| Update or check generated author fields | `pnpm identity:sync` writes the tracked outputs; `pnpm identity:check` checks for drift without writing |
| Run the checks a change calls for | `pnpm verify` ([`scripts/verify/`](../../scripts/verify/README.md)) |
| Format staged files | `pnpm format:staged` (the pre-commit hook), never a whole-repository formatter as a delivery step |

Checks that hold this area: `pnpm identity:check`, the `scripts/**/*.test.mjs` tests, the UI's `appConfig.test.ts`, and
the release workflow's validation of the tag, the secrets and `Cargo.lock`.
