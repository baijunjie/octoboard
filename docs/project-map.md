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
  succeeds.

Per-package commands are in that package's doc.

## Deliverables

- [`apps/daemon/`](../apps/daemon/README.md) — `octoboardd`, the Rust daemon and the core of the system rather than one
  of its clients: the one process that owns every agent process and all of Octoboard's data. Host role (PTYs, agent
  processes, directories, repositories) and coordinator role (console/project/session/page data in SQLite). Every
  client reaches it only through the WebSocket/HTTP protocol in
  [`apps/daemon/PROTOCOL.md`](../apps/daemon/PROTOCOL.md).
- [`apps/desktop/`](../apps/desktop/README.md) — `@octoboard/desktop`, the Tauri 2 shell of the macOS desktop
  application: window (with its overlay titlebar and remembered frame), native menu, `octoboardd` sidecar, exit flow
  and release scripts. It loads the UI from `packages/ui/` and has no UI of its own; a client of `apps/daemon/` over
  WebSocket only.
- [`apps/ios/`](../apps/ios/README.md) — placeholder for the native iOS client; no content, not a workspace package.
- [`apps/android/`](../apps/android/README.md) — placeholder for the native Android client; no content, not a
  workspace package.
- [`apps/web/`](../apps/web/README.md) — placeholder for the project website (not a client of `apps/daemon/`); no
  content, not a workspace package.

## Shared packages

- [`packages/ui/`](../packages/ui/README.md) — `@octoboard/ui`, the HeroUI + React 19 UI of the desktop application:
  the daemon client, store and platform adapter, the window-wide top bar, the console → project → session sidebar, the
  xterm.js terminal pane, the pane-layout state, the settings dialog, the other dialogs and the hub's report panel.
  Loaded by the `apps/desktop/` shell and also runs in a plain browser.
