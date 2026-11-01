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
`exec` on the machine blocks, and so do `kill` and `waitpid` on a child stuck in one. A healthy
daemon suite takes about 5 s; a run hit by a stall takes 10-30 s and fails whatever was bounded
in single seconds.

- A wait for anything a spawned process does — output, a file, an event it causes, its exit —
  polls up to `PATIENCE` from `apps/daemon/src/test_support/mod.rs`; a poll returns early, so a
  passing test pays nothing. Do not assert an elapsed time tighter than a stall: show "fails
  fast" by which error came back, and keep wall-clock checks only for ruling out a wait of
  minutes.
- Wait for the very condition the test goes on to assert, not for an earlier event that ought to imply it. On macOS
  a process stops being readable (`proc_pidinfo`, the ledger's `table::start_of`) as soon as its exit begins, while
  the kernel reparents its children to pid 1 later in that exit, so "the parent is gone" does not yet mean "its child's
  `ppid` is 1": a test asserting a reparenting waits for the child's `ppid` itself.
- A test proving that a production timeout fires keeps that timeout short. When the fixture was
  killed before it reached the point under test, nothing was tested: run the whole scenario
  again until `PATIENCE` runs out rather than failing or lengthening the timeout.
- Where the test cannot widen the window the production code times (a stand-in answering inside
  the daemon's own timeouts), the fixture does its one-time setup first and uses only shell
  builtins after it, so no exec lands inside that window.

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
  a launch's processes, a sweep — there is no announcement to wait for. Block the signal in the child's `pre_exec`
  (`pthread_sigmask(SIG_BLOCK, ...)`): the blocked mask survives `exec` and a handler does not, so the fixture
  installs its handler, then unblocks, and a signal sent in between stays pending for the handler. An ignored
  disposition (`SIG_IGN`) also survives `exec`, so a fixture that only ignores the signal can set it before `exec`.

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
`apps/daemon/src/coordinator/account_choice.rs`), so a request the test expects to be refused for that goes through to
the CLI.

So such a handler never calls an `env_shell` snapshot directly: it takes its shell environment from a
`#[cfg(not(test))]` / `#[cfg(test)]` pair of functions whose test variant bails (`reset_credit_shell` in
`apps/daemon/src/coordinator/mod.rs`, `held_shell` in `apps/daemon/src/accounts/quotas/poll.rs`), which keeps every test
off the real CLI by construction instead of by each test's care. The action itself is tested one level down, handed the
environment `test_support::fake_agent` returns, whose `PATH` finds only the fake.

## Test a daemon budget by lowering it, never by asserting its value

Applies to daemon tests of a limit in `apps/daemon/src/browse/budget.rs` whose real figure no small fixture reaches. A
test asserting the constant equals its figure fails when the figure is retuned and passes when the check enforcing it
is deleted, so it tests nothing. Have the production code read the limit through a getter that a `#[cfg(test)]`
`thread_local!` override can lower — `tree_diff::tree_diff_stdout` with `TREE_DIFF_STDOUT_OVERRIDE` is the shape — set
the override around the call in the test, clear it after, and assert what happens past the limit. The override is per
thread, so it reaches only code running on the test's own thread: the browse fixtures' `serve` calls in directly,
while anything moved onto another thread (`spawn_blocking`, a request through the coordinator) reads the real figure.
