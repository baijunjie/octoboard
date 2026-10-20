# 05 Split the slowest UI test files

> Goal: no single UI test file dominates the run, and the cause of the slow tests is known.
> Completion criteria: the cost per test of the slowest files is explained from profiling; those files are split so
> the longest file is a small share of the run (measured against milestone 01); the UI suite passes at the worker
> cap from milestone 02 with no timeouts across three runs under normal load.

## Implementation plan

- [ ] Profile the slowest files (the session, console and project dialogs, the agent accounts section, the hand-off
      dialog) to find whether time goes into imports, rendering or focus handling.
- [ ] Fix a shared cause where one is found (a test helper, a real timer that can be fake).
- [ ] Split each file along its existing groups of behaviour, keeping shared helpers in the dialogs' test support.

## Notes for the developer

- **Reusable capabilities**: the dialogs' shared test support module.
- **Development notes**: some waits are required to settle lazy imports; keep them. Splitting must not lose a test;
  compare test counts before and after.
- **Reference docs**: `docs/memory/writing-automated-tests.md`.
