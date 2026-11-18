# Writing daemon tests

The rules for daemon tests (one shared dispatcher executable, `PATIENCE`, windows ending on test events, never proving
absence by a window, signal-handling fixtures, files written by rename, markers, seeded models, no real agent CLI and no
live data directory, weak state, lowered budgets, permissions, cgroup and reftable skips, the paused clock, free ports)
are in `docs/conventions/tests.md`, "Daemon tests". What follows is how to carry some of them out, and how to read a
failure before changing a timeout.

## A test that fails only in a parallel run is the machine's exec cost or stalls first, not a short timeout

- A fresh executable per test: macOS evaluates a new executable's code signature on its first `exec` and serialises that
  across processes, so fresh scripts start some 80 ms apart under parallel runs. It reads as a too-short timeout in the
  code under test, and `--test-threads=1` apparently fixing it reinforces that reading. This costs tens of milliseconds
  per script; delays of whole seconds that hit every exec are the next cause.
- Endpoint-security stalls: failures only under a parallel run, a run far slower than usual, `--test-threads=1` passing,
  and a trivial exec (`/usr/bin/true`) timed in a loop alongside the suite stalling for seconds in the same windows.
  That is the machine: do not raise production timeouts or serialise the suite over it.
- A test that fails only when run as root is missing the permissions early return; it is not a regression in the change
  being verified.

## Techniques the rules call for

- A `#[cfg(test)]` counter for work handed to `spawn_blocking` or another thread is incremented before the handoff, so
  it is final once the call under test returns (`holder_signal_attempts` in `apps/daemon/src/holder/client/mod.rs`). To
  show a call is held back, wait for a count of callers at its wait (`waiting` in
  `apps/daemon/src/accounts/shared_config/cli_reads.rs`).
- To make sure the work has ended before asserting a non-timeout outcome, split the production function so the test gets
  the worker's handle (`asked` and `answer_within` in `apps/daemon/src/platform/open/paths.rs`).
- On a paused clock, let the clock run the code's own timers (`tasks_idle`, or a helper of the module's like `polls_ran`
  in `apps/daemon/src/adapter/claude/transcript_watch.rs`) and assert after it.
- A `thread_local!` override reaches only the test's own thread: the browse fixtures' `serve` calls in directly, while
  anything moved to `spawn_blocking` or sent through the coordinator reads the real figure.
- A test of what a socket serves past an ancestor-permission check serves its router on a loopback port, or binds the
  listener on a scratch path directly.
- `test_support::offer_models` seeds the default account only; a test that gives a model under another account seeds
  that account's own list.
- Signal fixtures: Linux delivers a signal that macOS loses when a handler is installed and then unblocked, so a pass on
  Linux proves nothing; check on macOS.
