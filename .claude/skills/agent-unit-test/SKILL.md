---
name: agent-unit-test
description: Add or update unit tests for this round's changes and run the affected tests; it should come to mind once the source changes are done, before delivery. Use when the user says "add unit tests", "write tests", "this file has no tests yet", "run the affected tests".
---

# Unit tests

What follows only sets the principles and the boundaries that must not be crossed; judge the rest of the details
yourself.
When unsure, report back: stop, spell out what needs deciding (with the options) or what information is missing,
together with what you have already found out, and let the recipient of the report (the user, or the dispatcher
passing it on to the user) decide before going on; do not decide for yourself.

## Project conventions

"Project conventions" is filled in only by the installer and is not changed at run time. If an item you need is still
`<…>`, a filled-in command or path is itself no longer valid (unrelated to this round's changes; missing environment
conditions such as a database or network are reported as usual and do not count), or something has to change along
with the project, stop and ask the user to rerun the installer that installed this skill, saying which item has to
change and why.

A filled-in framework that is no longer among the project's dependencies counts as no longer valid. If this round's
change is itself the switch of framework or of the test command, run with the changed command and remind the user in
the final reply to rerun the installer to update "Project conventions".

When filling in and reinstalling, check against the project as it stands, and keep what cannot be determined; in a
monorepo each package gets its own row in the table, and where the two items below the table differ per package,
write each out separately.

| Scope | Framework | Test root | Path mapping | Shared helpers | Run all | Run given files | Select tests by change |
|---|---|---|---|---|---|---|---|
| `packages/ui` | Vitest | None | `src/<path>/<name>.ts(x)` → `src/<path>/<name>.test.ts(x)`; `scripts/<name>.mjs` → `scripts/<name>.test.mjs` | `src/test/` (setup, `testDaemon`, `mountHost`, hosts, the clock helpers `clock.ts`, `lazyImport.ts`, `alertQueue.ts`), `*TestSupport.ts(x)` / `testSupport.tsx` / `*TestState.tsx` files beside the module that uses them, `scripts/test-fixtures/` for the `scripts/*.test.mjs` tests; tests may read `src/fixtures/` (data shared with the gallery), which is changed only through the `agent-gallery` skill | `pnpm --filter @octoboard/ui test` | `pnpm --filter @octoboard/ui exec vitest run <file>` | `pnpm --filter @octoboard/ui exec vitest related --run <changed files>` |
| `apps/daemon`, `apps/desktop/src-tauri` | cargo test | None | `src/<path>/<name>.rs` (or `<name>/mod.rs`) → `#[cfg(test)] mod tests` in the same file, or `src/<path>/<name>/tests.rs` (a `tests/` directory when large) declared in the module file with `#[cfg(test)] mod tests;`; some modules split their tests by topic into `<topic>_tests.rs` beside them | `apps/daemon/src/test_support/` (`#[cfg(test)]` module of the daemon crate) | `pnpm test:rust` | `node scripts/cargo.mjs test -p <crate> <test path filter>` (`<crate>` is `octoboardd` or `octoboard`) | None |
| `apps/desktop` scripts | node:test | None | `scripts/<name>.mjs` → `scripts/<name>.test.mjs` | None | `pnpm --filter @octoboard/desktop test` | `node --test <file>` | None |
| `apps/web` | node:test (run with `--experimental-strip-types`) | `apps/web/test/` | `<path>/<name>.ts` → `test/<name>.test.ts` | None | `pnpm --filter @octoboard/web test` | `node --experimental-strip-types --test <file>` | None |
| `scripts/` (repository root) | node:test | None | `scripts/<path>/<name>.mjs` → `scripts/<path>/<name>.test.mjs` | None | `node --test 'scripts/**/*.test.mjs'` | `node --test <file>` | None |

- What to do with existing tests that sit in the same directory as the source (those placed by a language convention
  excepted): option ③, new and old tests both go next to the source (every scope but `apps/web`, which has no such
  tests: None).
- Languages whose tests are placed by language convention: Rust (the `#[cfg(test)] mod tests` module in the source
  file, and the `tests.rs` / `tests/` submodule files beside a module, as `apps/daemon` already does).
