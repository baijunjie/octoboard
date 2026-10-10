> Severity: Moderate

## Symptom

A panic anywhere in the daemon kills every running agent process, including a panic in a single task or thread that
the daemon itself survives, so the daemon keeps running while all its sessions have lost their processes.

## Reproduction steps

1. Start the daemon with the panic hook installed (`state::install_panic_hook`, called at daemon start) and at least
   one running session registered for cleanup.
2. In a development build, put a temporary `panic!` in the Git check that `spawn_check` runs on a blocking thread — a
   panic the daemon is designed to recover from (see the test
   `a_git_check_claim_is_released_even_if_the_check_panics` in `apps/daemon/src/state.rs`).
3. Note the PIDs of the running sessions' agent processes and of the daemon (`ps`).
4. Trigger a Git check with **Sync repository** in a git project's action menu.
5. Compare the PIDs with step 3: the agents are gone, the daemon is still running.

These steps follow from the code and have not been run; no panic has been seen in the app.

## Expected vs. actual

- Expected: a panic the daemon recovers from leaves running sessions alone; agents are ended only when the daemon goes
  down. This rests on `docs/product/application-lifecycle.md`, "Crashes and forced termination" ("Agent processes do
  not outlive the daemon: it ends them on its way out, … when it goes down on an internal failure"), and on the panic
  hook's own stated purpose ("so a daemon crash never leaves agents running unattended").
- Actual: every registered agent process is killed outright on the panic, and the daemon carries on.

## Environment

macOS, `main` at 265d24d. Applies to every agent; the daemon runs a multi-threaded tokio runtime with the default
panic strategy (unwind).

## Scope of impact

Every running session of every console loses its process at once, mid-turn, whenever any recoverable panic happens.
Workaround: resume each session by hand; work in flight in the killed turns is lost.

## Leads

- Verified (code): the hook calls `cleanup::kill_all()` unconditionally before the default hook, on every panic,
  whatever thread or task raised it (`install_panic_hook` in `apps/daemon/src/state.rs`).
- Verified (code): none of the three Cargo manifests (the workspace root, `apps/daemon`, `apps/desktop/src-tauri`)
  sets `panic = "abort"`, so a panic unwinds and the daemon can survive it.
- Verified (code): a recoverable panic is anticipated elsewhere — a panicking Git check releases its claim "for the
  next sweep", which only matters if the daemon keeps running.
- Inferred: tokio catches a panic inside a spawned task and keeps the runtime running, and a panic in a connection's
  handler ends only that connection, so such panics do not bring the daemon down.
- Inferred: the session records of the killed agents are then updated by the daemon's ordinary process-exit handling;
  what status they end up with was not checked.

## Acceptance criteria

- [ ] A panic in a spawned task or background thread that the daemon survives leaves every running agent process
  alive.
- [ ] A panic that does bring the daemon down still ends every agent process before it exits.
- [ ] A test covers both cases.
