# Project Map

Navigation for the code tree: one line per module, linking to that module's own doc for how it works internally.

The repository is a pnpm-workspace monorepo split three ways:

- **`daemon/`** is the core, not a client: the one process that owns every agent process and all of Octoboard's data.
  It stays at the top level, outside the workspace, and is a Rust crate built with `cargo`.
- **`apps/`** holds the clients — one directory per deliverable, each talking to `daemon/` only over its protocol —
  and the project website, which is not a client.
- **`packages/`** holds code shared between apps.

The workspace root has `package.json`, `pnpm-workspace.yaml` (workspace packages are the `apps/*` and `packages/*`
directories that have a `package.json`) and `pnpm-lock.yaml`. Dependencies install once from the root with
`pnpm install`; `pnpm typecheck` and `pnpm build` run the package script of the same name across every workspace
package. Per-package commands are in that package's doc.

## Core

- [`daemon/`](../daemon/README.md) — `octoboardd`, the Rust daemon: host role (PTYs, agent processes, directories,
  repositories) and coordinator role (console/project/session/page data in SQLite), reachable only through the
  WebSocket/HTTP protocol in [`daemon/PROTOCOL.md`](../daemon/PROTOCOL.md).

## Clients and apps

- [`apps/desktop/`](../apps/desktop/README.md) — `@octoboard/desktop`, the Tauri 2 + React + TypeScript desktop
  application: the console/project/session UI, terminal and report panel, a client of `daemon/` over WebSocket only.
  The shipped application; it still carries its own UI.
- [`apps/ios/`](../apps/ios/README.md) — placeholder for the native iOS client; no content, not a workspace package.
- [`apps/android/`](../apps/android/README.md) — placeholder for the native Android client; no content, not a
  workspace package.
- [`apps/web/`](../apps/web/README.md) — placeholder for the project website (not a client of `daemon/`); no content,
  not a workspace package.

## Shared packages

- [`packages/ui/`](../packages/ui/README.md) — `@octoboard/ui`, the HeroUI + React 19 rebuild of the desktop UI: the
  daemon client, store and platform adapter, running in a plain browser. A workspace package; not yet loaded by the
  desktop shell, so `apps/desktop/` still ships its own UI.
