# Test conventions

How unit and integration tests are written in every end, and the code-level rules for the UI state gallery and its
fixtures. How a test is written from the requirement is in the `agent-unit-test` skill and the `test-writer` agent
definition; which test helper to use for what is in the "Shared entry points" tables of the structure docs
([`../structure/ui.md`](../structure/ui.md), [`../structure/daemon.md`](../structure/daemon.md)); how to write and run
gallery scenarios is in `packages/ui/src/gallery/README.md`.

## General principles

- **A test checks the required behaviour,** from outside the unit, never the implementation it happens to have.
- **A test's outcome never depends on how fast the machine runs it.** A test that passes alone and fails under the
  suite's load is broken, not intermittent; rerunning it alone only lets a merge go ahead and does not clear it. Time is
  driven by a fake clock started before the code arms its timers, and durations come from the production constant
  ("Time in tests").
- **Tests never hard-code the provider list.** Under test a fourth provider exists (`AgentId::TEST`, `test_agent`; the
  UI's `TEST_PROVIDERS` and `TEST_AGENTS`) that declares no code or account operation and only the capabilities generic
  code is shown to follow from a declaration, so `AgentId::ALL` holds four providers. A test of a rule every provider
  follows iterates all of them and derives each expectation from the descriptor; a test needing a real provider's
  fixtures picks by the capability or sub-interface the code reads
  (`provider(agent).accounts().and_then(|ops| ops.quota_probe()).is_some()`), not by id; a `match` over ids ends in a
  panicking arm (`other => panic!("no fixture for {other}")`). A test of one provider's own behaviour may name that
  provider. The gallery serves only `FIXTURE_PROVIDERS`.
- **An intermittent failure is fixed at its race** ([P8](principles.md)): name which two things in which order fail,
  make it fail on demand by injecting a delay at that point, keep the delay while checking the fix, and fix wherever
  the cause is: the fixture, the product or the test (a stand-in writing a file line by line, a kill returning before
  its processes were gone, a server answering before it read the request). Never a longer timeout, a retry, a sleep, a
  looser assertion or a serialized run. Then fix the same shape across the tree and, where it can be recognised in
  source, ban it with a source scan naming the sanctioned form (`docs/structure/daemon.md`, "Source rules").

## Where tests sit

- **UI tests read `packages/ui/src/fixtures/` for shared data, never gallery scenarios;** besides the fixtures, a UI
  test may read a daemon case table (next bullet).
- **A rule implemented in both the daemon and the UI is pinned by one case table under `apps/daemon/testdata/`, run by a
  test on each side** (Rust `include_str!`, vitest `readFileSync`). The table is registered in `UI_READS_DAEMON`
  (`scripts/verify/scope.mjs`) and in `UI_READERS` and `UI_DISK_TESTS_COVERED` (`scripts/verify/quick.mjs`), and each
  side's doc comment names the other and the table. Example: `apps/daemon/testdata/restart_impact_cases.json`, run by
  `apps/daemon/src/coordinator/handover_check.rs` and `packages/ui/src/lifecycle/useAppExit.test.ts`.
- **Platform contract tests hold every host to one behaviour:** `fs/contract_tests.rs`, `proc/contract_tests.rs` and
  `term/contract_tests.rs` run the same cases against whatever `platform` provides on the host. A new platform
  behaviour gets its case there rather than in one host's file.

## What a test asserts

- **A suspended reproduction case is commented out with a `TODO`** giving the reason, the condition for restoring it
  and the symptom; no known-failure marker is in use (no `it.fails`, and `#[ignore]` only for helper processes the tests
  start and for a test gated on its environment). The defect record points to the test, never the reverse.
- **Tests are lean: one representative case per rule,** in an `it.each` table when rules share a shape, not a `describe`
  per behaviour with near-duplicate cases.
- **Trimming tests as redundant keeps** a test that pins a past defect, every case and side assertion of a merge, and a
  test that cannot fail as named (give it the missing assertion or rename it to what it checks; do not delete it).
- **A new test is checked by breaking the rule it is named for:** empty the constant, feed the input the name says is
  ignored, delete the guard, and confirm this test reports it. Where a shared fixture stands for a set (variables,
  payloads, flags), drop each element in turn; an aggregate assertion passes for elements nothing can see.
- **A test that something is refused asserts the refuser's own message as well as the failure,** so a refusal from
  something else first (git refusing `branch -f` on a checked-out branch before any hook runs) cannot pass it.
- **A tooltip check names whose tooltip is up from its text and asserts there is none where none should open;** it never
  counts `[role=tooltip]`. React Aria keeps one tooltip open per window, so a count cannot fail when a mark gains a
  tooltip it should not have.
- **A jsdom lookup is asserted to have found something before a negative assertion about it,** or the assertion goes on
  the text of an element the markup still has. Not: `find(...)!` then "is not inside"; `contains(undefined)` is `false`.
- **A test that reads a site build asserts in terms of the deployment's origin and base path, not the production
  defaults:** CI runs it on the deployment's own build, whose origin and base path differ.
- **What a test holds about an outside program is taken from a real run of it** ([P7](principles.md)), never written
  from memory:
  - behaviour derived from what an agent or another tool prints (a hook payload, a transcript, a screen, a package
    manager's failure) is verified by replaying a real capture, committed under `apps/daemon/testdata/` with the CLI
    version beside it; replay the production entry conditions too (the offset the real caller starts from, a
    half-finished write), and commit the near-miss capture next to the matching one;
  - a reader that classifies an agent's records line by line is built from a survey of every record of that agent on
    the machine, across all the CLI versions they span and including long, compacted sessions, with a real line for
    each distinct value of the fields that tell lines apart (`type`, `origin`, attachment and synthetic-line markers,
    event types); one session's capture holds only the shapes already expected;
  - what the UI reads from a session's terminal buffer (links) is first checked by replaying a real capture into a real
    `@xterm/xterm` `Terminal` at more than one width, before hand-built rows are trusted: an agent that wraps its own
    text leaves rows xterm does not mark as wrapped, and a session's output is replayed at the terminal's current
    width, not the width the program laid it out for;
  - hand-written `git` output (a gallery change fixture, a viewer fixture, an expected patch) is taken from a run of
    real `git` in a throwaway repository;
  - a remedy keyed on a daemon failure's code, cause or text, and a fixture that models a daemon error, are designed
    only after reproducing the failure against a real daemon (throwaway `HOME`, read-only or small volume), since the
    daemon's failure cannot be read off the system failure underneath (SQLite words denied access and a full disk in
    its own codes, with no `(os error N)`); the fixture carries exactly the code, params and `detail` the daemon sends.
- **A committed capture holds no machine-local path** (a home directory, an external volume, a user name): probe in a
  neutral directory (`/tmp/probe/...`) so what the agent prints names only that, and where a capture already holds one,
  rewrite it consistently, together with any test that asserts on it.

## Time in tests

- **No inline real-time waits:** no `sleep`, `setTimeout`, `thread::sleep` or polling loop to let something happen,
  except one that polls up to `PATIENCE` and returns early (see "Daemon tests"), which is also where the one other
  sanctioned real-time window is set out: a test that proves something did not happen where no signal exists. Drive a
  fake clock started before the code arms its timers (before mounting): in the UI `fakeTimers()`, which also runs the
  pending timers out and requires none left at teardown, or `fakeTimersKeepingFrames()` under `framesAsTimers()`
  (`packages/ui/src/test/clock.ts`); in Rust `#[tokio::test(start_paused = true)]`, `tokio::time::pause()` or an
  injected clock (`test_support/clock.rs`).
- **Durations come from the production constant, never a literal:** the module owning a timing constant exports it and
  the test imports it. A library delay that cannot be imported is a named constant in the support code citing its
  source (`MOUNT_REFOCUS_MS`).
- **Real time is spent only inside a named, documented helper beside the test support,** waiting through
  `waitOnRealClock` (`packages/ui/src/test/clock.ts`), which tests never call directly; in the daemon
  `next_millisecond` (`apps/daemon/src/test_support/clock.rs`). A new exception is added only as such a helper with its
  reason.
- **In a UI component test advance the fake clock inside `act`, one timer per step when several are pending while React
  renders** (`await act(async () => { await vi.advanceTimersToNextTimerAsync(); })`), so what one timer did is committed
  before the next fires. Advancing by a production constant in one step is fine where no render between the timers
  matters.
- **Wait on the condition (`waitFor`, `findBy…`, an awaited promise), never read once after a fixed advance or delay.**
- **Never assert "unchanged" inside a window where a pending frame or timer can change it:** stop it (stub
  `requestAnimationFrame` with `vi.stubGlobal`, removed in `onTestFinished`; fake the timers) or run it out first. Known
  sources are a React Aria `FocusScope` returning focus one frame later, `useDialog`'s mount refocus, the tooltip
  warm-up and the status region's first write.
- Daemon tests that drive a real child process follow "Daemon tests" below (the patience window and the short-timeout
  override getters), not a fake clock.

## UI component tests (jsdom)

The helpers for tooltips, status regions, modality, lazy surfaces, dialog focus, window attention and rows leaving a
collection say on their doc comments when a test needs them.

- **A jsdom test leaves nothing running.** Unmount the root (inside `act`), registering the cleanup in the mounting
  helper with `onTestFinished` so it is bound to the test that mounted, or in an `afterEach` that unmounts every root
  the file mounted. Not: `document.body.replaceChildren()`, which leaves the tree mounted, so no effect cleanup runs and
  its timers stay armed; stray work fires after the file and surfaces as `window is not defined` in another file.
- **A test also stops or awaits whatever else it armed,** such as module-level queues an unmount cannot reach: HeroUI's
  toast queue. To settle a queued toast, under `fakeTimers()` run `toast.clear()` and `vi.runOnlyPendingTimers()`
  together in one `act`, and assert after the unmount that `toast.getQueue().visibleToasts` is empty (that also covers
  the queue's own timers). Not `destroy()`, which leaves every toast in `visibleToasts`, and not a reduced-motion
  `matchMedia` stub, which stops exercising the path the app runs.
- **A test that changes a `createPersistedPreference` value resets it in teardown, and a new window preference tests can
  change is added to `resetAppPreferences`** (`packages/ui/src/appTestSupport.tsx`): `localStorage.clear()` does not
  reset module memory. The error-alert queue and the window preferences the shared setup resets are listed in
  `docs/structure/ui.md`, "Shared entry points".
- **After `vi.resetModules()`, a module the test imported statically and the code under test must share is kept shared
  with `vi.mock(path, async (importOriginal) => importOriginal())`:** otherwise a singleton or an error class is no
  longer the one the code uses.
- **A fixture must not stand in for what is under test.** Set every custom property the code reads on
  `document.documentElement` (jsdom loads no stylesheet), keep stubbed geometry at the current CSS layout, and have the
  code under test throw on an unparsable token rather than default (`|| 0`). Render the real component that owns the
  behaviour and the real control it keys on, not a host that calls the hook and wires the listeners itself.
- **A faked daemon request that fails rejects with a `CodedError` carrying the code, whose message is the catalog
  sentence** (a lost connection is `CodedError(…, "connection_closed", {})`; a daemon code with its params is
  `daemonError(code, params)` from `src/test/daemonError.ts`, which words the message from the catalog), never a plain
  `Error` and never a made-up message (new code; existing fakes are not migrated); one that succeeds answers with the
  reply type `REPLY_TYPES` names, and a request whose real reply is not `ack` is never answered with a catch-all
  `{ type: "ack" }`. `looseRequestSender` checks no reply type, so the test would otherwise run a path the app never
  takes.

## Daemon tests

- **A test needing a script of its own as a program makes it with `test_support::fake_agent`**, which shares one
  dispatcher executable per test binary, never a fresh executable per test: a new executable's first `exec` costs tens
  of milliseconds of code-signature evaluation on macOS, which reads as a too-short timeout under parallel runs.
- **Every wait on anything a spawned process does polls up to `PATIENCE`** (`apps/daemon/src/test_support/mod.rs`) and
  returns early; no elapsed-time assertion is tighter than a stall, and wall-clock checks only rule out waits of
  minutes. Endpoint-security software can stall every `exec` for seconds; that is the machine, not a reason to raise
  production timeouts or serialize the suite.
- **A test waits for the very condition it asserts, not for an earlier event that ought to imply it:** a child's `ppid`
  itself, not "the parent is gone".
- **A fixture parent that must have its children gone when its exit is observed ends and reaps them before it exits.**
  Why: children polling for their parent's exit cannot establish that ordering.
- **A test proving a production timeout fires keeps that timeout short,** and where the fixture was killed before the
  point under test, reruns the whole scenario until `PATIENCE` runs out rather than failing or lengthening it.
- **A window or limit the production code sets is shortened, held or lowered for a test only through a `#[cfg(test)]`
  hook in production, never by asserting or widening the figure:**
  - a window a stand-in must answer inside is read through a getter that returns `PATIENCE` under `#[cfg(test)]` unless
    a `thread_local!` override is set (`settle` and `SETTLE_OVERRIDE` in `session/interrupt/mod.rs`); only the test
    proving the window runs out sets a short override, and a case where the window is not under test sets a zero
    override once its fixture is ready. Where a window cannot go through such a getter, the fixture does its one-time
    setup first and runs only shell builtins inside the window, so no `exec` lands in it;
  - a budget in `browse/budget.rs` is lowered the same way (`TREE_DIFF_STDOUT_OVERRIDE` in `browse/tree_diff.rs`), set
    around the call and cleared after, and the test asserts what happens past the limit; where the work runs on threads
    of its own, the limit is lowered through a bounds value passed in instead (`browse::content_search::Bounds`, whose
    `standard()` reads `budget.rs`);
  - a window a passing test must see run out or hold open ends on a `#[cfg(test)]` event (a `Notify` taken from a
    `thread_local!`, or a flag the test raises), not a widened `PATIENCE`; a window a passing test always waits out
    keeps its real figure.

  A `thread_local!` override reaches only the test's own thread, so the code reads it before work moves to another
  thread, `spawn_blocking`, the coordinator or its first `.await`.
- **A test never proves something did not happen by a window** (a fixed sleep, `timeout(..).is_err()`,
  `recv_timeout(..).is_err()`). It reads what the code already decided (state, a non-blocking `try_recv()`), or uses a
  `#[cfg(test)]` observable (a counter, a `JoinHandle`, a waiter count), or a paused clock advanced with
  `test_support::runtime_advanced` where nothing on the path waits on real I/O. Only where none of these exists (the
  code runs on another thread or in another process and shows nothing) is a window allowed: its length is a named
  constant and a comment says why there is no signal.
- **Where the code under test turns its own wait running out into an outcome, a test asserting any other outcome first
  makes sure the work has ended** (join, await the handle, or wait on a `#[cfg(test)]` observable); only the case where
  running out is the outcome goes through the timed wait.
- **On tokio's paused clock nothing blocks or times out on real I/O.** A test on real I/O stays on the real clock
  bounded by `PATIENCE`; a paused test does not call `test_support::within`, `output_containing` or
  `heard_after_marker`; `tasks_idle` and `runtime_settled` are trusted only where everything before the wait is CPU work
  or `spawn_blocking`.
- **A fixture process that traps or ignores a signal announces itself (pid, marker) only after the disposition is set;**
  where the code under test can signal it as soon as it exists, it starts with the signal blocked and takes it with
  `sigwait` (`respawning` in `processes/ledger/tests.rs` is the shape).
- **A stand-in that writes a file the daemon polls writes the whole content beside it during setup and `mv`s it into
  place,** as the real program replaces it by rename; a shell redirect writes a line at a time and a poll can act on a
  prefix.
- **A test asserting a stand-in session was sent nothing uses `test_support::running` and `heard_after_marker`; one
  asserting something was sent waits for the text with `test_support::output_containing`;** "the owner was not told of
  an end" first waits with `test_support::exit_settled` for every process whose end could have told it. The marker
  orders only against writes already made.
- **A daemon test that creates or edits a console or project, or opens a session, with a model or effort calls
  `test_support::offer_models` for that agent first:** the check runs against the CLI installed on the machine, so an
  unseeded test depends on it.
- **No daemon test may be able to reach the real agent CLI with an action that spends anything.** A request handler or
  task that runs an agent CLI for an irreversible effect on the account (a Codex reset credit, a kick) takes its shell
  environment from a `#[cfg(not(test))]` / `#[cfg(test)]` pair of functions whose test variant bails
  (`reset_credit_shell` in `coordinator/mod.rs`, `background_shell` in `accounts/quotas/poll.rs`), and the action is
  tested one level down with the environment `test_support::fake_agent` returns.
- **No daemon test may reach the user's live data directory.** Code a test can reach that touches Octoboard's data
  directory takes the directory from its caller (`ProcessLedger::new(data_dir)`, `paths::holders_dir_in(dir)`,
  `holder::client::ClientConfig`); only `main.rs` passes the global path, and no `#[cfg(test)]` setter redirects a
  global path. A test that needs a holder uses `holder::client::test_holders`.
- **A hook or transport a test installs into `AppState` holds the state weakly** (`Arc::downgrade`, `upgrade()` in the
  callback), never `state.clone()`: the cycle leaves cgroups behind on a Linux host.
- **A test that makes something unwritable or unreadable checks what the machine allows before asserting a refusal** and
  returns when it cannot happen (`libc::geteuid() == 0`, or try the write): root ignores permission modes. A test of a
  check on the directories above its scratch directory runs the check first and asserts the refusal if it already
  refuses, since `TMPDIR` may sit in a volume others can write to.
- **A test that returns early for want of a cgroup calls `test_support::cgroups::skip(reason)` (or takes the group
  through `grouped_launch` / `ChildCgroup::new`); one that returns early for an old `git` uses
  `test_support::init_reftable(dir)` or `skip_without_reftable`,** never its own `eprintln!("skipped: …")`. The helpers
  fail the test under `OCTOBOARD_REQUIRE_CGROUP=1` and `OCTOBOARD_TEST_REQUIRE_REFTABLE` in CI, so a broken setup cannot
  pass unnoticed.
- **A test takes a free port from `test_support::ClosedPort`,** kept in a local and named through `closed.port()`, never
  by binding port 0 and dropping the listener. After a listener closes, assert the release with
  `platform::proc_table::listening_ports(...)` and wait with `test_support::closed_port::let_go(port)` before rebinding
  or expecting a refusal: a child process holds a copy of the descriptor until its `exec`.
- **A test that a `git` read leaves the repository alone uses a fixture with stale stat information and a positive
  control** (the same read with the guard off, asserted to write), built from `test_support/git.rs`
  (`move_modification_time_back` after every `git` the fixture runs, `git_dir_state`).
- **Daemon test fixtures use only commands and options Git 2.36 has** (`worktree add --detach` then
  `checkout --orphan`); behaviour only a newer `git` has is tested only where `test_support::git_has_show_ref_exists`
  says so.

## Desktop shell tests

- **A test that starts a process holds `crate::spawning()` across the start**, code under test that starts one takes it
  under `#[cfg(test)]` (`command_line` in `remote/device_key.rs`), and a test that takes a file lock and asserts on its
  release holds the guard for the whole test. A process forked while another test has a locked file open inherits it
  until the exec, so the lock appears held and the test fails now and then.

## Windows and Linux runs

The daemon's tests also run on native Windows when the hosted `windows-check` job of `quality.yml` is started by hand,
and on Linux in `linux-daemon.yml`, which every release with Linux in it runs; all of them are expected to pass there.
These are the ways a test that passes on the Mac fails on those hosts.

- **A path a test reads back is resolved with `hostfs::canonical`,** and a path a process reports (a working
  directory, a path `git` prints) is compared by its canonical path, not its text: Windows gives the verbatim
  `\\?\C:\…` form, and the runner's `TEMP` is an 8.3 short path.
- **A Unix-spelled absolute path a test only names is built with `test_support::host_path`,** and an environment
  variable captured from a process is looked up without regard to case on Windows (`Path`, not `PATH`).
- **A fixture name Windows `git` cannot hold (`a:b`, `CON`) goes in through a commit, on every host, not gated to
  Unix:** `test_support::commit_holding` or `tree_holding`. Stage `CON` only with `-c core.protectNTFS=false`.
- **A stand-in session on Windows is a native stand-in** (`stand_in_launch_echoing`, `stand_in_running_steps`,
  `idle_stand_in`), not a Git Bash script; a fixture of your own sets the console raw before signalling ready, and the
  test writes nothing before that signal.
- **A line written to a program that reads its console a line at a time ends with `\r`, not `\n`;** a Codex stand-in
  uses `stand_in_launch_echoing` or the test provider (`AgentId::TEST`); a screen that must arrive whole is drawn with
  `test_support::console_writer`, started before the window the production code times.
- **A committed file a test compares byte for byte with rendered output is named in `.gitattributes` with
  `text eol=lf`;** a source file a test searches across a line end has `\r\n` turned into `\n` once it is read, or is
  matched line by line. Windows runners check out with `core.autocrlf` on. Do not loosen the comparison.
- **Tests of Linux-only code (cgroups, PTY ends) run on a Linux host or in a container;** a test that cannot run for
  want of a cgroup or a new enough `git` says so through the helpers above.

## The UI state gallery and its fixtures

The code-level rules for gallery scenarios and the fixture daemon (`packages/ui/src/gallery/`,
`packages/ui/src/fixtures/`); what a scenario shows and its steps are in `packages/ui/src/gallery/README.md`,
"Writing scenarios", and its viewing conditions in `.claude/skills/agent-gallery/SKILL.md`, "Project conventions".

- **Scenario `state` is built with `snapshotState`; consoles and projects with `consoleWith` / `projectWith` or a typed
  `new Map<string, Console>(...)`,** never an un-annotated `new Map([...])` of spread objects: leftover keys go
  unchecked and a removed protocol field survives in the fixture.
- **Mocks are shaped for the target and typed to the real contract,** so a contract change fails the mock instead of
  relying on someone to sync it; supply only the fields the target needs.
- **A scenario answering a request with a daemon error uses the code the real daemon sends for that request in that
  situation,** taken from its handler and `apps/daemon/PROTOCOL.md` (`directory_unreadable`, not a `permission_denied`
  that merely sounds right).
- **The fixture daemon answers as the real one would.** Write a handler only for a request some scenario sends and
  delete an unused one; every fixture repository read is answered
  (`packages/ui/src/gallery/fixtures/projectGit.test.ts`) and written import results follow the daemon's rules
  (`packages/ui/src/gallery/fixtures/configuration.test.ts`). When a reply gains a field the UI draws from, teach the
  fixture daemon the condition the real daemon fills it under (from `apps/daemon/src/`, not a doc or a UI helper) before
  reading a scenario as a failed fix, per `packages/ui/src/fixtures/README.md`, "Conventions for teaching the fixture
  daemon a reply"; give the website preview scenario (`createPreviewScenario`) the same state, since the sweep does not
  cover it.
- **`packages/ui/src/gallery/prepare.ts` sets stored preferences before the app's modules load,** so nothing the frame
  imports earlier, nor `packages/ui/src/gallery/fixtures/index.ts` or `packages/ui/src/gallery/updaterFixture.ts`, may
  read a stored preference at load.
- **Files directly in the git-ignored `packages/ui/src/gallery/preview/` default-export a `Scenario[]` and import
  fixtures from their own modules, never `packages/ui/src/gallery/fixtures/index.ts`** (an import cycle); anything else
  goes in a subdirectory. Preview-only scenarios and temporary mocks (`packages/ui/src/gallery/preview/mocks/`) never
  mix into formal scenarios.
