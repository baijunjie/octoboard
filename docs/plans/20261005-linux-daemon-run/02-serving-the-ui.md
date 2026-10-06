# 02 Serving the UI

> Goal: opening the daemon's address in a local browser shows the full UI.
> Completion criteria: on Linux, a user can run a hub and project sessions entirely through a browser on the same
> machine; the daemon binary contains the UI.

## Technical design

- [ ] The UI build is embedded into the daemon at build time and served same-origin.
- [ ] The UI finds the daemon at its own origin.

## Implementation plan

- [ ] Add the embedding to the daemon's build, ordered after the UI build.

## Handoff

- Decide whether `apps/daemon` becomes a pnpm workspace package — a `package.json` wrapping its cargo commands — so
  the workspace itself can order "build the UI, then embed it in the daemon" rather than leaving that order to be
  remembered by hand. It has no `package.json` today and is therefore skipped by the `apps/*` glob in
  `pnpm-workspace.yaml`, which was deliberate while nothing in it depended on a workspace package; this milestone is
  the first thing that makes it depend on one.

## Notes for the developer

- **Development notes**: requires the UI to run outside the Tauri shell through the adapter.
- **Reference docs**: `apps/daemon/README.md`, `apps/daemon/PROTOCOL.md`.
