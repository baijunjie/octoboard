# Writing automated tests

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
