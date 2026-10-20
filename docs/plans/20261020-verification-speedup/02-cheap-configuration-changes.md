# 02 Cheap configuration changes

> Goal: cut repeat-run cost and remove the random UI test timeouts with configuration alone.
> Completion criteria: the UI tests pass under load with no command-line flags; a repeat run of each step is faster
> than milestone 01's warm baseline, by the measured amount recorded here; the desktop tests run in a fresh worktree
> without building the daemon, and running the app without a real sidecar still fails with a clear message.

## Technical design

- [ ] UI tests: a default per-test and per-hook timeout suited to a loaded machine, a worker cap below the core count,
      and the persistent module transform cache turned on.
- [ ] UI type check: incremental, with its build info kept under the package's own cache directory.
- [ ] Lint: cached by file content, the cache kept out of the repository.
- [ ] Rust dev profile: no debug information for dependencies (optionally line tables only for the workspace crates).
- [ ] Rust tests for both crates in one workspace-wide invocation when both are in scope, so dependencies are built
      once with unified features.
- [ ] Desktop tests: the sidecar route settled under "Open" in the overview, documented where the desktop app's
      development steps are.

## Notes for the developer

- **Development notes**: the daemon enables extra serde_json and tokio features, so a workspace-wide build unifies
  features the two crates' own builds do not; check the desktop app still builds and runs as shipped. The UI test
  docs rely on module state living for one file (lazy imports, the toast queue), so test isolation is not touched here.
  A slow worker cap or timeout hides real slowness; milestone 05 deals with that.
- **Reference docs**: `docs/memory/writing-automated-tests.md`, `apps/desktop/README.md` "Development",
  `docs/memory/building-and-launching-the-app-for-verification.md`.
