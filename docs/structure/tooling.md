# Tooling and repository configuration

How the repository is built, checked and released, outside the code of any one end. The product ends are described in
[`daemon.md`](daemon.md), [`ui.md`](ui.md), [`desktop.md`](desktop.md), [`web.md`](web.md) and [`docs.md`](docs.md). The rules for the
scripts, the checks, the dependencies and the single sources of configuration are in `docs/conventions/tooling.md`.

## Workspaces and toolchains

- **pnpm workspace** (`package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`): the packages are the `apps/*` and
  `packages/*` directories that have a `package.json` (`apps/daemon/`, `apps/web/`, `apps/docs/`, `apps/desktop/`, `packages/ui/`);
  `apps/daemon/`'s only wraps the cargo build that embeds the UI (see [`daemon.md`](daemon.md)). The root `package.json` holds repository-wide verification, formatting, Rust workspace checks, identity
  maintenance and hook installation, with shared lint dev dependencies. Commands that build, type-check, install or
  release one package live in that package's `package.json`, invoked with `pnpm --filter <package> <command>` from the
  root. `patchedDependencies` in `pnpm-workspace.yaml` registers the patches in `patches/` (currently one for jsdom's
  selector engine, used by the UI's tests, and one for the file viewer's code renderer, `@pierre/diffs`, which ships in
  the UI).
- **Cargo workspace** (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, whose `disallowed-methods` bar `block_in_place` and `std::fs::canonicalize`): members `apps/daemon` and
  `apps/desktop/src-tauri`, built into one `target/`. The workspace `version` is the application's version, which
  `apps/desktop/package.json` carries too; the desktop crate's `build.rs` fails the build when they differ, and the UI's
  `vite.config.ts` builds the same version in, so `scripts/verify/scope.mjs` also routes `apps/desktop/package.json` to
  the UI. `rust-toolchain.toml` pins the toolchain used locally and in CI.
