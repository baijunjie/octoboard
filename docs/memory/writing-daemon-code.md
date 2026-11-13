# Writing daemon code

## A state a session can rest in after its process is gone is stored, never held in memory

Before choosing where a session's state or flag lives, ask whether the session can still be in that state once its
process has stopped — paused (last turn ended abnormally), done, interrupted. Such a state goes in a column of the
`sessions` row, so it reads the same after the process ends, the daemon restarts or the session is resumed; holding it
on `LiveSession` or a map on `AppState` loses it with the process, and a resume shows another state. Only a state the
process ending always moves the session out of — working, a live waiting-for-user prompt, dormant, background tasks,
an account switch under way — may live in process memory. (Source: user instruction)

Process memory is not the same as the daemon's memory: a daemon that restarts, planned or after a crash, loses
everything it held without ending any session's process, since the process holders keep them running and the next daemon
adopts them. A state that ends with the process but is not reported again after an adoption — the background tasks are
one: the agent lists them only at its next turn end or subagent stop — needs a copy outside memory, written on every
change (`apps/daemon/src/state/background_store.rs`); without it the adopted session reads as another status until the
agent next says something. Load that copy into memory at daemon start, right after `begin_adoption`, not when each
session is adopted: the reports the holders kept over the restart carry only the session id and are released after at
most the relay's adoption wait, so they can arrive before a session's adoption, and a copy seeded there overwrites them
with older state. Until the adoption confirms the session runs the launch the copy was saved for, keep it hidden from
clients and from what is persisted, and drop it if the launch differs.

When such a state follows from how a turn ended, do not rely on the agent's hook for it alone: a process killed
mid-turn fires no hook, so every write that records the process ending has to set it too. Judge it on the agent's own
status at that moment, not the stored one, which something other than the agent can hold (a waiting session request
holds it at `waiting_user` over an agent at its prompt); fall back to the stored status only where nothing else is left
to judge by (a daemon that went down without stopping its sessions). Nor is the process end final for that turn: a
hook the agent does not wait for (all of Codex's, Claude Code's command-form hooks) can arrive after the exit, so what
the end decided must stay revisable by a late end of the same turn, told apart by the turn id it carries.

## Live state the daemon derives goes on `AppState`, never into a stored record

A piece of per-entity state the daemon reads off the outside world, does not persist, and that ends with the process
or is recomputed — a project's git status, a repository's branch, anything recomputed rather than written by the
user — is held in its own map on `AppState` (`apps/daemon/src/state/mod.rs`), never in a column of the `Console`,
`Project` or `Session` row. It reaches clients in one of two shapes:

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
path in the coordinator. `Console` and `Project` do have a single route to clients, `AppState::publish_console` and
`AppState::publish_project` (the only senders of `console_upserted` and `project_upserted`, which re-read the stored
row under the publication lock and stamp its field versions; `snapshot` stamps them too), but that route exists for the
versions and fills in no derived state, so state about a console or a project still gets the event shape.

Neither shape gets a deletion of its own, so the cleanups are written by hand: the daemon drops its map entry wherever
the record itself is removed, and with the event shape the client's reducer also drops it on that record's
`*_deleted` event, cascading from a console down to each of its projects.

## A store write outside a transaction is a journalled pair of fsyncs under the one lock

`Store::open` (`apps/daemon/src/store/mod.rs`) sets only `foreign_keys`: `journal_mode` and `synchronous` keep SQLite's
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
mutex gets another caller's writes in between and one of the two is lost. `Store::apply_branch_origins`
(`apps/daemon/src/store/branch_origins.rs`) is the shape to copy.

## Raise a version and write its migration only between released versions

