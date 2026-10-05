# Monorepo Layout Development Plan

## Problem and approach

Octoboard is one application in one repository today. The intended final shape has several clients of the same daemon:
the macOS application, a Linux browser UI, native iOS and Android apps, and the project website. The repository has to be
laid out to hold them before any of them is added.

The layout is a pnpm-workspace monorepo with clients under `apps/`, the UI shared by the desktop clients under
`packages/`, and the daemon kept at the top level because it is the core rather than a client. This plan only moves
things into that layout and plants placeholders; it changes no behavior.

## Key design decisions

- **pnpm workspaces** for the JavaScript side (replacing the application's current npm setup).
- **`apps/` holds clients**: `apps/desktop` (the existing application: the native shell for macOS), `apps/ios` and
  `apps/android` (native apps, placeholders), `apps/web` (the project website, placeholder).
- **`packages/ui` holds the UI shared by the desktop clients.** It starts as a placeholder here; the UI itself arrives
  with its own plan, and until then the UI stays where it is inside the desktop application.
- **The daemon stays at the top level.**
- **Placeholders hold only a README** that states what the directory will be; they build nothing and join no workspace
  command until they have content.
- **A pure move.** No behavior change rides along, and file history is preserved.

## Milestone

> Goal: the repository is laid out as above and everything that worked before still works.
> Completion criteria: the application type-checks, builds and launches from `apps/desktop` with its daemon sidecar; the
> release script and the git hooks work against the new paths; installing and running the common commands from the
> workspace root works with pnpm; the placeholders exist; the project map and documentation index describe the new layout.

### Implementation plan

- [ ] Switch the application to pnpm and add the workspace root with the common commands (typecheck, build) runnable
  across packages.
- [ ] Move the application under `apps/desktop` and fix every place that names its old location: scripts, the Tauri
  configuration's path to the daemon sidecar, the release script, the git hooks, ignore files, docs.
- [ ] Add the `apps/ios`, `apps/android`, `apps/web` and `packages/ui` placeholders.
- [ ] Update the project map and the documentation index.

### Notes for the developer

- **Reusable capabilities**: the daemon-sidecar build step and the release script of the application; both should keep
  working apart from paths.
- **Development notes**: do this when no other branch is mid-flight on the application, because every open branch will
  conflict with the move. The reference project's layout (`apps/` per platform) is the model for the placeholders.
- **Reference docs**: `docs/project-map.md`, `docs/README.md`, the READMEs of the application and the daemon.

## Open

- The shape of the root workspace configuration beyond the common commands (shared TypeScript settings and the like) is
  left to when the first shared package has content.
