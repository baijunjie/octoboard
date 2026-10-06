# Daemon Under Apps Development Plan

## Problem and approach

The monorepo layout keeps the daemon at the top level on the grounds that it is the core rather than a client, and
`apps/` holds clients. But the daemon is a deliverable in its own right: on macOS it ships inside the desktop application
as its sidecar, and on Linux it is to be the whole deployment, serving the UI to a browser by itself. Once it serves the
UI it also depends on a workspace package (`packages/ui`), so it is no longer a crate standing apart from the workspace.

The Rust side is also not unified: the daemon and the desktop shell's Tauri crate are two independent cargo projects, each
with its own lockfile and build directory, so shared dependencies are compiled twice and their versions can drift apart.

The approach: move the daemon into `apps/`, changing what `apps/` holds from clients to deliverables, then add a root
Cargo workspace whose members are the daemon and the desktop shell's Tauri crate.

## Key design decisions

- **`apps/` holds deliverables, not just clients, and the daemon moves to `apps/daemon`.** It is one (the macOS sidecar,
  the whole Linux deployment), so it sits beside `apps/desktop`. The distinction between the core and its clients is
  carried by the docs and by the protocol constraint (clients reach the daemon only over its protocol), not by a
  top-level directory of its own.
- **One root Cargo workspace** with the daemon and the Tauri crate as members: one lockfile and one build directory, so
  shared dependencies build once and resolve to one version.
- **Done before the work on building and running the daemon on Linux starts**, because that work happens mostly inside
  the daemon and every open branch touching it will conflict with the move.

## Milestones

- [01 Move the daemon to apps/daemon](01-move-daemon-to-apps.md)
- [02 Root Cargo workspace](02-root-cargo-workspace.md)

## Open

- Whether `apps/daemon` becomes a pnpm workspace package (a `package.json` wrapping its cargo commands) so the workspace
  can order "build the UI, then embed it in the daemon"; the need arises once the daemon serves the UI.
- Whether package metadata the two crates share (edition, minimum Rust version, authors, license, repository) is lifted
  into the workspace.
