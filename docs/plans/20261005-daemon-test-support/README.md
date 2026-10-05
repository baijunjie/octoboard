# Shared test support for the daemon

## Problem

The daemon's tests build the same few fixtures over and over, each module writing its own copy.

- **A scratch directory** is constructed inline by seven modules' tests — `transcript`, `coordinator`, `state`,
  `env_shell`, `store`, `trust` and the adapter module — each formatting its own `octoboardd-<module>-…` name out of
  the pid and either a thread id or a counter.
- **Cleanup is inconsistent across those copies**, so some of them accumulate directories in the system temporary
  directory on every run: `trust`, `store` and `transcript` remove theirs, `state` and `coordinator` do not.
- **Thread-id-based names are reused within a process**, where a counter is not. A module whose scratch directory
  holds a SQLite database and whose name can repeat is one leftover database away from failing a plain `INSERT`'s
  unique constraint — which is why the copy that removes the directory *before* creating it is the one that survives
  a run killed outright.
- **`app_state`** — open a store, insert one console, wrap it in an `AppState` — exists three times, in `state`,
  `trust` and `transcript`, differing only in what each returns.
- **A stand-in live session** on a PTY exists twice: `trust`'s version runs a script and attaches a reader thread,
  `transcript`'s runs an inert command and does not.
- **`wait_for(timeout, predicate)`** exists twice, once as production code in `trust` and once copied into
  `transcript`'s test module, because a test module cannot reach another module's test module.

None of this is a defect in behaviour; all of it is duplication that grows by one copy each time a module gains
tests, and the inconsistent cleanup is a latent flake rather than a visible one.

## Plan

Put the shared fixtures in a single test-support module in the daemon, compiled under `cfg(test)`, and have each
test module use it instead of its own copy. Two milestones, in dependency order: the scratch directory first,
because the other fixtures are built on it.

## Key design decisions

- **The scratch directory owns its own removal.** A guard whose `Drop` removes the directory, rather than a removal
  written as each test's last statement — a failing assertion must clean up too, and a test that cleans up only when
  it passes leaves exactly the residue that is hardest to notice.
- **Remove before creating, as well as on drop.** Dropping covers a test that fails; it does not cover a run killed
  outright, and a name built from a recyclable pid can collide with that run's leftovers.
- **Names stay per-module.** The existing `octoboardd-<module>-…` prefixes are what makes a leftover directory
  attributable to the test that made it; the shared helper takes the label rather than inventing one.
- **One module, not a crate-wide `tests/` directory.** The fixtures reach into private items (`LiveSession::new`,
  `AppState::new`), which an integration test binary cannot see.

## Open

- Whether the shared `wait_for` should be the existing production one in `trust`, promoted to a shared home, or a
  separate test-only copy — the production one has a non-test caller, so the two have different lifetimes.
- Whether the two stand-in live sessions become one fixture with an option for the reader thread, or stay two
  fixtures sharing the PTY setup underneath.

## Milestones

1. [A shared scratch directory for the daemon's tests](01-shared-scratch-directory.md)
2. [Shared app state, stand-in session and wait helpers](02-shared-state-and-session-fixtures.md)
