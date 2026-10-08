# Writing automated tests

## Write only the unit tests a rule needs, one case per rule

The user wants tests lean and has had over-written ones trimmed. Test a module's rules, not every variant of them:
one representative case per rule, in a table (`it.each`) when several rules share one shape, rather than a
`describe` block per behaviour with a handful of near-duplicate cases each.

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
  polls up to `PATIENCE` from `apps/daemon/src/test_support.rs`; a poll returns early, so a
  passing test pays nothing. Do not assert an elapsed time tighter than a stall: show "fails
  fast" by which error came back, and keep wall-clock checks only for ruling out a wait of
  minutes.
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

## Verify behaviour derived from an agent's output by replaying a committed capture

Anything the daemon derives from what an agent CLI emits — a hook payload, the transcript it writes,
the screen it draws — is verified by replaying a real capture through the real function, not by
staging a live session. Producing that output live costs a model turn, writes into the user's own
agent configuration and their live Octoboard data, and for the paths that need a person at the
keyboard (answering a prompt, cancelling a turn) cannot be scripted at all.

This holds even when the acceptance criteria are written in terms of what the window shows: where the
UI only renders what the daemon derived, the derivation is the new link, and that is what has to be
exercised. Capture the output once from a real session, commit it under `apps/daemon/testdata/` with the
CLI version it came from recorded beside it, and replay the production entry conditions too, not just
the bytes — the offset the real caller would have started from, the truncation a half-finished write
leaves. Commit the near-miss capture next to the matching one: the shape that must *not* be
recognised is what makes recognising the other one mean anything.