- Tests that depend on time (every scope; mandatory for every test): a test's outcome must not depend on how fast the
  machine runs it. A test that passes alone and fails under the full suite's load is a broken test, never an
  intermittent one to rerun. The quick check's solo rerun in `CLAUDE.md` only lets a merge go ahead; it does not clear
  the test, which is still fixed or reported.
  - **No inline real-time waits.** No sleep, `setTimeout`, `thread::sleep` or fixed-length polling loop written in a
    test to let something happen. Where the code under test waits (a delay, debounce, cooldown, retry), drive a fake
    clock started before the code arms its timers (before mounting): in `packages/ui`, `fakeTimers()`
    (`packages/ui/src/test/clock.ts`), or `fakeTimersKeepingFrames()` under `framesAsTimers()`; Rust
    `#[tokio::test(start_paused = true)]`, `tokio::time::pause()` (crates with tokio's `test-util`) or an injected
    clock. Durations come from the production constant (import it), never a literal; a library delay that cannot be
    imported is a named constant in the support code, citing its source, as `MOUNT_REFOCUS_MS` does.
  - **Real time only through the named helpers.** Where real time cannot be avoided — a library singleton the test
    cannot reset or fake, a library step that only completes on the real event loop, a timer that a test which has
    to stay on the real clock cannot avoid meeting, or an ordering yield past queued promise work — the wait lives in
    one named helper beside the other test support code, documented with its reason, and tests call it. The sanctioned
    set:
    - `trackMountRefocus` and `settleFrames` (`packages/ui/src/components/dialog/dialogTestSupport.ts`);
    - the react-aria tooltip waits in `packages/ui/src/components/hint/tooltipTestSupport.ts`: the touch-hover
      suppression (in `press`) and `tooltipsCooledDown`;
    - the lazy-import settle, `settleLazyImport` (`packages/ui/src/test/lazyImport.ts`);
    - the status region's first-write wait, `firstWriteDelayPassed`
      (`packages/ui/src/components/announce/statusAnnouncerTestSupport.ts`), for a test that must mount on the real
      clock; otherwise fake the clock before mounting and advance `FIRST_WRITE_DELAY_MS`;
    - `afterQueuedWork` and `flushPromises` (`packages/ui/src/test/clock.ts`), one real 0 ms macrotask that lets
      queued microtasks run, for code that finishes on promise chains alone; where a positive assertion follows,
      wait on it instead;
    - in the daemon, `next_millisecond` (`apps/daemon/src/test_support/clock.rs`), for production stamps in
      wall-clock milliseconds.

    A new exception is added only as such a helper, with its reason, waiting through `waitOnRealClock`
    (`packages/ui/src/test/clock.ts`), which tests never call directly. Daemon tests that drive a real child process
    follow the real-process rules in `docs/memory/writing-daemon-tests.md` (the patience window, the short-timeout
    override getters), not a fake clock.
  - **Let React commit between timers.** In a `packages/ui` component test, advance the fake clock inside `act`, and
    where more than one timer is pending while React renders, one timer per step
    (`await act(async () => { await vi.advanceTimersToNextTimerAsync(); })`), so what one timer did is committed
    before the next fires. Advancing by a production constant in one step, or `vi.runOnlyPendingTimers()` inside one
    `act` to settle at teardown, is fine where no render between the timers matters to the assertion.
  - **Wait on the condition, not the clock.** Where the code under test finishes on its own (a reply, a lazy import,
    a render), wait until the asserted state appears (`waitFor`, `findBy…`, an awaited promise) and assert what was
    waited for; never read once after a fixed advance or delay.
  - **Never assert "unchanged" inside a window where a pending frame or timer can change it.** Before asserting that
    focus, a tooltip or a value stayed put, either stop the frame or timer that would move it (stub
    `requestAnimationFrame` with `vi.stubGlobal`, removed in `onTestFinished`; fake the timers) or let it run out and
    assert after it. Known sources: a containing react-aria `FocusScope` puts focus back one frame after it leaves the
    scope; `useDialog` refocuses 500 ms after mount when focus is on the dialog or `<body>` (`trackMountRefocus()`,
    armed after the dialog mounts); the tooltip warm-up and the status region's first write are in
    `docs/memory/writing-automated-tests.md`.
  - **Leave nothing running.** A test that faked timers runs out its pending timers and asserts `vi.getTimerCount()`
    is 0 before restoring real ones; teardown is bound to the test with `onTestFinished`. In `packages/ui`,
    `fakeTimers()` does all of this.
  - **Prove it under load.** On top of the single run that "Run scope" and `test-writer` prescribe, a new or changed
    test that touches time is also run under load (for example the file run several times in parallel, or with
    `--sequence.shuffle`), and a fixed-delay check (an injected delay before the assertion) shows it still passes;
    `test-writer` does both before it reports, and its report says how; the dispatcher checks that it does.

## Framework configuration requirements

Whether the framework is newly installed or already in the project, its configuration has to satisfy:

- Collect the test locations that "Project conventions" sets; with option ① or ②, old tests left in the source
  directory are collected as well, and no other location is collected. The same test file name is allowed in
  different directories, as a mirrored structure necessarily produces it.
- Test files are included in type checking and lint, and excluded from build output and published packages.

## Wrap-up: add tests

When you are a subagent, do not follow this skill: finish and hand back, and let whoever dispatched you arrange it.

1. List the source files changed this round, new, renamed and deleted ones included.
2. **Dispatch the `test-writer` subagent to add the tests; when there is a subagent mechanism, do not write them
   yourself** (for the case without one see below the table): whoever wrote the code writes assertions that follow
   their own implementation, and the tests merely stamp the implementation. How to write is in its own definition;
   you only hand over the list of changes, the behavior each change **should have**, and the trade-offs the user
   settled: it cannot see your conversation, and any expectation you leave out can only be inferred back from the
   implementation. When only untestable files changed (the criterion is at the end of this section), there is no need
   to dispatch; run per "Run scope" yourself.
3. Work through its report item by item with the table below.
4. If the source changes again afterwards (review comments, a defect fix), rerun the affected tests; if that produces
   new behavior, dispatch it again to add tests.

| In the report | What to do |
|---|---|
| A failing case points to a defect introduced by this round's changes | You fix the source; do not have it change the assertion to fit the implementation |
| A failing case points to a pre-existing defect outside this round's scope | Handle per "When tests find a source defect" |
| The test itself is wrong | It fixes it |
| Expected behavior unclear, listed as undecided | You decide from the requirement; if the requirement cannot settle it either, leave it to the user; if after it is settled the implementation does not match, handle per "When tests find a source defect" |
| Hard to test, needs a change to the source structure | You judge whether to change it; leave it to the user when the change is large |
| Suggests migrating old tests in bulk | Leave it to the user |

- The subagent this skill dispatches is installed together with it: if there is a subagent mechanism but the
  definition of the subagent to dispatch cannot be found, the installation is incomplete: stop and ask the user to
  rerun the installer that installed this skill. With no subagent mechanism, read the installed definition and do the
  work yourself following the rules inside.

How to judge whether a file is testable:

A file with logic of its own (branches, computation, transformation, validation, state changes) is testable. The only
untestable ones are:

- files holding only type declarations, constants, or re-exports
- configuration files and generated code
- files that only do wiring or entry-point glue and have no behavior to assert without a real process, network,
  database, or browser rendering

## When tests find a source defect

- A defect introduced by this round's changes: you fix the source. A pre-existing defect outside this round's scope:
  leave it to the user to decide whether to fix it in this round or hand it off first.
- An implementation that looks suspicious but whose expected behavior has no basis to judge by does not count as a
  defect: do not write a guessed expectation into a test, leave it to the user; where a ruling is given, write the
  test to it, and only when the implementation does not match does it go the defect route; where it cannot be
  settled, go through the project's requirements process to settle the expectation first.

Where the decision is to hand it off, all of the following must be done:

- **Every defect has a reproduction case that asserts the expected behavior**; a list in words alone is not a
  handoff.
- A case that fails because the source does not meet expectations does not get turned green by loosening the
  assertion, asserting the defective behavior instead, skipping, or deleting it; suspending it as a known failure per
  convention does not count as skipping.
- The record must be handable over on its own: if the project has its own way of recording defects, use it; if not,
  list them in the final reply and let the user decide where they go. Its elements are the location, the symptom, the
  expected behavior and its basis, the file path and case name of the reproduction case, how to restore it to a
  running state, and a suggested fix (only when one is evident); one record per defect (or per group with the same
  root).
- References go one way: the record points to the test, and the test does not write the record's path, which would
  become a dead reference once the record is deleted.
- Where no reliable reproduction can be written (dead code, something fixed at load time), do not force one; state in
  the record why.
- To suspend a reproduction case, first reuse the way the project already uses; second, the "known failure" marker
  that reports an error in reverse once the framework is fixed (`it.fails`, strict `xfail`, etc.); if there is none,
  comment the whole case out with a `TODO` line above stating the reason, the condition for restoring it, and the
  defect's symptom. After it is restored and before the fix it must fail.

The change that fixes the defect restores the reproduction case and clears the corresponding record at the same time.

## Run scope

By default run only the **affected tests**: the test files added or modified this round, and the test files whose
import chain (direct or indirect) contains a changed source file. Where the framework has its own ability to select
tests by change or dependency, prefer it, but confirm that the range it selects covers these.

Switch to running the **full suite** when the change reaches everywhere (test configuration, global setup, shared
helpers, dependency versions, build configuration, a file depended on widely), or when the dependency surface cannot
be traced (dynamic import, registration by string, reflection, dependency injection, a referenced file deleted or
renamed); when unsure, run the full suite too.

State in the final reply which scope was run and why.
