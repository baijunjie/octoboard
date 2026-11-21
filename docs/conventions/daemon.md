# Daemon conventions

How Rust code in the daemon (`apps/daemon/`) is written in general: modules, state, the store, timing and concurrency,
and agent accounts. Processes, paths, git and writing into a session are in
[`daemon-processes-and-io.md`](daemon-processes-and-io.md); where agent-specific code goes is in
[`provider-neutral-core.md`](provider-neutral-core.md); wire types, settings and MCP tools are in
[`protocol-and-contracts.md`](protocol-and-contracts.md). Where a file sits, the layers and the shared entry points are
in [`docs/structure/daemon.md`](../structure/daemon.md).

## General principles

- **State that outlives the process is a store column, and each write touches only its own columns.** Process memory
  loses what it holds with the process, and a whole-row write undoes the writes other tasks made meanwhile.
- **Every check that matters is made again where the act happens: under the claim, or after the long step.** Every
  request runs on a task of its own on a multi-threaded runtime, so a check made earlier is stale by the time it acts.

## Modules and entry points

- **An upward edge between top-level modules is added to `UPWARD` and to the table in `docs/structure/daemon.md`,
  "Allowed upward edges", together, with its reason.** The layering test checks `UPWARD` against the code, not the
  table, so a reason left out of the table is lost to the next reader.
- **An operation that changes or ends a terminal, or removes what one works in, takes `AppState::admit_change` at its
  shared entry point**, before it changes a record or the disk, and holds it until done, so a planned stop never
  interleaves with it.
- **A live session is dropped only through `coordinator::exit_watch::unregister_live`, and a finished session is
  archived only through `coordinator::lifecycle::archive_finished`.** Each does the surrounding bookkeeping (the
  withdrawal of the session's request, the finish rule) that a direct `remove_live` or archive write skips.
- **Wording that goes to an owner or into a model-facing message lives in `messaging/`.** Notices to an owner, delivery
  outcomes, session names and takeover text come from `owner_notices.rs`, `Delivery::note`, `naming.rs` and `texts.rs`,
  not from a string built at the call site; one place keeps the wording consistent and testable.

## State

- **A state a session can rest in after its process is gone (paused, done, interrupted) is stored in a column of the
  `sessions` row**, never held on `LiveSession` or in a map on `AppState`; a resume would otherwise show another state.
  Only a state the process ending always moves the session out of (working, a live waiting-for-user prompt, dormant,
  background tasks, an account switch under way) may live in memory.
- **A state that follows from how a turn ended is set by every write that records the process ending, not by the hook
  alone.** Judge it on the agent's own status at that moment (the stored one may be held by something else), using the
  stored status only where nothing else is left to judge by (a daemon that went down without stopping its sessions), and
  keep it revisable by a late end of the same turn, told apart by its turn id, since a hook the agent does not wait for
  can arrive after the exit. A process killed mid-turn fires no hook.
- **State the daemon writes outside a hook (a status it reads off the screen, the hand coming down once a published
  interaction leaves the screen) never invalidates hook logic armed on the state before it.** What a hook armed (a
  turn it waits to end, a prompt it waits to be answered) keeps the identity it was armed with, and the daemon's own
  write neither clears it nor makes a later hook of the same turn look stale.
- **A session row is never written back whole from a copy read earlier; each change writes only its own columns.**
  Hooks, renames, archives, the exit watcher and account switches change it on tasks of their own. A change of status
  that must not undo another is a conditional write: a status the agent reports never moves a session out of
  `interrupted` or `archived`, and the process ending never moves it out of `archived`.
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
- **A `subscribe_X` push stream is the one allowed exception: the daemon works on its own while a connection is
  subscribed**, provided it works only while some connection is subscribed, its subscribers share one read (one task
  per followed entity, not one per connection), and the subscriptions are limited per connection
  (`subscribe_session_conversation`: `AppState::conversation_feeds`, `conversation_feed::SUBSCRIPTION_LIMIT`).
- **Lag recovery of a per-connection push takes the replacement page and the new receiver together, under the
  publisher's lock** (`FeedChannel::resubscribe`), so nothing published between the two is lost or sent before the
  page; **and a reply that supersedes a forwarding task waits for that task, once aborted, to end before it is sent**
  (`Subscription::end`), so nothing the task forwarded arrives after the reply.
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
- **Claims.** Where acting on a stale check would do harm, the act takes a claim on `AppState` that the competing path
  checks too (`begin_launch`; `begin_switch` where the operation may still refuse and must not cancel pending recoveries
  first, or needs the session while its process still runs), or redoes the check after the long step.
  - **Under the claim, every check made before it is repeated and what decided it re-read**: the caller's too (an MCP
    tool's check that the session is its own) and each member of a group, not only one row. The check for a
    destructive act is made after the last wait, and the writes that go with the operation are made under the claim,
    after its checks, and put back on each failure path.
  - **A wait on something that may never come (polling a record, a sign-in ending) runs before the claim or lock is
    taken, with one confirming read under it.**
  - **Work started from an event that will not come again waits for a claim with a bound and re-reads, instead of
    returning**, or the event is lost; the refusal that suits a request the user can repeat does not suit it.
- **Provider code that parses an agent's records never runs inside a lock that request handling also takes.** A
  provider's reader works through files of any size and shape the agent wrote; under such a lock one slow or
  pathological record stalls every request that needs it. Read and parse outside, and take the lock only to put the
  result in place.

## Agent accounts

- **A new reason an account cannot be used is checked at every place that picks or accepts an account, in the same
  change**: the daemon's refusals and automatic choices, what agents are told before they pick, the work that spends an
  account, and every UI picker and the fixture daemon. A check at only some of them leaves the others putting work on
  the account or offering it unmarked.
