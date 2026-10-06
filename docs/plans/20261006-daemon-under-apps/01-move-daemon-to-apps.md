# 01 Move the daemon to apps/daemon

> Goal: the daemon lives at `apps/daemon` and everything that worked before still works.
> Completion criteria: the daemon builds and its tests pass from `apps/daemon`; the desktop application type-checks,
> builds and launches with its daemon sidecar, and the release script still produces it; nothing outside git history
> still names the old top-level `daemon/` path; the project map states the new meaning of `apps/`.

## Implementation plan

- [ ] Move the daemon directory to `apps/daemon`.
- [ ] Fix every place that names the old location: the desktop shell's sidecar build step, ignore files, source comments
  that name `daemon/PROTOCOL.md` or other daemon paths, and docs outside the daemon that link into it.
- [ ] Update the project map: the repository split becomes `apps/` (deliverables, the daemon among them) and `packages/`
  (shared code); keep the statement that the daemon is the core and clients reach it only over its protocol, now under the
  daemon's entry in `apps/`.
- [ ] Update the documentation index, the architecture doc and the module READMEs that link to the daemon.

## Notes for the developer

- **Development notes**: do this when no other branch is mid-flight on the daemon. Move with git so file history follows.
  Other open plans under `docs/plans/` also name `daemon/` paths; update them in the same change so the next developer is
  not sent to a path that no longer exists. `apps/daemon` has no `package.json`, so the `apps/*` glob in the pnpm
  workspace skips it the way it skips the README-only placeholders — keep it that way here (see Open in the overview). The
  release script reaches the daemon only through Tauri's before-build command, so it needs no path change but must be
  re-run as a regression check. `tauri dev` does not rebuild the sidecar, so verify the launch after running the sidecar
  build step by hand.
- **Reference docs**: `docs/project-map.md`, `docs/README.md`, `docs/architecture.md`, the READMEs of the daemon and the
  desktop shell, `docs/memory/writing-automated-tests.md` (names the daemon's test-data directory).
