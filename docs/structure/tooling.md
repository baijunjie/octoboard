# Tooling and repository configuration

How the repository is built, checked and released, outside the code of any one end. The product ends are described in
[`daemon.md`](daemon.md), [`ui.md`](ui.md), [`desktop.md`](desktop.md) and [`web.md`](web.md).

## Workspaces and toolchains

- **pnpm workspace** (`package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`): the packages are the `apps/*` and
  `packages/*` directories that have a `package.json` (`apps/web/`, `apps/desktop/`, `packages/ui/`); `apps/daemon/` has
  none. The root `package.json` holds the cross-package commands (`verify`, `lint`, `format`, `test:rust`,
  `clippy:rust`, `hooks:install`) and the shared lint dev dependencies. `patchedDependencies` in `pnpm-workspace.yaml`
  registers the patches in `patches/` (currently one for jsdom's selector engine, used by the UI's tests).
- **Cargo workspace** (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `rustfmt.toml`): members `apps/daemon` and
  `apps/desktop/src-tauri`, built into one `target/`. The workspace `version` is the application's version and must
  equal `version` in `apps/desktop/package.json`; the desktop crate's `build.rs` fails the build when they differ.
  `rust-toolchain.toml` pins the one toolchain used locally and in CI, and the workspace `rust-version` matches it.
- `eslint.config.mjs` — the repository-wide lint and format rules (JavaScript, TypeScript, TSX, Vue), including the UI's
  import-layering blocks, which live in `packages/ui/eslint.layers.mjs` (see [`ui.md`](ui.md)); `pnpm format` runs it
  with `--fix --fix-type layout` and `cargo fmt`.
- `config/app.json` — the single source for product identity, repository and issue-report destinations, author and
  support contacts, website address, license and copyright year. Read by the UI (`packages/ui/src/appConfig.ts`), the
  website (`apps/web/nuxt.config.ts`) and both Cargo crates' `build.rs`; copies the toolchain requires elsewhere are
  checked against it (the UI's `appConfig.test.ts`, the build scripts).
- `.env.local.example`, `.env.secret.example` — the templates of the gitignored machine-local and release-credential
  files. `scripts/lib/env.mjs` is the reader; it reads `.env.secret` and `.env.local`.
- `assets/branding/` — brand source artwork that no app serves. `.claude/` holds the agent skills and subagent
  definitions of the repository's workflow.

## Directory map

| Path | Role |
|---|---|
| `scripts/lib/git.mjs`, `scripts/lib/env.mjs` | The one way repository scripts run `git` (and other commands), and the shared env-file reader |
| `scripts/verify.mjs`, [`scripts/verify/`](../../scripts/verify/README.md) | `pnpm verify`: selects and runs the checks a change calls for |
| `scripts/hooks/pre-push.mjs`, `scripts/hooks/format-staged.mjs` | The two hook stages, with their tests beside them (see the hooks' README; the pre-push stage takes its baseline from `remoteTargetBase` in `scripts/verify/changes.mjs`); `scripts/pre-push.mjs` and `scripts/format-staged.mjs` are forwarding stubs for hook copies installed before the move |
| `scripts/cargo.mjs` | `pnpm test:rust`, `pnpm clippy:rust` and the Linux CI `cargo check`: both crates in one cargo invocation, without needing the daemon sidecar (the one place that empties `bundle.externalBin`) |
| [`.githooks/`](../../.githooks/README.md) | The hook entry point and its installers for the commit and push hooks |
| `.github/workflows/` | `quality.yml` (the application checks: `pnpm verify --release --fresh` and a Linux `cargo check` of both crates), `release.yml` (on a version tag: validate, run the checks, build, notarize, sign the update archive in a step of its own that alone holds the update key, and publish), `website.yml` (generate the website, run its tests and lint, deploy it to GitHub Pages) |
| `.github/release-notes.sh`, `.github/dependabot.yml` | Validates a release tag and prints its notes for `release.yml`; Dependabot's updates of the actions the workflows use |
| `patches/` | The pnpm dependency patches |

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
| Run the checks a change calls for | `pnpm verify` ([`scripts/verify/`](../../scripts/verify/README.md)) |
| Format staged files | `pnpm format:staged` (the pre-commit hook), never a whole-repository formatter as a delivery step |

Checks that hold this area: the `scripts/**/*.test.mjs` tests, the UI's `appConfig.test.ts`, and the release workflow's
validation of the tag, the secrets and `Cargo.lock`.
