# Test conventions

How unit and integration tests are written in every end: where they sit, what they assert, how they treat time, the
rules specific to jsdom component tests, daemon tests and Windows and Linux runs, and the code-level rules for the UI
state gallery and its fixtures. How to run the tests, and which checks run when, is in
[`tooling.md`](tooling.md); which test helper to use for what is in the "Shared entry points" tables of the structure
docs ([`../structure/ui.md`](../structure/ui.md), [`../structure/daemon.md`](../structure/daemon.md)); the visual and
interaction rules a gallery scenario shows are in [`ui-standard.md`](ui-standard.md).

## Where tests sit

- **A test sits beside the code it tests, under the name the code's language and package give it:** UI
  `src/<path>/<name>.ts(x)` has `<name>.test.ts(x)` next to it; Rust has a `#[cfg(test)] mod tests` in the file, or
  `<name>/tests.rs` (`<topic>_tests.rs` beside the module where one module's tests split by topic); `scripts/<name>.mjs`
  (the repository's or a package's, `apps/*/scripts/`, `packages/*/scripts/`) has its test beside it, and the package's
  test command collects that directory; `apps/web` tests are the only ones in a separate root, `apps/web/test/<name>.test.ts`.
  Not: a mirrored `tests/` tree for the UI or the daemon.
- **A test file keeps the `.test` marker in its name, and test file names are unique within a package:** two files of
  the same name shadow each other in imports and in the editor. A source file may have several test files split by topic
  (`App.report.test.tsx`, `TerminalPane.focusReporting.test.tsx`).
- **Helpers shared by several tests live in the places the structure docs name:** `packages/ui/src/test/`,
  `*TestSupport.ts(x)` / `*TestState.tsx` beside the module that uses them, `apps/daemon/src/test_support/`. They are
  not named like the test of a source file.
- **UI tests read `packages/ui/src/fixtures/` for shared data and never gallery scenarios;** it is changed only through
  the gallery workflow (see "The UI state gallery and its fixtures" below). Besides the fixtures, a UI test may read a daemon
  case table (next bullet).
- **A rule implemented in both the daemon and the UI is pinned by one case table under `apps/daemon/testdata/`, run by a
  test on each side** (Rust `include_str!`, vitest `readFileSync`). The table is registered in `UI_READS_DAEMON`
  (`scripts/verify/scope.mjs`) and in `UI_READERS` and `UI_DISK_TESTS_COVERED` (`scripts/verify/quick.mjs`), and each
  side's doc comment names the other and the table. Example: `apps/daemon/testdata/restart_impact_cases.json`, run by
  `apps/daemon/src/coordinator/handover_check.rs` and `packages/ui/src/lifecycle/useAppExit.test.ts`.
- **Test files are type-checked and linted but are not part of build output or published packages** (the
  `tsconfig.app.json` production check and the Vite build leave them out; ESLint covers them). Do not move a test file
  into a path the production build includes.
- **Platform contract tests hold every host to one behaviour:** `fs/contract_tests.rs`, `proc/contract_tests.rs` and
  `term/contract_tests.rs` run the same cases against whatever `platform` provides on the host. A new platform
  behaviour gets its case there rather than in one host's file.
- **A file with logic of its own gets tests;** which files are exempt and how a hard-to-test dependency is reported are
  in the `test-writer` agent definition (`.claude/agents/test-writer.md`, "Judge file by file").

## What a test asserts

- **How a test is written (from the required behaviour, external behaviour only, mocks only at the unit's boundary, no
  defect frozen into a passing case, a reproduction case for a defect a test finds) is in the `agent-unit-test` skill
  and the `test-writer` agent definition.** Project facts for suspending a reproduction case: no known-failure marker is
  in use today (no `it.fails` in `packages/ui`, and no `#[ignore]` marks a suspended case in the daemon; it is used only
  for helper processes the tests start and for a test gated on its environment), so the skill's next step applies, a
  commented-out case with a `TODO` giving the reason, the condition for restoring it and the symptom; the defect record
  points to the test, never the reverse.
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
- **Behaviour derived from what an agent or another tool prints is verified by replaying a real capture,** committed
  under `apps/daemon/testdata/` with the CLI version beside it, never from text written from memory (a hook payload, a
  transcript, a screen, a package manager's failure). Replay the production entry conditions too (the offset the real
  caller starts from, a half-finished write), and commit the near-miss capture next to the matching one.
- **A reader that classifies an agent's records line by line is built from a survey of every record of that agent on
  the machine, across all the CLI versions they span and including long, compacted sessions,** with a real line for each
  distinct value of the fields that tell lines apart (`type`, `origin`, attachment and synthetic-line markers, event
  types); one session's capture holds only the shapes already expected.
- **What the UI reads from a session's terminal buffer (links) is first checked by replaying a real capture into a real
  `@xterm/xterm` `Terminal` at more than one width,** before hand-built rows are trusted. An agent that wraps its own
  text leaves rows xterm does not mark as wrapped, and a session's output is replayed at the terminal's current width,
  not the width the program laid it out for, so a row reaching the last column says nothing about whether the program
  broke the line there.
- **Hand-written `git` output (a gallery change fixture, a viewer fixture, an expected patch) is taken from a run of
  real `git` in a throwaway repository,** never written from memory.
- **A remedy keyed on a daemon failure's code, cause or text, and a fixture that models a daemon error, are designed
  only after reproducing the failure against a real daemon** (throwaway `HOME`, read-only or small volume); the fixture
  carries exactly the code, params and `detail` the daemon sends.
- **Tests never hard-code the provider list.** Under test a fourth provider exists (`AgentId::TEST`, `test_agent`; the
  UI's `TEST_PROVIDERS` and `TEST_AGENTS`) that declares no capability, code or account operation, so `AgentId::ALL`
  holds four providers. A test of a rule every provider follows iterates all of them and derives each expectation from
  the descriptor; a test needing a real provider's fixtures picks by the capability or sub-interface the code reads
  (`provider(agent).accounts().and_then(|ops| ops.quota_probe()).is_some()`), not by id; a `match` over ids ends in a
  panicking arm (`other => panic!("no fixture for {other}")`). A test of one provider's own behaviour may name that
  provider. The gallery serves only `FIXTURE_PROVIDERS`. See [`provider-neutral-core.md`](provider-neutral-core.md).
- **A test that reads a site build asserts in terms of the deployment's origin and base path, not the production
  defaults:** CI runs it on the deployment's own build, whose origin and base path differ.

## Time in tests

- **A test's outcome must not depend on how fast the machine runs it.** A test that passes alone and fails under the
  suite's load is broken, not intermittent; rerunning it alone only lets a merge go ahead and does not clear it.
- **No inline real-time waits:** no `sleep`, `setTimeout`, `thread::sleep` or polling loop to let something happen,
  except one that polls up to `PATIENCE` and returns early (see "Daemon tests"), which is also where the one other
  sanctioned real-time window is set out: a test that proves something did not happen where no signal exists. Drive a
  fake clock started before the code arms its timers (before mounting): in the UI `fakeTimers()`, or
  `fakeTimersKeepingFrames()` under `framesAsTimers()` (`packages/ui/src/test/clock.ts`); in Rust
  `#[tokio::test(start_paused = true)]`, `tokio::time::pause()` or an injected clock (`test_support/clock.rs`).
- **Durations come from the production constant, never a literal:** the module owning a timing constant exports it and
  the test imports it. A library delay that cannot be imported is a named constant in the support code citing its
  source (`MOUNT_REFOCUS_MS`).
- **Real time is spent only inside a named, documented helper beside the test support,** waiting through
  `waitOnRealClock` (`packages/ui/src/test/clock.ts`), which tests never call directly. A new exception is added only as
  such a helper with its reason. The sanctioned set:
  - `trackMountRefocus` and `settleFrames` (`packages/ui/src/components/dialog/dialogTestSupport.ts`);
  - the react-aria tooltip waits in `packages/ui/src/components/hint/tooltipTestSupport.ts`: the touch-hover suppression
    (in `press`) and `tooltipsCooledDown`;
  - `settleLazyImport` (`packages/ui/src/test/lazyImport.ts`);
  - `firstWriteDelayPassed` (`packages/ui/src/components/announce/statusAnnouncerTestSupport.ts`), for a test that must
    mount on the real clock; otherwise fake the clock before mounting and advance `FIRST_WRITE_DELAY_MS`;
  - `afterQueuedWork` and `flushPromises` (`packages/ui/src/test/clock.ts`), one real 0 ms macrotask that lets queued
    microtasks run, for code that finishes on promise chains alone; where a positive assertion follows, wait on it
    instead;
  - in the daemon, `next_millisecond` (`apps/daemon/src/test_support/clock.rs`), for production stamps in wall-clock
    milliseconds.
- **In a UI component test advance the fake clock inside `act`, one timer per step when several are pending while React
  renders** (`await act(async () => { await vi.advanceTimersToNextTimerAsync(); })`), so what one timer did is committed
  before the next fires. Advancing by a production constant in one step is fine where no render between the timers
  matters.
- **Wait on the condition (`waitFor`, `findBy…`, an awaited promise), never read once after a fixed advance or delay.**
- **Never assert "unchanged" inside a window where a pending frame or timer can change it:** stop it (stub
  `requestAnimationFrame` with `vi.stubGlobal`, removed in `onTestFinished`; fake the timers) or run it out first. Known
  sources are a React Aria `FocusScope` returning focus one frame later, `useDialog`'s mount refocus, the tooltip
  warm-up and the status region's first write.
- **A test that faked timers runs out the pending ones and asserts `vi.getTimerCount()` is 0 before restoring the real
  clock, bound with `onTestFinished`;** `fakeTimers()` does all of it.
- **A new or changed time-touching test is also run under load** (the file run several times in parallel, or with
  `--sequence.shuffle=true`) **and with a delay injected before the assertion.** Inject it as a busy loop or
  `Atomics.wait`, never a `setTimeout` (on a fake clock it never fires, so the test hangs instead of showing anything),
  and write `--sequence.shuffle=true` with its value when file filters follow.
- **An intermittent failure is fixed at its race:** name which two things in which order fail, make it fail on demand by
  injecting a delay at that point, keep the delay while checking the fix, and fix wherever the cause is: the fixture,
  the product or the test (a stand-in writing a file line by line, a kill returning before its processes were gone, a
  server answering before it read the request). Never a longer timeout, a retry, a sleep, a looser assertion or a
  serialized run. Then fix the same shape across the tree and, where it can be recognised in source, ban it with a
  source scan naming the sanctioned form (`docs/structure/daemon.md`, "Source rules").
- Daemon tests that drive a real child process follow "Daemon tests" below (the patience window and the short-timeout
  override getters), not a fake clock.

## UI component tests (jsdom)

- **A jsdom test leaves nothing running.** Unmount the root (inside `act`), registering the cleanup in the mounting
  helper with `onTestFinished` so it is bound to the test that mounted, or in an `afterEach` that unmounts every root
  the file mounted. Not: `document.body.replaceChildren()`, which leaves the tree mounted, so no effect cleanup runs and
  its timers stay armed; stray work fires after the file and surfaces as `window is not defined` in another file.
- **A test also stops or awaits whatever else it armed,** such as module-level queues an unmount cannot reach: HeroUI's
  toast queue. To settle a queued toast, under `fakeTimers()` run `toast.clear()` and `vi.runOnlyPendingTimers()`
  together in one `act`, and assert after the unmount that `toast.getQueue().visibleToasts` is empty (that also covers
  the queue's own timers). Not `destroy()`, which leaves every toast in `visibleToasts`, and not a reduced-motion
  `matchMedia` stub, which stops exercising the path the app runs.
- **A test counting on a tooltip that still waits out its delay calls `tooltipsCooledDown()` before faking the clock; a
  test that can leave a tooltip open closes it before unmounting** (`mountForTooltip`, or `closeOpenTooltip()` first in
  its teardown). React Aria's warm-up is window-wide and cannot be faked, so the test otherwise passes alone and fails
  after any test that showed a tooltip.
- **A test that mounts a component holding a status region (`App`, `StatusAnnouncer`) fakes the clock before mounting;**
  one that must mount on the real clock waits `firstWriteDelayPassed()` first. A timer armed before the clock is faked
  never fires on the fake one.
- **A test whose outcome depends on React Aria's interaction modality sets it with `withModality(...)` before
  `fakeTimers()`:** a virtual click leaves the window at `virtual`, where a dialog puts its opening focus a frame late.
- **A test that changes a `createPersistedPreference` value resets it in teardown, and a new window preference tests can
  change is added to `resetAppPreferences`** (`packages/ui/src/appTestSupport.tsx`): `localStorage.clear()` does not
  reset module memory. The error-alert queue and the window preferences the shared setup resets are listed in
  `docs/structure/ui.md`, "Shared entry points".
- **After `vi.resetModules()`, a module the test imported statically and the code under test must share is kept shared
  with `vi.mock(path, async (importOriginal) => importOriginal())`:** otherwise a singleton or an error class is no
  longer the one the code uses.
- **A test rendering a surface behind a `React.lazy` boundary awaits `settleLazyImport()` after the first render that
  needs it,** not folded into a shared render helper. The settle needs the real clock, so a test on a fake clock asserts
  only on what sits outside the boundary. A `lazy` object caches its resolution, so only the first test per `lazy`
  object ever suspends (two exports built with `lazy` give a file two first tests): check such a test by running it
  alone with `-t`, not by a clean whole-file run.
- **A dialog test calls `trackMountRefocus()` right after the mount and awaits it before the first step whose focus it
  asserts** (not on a fake clock, where it never resolves). On a fake clock it asserts the opening focus before anything
  runs the timers out. For a dialog or popover opened by a virtual press it uses `framesAsTimers()` with
  `fakeTimersKeepingFrames()`, `await settleFrames()` after the open, and asserts focus is inside the overlay before the
  next step, so the order does not depend on test speed.
- **A jsdom test does not assert where focus lands after a session is selected:** the terminal attach moves it. Assert
  focus only for moves that select no session.
- **A test of a press while a React Aria modal overlay is open sends the press to `document.body`, blurs what had focus
  behind the overlay, and sends the target only the events that follow the close:** jsdom has no `inert`, so a press
  sent to the target runs handlers a browser never reaches.
- **Viewer jsdom tests assert only what a copy or select-all handler does to the DOM;** what a selection over code
  copies is settled by copying in a real browser, because the renderer's shadow root is mocked away in jsdom.
- **A fixture must not stand in for what is under test.** Set every custom property the code reads on
  `document.documentElement` (jsdom loads no stylesheet), keep stubbed geometry at the current CSS layout, and have the
  code under test throw on an unparsable token rather than default (`|| 0`). Render the real component that owns the
  behaviour and the real control it keys on, not a host that calls the hook and wires the listeners itself.
- **A faked daemon request that fails rejects with a `CodedError` carrying the code, whose message is the catalog
  sentence** (a lost connection is `CodedError(…, "connection_closed", {})`; a daemon code with its params is
  `daemonError(code, params)` from `src/test/daemonError.ts`, which words the message from the catalog), never a plain
  `Error` and never a made-up message (new code; existing fakes are not migrated); one that succeeds answers with the reply type `REPLY_TYPES` names, and a request whose real
  reply is not `ack` is never answered with a catch-all `{ type: "ack" }`. `looseRequestSender` checks no reply type, so
  the test would otherwise run a path the app never takes.
- **A test of focus after a handoff that React commits asserts `data-focus-visible` on the element React Aria marks
  after `await act(async () => {})`,** and sends the change as a new array rather than an element replaced inside the
  fixture's array.
- **A test of a modal raised over another dialog** uses `framesAsTimers()`, an opener outside focus, the `openable`
  mounted first, `settleFrames()` and `StrictMode`, asserts the modal is not inside `[aria-hidden], [inert]`, and covers
  the dialog covered by a later one (`ErrorAlerts.test.tsx` is the shape).
- **A test of a window-focus-gated rule sets the answer through `windowAttention()`, imports `input/focusGuard` first,
  and recognises window-targeted events with `!(event.target instanceof Node)`,** never `event.target === window`.
- **A test of a row leaving a React Aria collection runs inside a `useFocusHandoff` host or asserts focus at removal
  (`focusAtRemoval`), and uses `sizedElements` for scrolled-out virtualized rows.**

## Daemon tests

- **A test needing a script of its own as a program shares one dispatcher executable per test binary,** created once
  behind a `OnceLock` and installed by writing a file of the process's own and renaming it into place; each test gives
  it behaviour as a non-executable companion file reached through a symlink. On Windows the dispatcher is a compiled
  program each fake hard-links to (`test_support::fake_agent`), never a `.cmd` or `.bat` shim. A fresh executable per
  test costs tens of milliseconds of code-signature evaluation on macOS, which reads as a too-short timeout under
  parallel runs.
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
- **Where a stand-in must answer inside a window the production code times, production reads the window through a getter
  that returns `PATIENCE` under `#[cfg(test)]` unless a `thread_local!` override is set** (`settle` and
  `SETTLE_OVERRIDE` in `session/interrupt/mod.rs`). Only the test proving the window runs out sets a short override, and
  the code reads the getter before work moves to another thread or its first `.await`, since the override reaches only
  the test's own thread. Where a window cannot go through such a getter, the fixture does its one-time setup first and
  runs only shell builtins inside the window, so no `exec` lands in it; a case where the window is not under test sets a
  zero override once its fixture is ready.
- **Where a passing test must see a window run out or hold it open, the window ends on a `#[cfg(test)]` event** (a
  `Notify` taken from a `thread_local!`, or a flag the test raises), not a widened `PATIENCE`; a window a passing test
  always waits out keeps its real figure.
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
- **A fixture process that traps or ignores a signal announces itself (pid, marker) only after the disposition is set.**
  Where the code under test can signal it as soon as it exists, start it with the signal blocked
  (`pthread_sigmask(SIG_BLOCK)` in `pre_exec`) and take it with `sigwait`, checking the returned signal number; an
  ignored disposition is set before `exec`. Installing a handler and then unblocking loses the signal on macOS.
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
- **A daemon budget in `browse/budget.rs` is tested by lowering it, never by asserting the constant's value:** the
  production code reads the limit through a getter that a `#[cfg(test)]` `thread_local!` override can lower, as
  `tree_diff::tree_diff_stdout` does with `TREE_DIFF_STDOUT_OVERRIDE` (`browse/tree_diff.rs`; `budget.rs` holds only the
  constant). Set the override around the call in the test, clear it after, and assert what happens past the limit. The
  override is per thread, so it reaches only code on the test's own thread: anything moved onto `spawn_blocking` or
  through the coordinator reads the real figure. Where the work runs on threads of its own, which such an override
  cannot reach, the limit may instead be lowered through a bounds value passed to the function: a `Bounds` struct whose
  `standard()` reads `budget.rs`, as `browse::content_search::Bounds` does.
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

## Windows and Linux runs

Every daemon test also runs on native Windows (the hosted `windows-check` job, see `tooling.md`), which the Mac never
runs; these are the ways a test that passes on the Mac fails there.

- **A path a test reads back is resolved with `hostfs::canonical`, never `std::fs::canonicalize`,** and a path a process
  reports (a working directory, a path `git` prints) is compared by its canonical path, not its text: Windows gives the
  verbatim `\\?\C:\…` form, and the runner's `TEMP` is an 8.3 short path. The platform scan exempts test code, so
  nothing enforces this.
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

The gallery (`packages/ui/src/gallery/`) shows the UI's states for people to confirm and for the sweep to walk
(`pnpm gallery:sweep`); its fixture daemon, builders and step driver are shared with the website preview and the
application tests (`packages/ui/src/fixtures/`). How to run, sweep and compare is in
`packages/ui/src/gallery/README.md`.

- **One scenario shows one identifiable, settled view:** once initialised it stays there until someone interacts.
  Presentation and verification are separate: behavioural assertions go into tests, and a few more assertions are no
  reason for a new scenario.
- **Initialise straight to the target:** prefer a declarative initial state; where that is impossible, the shortest
  `steps`, pressing controls by name (`ui.press(ui.t("rail.settings"))`) with the `Ui` helpers of
  `packages/ui/src/fixtures/interact.ts`, without visiting unrelated views or regression assertions.
- **Scenarios verify the UI, not copy:** the same UI with different copy or a different form-control value gets no
  second scenario, except where it changes layout (the longest option label). Cases that can coexist on screen go in one
  scenario; error scenarios merge only where message format, layout and the offered actions are the same. The one
  exception is a scenario a docs screenshot needs (`docs/conventions/docs-site.md`, "Screenshots"): it may repeat
  another scenario's UI with plainer data, or without its hover or focus, and its description names what it leaves out
  and the scenario it differs from.
- **Viewing conditions (theme, language, width) are chosen with the gallery's controls and are not copied into
  scenarios.** A copy of a scenario made to open the drawer first, or because its layout differs at a width, earns
  nothing: write the pointer ("choose 560px to see the rows stack") in the description of the scenario that shows the
  state.
- **Write no `width` unless the window itself is part of the scenario** (the page opens at 1440). Set one only where the
  target can be reached only in a band of widths the default is outside of and steps cannot get there at whatever width
  the control picks (a swipe, or the open drawer card itself), and say in the title or description what it buys.
  `minWidth` states instead the narrowest window a view exists in when it exists only from a breakpoint up (a card
  floated in by hovering a toggle, a docked pane's width grip); a width chosen with the control is raised to it. A
  scenario carries one or the other, and a comment beside `minWidth` says why. Write a `height` only where a step needs
  a window too short for a dialog's body, which then scrolls.
- **A step waits on a condition** (the `Ui` helpers, which wait for the control they act on; `waitForControl`,
  `waitForText` and `waitUntil` for a state with nothing to act on), not on time; a fixed `ui.wait(ms)` only with a
  comment saying why the UI shows nothing to wait on (a timer inside the page). A fixed wait in a shared step list (such
  as `openFiles` in `src/gallery/fixtures/projectFiles.ts`) carries that comment once, where the list is defined, not
  at each scenario that spreads it.
- **Steps run at any width the page's width control picks,** so they name controls as in a docked window; use
  `ui.showPanel` and `ui.closeDrawer` where the layout needs them.
- **`held: true` and `held: "from-start"` only for views that exist only while the shortcut modifier is held.**
- **Scenario `state` is built with `snapshotState`; consoles and projects with `consoleWith` / `projectWith` or a typed
  `new Map<string, Console>(...)`,** never an un-annotated `new Map([...])` of spread objects: leftover keys go
  unchecked and a removed protocol field survives in the fixture.
- **Mocks are shaped for the target and typed to the real contract,** so a contract change fails the mock instead of
  relying on someone to sync it; supply only the fields the target needs.
- **Scenarios live in `packages/ui/src/gallery/fixtures/` (an area file, or a new file listed in
  `packages/ui/src/gallery/fixtures/index.ts`); `group` is a title in `GROUPS`
  (`packages/ui/src/gallery/fixtures/groups.ts`); `category` is `"normal"` or `"exceptional"` by the view being
  confirmed,** normal before exceptional in each group. `packages/ui/src/gallery/fixtures/fixtures.test.ts` checks the
  groups.
- **Live changes are written with the builders in `packages/ui/src/gallery/fixtures/liveChanges.ts`,** not raw
  `daemonEvents` steps; add a builder for another kind of collection.
- **Steps never write copy out:** controls through `ui.t`, session rows through `ui.session`, change rows through
  `ui.changeRow`; a name that is not copy is a plain string; a matcher is never a regular expression.
  `packages/ui/src/gallery/fixtures/fixtures.test.ts` fails an English-text or English-separator name.
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
- **Nothing in `packages/ui/src/fixtures/` or `packages/ui/src/website-preview/` imports `packages/ui/src/gallery/`**
  (`packages/ui/eslint.layers.mjs`).
- **A scenario for a new change presentation is shown in the "Project pane: Git" group:** the File viewer group carries
  no loader for expanding collapsed diff lines, and `packages/ui/src/gallery/frame.tsx` wires viewer contexts for that
  branch alone.
- **A quick-check failure is fixed by its cause:** never by deleting scenarios, skipping or loosening checks, or bending
  a scenario to an app error. A scenario is deleted only where its UI state no longer exists.
- **The sweep and compare commands run only through `pnpm gallery:sweep` and `pnpm gallery:compare`;** compare output
  goes in a new directory outside the repository or under the ignored `out/`.
- **A brief that hands gallery or browser work to a subagent names which scenarios, how many, the most pages open at
  once,** and requires every browser or server it started to be stopped; never overwrite, stop or uninstall a service
  this working copy did not start.
