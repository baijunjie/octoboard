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
  package; `pnpm verify` runs the checks a change calls for. `apps/daemon/` has no `package.json` and so is not one
  of them.
- The **Cargo workspace**: `Cargo.toml` and `Cargo.lock`, with the daemon (`apps/daemon/`) and the desktop shell's
  Tauri crate (`apps/desktop/src-tauri/`) as its two members. `cargo build` and `cargo test` from the root cover
  both, into the one `target/` directory here. The Tauri crate resolves its `octoboardd` sidecar at compile time, so
  on a fresh checkout `pnpm --filter @octoboard/desktop build:daemon` has to have run once before either command
  succeeds; `pnpm test:rust` (`scripts/test-rust.mjs`) runs both crates' tests without it.

Per-package commands are in that package's doc.

## Shared configuration

- [`eslint.config.mjs`](../eslint.config.mjs) — the repository-wide JavaScript, TypeScript, TSX and Vue lint and
  formatting rules, combining ESLint, Stylistic, TypeScript, Vue and React Hooks checks.
- [`patches/`](../patches/) — the pnpm dependency patches that `patchedDependencies` in `pnpm-workspace.yaml` registers,
  applied on `pnpm install`; the patch for `@asamuzakjp/dom-selector` (the selector engine of the jsdom behind the
  `packages/ui` tests) is the only one, and the reason and removal condition are in the comment above that entry.
- [`rustfmt.toml`](../rustfmt.toml) — the formatting configuration shared by both Cargo workspace crates.
- [`.github/workflows/quality.yml`](../.github/workflows/quality.yml) — the CI checks for ESLint and Rust formatting.
- [`config/app.json`](../config/app.json) — the single source for product identity, repository and issue-report destinations,
  author and support contact details, the website address, the license and the copyright year. The UI and website fill
  the name into their copy as `{appName}` and into page titles; the website exposes `author.name` as `operatorName` and
  `support.email` as `contactEmail`. The desktop and daemon crates' `build.rs` hand the name to the menu, window title
  and daemon messages, and the desktop crate's also the repository address, where its update manifests are. The copies
  Tauri and Cargo require (`productName` in `tauri.conf.json`, `repository` and `license` in the root `Cargo.toml`)
  mirror it, and the desktop crate's `build.rs` fails the build when they drift. Two more copies cannot be derived:
  the root `LICENSE` (license, copyright year and author) and the website's default origin in
  `apps/web/site.config.ts` (the website address); `packages/ui/src/appConfig.test.ts` fails when either drifts.
- [`.env`](../.env), [`.env.secret.example`](../.env.secret.example) and [`scripts/env.mjs`](../scripts/env.mjs) — the
  env files and the small reader they share. `.env` (committed) holds non-secret configuration, read by the UI's dev
  server, with a gitignored `.env.local` overriding it; `.env.secret` (gitignored, template `.env.secret.example`) holds
  the release credentials (Apple's, and the update signing key), read by `apps/desktop/scripts/release.mjs` and by
  `apps/desktop/scripts/install-app.mjs` for its signing identity.
  [`.env.install.example`](../.env.install.example) is the template for the gitignored `.env.install.local`, read
  only by the desktop's `install:macos` command for its installation directory.
- [`scripts/`](../scripts/) — the repository-level scripts behind root `package.json` commands: `env.mjs` (the env
  reader above), `test-rust.mjs` (`pnpm test:rust`) and `verify.mjs` with `verify/` (`pnpm verify`, the pre-delivery
  checks). `pnpm verify` picks the checks a branch's changes call for (`verify/scope.mjs` implements the scope rule,
  `verify/steps.mjs` lists the commands), takes the branch's base (its merge base with its recorded target branch) and changed paths from
  `verify/changes.mjs`, skips steps already passed on the same content and toolchain (`verify/stamp.mjs`, keyed by the
  working tree's content from `verify/tree.mjs`), re-runs only the failed UI test files after a failed UI test step when
  nothing but test files nothing else imports changed since (`verify/rerun.mjs`, fed by the reporter in
  `packages/ui/src/test/failedFilesReporter.ts`), and caps concurrent heavy steps across worktrees (`verify/lock.mjs`); its tests are
  `verify/*.test.mjs`. The UI package's gallery sweep reuses the lock, the stamps directory, `tree.mjs` and `changes.mjs`
  (to choose its languages). The scope rule's source is the "Formatting and checks" step of the root
  `CLAUDE.md`.

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
  (a Settings choice, off by default), the macOS privacy-permission grants the Settings dialog offers, the in-app
  update commands (looking for, downloading and installing a signed update from a release channel), and build,
  local-install, release and release-publishing scripts. It loads the UI from `packages/ui/` and has no UI of its own; a client of `apps/daemon/` over WebSocket
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
  dialog (agent accounts, with the added agents list, sign-in and plan quota, and About, with the version, update
  controls and third-party licenses, among its sections), the in-app update state and notice, the other dialogs, the read-only file viewer (code, rendered Markdown, image and diff rendering behind an application-owned
  interface), the right pane at the end side of the content panel (a stack of panel cards: a project's Files, the browser of its
  files and a local branch's uncommitted and committed changes, and its Terminal, its shell terminals as tabs, or a console session's Report) and the internationalization (language choice and message catalogs), the keyboard shortcut registry (rebindable bindings, their store and the Settings section), and a dev-only gallery of UI states
  over a fixture daemon (`src/gallery/`, served by the dev server), plus the separate website-preview build. Loaded by
  the `apps/desktop/` shell and also runs in a plain browser.
