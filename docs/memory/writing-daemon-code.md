# Writing daemon code

## Live state the daemon derives goes on `AppState`, never into a stored record

A piece of per-entity state the daemon reads off the outside world and does not persist — a project's git status, a
repository's branch, anything recomputed rather than written by the user — is held in its own map on `AppState`
(`apps/daemon/src/state.rs`), never in a column of the `Console`, `Project` or `Session` row. It reaches clients in
one of two shapes:

- An event of its own carrying the whole derived record, replayed inside `snapshot` so a reconnecting client never has
  to ask for it separately. This is the shape for anything about a console or a project; the git statuses are the
  worked example, and the trust prompts are the same idea with the repeat sent after each `snapshot` rather than
  inside it.
- For state about one session, a field on `Session` that the store never reads or writes, filled in by
  `AppState::wire_session`, through which every session record a client receives passes: `AppState::publish_session`
  (the only sender of `session_upserted`, which re-reads the stored row first), the coordinator's `session_opened`
  responses, and `snapshot`. `background_tasks` and `default_effort` are this shape: change the map, then publish the
  session. Any new path that hands a client a `Session` goes through `wire_session` as well.

What is wrong is a field the record carries but some write path has to fill. Every `*_upserted` event carries the
whole record (`apps/daemon/PROTOCOL.md`, section "Daemon to client"), so such a field is blanked in the client the
moment an upsert built without it is applied — for state that changes on a schedule of its own, that means every write
path in the coordinator. `Console` and `Project` have no `wire_session` counterpart, so they get the event shape.

Neither shape gets a deletion of its own, so the cleanups are written by hand: the daemon drops its map entry wherever
the record itself is removed, and with the event shape the client's reducer also drops it on that record's
`*_deleted` event, cascading from a console down to each of its projects.

## A store write outside a transaction is a journalled pair of fsyncs under the one lock

`Store::open` (`apps/daemon/src/store.rs`) sets only `foreign_keys`: `journal_mode` and `synchronous` keep SQLite's
defaults, and the store is a single `Connection` behind a mutex that every method takes. So each statement run outside
a transaction commits on its own — a rollback journal written and fsynced, the pages written and fsynced, the journal
removed — while every other caller of the store waits on the mutex for it. A loop calling a one-row method is
therefore the wrong shape wherever the number of rows follows a repository, a listing or anything else rather than
the single thing the user did: reconciling a repository's branch records writes up to a thousand rows from inside a
browse slot.

So a store method that can write more than one row opens one `conn.transaction()` for the lot. When those writes
follow from a read of the same rows, or the caller needs what the rows hold afterwards, do both reads inside that
transaction instead of around the call: a method of this kind is reached by callers that run at the same time — the
branch records from the `POST /git-ref/:session` route, which any number of a session's background reporter
processes post to at once and which records each on the blocking pool, and from the Git mode's branch listing, a
repeating refresh each connected client runs on its own phase — so a read-modify-write split across two takes of the
mutex gets another caller's writes in between and one of the two is lost. `Store::apply_branch_origins` is the shape
to copy.

## Time a repeating refresh from the client, and bound it in the daemon

Work that only matters while a client is showing something — the projects of the console in the sidebar, a list in
view — gets a one-shot request in the protocol and no timer in the daemon, which cannot know what any window is
showing. The client owns the interval for as long as it keeps showing that thing, and asks once more on every fresh
`snapshot`, since the daemon may have restarted and lost whatever it was holding in memory.

The daemon then has to bound that work itself, and an in-flight claim is not enough. Several clients can be connected
at once — another window, the UI in a plain browser, any local program — each running the same interval on its own
phase, so their requests interleave rather than overlap and the claim never fires; N clients turn one unit of work
per interval into N. Add a floor per entity on how recently the work last completed, short enough that a refresh the
user's own action asks for still does something.

A claim like that is a guard released on `Drop`, never a `begin`/`end` pair: the work runs on a blocking task that
can panic, and an entity whose claim is never released is excluded from every later refresh for the rest of the
daemon's life.

## Count a long period of the daemon's own on the wall clock, not on a timer of that length

Applies to work the daemon schedules for itself every few hours or days, whatever any client shows. A
`tokio::time::interval` or `sleep` (and `std::time::Instant`) runs on a clock that does not advance while the machine
sleeps — on macOS, and on Linux across a suspend — so a 24-hour timer on a laptop that sleeps every night fires days
late. Wake on a short period instead (an hour), compare the wall-clock time against when the work last ran, and treat
a wall clock that went backwards past that point as due. Run the work once at start too, which covers the time the
daemon was not running without keeping the last run across restarts.

