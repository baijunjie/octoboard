# Project Map

Top-level map of the code: what is built with what, and which structure doc answers where things are and which rules new
code follows.

Octoboard is a monorepo that orchestrates AI coding agents (Claude Code, Codex, Grok Build) on a developer's machine.
Everything revolves around one daemon; every client reaches it only through the WebSocket/HTTP protocol in
[`apps/daemon/PROTOCOL.md`](../apps/daemon/PROTOCOL.md).

## Technology stack

| End | Stack |
|---|---|
| Daemon | Rust (edition 2021, toolchain pinned in `rust-toolchain.toml`), tokio, axum (HTTP and WebSocket), SQLite through rusqlite, `portable-pty` and `alacritty_terminal`, `rmcp` for the MCP server, clap |
| UI | TypeScript, React 19 with the React Compiler, HeroUI 3 (react-aria), Tailwind 4, Zustand, xterm.js, Vite 8, Vitest |
| Desktop shell | Tauri 2 (Rust crate plus npm scripts), macOS and Windows |
| Website | Nuxt 4 (Vue 3, Nuxt i18n, sitemap), generated as static files for GitHub Pages |
| Tooling | Node scripts (`.mjs`), pnpm 10, ESLint (also formats), rustfmt, GitHub Actions |

## Workspaces

The repository root is two workspace roots at once:

- The **pnpm workspace** (`package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`): the `apps/*` and `packages/*`
  directories that have a `package.json`, namely `apps/desktop/`, `apps/web/` and `packages/ui/`.
- The **Cargo workspace** (`Cargo.toml`, `Cargo.lock`): the daemon (`apps/daemon/`) and the desktop shell's Tauri crate
  (`apps/desktop/src-tauri/`), built into one `target/` directory.

`apps/` holds the deliverables, one directory per thing that ships; `packages/` holds code shared between them.
`contracts/protocol/` holds the protocol's JSON Schema, which a daemon test renders from the wire types and native
clients are generated from, and `contracts/protocol/released/`, the snapshot of the last release that a daemon test holds
the schema to (see "Versions and compatibility" in `apps/daemon/PROTOCOL.md`).

## Structure docs

Each doc holds that end's directory map, its dependency direction, where new code goes, and the shared entry points new
code must use.

- [Daemon](structure/daemon.md) — `apps/daemon/` ([README](../apps/daemon/README.md)): `octoboardd`, the core of the
  system; owns every agent process and all of Octoboard's data.
- [UI](structure/ui.md) — `packages/ui/` ([README](../packages/ui/README.md)): the React UI of the desktop application,
  which reaches HeroUI and React Aria only through its wrapper layer (`src/primitives/`), with the dev-only state
  gallery, the fixtures runtime and the website preview build.
- [Desktop shell](structure/desktop.md) — `apps/desktop/` ([README](../apps/desktop/README.md)): the Tauri 2 shell for macOS and Windows
  with its build, install and release scripts; loads the UI and starts the daemon as a sidecar.
- [Website](structure/web.md) — `apps/web/` ([README](../apps/web/README.md)): the public Nuxt website.
- [Tooling](structure/tooling.md) — `scripts/`, `.githooks/`, `.github/`, `config/`, `patches/`, `eslint.config.mjs`,
  `rustfmt.toml` and the other repository configuration: verification, hooks, CI, release.

Placeholders for future native clients, with no content and not workspace packages: [`apps/ios/`](../apps/ios/README.md)
and [`apps/android/`](../apps/android/README.md).
