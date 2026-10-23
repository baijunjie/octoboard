# Project Map

Navigation for the code tree: one line per module, linking to that module's own doc for how it works internally.

The repository is a pnpm-workspace monorepo split two ways:

- **`apps/`** holds the deliverables — one directory per thing that ships, the daemon among them, since it ships as the
  desktop application's sidecar and is the whole of a server-side deployment on its own.
- **`packages/`** holds code shared between them.

The repository root is two workspace roots at once:

- The **pnpm workspace**: `package.json`, `pnpm-workspace.yaml` (workspace packages are the `apps/*` and `packages/*`
  directories that have a `package.json`) and `pnpm-lock.yaml`. Dependencies install once from the root with
  `pnpm install`; `pnpm typecheck` and `pnpm build` run the package script of the same name across every workspace
  package. `apps/daemon/` has no `package.json` and so is not one of them.
- The **Cargo workspace**: `Cargo.toml` and `Cargo.lock`, with the daemon (`apps/daemon/`) and the desktop shell's
  Tauri crate (`apps/desktop/src-tauri/`) as its two members. `cargo build` and `cargo test` from the root cover
  both, into the one `target/` directory here. The Tauri crate resolves its `octoboardd` sidecar at compile time, so
  on a fresh checkout `pnpm --filter @octoboard/desktop build:daemon` has to have run once before either command
  succeeds; `pnpm test:rust` (`scripts/test-rust.mjs`) runs both crates' tests without it.

Per-package commands are in that package's doc.

## Shared configuration

- [`eslint.config.mjs`](../eslint.config.mjs) — the repository-wide JavaScript, TypeScript, TSX and Vue lint and
  formatting rules, combining ESLint, Stylistic, TypeScript, Vue and React Hooks checks.
- [`rustfmt.toml`](../rustfmt.toml) — the formatting configuration shared by both Cargo workspace crates.
- [`.github/workflows/quality.yml`](../.github/workflows/quality.yml) — the CI checks for ESLint and Rust formatting.
- [`config/app.json`](../config/app.json) — the single source for product identity, repository and issue-report destinations,
  author and support contact details. The UI and website fill the name into their copy as `{appName}` and into page titles; the website
  exposes `author.name` as `operatorName` and `support.email` as `contactEmail`. The desktop and daemon crates'
  `build.rs` hand the name to the menu, window title and daemon messages. The copies Tauri and Cargo require
  (`productName` in `tauri.conf.json`, `repository` in the root `Cargo.toml`) mirror it, and the desktop crate's
  `build.rs` fails the build when they drift.
- [`.env`](../.env), [`.env.secret.example`](../.env.secret.example) and [`scripts/env.mjs`](../scripts/env.mjs) — the
  env files and the small reader they share. `.env` (committed) holds non-secret configuration, read by the UI's dev
  server, with a gitignored `.env.local` overriding it; `.env.secret` (gitignored, template `.env.secret.example`) holds
  the release credentials, read by `apps/desktop/scripts/release.mjs` and by `apps/desktop/scripts/install-app.mjs`
  for its signing identity.
  [`.env.install.example`](../.env.install.example) is the template for the gitignored `.env.install.local`, read
  only by the desktop's `build:install` command for its installation directory.
  Root `scripts/` holds the helpers the build scripts share.

## Brand artwork

- [`assets/branding/`](../assets/branding/) — brand source artwork that no app serves, including the logo the root
  README shows. The website's wordmark image is `apps/web/public/wordmark.png`; the slogan is localized text
  and is not baked into any image.

## Deliverables

- [`apps/daemon/`](../apps/daemon/README.md) — `octoboardd`, the Rust daemon and the core of the system rather than one
  of its clients: the one process that owns every agent process and all of Octoboard's data. Host role (PTYs, agent
  processes and project shell terminals, directories, repositories) and coordinator role (console/project/session/page and agent account data in
  SQLite, and the configuration document exported from it and read back into it). Every client reaches it only
  through the WebSocket/HTTP protocol in [`apps/daemon/PROTOCOL.md`](../apps/daemon/PROTOCOL.md).
- [`apps/desktop/`](../apps/desktop/README.md) — `@octoboard/desktop`, the Tauri 2 shell of the macOS desktop
  application: window (with its overlay titlebar, translucent native material and remembered frame), native menu
  (labelled in the UI's language), `octoboardd` sidecar, exit flow, optional background running behind a menu bar icon
  (a Settings choice, off by default), the macOS privacy-permission grants the Settings dialog offers, and build,
  local-install and release scripts. It loads the UI from `packages/ui/` and has no UI of its own; a client of `apps/daemon/` over WebSocket
  only.
- [`apps/ios/`](../apps/ios/README.md) — placeholder for the native iOS client; no content, not a workspace package.
- [`apps/android/`](../apps/android/README.md) — placeholder for the native Android client; no content, not a
  workspace package.
- [`apps/web/`](../apps/web/README.md) — `@octoboard/web`, the public Nuxt website: localized product and privacy
  pages generated as static files for GitHub Pages, including a fixture-backed preview built from the shared React UI,
  without a live daemon connection.

## Shared packages

- [`packages/ui/`](../packages/ui/README.md) — `@octoboard/ui`, the HeroUI + React 19 UI of the desktop application: the
  daemon client, store and platform adapter, the window chrome (the top bar and the left rail with the console
  switcher and the plan quota entry), the content panel with the sidebar (projects and sessions, the project and console session focus modes) or, while there is no console, the welcome screen,
  the archive view, the xterm.js terminal pane, the pane-layout state, Back/Forward navigation history, the settings
  dialog (agent accounts, with the added agents list, sign-in and plan quota, among its sections), the other dialogs, the read-only file viewer (code, rendered Markdown, image and diff rendering behind an application-owned
  interface), the right pane at the end side of the content panel (a stack of panel cards: a project's Files, the browser of its
  files and a local branch's uncommitted and committed changes, and its Terminal, its shell terminals as tabs, or a console session's Report) and the internationalization (language choice and message catalogs), and a dev-only gallery of UI states
  over a fixture daemon (`src/gallery/`, served by the dev server), plus the separate website-preview build. Loaded by
  the `apps/desktop/` shell and also runs in a plain browser.
