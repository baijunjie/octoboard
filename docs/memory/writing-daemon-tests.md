# Writing daemon tests

## Never write a fresh executable per test — share one that has already been exec'd

Applies to any test that needs a script of its own to run as a program: a fake shell, a fake agent
CLI, a hook stub. macOS evaluates a brand-new executable's code signature on its first `exec` and
serializes that evaluation across processes, so concurrently launched fresh scripts start roughly
80 ms apart (measured here at eight at a time), against 34-39 ms each for one shared script already
exec'd once. A test that writes its script and then runs the code under test against a short
timeout can spend most of that timeout before its own fixture body ever runs.

So write **one** dispatcher executable per test binary, created once behind a `OnceLock` or
equivalent, and give each test its behaviour as a plain, non-executable companion file that the
dispatcher sources, reached through a symlink — creating and resolving a symlink carries none of
the first-exec cost. Give the dispatcher a fixed name in the temp directory, and install it by
writing a file of the process's own and renaming it into place: test binaries from other worktrees
run at the same time and may be executing it, and a plain overwrite truncates it under them.

The symptom points the wrong way, so suspect this before changing a timeout or calling a test
flaky: a test that fails only when the suite runs in parallel reads as a too-short timeout in the
code under test, and `--test-threads=1` apparently fixing it reinforces that reading. This cost is
tens of milliseconds per fresh script; delays of whole seconds that hit every exec are a different
cause.

## Bound every wait on a spawned process by `PATIENCE`, not by how fast it should be

Applies to daemon tests whose fixture is a real process: a fake shell, a stand-in agent on a PTY.
On a Mac running endpoint-security software, as managed machines do, the security agent can stop
answering for seconds at a time under the suite's parallel burst of shells; while it does, every
`exec` on the machine blocks, and so do `kill` and `waitpid` on a child stuck in one. Such stalls
can exhaust waits bounded in single seconds even when the code under test behaves correctly.

- A wait for anything a spawned process does — output, a file, an event it causes, its exit —
  polls up to `PATIENCE` from `apps/daemon/src/test_support/mod.rs`; a poll returns early, so a
  passing test pays nothing. Do not assert an elapsed time tighter than a stall: show "fails
  fast" by which error came back, and keep wall-clock checks only for ruling out a wait of
  minutes.
