# Daemon conventions

How Rust code in the daemon (`apps/daemon/`) is written in general: module layering, where state lives, the store,
timing and concurrency, and agent accounts. Processes, blocking work, paths and files, git and writing text into a
session are in [`daemon-processes-and-io.md`](daemon-processes-and-io.md); where agent-specific code goes is in
[`provider-neutral-core.md`](provider-neutral-core.md); wire types, settings and MCP tools are in
[`protocol-and-contracts.md`](protocol-and-contracts.md); formatting and lint are in [`tooling.md`](tooling.md). Where a
file sits and which directory owns what is in [`docs/structure/daemon.md`](../structure/daemon.md).

## Modules and entry points

- **Every top-level module sits in one layer and uses only its own and lower layers.** An upward edge is added to
  `UPWARD` and to the allow-list table in `docs/structure/daemon.md`, "Allowed upward edges", together, with its reason.
  Enforced by `layering_tests::every_top_level_module_depends_only_downward_or_as_listed`, which also fails on a `.rs`
  file that no `mod` chain from `main.rs` reaches and on a module in no layer.
- **`platform` uses nothing else in the crate.** It is the foundation every host-specific thing hangs from; the same
  layering test fails on an edge out of it.
- **A per-host alternative module is declared `mod x_stub;` and renamed with `use x_stub as x;` under `cfg`, never
  `#[path]`.** The scan finds a file only as `<name>.rs` or `<name>/mod.rs`, so a `#[path]` file is reported unreached
  on every OS (`src/platform/proc/mod.rs` does it for `cgroup_stub`).
- **The daemon is reached only through the WebSocket/HTTP protocol in `apps/daemon/PROTOCOL.md`.** No other client path
  exists; an operation another end needs becomes a protocol request (see `docs/conventions/protocol-and-contracts.md`).