## Cap how many blocking threads a per-entity pass starts at once

Applies to daemon work that runs one blocking task per project, per directory or per session — a sweep over a console's
projects, a reconciliation at daemon start, a read of every directory under a parent being associated. One
`tokio::task::spawn_blocking` per item is wrong however cheap the item looks: that pool is one pool for the whole
daemon, and every user-visible path shares it — opening a session, answering a trust prompt, a browse read — so
associating a parent directory that holds a few hundred repositories fills it and the user's next action waits behind
the slowest item. Such a task usually also waits on `env_shell::cached_snapshot()`'s single lock, held for a whole
login-shell fork while the cache is cold, so the threads past the first few are not even doing work in parallel.

So run the items through a bounded runner instead — a `JoinSet` held at a fixed width, starting the next item as soon
as any one finishes — and give each item a deadline of its own, so one that stalls holds a single place rather than
the pass. Four at a time is this daemon's figure everywhere it does this.

## Keep process starts off every path that runs per launch, per hook event or per sweep item

Applies to daemon code that would start a process — a `--version` or `--help` probe, a login shell, a hook forwarder,
a `git` per project — on a path that runs for every session launch, every hook event, or every item of a periodic pass.
On a Mac running endpoint-security software, as managed machines do, each exec can stall for seconds (and a login-shell
snapshot is a serial chain of them), so a process that costs milliseconds on a development
machine is what the user waits on, multiplied by the number of sessions.

So look for a route with no process first: read what is wanted from where it is installed or stored (as
`AgentAdapter::installed_version` reads an agent's version without running it), serve a cached result (the
`env_shell` snapshots, never a fresh login shell per call), or let the other side call the daemon (a Claude Code that
takes HTTP hooks posts to the hook route with no forwarder process behind it). Where a process cannot be avoided, run
it off the path the user waits on, cache what it returns, and bound how many run at once.

## Ending everything an agent CLI started takes its session, not its process group

Applies to daemon code that runs an agent CLI and has to end whatever it left running — when it exits, when it is
stopped, when the daemon goes down. Agent CLIs move some helpers (a REPL or code-execution host, a background shell
task) into process groups of their own, so the process-group stop other subprocesses get leaves them running; they
keep the CLI's session. Do not write a sweep of your own: run the CLI through `apps/daemon/src/subprocess.rs` with
`Reach::Session` (`run_agent_with_timeout`, or `Bounds { reach: Reach::Session, .. }` for `run_bounded` and
`run_exchange`), and where it cannot go through there (it runs on a PTY, or a flow reads it while it runs), build on
`apps/daemon/src/process_session.rs`. Anything that is not an agent CLI, such as `git`, keeps `Reach::ProcessGroup`
(`run_with_timeout`): a session sweep lists every process on the system.

`Reach::Session` sweeps the session when the CLI ends by itself too. Where the CLI can start something the user goes
on using — a login that opens a browser, which `xdg-open` on a Linux desktop starts straight into the CLI's session —
sweep only when the daemon ends the CLI (cancel, timeout, daemon exit) and just reap one that exits by itself, as the
account login flows in `apps/daemon/src/account_login/flows.rs` do.

## A request handler or daemon task is never alone with the state: re-check after a step, or hold a claim

Every control request is handled on a task of its own (`apps/daemon/src/server.rs`) on a multi-threaded runtime
(`apps/daemon/src/main.rs`), so any other request — another client's, or the same client's next one — can run between
any two lines of a handler, not only across an `.await`; a background task the daemon runs on its own is in the same
position. Making a step synchronous, or dropping an `.await` from it, gives the code no exclusivity. A check made
early (no session running in this project, nothing else using this path, this session still archived and unpinned)
can therefore be stale by the time the code acts on it.