- Wait for the very condition the test goes on to assert, not for an earlier event that ought to imply it. On macOS
  a process stops being readable (`proc_pidinfo`, the ledger's `table::start_of`) as soon as its exit begins, while
  the kernel reparents its children to pid 1 later in that exit, so "the parent is gone" does not yet mean "its child's
  `ppid` is 1": a test asserting a reparenting waits for the child's `ppid` itself.
  When a test requires children to be gone when their parent's exit is observed, have the fixture parent end and
  reap them before it exits; children polling for their parent's exit cannot establish that ordering.
- A test proving that a production timeout fires keeps that timeout short. When the fixture was
  killed before it reached the point under test, nothing was tested: run the whole scenario
  again until `PATIENCE` runs out rather than failing or lengthening the timeout.
- Where a stand-in has to answer inside a window the production code times, widen that window
  under test instead of racing it: the production code reads it through a getter that returns the
  real figure, and under `#[cfg(test)]` returns `PATIENCE` unless a `thread_local!` override is
  set; only the test proving the window runs out sets a short override (`settle` and
  `SETTLE_OVERRIDE` in `apps/daemon/src/session/interrupt/mod.rs` are the shape). The override
  reaches only the test's own thread, so the code reads the getter there and passes the value on
  before the work moves onto `spawn_blocking` or another thread, and in async code before its
  first `.await`. Where a window cannot be routed through such a getter, the fixture does its
  one-time setup first and uses only shell builtins after it, so no exec lands inside that window.
- Widening to `PATIENCE` costs nothing only where the window ends early on the awaited answer.
  Where a passing test has to see the window run out, or needs it held open until its own
  concurrent action is done — a fixture that ignores TERM inside a stop grace, a second trigger
  that must find a claim still held — the widened window makes every passing run wait the whole of
  `PATIENCE`. There, end the window on an event instead: the production wait also returns on a
  `#[cfg(test)]` signal taken from a `thread_local!` — a `tokio::sync::Notify` the other path
  notifies once it reaches the point the test needs (`CLAIM_WAITING` in
  `apps/daemon/src/coordinator/auto_switch.rs` ending `SWITCH_SETTLED` in
  `apps/daemon/src/coordinator/restart.rs`; a `Notify` keeps a permit, so it does not matter which
  side gets there first), or a flag the test raises after its action (`GRACE_ENDED` in
  `apps/daemon/src/processes/ledger/stop.rs`). A window a passing test always waits out in full
  keeps its real figure under test (`switch_settle` in `restart.rs`), and a case where the window is
  not what is tested sets a zero override once its fixture is ready.
- Do not prove that something did not happen by sleeping a fixed window first. Give the
  production path a `#[cfg(test)]` observable — a counter, or the `JoinHandle` of the task that
  would act — and wait on it or assert it; a counter for work handed to `spawn_blocking` or another
  thread is incremented before the handoff, so it is final once the call under test returns
  (`holder_signal_attempts` in `apps/daemon/src/holder/client/mod.rs`).

To recognise it: failures only under a parallel run, a run far slower than usual,
`--test-threads=1` passing, and a trivial exec (`/usr/bin/true`) timed in a loop alongside the
suite stalling for seconds in the same windows. That is the machine, not the code under test: do
not raise production timeouts or serialize the suite over it.

## A fixture that handles or ignores a signal must not be reachable by it before that is in place

Applies to daemon test fixtures that are real processes trapping, ignoring or handling a signal that the test or the
code under test then sends — a shell with `trap`, a Rust fixture calling `libc::signal`. Until the disposition is set
the process has the default action and the signal just ends it; the window is small (some 25 ms after `exec` for a
freshly exec'd test binary) and is hit only under the suite's load, as a fixture that never appeared or ended early.

- When the test sends the signal, the fixture announces itself — writes its pid or a marker file, prints a line —
  only after the disposition is set, from the process that set it, and the test waits for that announcement.
- When the code under test can find and signal the fixture on its own as soon as it exists — a ledger stop round over
  a launch's processes, a sweep — there is no announcement to wait for. Start the fixture with the signal blocked:
  `pthread_sigmask(SIG_BLOCK, ...)` in the child's `pre_exec`, or the same call in whatever launcher execs it (a
  `perl -MPOSIX` one-liner from a shell script); the blocked mask survives `exec` and every thread inherits it. Then
  never unblock it: take it with `sigwait`, and check the signal number it hands back, not only its return value —
  macOS returns 0 for an interrupted wait without filling it in. Installing a handler and then unblocking loses the
  signal on macOS whenever it arrived first: a signal sent while every thread blocks it stays pending on one
  particular thread, and unblocking it on another thread never delivers it — and a Rust fixture run from the test
  binary runs on libtest's test thread, not the main thread. Linux delivers it, so a pass there proves nothing. An
  ignored disposition (`SIG_IGN`) survives `exec`, so a fixture that only ignores the signal sets it before `exec`.

## Assert a stand-in session was sent nothing behind a marker, and that it was sent something by waiting for it

Applies to daemon tests reading what reached a stand-in session: an owner told or not told, a session written to or
not. A stand-in's output is filled in by its reader thread some time after the write, so reading `recent_output`
straight after the call under test passes an absence whether or not the message was sent, and on a stand-in with no
reader thread (`idle_stand_in`) it is always empty.

So use a stand-in started with `test_support::running`, and read what it was sent through
`test_support::heard_after_marker`: it sends a marker of its own through `delivery::write_message`, which queues
every message so none overtakes one sent before it, waits for the marker to arrive, and only then hands back the
output for the absence to be asserted in.

The marker orders only against writes already made when it is sent. Some daemon paths write into a session from a
task they spawn after recording the state a test can wait on — `coordinator::exit_watch::watch_exit` saves the session's
`ended_at` and only then spawns the task that tells the owner — so a marker sent once that state is seen can reach the
stand-in first. A test asserting such a message is present therefore waits for the message's own text with
`test_support::output_containing` and asserts it in what that returns, keeping the marker only for what must be absent
or counted afterwards; reading behind the marker and asserting presence fails intermittently under the full suite's
load. A test asserting the owner was not told of an end first waits with `test_support::exit_settled` for every process
whose end could have told it — including one production code started and watched on its own, such as the process a
relaunch started, which the test reaches through its stand-in launch's list of started processes — and only then reads
behind the marker.

## Seed the agent's model list before a daemon test gives anything a model or an effort

Applies to any daemon test — coordinator, control socket or console session tool — that creates or edits a console
or a project, or opens a session, with a model or a reasoning effort in it. The daemon checks every such value
against the list the agent offers, and for Codex and Grok that list comes from running the CLI installed on the
machine; a list that cannot be read checks nothing. So an unseeded test passes on a machine without the CLI, or
whose catalogue happens to hold the test's model, and fails on another — a made-up model id like `gpt-y` is refused
wherever the CLI is installed. Call `test_support::offer_models` for that agent before the call under test, with the
models and efforts the test uses. It seeds the default account only; a test that gives a model under another account
is checked against that account's own list.

## No daemon test may be able to reach the real agent CLI with an action that spends anything

Applies to a daemon request handler or task that runs an agent CLI for an effect on the account that cannot be undone
— using a Codex reset credit, a kick that starts a usage window — and to any test that sends such a request through
`coordinator::handle`. A daemon test runs in the user's own environment: `env_shell::launch_snapshot` and the other
`env_shell` snapshots take their real login shell, so the CLI that runs is the one installed on the machine, signed in
to their default config directory, and the spend is real. A test meant only to check a refusal is no guard: an agent
whose availability has not been determined yet counts as available (`agent_is_available` in
`apps/daemon/src/accounts/choice.rs`), so a request the test expects to be refused for that goes through to
the CLI.

So such a handler never calls an `env_shell` snapshot directly: it takes its shell environment from a
`#[cfg(not(test))]` / `#[cfg(test)]` pair of functions whose test variant bails (`reset_credit_shell` in
`apps/daemon/src/coordinator/mod.rs`, `held_shell` in `apps/daemon/src/accounts/quotas/poll.rs`), which keeps every test
off the real CLI by construction instead of by each test's care. The action itself is tested one level down, handed the
environment `test_support::fake_agent` returns, whose `PATH` finds only the fake.

## No daemon test may reach the user's live data directory: code under it takes its directory

Applies to daemon code a test can reach that reads, writes or removes anything under Octoboard's data directory (the
holders' registry, the build copies, anything newly kept there). `cargo test` runs with the developer's own `HOME`, so
`paths::data_dir()` and the paths built on it (`paths::holders_dir()`, `paths::builds_dir()`) are their live
`~/.octoboard`: code a test reaches that calls them reads the user's running holders or removes their files. So such
code takes the directory from its caller (`ProcessLedger::new(data_dir)`,
`paths::holders_dir_in(dir)`, the holders' registry and build directories in `holder::client::ClientConfig`), and only
the daemon's own start in `apps/daemon/src/main.rs` passes the global path; a test passes its temporary directory, and
a test that needs a holder uses `holder::client::test_holders` with its own registry directory. Do not add a
`#[cfg(test)]` setter that redirects a global path instead: every test that forgets it, or runs before it is set,
reaches the live directory.

## A hook or transport a test installs into the state holds the state weakly

Applies to daemon tests that install into `AppState` something that calls back into that state: a relaunch hook
(`account_sharing.relaunch`), a session's transport, any closure or struct the state keeps. Capturing
`fixture.state.clone()` there makes a cycle, so the state outlives the test: its process ledger never ends the test's
launches, and on a Linux host with a writable cgroup v2 subtree each launch's group (`octoboard-<instance>-<launch>`)
is left behind after the test binary exits. Nothing shows on macOS or in a plain container. Capture
`Arc::downgrade(&fixture.state)` and `upgrade()` it inside the callback
(`the_account_stays_locked_until_its_sessions_are_resumed` in `apps/daemon/src/accounts/shared_config/tests.rs`).

## Test a daemon budget by lowering it, never by asserting its value

Applies to daemon tests of a limit in `apps/daemon/src/browse/budget.rs` whose real figure no small fixture reaches. A
test asserting the constant equals its figure fails when the figure is retuned and passes when the check enforcing it
is deleted, so it tests nothing. Have the production code read the limit through a getter that a `#[cfg(test)]`
`thread_local!` override can lower — `tree_diff::tree_diff_stdout` with `TREE_DIFF_STDOUT_OVERRIDE` is the shape — set
the override around the call in the test, clear it after, and assert what happens past the limit. The override is per
thread, so it reaches only code running on the test's own thread: the browse fixtures' `serve` calls in directly,
while anything moved onto another thread (`spawn_blocking`, a request through the coordinator) reads the real figure.

## A test whose outcome depends on permissions checks what the machine allows before asserting it

Applies to daemon tests that make a file or directory unwritable or unreadable (`set_permissions` to `0o555`, `0o000`)
and expect the operation under test to fail. A Linux container runs the suite as root unless told otherwise (the
default user of the `rust` images), and root ignores permission modes, so such a test fails there, while on macOS
nothing shows it. Before
asserting the refusal, return when it cannot happen: check `libc::geteuid() == 0` (`apps/daemon/src/path_links/tests.rs`)
or try the write and return if it succeeds (`apps/daemon/src/accounts/relocate/tests.rs`). A test that fails only when
run as root is missing this early return; it is not a regression in the change being verified.

The opposite holds for tests whose code under test checks the directories above its scratch directory (the browser
socket's check that every ancestor of the data directory is closed to writes by others): do not assume the temporary
directory's ancestors pass it. `TMPDIR` can point into a volume whose directories others may write to, where such a
test fails although `/tmp` on Linux and macOS's default temporary directory pass. Run the check on the scratch
directory first and, where it already refuses, assert the refusal instead (`a_private_data_directory_gets_an_owner_only_socket`
in `apps/daemon/src/browser_socket.rs`); to test what is served over the socket, serve its router on a loopback port
or bind the listener on a scratch path directly, past the check.

## A test that returns early for want of a cgroup says so through `test_support::cgroups::skip`

Applies to Linux daemon tests that need a cgroup v2 group the host may not let them make or use (making a group,
changing its mode, moving a process into it). Before returning, call `test_support::cgroups::skip(reason)` (or take
the group through `grouped_launch` / `ChildCgroup::new`, which do), not an `eprintln!("skipped: …")` of your own:
`skip` fails the test where `OCTOBOARD_REQUIRE_CGROUP=1`, as in the CI run that gives the suite a writable hierarchy,
while a test's own line lets a broken setup pass there without running it.

## On tokio's paused clock, nothing in the test blocks or times out on real I/O

Applies to daemon tests on `#[tokio::test(start_paused = true)]`, and to the stretches a real-clock test pauses with
`runtime_settled` / `runtime_advanced` (`apps/daemon/src/test_support/clock.rs`). The paused clock jumps to the next
timer as soon as the runtime has nothing to run; a task waiting on I/O counts as nothing to run, while a
`spawn_blocking` task (so any `tokio::fs` call) holds the jump until it ends. `pause()` works only on a current-thread
runtime and panics on a multi-thread one.

- A `tokio::time::timeout` or a sleep-bounded wait around real I/O — a socket, a pipe, a child process — runs out at
  once. A test that waits on real I/O stays on the real clock and bounds the wait by `PATIENCE`.
- Do not call `test_support::within`, `output_containing` or `heard_after_marker` from a paused test: `within` polls
  in `spawn_blocking`, which holds the clock, so a condition that needs a timer to fire is never met and the wait runs
  to its deadline; the other two block the runtime's thread outright. Let the clock run the code's own timers instead
  (`tasks_idle`, or a helper of the module's like `polls_ran` in `apps/daemon/src/adapter/claude/transcript_watch.rs`)
  and assert after it.
- `tasks_idle` and `runtime_settled` show that the code under test reached its wait only when everything before that
  wait is CPU work or `spawn_blocking`. With real I/O in between they return before the code got there, and an
  absence asserted after them passes whatever the code does: wait for an event instead.

## A test that needs a port nothing listens on takes `test_support::ClosedPort`

Applies to daemon tests that connect to a loopback port expecting the connection to be refused. Never bind port 0 and
drop the listener to get one: the system hands that port to the next test in parallel that binds port 0, and the
connection reaches that test's server instead of being refused. Holding the port in a bound socket that never listens
does not work either: on macOS the connection times out rather than being refused. `ClosedPort::new()` picks a port
below the ephemeral range, which no other test is given.
