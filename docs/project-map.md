# Project Map

Navigation for the code tree: one line per module, linking to that module's own doc for how it works internally.

The repository is a pnpm-workspace monorepo split two ways:

- **`apps/`** holds the deliverables — one directory per thing that ships, the daemon among them, since it ships as the
  desktop application's sidecar and is the whole of a server-side deployment on its own.
- **`packages/`** holds code shared between them.

The repository root is two workspace roots at once:

- The **pnpm workspace** (`package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`): its packages are the `apps/*` and
  `packages/*` directories that have a `package.json`; `apps/daemon/` has none and so is not one of them.
- The **Cargo workspace** (`Cargo.toml`, `Cargo.lock`): the daemon (`apps/daemon/`) and the desktop shell's Tauri crate
  (`apps/desktop/src-tauri/`) are its two members, built into one `target/` directory.

Per-package commands are in that package's doc.

## Deliverables

- [`apps/daemon/`](../apps/daemon/README.md) — `octoboardd`, the Rust daemon and the core of the system: the one process
  that owns every agent process and all of Octoboard's data, reached by every client only through the WebSocket/HTTP
  protocol in [`apps/daemon/PROTOCOL.md`](../apps/daemon/PROTOCOL.md).
- [`apps/desktop/`](../apps/desktop/README.md) — `@octoboard/desktop`, the Tauri 2 shell of the macOS desktop
  application: window, native menu, menu bar icon, `octoboardd` sidecar, exit flow, macOS privacy grants, in-app
  updates, and the build, install, release and release-publishing scripts; it loads the UI from `packages/ui/` and has
  no UI of its own.
- [`apps/web/`](../apps/web/README.md) — `@octoboard/web`, the public Nuxt website: localized product and privacy pages
  generated as static files for GitHub Pages, including a fixture-backed preview built from the shared React UI.
- [`apps/ios/`](../apps/ios/README.md) — placeholder for the native iOS client; no content, not a workspace package.
- [`apps/android/`](../apps/android/README.md) — placeholder for the native Android client; no content, not a workspace
  package.

## Shared packages

- [`packages/ui/`](../packages/ui/README.md) — `@octoboard/ui`, the HeroUI + React 19 UI of the desktop application:
  daemon client and store, platform adapter, window chrome, sidebar, terminal pane, right pane (project files and
  changes, shell terminals, session report), dialogs and settings, file viewer, internationalization and keyboard
  shortcuts, plus a dev-only gallery of UI states over a fixture daemon and the separate website-preview build; loaded
  by `apps/desktop/` and also runs in a plain browser.

## Repository-level code and configuration

- [`scripts/`](../scripts/) — the repository-level scripts behind root `package.json` commands: `env.mjs` (the shared
  env-file reader), `test-rust.mjs` (`pnpm test:rust`), and `verify.mjs` with `verify/` (`pnpm verify`, which picks and
  runs the checks a branch's changes call for).
- [`.githooks/`](../.githooks/) — the git hook scripts: the revert gate (`hook.sh`, `revert-gate.py`, installed by
  `install.sh`, resident guarded branches in `branches`) and the `flow-*.sh` scripts for the worktree merge flow.
- [`.github/workflows/`](../.github/workflows/) — CI: `quality.yml` (ESLint and Rust formatting) and `website.yml`
  (publishing the website to GitHub Pages).
- [`config/app.json`](../config/app.json) — the single source for product identity, repository and issue-report
  destinations, author and support contacts, website address, license and copyright year, read by the UI, the website
  and both Cargo crates' `build.rs`; the copies the toolchain requires elsewhere are checked against it.
- [`eslint.config.mjs`](../eslint.config.mjs) — the repository-wide lint and formatting rules for JavaScript,
  TypeScript, TSX and Vue; [`rustfmt.toml`](../rustfmt.toml) is the Rust counterpart.
- [`.env`](../.env), [`.env.local.example`](../.env.local.example), [`.env.secret.example`](../.env.secret.example) —
  committed non-secret configuration and the templates for the gitignored machine-local and release-credential files.
- [`patches/`](../patches/) — the pnpm dependency patches registered by `patchedDependencies` in `pnpm-workspace.yaml`.
- [`assets/branding/`](../assets/branding/) — brand source artwork that no app serves, including the logo the root
  README shows.
