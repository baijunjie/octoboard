# A shared scratch directory for the daemon's tests

> Goal: one scratch-directory fixture, owning its own cleanup, used by every test module that currently builds its own.
> Completion criteria: no test module constructs a temporary directory name itself; the whole daemon test suite passes;
> and the system temporary directory holds no `octoboardd-*` directories after a full run, including a run in which a
> test is made to fail deliberately.

## Technical design

- [ ] A guard type representing one scratch directory, created from a caller-supplied label, dereferencing to the
      directory path so it is passed where a path is wanted.
- [ ] Its name combines the label, the process id and a per-process counter — a counter rather than a thread id,
      which is reused within a process.
- [ ] Creation removes any directory of that name first, then creates it.
- [ ] `Drop` removes the directory, so a test that fails part-way still cleans up.
- [ ] A convenience for the common case of one file inside a scratch directory, where the caller wants the file's
      path and wants the directory to go away with it.

## Implementation plan

- [ ] Add the test-support module, compiled only under test, and declare it alongside the daemon's other modules.
- [ ] Move the scratch-directory construction out of each test module that has its own copy, one module at a time,
      leaving each module's directory label as it is today.
- [ ] Replace each hand-written removal at the end of a test with the guard's own, including the ones that currently
      run only on the happy path.
- [ ] Give the modules that never removed their directory the same guard, so the accumulation stops.

## Notes for the developer

### Reusable capabilities

- The `trust` module's tests already hold the pattern this generalises: remove before creating, and remove again when
  done. Read it before designing the guard rather than after.
- The `transcript` module's tests already hold a guard of this shape for one file inside a directory.

### Development notes

- Scratch directory names are the only thing that attributes a leftover directory to the test that made it, so
  keep each module's existing label.
- Where a scratch directory holds a SQLite database, a name that can repeat plus a leftover database means a failing
  unique constraint on insert, not an overwritten row — this is the failure the remove-before-create step prevents.
- A fresh executable created per test serialises on macOS code-signature evaluation and makes a suite flake only
  under parallel execution; if this milestone touches anything that writes an executable, keep the existing one
  shared rather than creating one per test.

### Reference docs

- `docs/memory/writing-automated-tests.md` — the fixture conventions this project's tests need on macOS.
- `docs/project-map.md` and the daemon's own `README.md` — where a new module has to be registered.
