# Shared app state, stand-in session and wait helpers

> Goal: the app-state fixture, the stand-in live session and the wait-for-a-condition helper exist once each, in the
> shared test-support module.
> Completion criteria: no test module defines its own copy of any of the three; the whole daemon test suite passes;
> no stand-in process outlives the test that started it; and the suite's wall-clock time is no worse than before.

## Technical design

- [ ] An app-state fixture: opens a store in a scratch directory, inserts the one console the tests need, and returns
      the state together with the scratch directory guard that owns it, so the caller keeps both alive for the test.
- [ ] A stand-in live session on a PTY, registered as a live session, serving the two existing needs: one that runs a
      given script with a reader thread attached, and one that merely stays alive with no reader.
- [ ] Termination of the stand-in belongs to the fixture that owns it, so a failing assertion still reaps the process.
- [ ] A wait-for-a-condition helper taking a timeout and a predicate, replacing fixed sleeps.

## Implementation plan

- [ ] Settle the two "Open" questions in the overview first — whether the wait helper is shared with the existing
      production one, and whether the two stand-in sessions become one fixture or two sharing their PTY setup.
- [ ] Move the app-state fixture in, one test module at a time, keeping each module's current return shape working.
- [ ] Move the stand-in session in, then have each test that terminates it by hand rely on the fixture instead.
- [ ] Remove the duplicated wait helper from the module that copied it.

## Notes for the developer

### Reusable capabilities

- The scratch directory guard from the previous milestone: the app-state fixture is built on it rather than
  constructing a directory of its own.
- The `trust` module's tests hold the fuller stand-in session (script plus reader thread) and the `transcript`
  module's hold the inert one; both already work, so this is consolidation, not new behaviour.

### Development notes

- Terminating a stand-in is a blocking call that needs no async runtime, so it can run from a `Drop`.
- A fixture that registers a live session leaves it in the daemon's process-wide registry for the life of the test
  binary; that is accepted today, and this milestone is not the place to change it.
- The point of the wait helper over a fixed sleep is that a test must not fail merely because a write landed later
  than a fixed budget assumed. It does not make a test that waits for nothing to happen any faster — such a test
  has to wait out its window either way.

### Reference docs

- `docs/memory/writing-automated-tests.md` — the fixture conventions this project's tests need on macOS.
