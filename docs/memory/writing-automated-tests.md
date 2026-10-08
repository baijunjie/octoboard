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
dispatcher `eval`s, reached through a symlink — creating and resolving a symlink carries none of
the first-exec cost.

The symptom points the wrong way, so suspect this before changing a timeout or calling a test
flaky: a test that fails only when the suite runs in parallel reads as a too-short timeout in the
code under test, and `--test-threads=1` apparently fixing it reinforces that reading.

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

## End a stand-in session's process in the test that spawned it

`fake_live_session` (in `apps/daemon/src/state.rs`'s test module) spawns a real `/bin/sh` on a real PTY, and
`LiveSession` has no `Drop` that ends it. A test whose stand-in script outlives the test body (`"sleep 30"` and
the like) therefore leaves that process, its PTY master and its reader thread held for the rest of the test
binary's run — they go when the binary exits and the master closes, so nothing survives the suite, but a whole
file's worth of them piles up while it runs. So end every such session before the test returns with
`live.terminate()`, except where the behaviour under test already ended the process and the test asserts that
(`poll_exit()`).