- **Use the shared entry point for what it covers; do not write a second implementation.** The list, with what each one
  is for, is `docs/structure/daemon.md`, "Shared entry points" (git, bounded processes and reads, loopback calls,
  request guards, account CLI runs, the user's shell environment, time, text trimming, terminal escapes, tags, delivery
  and notice wording, session naming, bound-session walks, provider facts). Where the list lacks one you need, add it
  there in the same change. Four of them carry a rule beyond "use it":
  - **An operation that changes or ends a terminal, or removes what one works in, takes `AppState::admit_change` at its
    shared entry point**, before it changes a record or the disk, and holds it until done, so a planned stop never
    interleaves with it.
  - **A live session is dropped only through `coordinator::exit_watch::unregister_live`, and a finished session is
    archived only through `coordinator::lifecycle::archive_finished`.** Each does the surrounding bookkeeping (the
    withdrawal of the session's request, the finish rule) that a direct `remove_live` or archive write skips.
  - **Whether a session may be handed off is asked through `refuse_unless_it_can_be_handed_off`**, so the control
    request and the MCP tool refuse alike.
  - **A session's launch overrides are written through `Store::set_session_launch*` and published through
    `AppState::set_session_launch`**, the one guarded writer and the one route to clients.
- **Wording that goes to an owner or into a model-facing message lives in `messaging/`.** Notices to an owner, delivery
  outcomes, session names and takeover text come from `owner_notices.rs`, `Delivery::note`, `naming.rs` and `texts.rs`,
  not from a string built at the call site; one place keeps the wording consistent and testable.
- **Serialise adding projects to a console with `AppState::claim_project_association`, and share the steps of the two
  transfer tools through `Parties` in `mcp/tools/transfer/mod.rs`.** Two paths with their own copies drift apart.

## State

- **A state a session can rest in after its process is gone (paused, done, interrupted) is stored in a column of the
  `sessions` row**, never held on `LiveSession` or in a map on `AppState`; process memory loses it with the process and
  a resume then shows another state. Only a state the process ending always moves the session out of (working, a live
  waiting-for-user prompt, dormant, background tasks, an account switch under way) may live in memory.
- **A state that ends with the process but is not reported again after an adoption (the background tasks) has a copy
  outside memory, written on every change and loaded right after `begin_adoption`**, not per session. Keep it hidden
  from clients and from persistence until the adoption confirms the same launch, and drop it if the launch differs; the
  holders' reports can arrive before a session's adoption and a copy seeded there overwrites them with older state.
- **A state that follows from how a turn ended is set by every write that records the process ending, not by the hook
  alone.** Judge it on the agent's own status at that moment (the stored one may be held by something else), using the
  stored status only where nothing else is left to judge by (a daemon that went down without stopping its sessions), and
  keep it revisable by a late end of the same turn, told apart by its turn id, since a hook the agent does not wait for
  can arrive after the exit. A process killed mid-turn fires no hook.
- **A session row is never written back whole from a copy read earlier; each change writes only its own columns.**
  Hooks, renames, archives, the exit watcher and account switches change it on tasks of their own, and a whole-row
  write undoes theirs. A change of status that must not undo another is a conditional write: a status the agent reports
  never moves a session out of `interrupted` or `archived`, and the process ending never moves it out of `archived`.
- **Every write that sets or clears `Session.bound_to` writes `Session.role` with it**, since capability checks read
  the stored role (`Session::is_worker()`); a console session and a session with no parent are always coordinators.
- **Per-entity state the daemon derives from the outside world and does not persist is held in its own map on `AppState`
  (`src/state/mod.rs`), never in a column of the `Console`, `Project` or `Session` row.** How it reaches clients is in
  `docs/conventions/protocol-and-contracts.md`, "The envelope and field conventions".
- **The daemon drops a derived-state map entry wherever the record is removed**, and with the event shape the client
  reducer also drops it on the `*_deleted` event, cascading from a console to its projects. Nothing deletes it
  otherwise, so the cleanup is written by hand.

## Store

- **A store method that can write more than one row opens one `conn.transaction()` for the lot, with its dependent reads
  inside it.** The store is one `Connection` behind a mutex and each statement outside a transaction is its own
  journalled fsync pair while every other caller waits; a read-modify-write split across two takes of the mutex also
  loses a concurrent caller's write. `Store::apply_branch_origins` is the shape.

## Timing and concurrency

- **Work that only matters while a client shows something is a one-shot protocol request with no daemon timer.** The
  client owns the interval and asks again on every fresh `snapshot`; the daemon bounds the work itself with a per-entity
  floor on how recently it last completed, since several clients each run the interval on their own phase and an
  in-flight claim alone never fires.
- **An in-flight claim for such work is a guard released on `Drop`, never a `begin`/`end` pair**, since the work can
  panic and the entity would stay excluded for the daemon's life. A wait on the claim to share one computation is
  bounded and answers the last result noted, or fails; it never answers an empty result, which a client shows as
  "nothing".
- **A remembered result is not noted if what clears it ran since the work began.** Read a generation counter before the
  work, advance it in the clearing, and drop the note when it changed (`coordinator::processes::list_all` with
  `ProcessLedger::lists_generation`).
- **A period of hours or days that the daemon schedules for itself is counted on the wall clock.** Wake hourly, compare
  wall time with the last run, treat a clock gone backwards as due and run once at start; a `tokio::time::interval`,
  `sleep` or `Instant` of that length does not advance while the machine sleeps and fires days late
  (`coordinator/archive_cleanup.rs` is the shape).
- **A worker that waits for a shared condition (a bound reached, a count dropping) holds nothing that counts toward
  that condition while it waits.** What it holds is handed over first, or it does not wait: a condition only the
  waiters can satisfy is satisfied only by the timeout.
- **A check made early in a request handler or daemon task is stale by the time it acts.** Every control request runs on
  a task of its own on a multi-threaded runtime, so another request can run between any two lines. Where acting on it
  would do harm, take a claim on `AppState` that the competing path checks too (`begin_launch`;
  `begin_launch_leaving_recoveries` where the operation may still refuse and must not cancel pending recoveries first),
  or redo the check after the long step.
- **Under a claim, every check made before it is repeated and what decided it re-read.** That includes the caller's (an
  MCP tool's check that the session is its own) and each member of a group, not only one row. The check for a
  destructive act is made after the last wait, and the writes that go with the operation are made under the claim, after
  its checks, and put back on each failure path.
- **A wait on something that may never come (polling a record, a sign-in ending) runs before the claim or lock is taken,
  with one confirming read under it.** Work started from an event that will not come again waits for a claim with a
  bound and re-reads, instead of returning, or the event is lost; the refusal that suits a request the user can repeat
  does not suit it.

## Agent accounts

- **A new reason an account cannot be used is checked at every place that picks or accepts an account, in the same
  change.** The places: the open and switch refusals, automatic choice, the MCP fallback and its descriptions,
  `list_agents`'s `can_open` and `cannot_open_reason`, the quota kick, the UI `accounts/` rules and every picker, and
  the gallery's fixture daemon. A check at only some of them leaves the others putting work on the account or offering
  it unmarked.
- **A reading that only labelled a state and becomes a reason an account cannot be used is re-measured against every
  sign-in kind the agent supports first.** Gate only on a reply that tells the kinds apart, and count a reply that
  matches no measured marker as unknown, not as the refusing reason (an API-key or custom-provider account answers a
  quota probe like a signed-out one and still works). What was measured is in `docs/agent-cli-reference.md`, "Signing in
  a config directory without a terminal".