Where acting on a stale check would do harm, either take a claim on `AppState` that the competing path checks too —
`AppState::begin_launch` is the existing one — or redo the check after the long step and deal with what changed. A
claim covers only the checks made once it is held: one made before it — including one the caller made before calling
the operation that takes the claim, such as an MCP tool checking that the session asked about is the caller's — is
repeated inside the operation under the claim, with whatever it needs passed in. When the judging ran before the claim
was taken, re-read everything that decided it under the claim — each member of a group and its standing, not only the
one row — and back out if any of it changed. A claim is not free to hold: `AppState::begin_switch` and its kin
refuse every other holder (a handoff, a handover, a binding change, a resume) for as long as the guard lives, and an
account switch's claim shows the session as switching to every client. So a wait on something that may never come —
polling a record for what an agent has yet to write — runs before the claim is taken, and only one confirming read of
it runs under the claim. The same goes for what the operation writes on the way: a write that
goes with it (the model and effort an account switch sets) is made under the claim, after its checks, and put back on
each of its failure paths, since one made before the claim is read by whatever launches in between. Put
blocking filesystem or process work in a handler on `tokio::task::spawn_blocking` and await it; do not use
`block_in_place`, which panics under the current-thread runtime plain `#[tokio::test]` gives the coordinator tests.

## On macOS, reach the system through APIs that need no consent, never through Apple Events

Applies to host integrations in the daemon and in the desktop shell: moving to the Trash, revealing or opening
something in Finder, driving Terminal or any other app. The app is signed with the hardened runtime
(`apps/desktop/src-tauri/tauri.conf.json`), and the daemon is its sidecar signed the same way, with no Apple Events
entitlement and no `NSAppleEventsUsageDescription` in `apps/desktop/src-tauri/Info.plist`. So anything that sends
Apple Events — `osascript`, AppleScript, Scripting Bridge — fails or waits on an Automation prompt in the shipped app.

Development hides this: a daemon started from a terminal has the consent asked of, and usually already granted to,
that terminal, so the call works there. Check what a crate does on macOS before adopting it, since some default to
Finder through `osascript` (the `trash` crate's default `DeleteMethod::Finder`), and pick the consent-free path
instead: a Foundation API such as `NSFileManager`, or `/usr/bin/open`.

## Lint the Rust crates with `cargo clippy --all-targets`

Plain `cargo clippy` checks only the lib and bin targets, and every test in these crates sits in a `#[cfg(test)]`
module (there is no `tests/` directory), so it lints none of the test code — and most changes here add more test code
than production code. A clean run without `--all-targets` is therefore no evidence about the tests just written, and a
warning in them surfaces much later, whenever something else makes that target rebuild. Do not read that delay as
cargo withholding cached warnings: warnings are replayed for an unchanged target, so a second run does repeat them,
and neither `cargo clean` nor touching a file is the fix.

## Run the tests of Linux-only code, and of how a PTY ends, in a Linux container

Applies when you are developing on macOS (the daemon is meant to run on a Linux host too) and change either kind of
daemon code below, since a clean run on macOS says nothing about it:

- Code under `#[cfg(target_os = "linux")]`. `cargo check`, `cargo clippy` and the test suite on macOS skip it
  entirely, and `--target x86_64-unknown-linux-gnu` from macOS fails in rusqlite's `bundled` build script without a
  Linux C cross-compiler.
- How a terminal's PTY comes to an end — the reader thread, closing the master, hanging the terminal up
  (`apps/daemon/src/pty.rs`). On macOS a session leader's exit revokes its controlling terminal, so the reader ends
  with the shell or agent even while a background job it left behind still holds the PTY's other side; Linux revokes
  nothing, and the reader, the master and the buffers live on until that job goes. A test of a terminal outliving its
  process therefore cannot fail on macOS.

So run `cargo test -p octoboardd`, and `cargo clippy --all-targets` for the Linux-only code, inside a Linux container
(Docker). Start the container with `docker run --init`: without an init that reaps, orphans reparented to PID 1 (here
cargo) stay zombies, and every test that waits for a process to be gone (`test_support::assert_gone_by`, which waits for
`ESRCH`) fails, on `main` as on the change.

To exercise a fallback for a refused system call (the browse reads' `openat2` refused as `ENOSYS` or `EPERM`), run
the container with a seccomp profile that returns that errno for the call.

Without a container runtime, Linux-only code gets a compile check only: copy the Linux-only functions into a scratch
crate outside the repository that depends only on what they use (typically `libc`), and run
`cargo check --target x86_64-unknown-linux-gnu` there after `rustup target add x86_64-unknown-linux-gnu`; say when
reporting that it proves it compiles, not how it behaves. How a PTY ends has no such fallback: report it as
unverified on Linux.