Applies to the three versions the daemon owns: the protocol generation (`GENERATION` in
`apps/daemon/src/protocol/version.rs`, rendered as `PROTOCOL_GENERATION` into `packages/ui/src/protocol.generated.ts`),
the store's `SCHEMA_VERSION` with its `MIGRATIONS`, `schema/v{N}.sql` and `PAST_SCHEMAS`
(`apps/daemon/src/store/schema.rs`), and the configuration document's `CONFIGURATION_VERSION`
(`apps/daemon/src/protocol/configuration.rs`). The usual habit — every breaking change or schema change raises the
version and gets a migration — is wrong here. A version is raised, and a migration or upgrade step written, only to
bridge one released version to the next: a change to anything no release has carried, however large and however many
commits or plan milestones it spans, is made in place at the version it already has, with no bump, no step, no kept
past schema and no migration test. After a release, the first change to what it carried raises the version once, and
later changes before the next release fold into that same version and step.

Amending an unreleased step in place leaves behind any developer database an earlier build already brought to that
version: the store opens a database at its own `SCHEMA_VERSION` as it is, so it fails at the first read of what the
amendment added ("no such column"). That is expected, not a defect to fix in code: such a database is moved aside and
created afresh, never bridged by a bump, a second step or a guard that tolerates the missing column. Say so to the user
when the branch carrying the amendment replaces a build they have run.

A version is released only once a release tag (`desktop-v*`, the tags `.github/workflows/release.yml` builds) is on a
commit that carries it; check the remote's tags (`git ls-remote --tags origin 'desktop-v[0-9]*'`), not only the local
`git tag`. A prerelease tag such as `desktop-v0.1.0-beta.1` is a release like any other. Once a version is released, a
field or column removed, moved or renamed is bridged by the raised version and its step, never by keeping the old one
readable beside the new. What each version guards is in `apps/daemon/PROTOCOL.md`, "Versions and compatibility" and
the "Version" paragraph of "Configuration export", and in `docs/structure/daemon.md`, "Where new code goes". (Source:
user instruction)

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
daemon's life. Where overlapping requests wait on the claim to share one computation instead of skipping, that wait has
a bound, past which the request answers with the last result noted, however old, or fails (not with an empty result,
which a client shows as "nothing"): work stuck on a dead mount otherwise holds every later request behind it for good.

The result the floor remembers must not outlive what clears it. Where something the daemon does makes it wrong (a stop
forgets the remembered listings), a result computed from a read made before the clearing but noted after it brings the
old state back for the rest of the floor. Read a generation counter before the work reads anything, move it on in the
clearing, and drop the note when it has changed; `coordinator::processes::list_all` with
`ProcessLedger::lists_generation` is the shape.

## Count a long period of the daemon's own on the wall clock, not on a timer of that length

Applies to work the daemon schedules for itself every few hours or days, whatever any client shows. A
`tokio::time::interval` or `sleep` (and `std::time::Instant`) runs on a clock that does not advance while the machine
sleeps — on macOS, and on Linux across a suspend — so a 24-hour timer on a laptop that sleeps every night fires days
late. Wake on a short period instead (an hour), compare the wall-clock time against when the work last ran, and treat
a wall clock that went backwards past that point as due. Run the work once at start too, which covers the time the
daemon was not running without keeping the last run across restarts.

## A deadline that waits for a loopback refusal outlasts Windows's slow one

Applies to daemon code, a session helper included, that connects to a loopback port and falls back to another route
when the connection is refused. On Windows a connection to a closed `127.0.0.1` port is refused only after about 2.0 to
2.5 seconds, and past 3 seconds on a loaded host, where macOS and Linux refuse at once. A deadline shorter than that
times out first, so the fallback never runs and whatever the call carried is dropped. Give such a connect a deadline
well past that: git-ref's launch route, which nothing waits on, allows 6 seconds
(`apps/daemon/src/session_git_hooks/ref_mode.rs`).

## A request handler or daemon task is never alone with the state: re-check after a step, or hold a claim