- `eslint.config.mjs` — the repository-wide lint and format rules (JavaScript, TypeScript, TSX, Vue), including the UI's
  import-layering blocks, which live in `packages/ui/eslint.layers.mjs` (see [`ui.md`](ui.md)). The local plugins from
  `scripts/eslint/` apply as follows: `lucide/lucide-aria-hidden` to `packages/ui/**/*.{ts,tsx}`; the class-name,
  `title` and modifier-key rules to `packages/ui/src/**/*.{ts,tsx}` minus tests, `test/`, `gallery/`, `fixtures/` and
  `website-preview/`. The same block bans `navigator.userAgent` / `platform` / `userAgentData` (also through `window`,
  `globalThis` and `self`), `React.lazy`, and the import of a `Loader*` glyph from `lucide-react` (a wait is the layer's `Spinner`) with core `no-restricted-syntax`; the `@tauri-apps/*` and `lazy` import bans
  are `@typescript-eslint/no-restricted-imports` blocks in `packages/ui/eslint.layers.mjs` over `packages/ui/src` minus
  tests and `test/` (`@tauri-apps/*` is allowed in `platform/`, and `gallery/`, `fixtures/` and `website-preview/` are
  outside the application-source blocks). `pnpm format` runs it with `--fix --fix-type layout` and `cargo fmt`.
- `config/app.json` — the single source for product identity, repository and issue-report destinations, author and
  support contacts, website address, the default browser port (`defaultBrowserPort`), license and copyright year. Read by
  the UI (`packages/ui/src/appConfig.ts`), the website (`apps/web/nuxt.config.ts`, `apps/web/seo/downloads.ts`), the documentation site (`apps/docs/site.config.ts`, `apps/docs/src/commands.ts`) and both
  Cargo crates' `build.rs` (the daemon's turns the port into the browser port's default); the website's
  `install-script.test.ts` holds the install script and the example systemd unit to naming no port of their own; copies the toolchain
  requires elsewhere are checked against it (the UI's `appConfig.test.ts`, the build scripts). `pnpm identity:sync`
  regenerates the tracked `LICENSE` copyright line and `Cargo.toml` workspace authors from it (when to run it:
  `docs/conventions/tooling.md`, "Dependencies and single sources of configuration"); builds and installation do not
  rewrite these outputs.
- `config/launchable-extensions.json` — the lists of file extensions the system launches or acts on rather than shows, the one
  source of that security judgement: read by the daemon (`apps/daemon/src/platform/open/mod.rs`), the desktop shell
  (`apps/desktop/src-tauri/src/remote/files/mod.rs`) and the UI (`packages/ui/src/launchable.ts`).
- `.env.local.example`, `.env.secret.example` — the templates of the gitignored machine-local and release-credential
  files. `scripts/lib/env.mjs` is the reader; it reads `.env.secret` and `.env.local`.
- `assets/branding/` — brand source artwork that no app serves. `.claude/` holds the agent skills and subagent
  definitions of the repository's workflow.

## Directory map

| Path | Role |
|---|---|
| `scripts/lib/git.mjs`, `scripts/lib/env.mjs` | The one way repository scripts run `git` (and other commands), and the shared env-file reader |
| `scripts/verify.mjs`, [`scripts/verify/`](../../scripts/verify/README.md) | `pnpm verify`: selects and runs the checks a change calls for, including the provider-neutral core search (`provider-neutral.mjs`) and the registered writes check (`registered-writes.mjs`: every write of the application's own Rust code stands in a file that the crate's `registered_writes.json` names, `apps/daemon/src/registered_writes.json` and `apps/desktop/src-tauri/src/registered_writes.json`) |
| `scripts/hooks/` | The staged formatter (`format-staged.mjs`), AI attribution check (`commit-msg.mjs`) and push verifier (`pre-push.mjs`), with their tests beside them (see the hooks' README; the pre-push stage takes its baseline from `remoteTargetBase` in `scripts/verify/changes.mjs`); `scripts/pre-push.mjs` and `scripts/format-staged.mjs` are forwarding stubs for hook copies installed before the move |
| `scripts/eslint/` | Local ESLint rules the root `eslint.config.mjs` loads as plugins, each with its `node:test` file beside it: `lucide-aria-hidden.mjs` (plugin `lucide`, rule `lucide/lucide-aria-hidden`) flags, and autofixes away, a redundant `aria-hidden` on an icon imported from `lucide-react` (one lucide adds itself); `ui-class-names.mjs` (plugin `ui-class-names`: no `focus-visible`, ellipsis or line-clamp, physical-direction or `border-separator` / `divide-*` divider class names), `native-title.mjs` and `primary-modifier.mjs` (a `title` attribute on a DOM element or a primitive; a test accepting both `metaKey` and `ctrlKey`), all report only |
| `scripts/cargo.mjs` | `pnpm test:rust`, `pnpm clippy:rust` (both reached through `scripts/verify/heavy.mjs`, which waits for the heavy-step slot first), the Linux CI `cargo check` and `pnpm check:windows` (a `cargo xwin` cross-check for Windows, for `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`, `pnpm check:windows [desktop|daemon] [x64|arm64]` narrowing it: both crates gate, all targets, tests included, and once the daemon checks clean its executable is also linked with `cargo xwin build`, which a compile check never reaches; the desktop crate is only checked; `pnpm verify --release` runs it before every release tag; the `windows-check` CI job on a Windows runner runs this script with `-p` to check, lint and test the desktop crate natively, and lints and tests the daemon with `cargo` directly): both crates in one cargo invocation, without needing the daemon sidecar (the one place that empties `bundle.externalBin` and `bundle.resources`) |
| `scripts/lib/windows-toolchain.mjs` | The toolchain `pnpm check:windows` needs, checked before it runs and before `release:version` prepares a version: cargo-xwin, the `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc` targets (each check narrowed to one asks only for its own), LLVM's clang first on the PATH (Homebrew's is moved there) and the downloaded MSVC CRT and Windows SDK; it names how to install each missing piece and never accepts the SDK license itself. It holds the one table of Windows architectures and their Rust targets (`WINDOWS_TARGETS`, which the desktop release scripts import), and writes the `clang` shim the arm64 check puts first on the PATH (`arm64-clang-shim/` in cargo-xwin's cache, so the machine-wide `clang-cl` link cargo-xwin points at it never dangles), without which `ring` cannot build for `aarch64-pc-windows-msvc` under cargo-xwin |
| `scripts/fetch-conpty.mjs` | `pnpm fetch:conpty`: downloads the pinned NuGet `Microsoft.Windows.Console.ConPTY` package, checks its SHA-256, caches it under `target/conpty/` and takes `runtimes/win-<arch>/native/conpty.dll` and `build/native/runtimes/<arch>/OpenConsole.exe` out of it (the package carries no licence file; the Windows Terminal licence texts the installer ships come from [`apps/desktop/src-tauri/licenses/windows-terminal/`](../../apps/desktop/src-tauri/licenses/windows-terminal/README.md)) and places `conpty.dll` and `OpenConsole.exe` beside the daemon's test binaries and dev build (`--arch`, `--into`), so the Windows daemon tests run on the bundled console host, or with `--installer [--arch x64|arm64]` lays out what that architecture's Windows installer ships in `target/conpty-bundle/` (that architecture's `conpty.dll` and an `OpenConsole.exe` per architecture; the target `tauri build` announces, else x64); the `windows-check` CI job runs it before the daemon's tests, and `windows-signpath-finish` of `windows-signpath.yml` runs it with `--installer --arch <arch>` |
| `scripts/sync-identity.mjs` | Generates the required literal author fields in `LICENSE` and `Cargo.toml` from `config/app.json`, or checks them without writing |
| [`.githooks/`](../../.githooks/README.md) | The hook entry point and its installers for the commit and push hooks |
| `.github/workflows/` | `quality.yml` (manual hosted checks: `frontend`, macOS `native`, Linux `linux-check` (compiles both crates) and `linux-daemon` (the reusable `linux-daemon.yml`, below), and on `windows-latest` `windows-check` (the desktop crate checked with all targets, linted with `clippy -D warnings` and tested, then the daemon linted and tested on the ConPTY files `fetch-conpty.mjs` places) and `windows-bundle` (builds the unsigned Windows installer of each architecture, a matrix that runs both legs to the end)), `linux-daemon.yml` (next rows), `windows-signpath.yml` (the `signpath` route of the release, below), `release.yml` (the row after), `warm-rust-cache.yml` (manual dependency-cache warming on main: the `checks` and `release` profiles on macOS, `linux-check` on Linux, `linux-release` on both Linux architectures (the unsigned Linux archive build), `linux-musl` on both Linux architectures (compiling the daemon's musl tests) and `windows-release-x64` and `windows-release-arm64` on Windows, by building each architecture's unsigned installer, the profiles `release.yml`'s Windows jobs, `windows-signpath.yml` and `windows-bundle` restore; `windows-check` is not warmed), `website.yml` (generate the website, build the documentation site (`apps/docs/`) with the same origin and base path and copy its `dist` into `docs/` of the website's output (failing if the output already has that path), run the website's and the docs' tests and lint both, and deploy the one output to GitHub Pages; a change under `apps/docs/` triggers it, Markdown included, where Markdown elsewhere does not)) |
| `.github/workflows/linux-daemon.yml`, `.github/daemon-cgroup-tests.sh` | The daemon's Linux checks, a reusable workflow (`workflow_call`, input `ref`) that `quality.yml` calls on request and `release.yml` calls for a release with Linux in it, whose gate they are: `clippy` (`-D warnings`, profile `linux-check`), `cgroup-tests` (the whole daemon suite on glibc in a privileged container with a cgroup namespace of its own, where the script, run from the workflow's own commit, hands a writable cgroup v2 hierarchy to a test user and fails if a test leaves a cgroup behind; `OCTOBOARD_REQUIRE_CGROUP=1` makes a test that would skip for want of a cgroup fail instead) and `musl` (the daemon's tests on `x86_64-unknown-linux-musl` and, on an arm runner, `aarch64-unknown-linux-musl`, the targets of the Linux release; profile `linux-musl`; `OCTOBOARD_TEST_REQUIRE_REFTABLE=1` makes a reftable test that would skip for want of a new enough `git` fail instead, which the cgroup job's image, with Git 2.47, cannot take) |
| `.github/signer/` | `package.json` and `package-lock.json` of the Tauri CLI that `release.yml`'s `sign` job installs, pinned with integrity hashes; its version is bumped with the workspace's `@tauri-apps/cli` in `apps/desktop/package.json` |
| `.github/workflows/release.yml` | Application checks run locally before the tag (`pnpm verify --release`); the workflow does not call `quality.yml`, and the one hosted check that gates publication is `linux-daemon`, for a release with Linux in it. On a version tag, or by hand for a tag and a platform selection: `validate` (platforms, `WINDOWS_SIGNING` mode, the Apple secrets, tag on main, `Cargo.lock`), then in parallel the selected platforms' builds (and, with Linux selected, `linux-daemon`), then `sign`, then `publish`; `sign` waits for every selected platform and only those: a release of some platforms (the tag message's `Platforms:` line, or the `platforms` input of a manual run, which wins; `all` or no selection is macOS, Windows and Linux) builds and publishes just those. `macos` builds, signs and notarizes on macOS and uploads the bundle and, apart, the unsigned update archive. `linux` is a two-runner matrix (x86_64 and aarch64, each on a runner of its own architecture) that builds the daemon's static musl archive with the UI inside it and uploads `linux-<arch>`. `linux-daemon` calls `linux-daemon.yml` on the tag. Windows builds both architectures, x64 and arm64 (cross-compiled on the x64 runner), by `WINDOWS_SIGNING`: `windows` (a Windows-runner matrix job, one leg per architecture) for `none` and `command`; for `signpath`, the job `windows-signpath` calls `windows-signpath.yml` once per architecture (a called workflow rather than a matrix, since its jobs hand artifact ids on through job outputs, which matrix legs would overwrite), five jobs in a chain, because each SignPath request waits for a manual approval that can outlast one hosted job: `windows-signpath-build` (builds the executables, uploads `unsigned-binaries-<arch>`), `windows-signpath-sign-binaries` (Linux; submits them to SignPath, waits, uploads `signed-binaries-<arch>`), `windows-signpath-bundle` (builds the tree again, bundles the installer from the signed executables, uploads `unsigned-installer-<arch>` for SignPath and `unsigned-nsis-<arch>` under the bundler's name), `windows-signpath-sign-installer` (Linux; submits and waits, uploads `signed-installer-<arch>`) and `windows-signpath-finish` (puts the signed installer back under the bundler's name, checks it against freshly fetched ConPTY files, uploads `windows-installer-<arch>`). Either route uploads `windows-installer-x64` and `windows-installer-arm64`. No build job holds the update key. `sign` (on Linux, in the `release-signing` environment, whose secrets hold the update key and whose rules gate the run) runs once every selected platform's last job and `linux-daemon` succeeded (the other Windows route's jobs are skipped), installs nothing but the Tauri CLI of `.github/signer/` and signs the update files the builds uploaded, uploading `update-signatures`. `publish`, holding the workflow's token (`contents: write`, plus `id-token: write` and `attestations: write` for the attestations alone) and running on Linux when macOS is not selected, needs `validate` and `sign`, checks every signature against the key's public half (`publish-release.mjs --dry-run` first, so that only files it accepts are attested), attests the build provenance of the macOS disk image and update archive, both Windows installers and the Linux archives with `actions/attest-build-provenance` (in addition to the minisign signatures, which the installer and updater check; the attestations are verified with `gh attestation verify`, and the job holding the update key gains no right for it) and publishes from the `macos-bundle`, `windows-installer-*`, `linux-*` and `update-signatures` artifacts; after it, the `website` job starts `website.yml` so the website's download links read the new manifests |
| [`.github/actions/rust-cache/`](../../.github/actions/rust-cache/README.md) | Shared Rust dependency-cache profiles for checks, universal release builds, Linux compilation, the Linux musl builds, the Windows release builds of each architecture and the Windows check; its README holds the save/restore policy, warming procedure, retention and measurement guidance |
| `.github/release-notes.sh`, `.github/dependabot.yml` | Validates a release tag and prints its notes, or the platforms it selects, for `release.yml`; Dependabot's updates of the actions the workflows use |
| `patches/` | The pnpm dependency patches |

## Hosted Windows checks

`quality.yml`'s `windows-check` runs on `windows-latest` (x64, the MSVC host target), whose `TEMP` is an 8.3 short path:
`scripts/cargo.mjs check`, `clippy` and `test` with `-p octoboard`, then clippy and `cargo test -p octoboardd` on the
ConPTY files `fetch-conpty.mjs --arch x64` places. All of it is expected to pass, the daemon's tests included, on the
bundled console host. Some daemon tests are sensitive to load: Grok's three interrupt tests and
`processes::subprocess::tests::cancelling_during_the_grace_kills_the_program_and_keeps_the_reply` once failed under the
full suite on a loaded ARM64 virtual machine and passed run alone; they are expected to pass on the runner.
`windows-bundle` builds the unsigned installer of each architecture exactly as the release builds it (`WINDOWS_SIGNING=none`,
no update key; arm64 cross-compiled), so a green run says `tauri build` passes on Windows for both. The Windows code-signing modes and the `release.yml` route are described
in the [desktop scripts README](../../apps/desktop/scripts/README.md).

## Dependency direction

The rules behind these edges are in `docs/conventions/tooling.md`, "Scripts".

- Repository scripts depend on nothing in the product code. The desktop scripts (`apps/desktop/scripts/`) import
  `scripts/lib/env.mjs` and `scripts/lib/git.mjs`, the UI's gallery sweep and the docs' screenshot script import `scripts/verify/lock.mjs` (and through it
  `scripts/verify/process-group.mjs`), and the
  UI's test scripts run `scripts/verify/heavy.mjs`.
- Reads across package boundaries are declared in `scripts/verify/scope.mjs`, which the verification README explains;
  `scope.test.mjs` scans the packages' sources (`.rs`, `.ts`, `.tsx`, `.mjs`, `.astro`, `.mdx`) for ones not declared, and the
  documentation site's reads of the UI and the website (its screenshot script's use of the gallery sweep's helpers included), and the website's read of the docs' pages, are declared there.

## Where new code goes

- A new repository-level command is a script in `scripts/` run through a root `package.json` script; code shared by
  scripts goes in `scripts/lib/` (the rules: `docs/conventions/tooling.md`, "Scripts").
- A new check or workspace package follows the verification README's "Changing it".
- A new fact about product identity goes in `config/app.json`; a dependency patch in `patches/` and
  `patchedDependencies` (the rules: `docs/conventions/tooling.md`, "Dependencies and single sources of configuration").

## Shared entry points

| Need | Entry point |
|---|---|
| Run `git` or another command from a script | `scripts/lib/git.mjs` (`git` probes and answers null; `gitChecked` and `gitCheckedWith` require an answer; `gitResult` returns the exit status) |
| Read env files with environment precedence | `scripts/lib/env.mjs` |
| Update or check generated author fields | `pnpm identity:sync` writes the tracked outputs; `pnpm identity:check` checks for drift without writing |
| Run the checks a change calls for | `pnpm verify` ([`scripts/verify/`](../../scripts/verify/README.md)) |
| `pnpm verify` fails with `unregistered write` after a new file write in Rust | The verification README's "Registered writes" and the crate's `registered_writes.json` (beside its path helpers) |
| Format staged files | `pnpm format:staged` (the pre-commit hook) |

Checks that hold this area: `pnpm identity:check`, the `scripts/**/*.test.mjs` tests, the UI's `appConfig.test.ts`, and
the release workflow's validation of the tag, the Apple secrets and `Cargo.lock` and its `sign` job's check of the update key, and `publish-release.mjs`'s check of every update signature.