Every control request is handled on a task of its own (`apps/daemon/src/server/control.rs`) on a multi-threaded runtime
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
refuse every other holder (a handoff, a transfer, a binding change, a resume) for as long as the guard lives, and an
account switch's claim shows the session as switching to every client. That refusal suits a request the user can
repeat; work the daemon starts from an event that will not come again — a turn that failed, a hook — and that finds
the session under a claim waits for the claim to end, with a bound, and then re-reads what it decided on, instead of
returning, or the event is lost. So a wait on something that may never come — polling a record for what an agent has
yet to write, a sign-in or other runs under the same directory ending — runs before the claim, or a lock such as
`shared_config::lock_agent`, is taken, and only one confirming read of it runs under the claim. Every wait makes the
checks before it stale again: where an operation waits more than once (for a lock, a sign-in to end, other runs to
finish), the check that decides a destructive act is made after the last wait, immediately before the act. The same goes for what the operation writes on the way: a write that
goes with it (the model and effort an account switch sets, the account a switch for a usage limit left) is made under
the claim, after its checks, and put back on each of its failure paths, since one made before the claim is read by
whatever launches in between, and one made after it by whatever was waiting for the claim. Put
blocking filesystem or process work in a handler on `tokio::task::spawn_blocking` and await it; do not use
`block_in_place`, which panics under the current-thread runtime plain `#[tokio::test]` gives the coordinator tests.

## A provider interface's default method claims nothing and does nothing

Applies to every default body of `AgentProvider` (`apps/daemon/src/adapter/mod.rs`) and of the per-provider traits it
hands out (`HookInterpreter`, `RecordStore`, `ScreenReader`, `AccountOps`, `ModelCatalogue`, `StatusLine`,
`CompatibilitySource`) or a launch plan carries (`TrustCarrier`), and of any added later. A provider that does not override a method gets its default silently — the test-only provider, one being
added, one that has nothing of the kind — so the default is the answer of a provider with none of the thing: it
recognises nothing, hints at nothing, proves nothing and acts on nothing. Never write the default as what the built-in
providers happen to share, least of all where the answer leads to a refusal or a deletion: a `RecordStore` default
that took a missing record as conclusive made every session of a provider whose records it cannot recognise count as
gone, so its resume was refused and an archived one's cleanup could delete it. Behavior several providers share goes
in a helper their overrides call (as `transcript_path_hint` in `apps/daemon/src/records/store.rs` does for the hook
hint Claude Code and Codex both carry), not in the default.

## On macOS, reach the system through APIs that need no consent, never through Apple Events

Applies to host integrations in the daemon and in the desktop shell: moving to the Trash, revealing or opening
something in Finder, driving Terminal or any other app. The shipped app carries the Apple Events entitlement and
`NSAppleEventsUsageDescription` only so that programs agents run in its terminals can ask for Automation consent (see
"Other consents programs ask for" in `docs/product/macos-privacy-permissions.md`), and macOS attributes whatever the
daemon (its sidecar) or the shell sends to Octoboard too. So anything that sends Apple Events — `osascript`,
AppleScript, Scripting Bridge — raises an Automation prompt naming Octoboard in the middle of an ordinary action,
worded for a program in a terminal, and fails if the user refuses; the only prompt Octoboard itself raises is the one
the user asks for from Settings' Permissions section.

Development hides this: a daemon started from a terminal has the consent asked of, and usually already granted to,
that terminal, so the call works there without a prompt. Check what a crate does on macOS before adopting it, since
some default to Finder through `osascript` (the `trash` crate's default `DeleteMethod::Finder`), and pick the
consent-free path instead: a Foundation API such as `NSFileManager`, or `/usr/bin/open`.

## Lint both Rust crates and their tests with warnings denied

`pnpm clippy:rust` runs `cargo clippy --workspace --all-targets -- -D warnings`, with the same sidecar override as
`pnpm test:rust`. Both package-scoped and release verification include it.

Plain `cargo clippy` checks only the lib and bin targets, and every test in these crates sits in a `#[cfg(test)]`
module (there is no `tests/` directory), so it lints none of the test code — and most changes here add more test code
than production code. A clean run without `--all-targets` is therefore no evidence about the tests just written, and a
warning in them surfaces much later, whenever something else makes that target rebuild. Do not read that delay as
cargo withholding cached warnings: warnings are replayed for an unchanged target, so a second run does repeat them,
and neither `cargo clean` nor touching a file is the fix.
