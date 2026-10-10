> Severity: Minor

# The toast test leaves HeroUI's toast queue mid-exit, updating React outside `act`

## Symptom

`packages/ui/src/components/Toasts.test.tsx` clears HeroUI's toast queue outside `act`, which updates React
state four times without the test's knowledge and leaves four of the queue's exit timers pending when the
test file's jsdom environment is torn down.

## Reproduction steps

Nothing shows in the suite's output, so the reproduction is an instrumented run. Rebuilding the instrument:

1. In `packages/ui`, write a vitest setup file (any name; it must sit inside the package, because Vite's
   file-system allow-list rejects a setup file outside the package root) that:
   - wraps `globalThis.setTimeout` so each call records its id, its delay and `new Error().stack` in a map,
     and removes the entry when the callback runs;
   - wraps `globalThis.clearTimeout` so it removes the entry too;
   - wraps `console.error` to record `new Error().stack` whenever the first argument contains
     "not wrapped in act";
   - registers an `afterAll` that appends the still-pending entries, with their stacks, to a file outside the
     package (`appendFileSync`), together with `globalThis.__vitest_worker__.filepath` so each line says which
     test file it came from.
2. Write a vitest config beside `packages/ui/vite.config.ts` that spreads that config's resolved value and
   adds `test: { setupFiles: ["./<the setup file>"] }`. Vitest 5 has no `--setupFiles` command-line option, so
   the config is the only way in.
3. Run `npx vitest run --config <that config>` in `packages/ui`.
4. Read the log: the entry for `Toasts.test.tsx` reports four pending timers of 300 ms and four
   "not wrapped in act" records.

The counts came out the same on each instrumented run made, and the trigger is a synchronous call in the
test's own `finally`, so neither load nor the order files run in is involved. A plain
`npx vitest run` in `packages/ui` reports 68 files and 615 tests passing with no warning and no error about
this, so the suite gives no sign of it.

## Expected vs. actual

- **Expected**: the test's own calls that update React state run inside `act`, and the test leaves no timer of
  the toast queue pending when it ends. The file opts into React's test contract at its line 13
  (`IS_REACT_ACT_ENVIRONMENT = true`), and React's own rule for that mode — reported by the
  "An update to ... inside a test was not wrapped in act(...)" warning it prints — is that every update goes
  through `act`. An update React schedules outside `act` goes to the real scheduler, and the same shape in
  `packages/ui/src/browser/ChangeList.test.tsx` is what makes that file throw
  `ReferenceError: window is not defined` as a vitest unhandled error in roughly one full run in six (see
  Leads).
- **Actual**: four updates are scheduled outside `act`, and four 300 ms timers of the queue are still pending
  when the file's environment goes.

## Environment

- `packages/ui` under vitest 5.0.3, jsdom 30.1.2 (per-file, via the file's `// @vitest-environment jsdom`),
  default isolation (one worker per test file), React and react-dom 19.3.0, `@heroui/react` 3.2.6.
- macOS 26.2, Node 22.22.2. Nothing about the reproduction looks platform-specific, but it was only run here.
- `Toasts.test.tsx` holds a single test that loops over four window widths, which is where the four of
  everything comes from.

## Scope of impact

No user is affected and nothing currently fails: the four updates are scheduled after the test's last
assertion, and the four timers come due after the root has been unmounted. What is at risk is the
suite's trustworthiness — these are updates and timers the test does not control, of the kind that already
produces an intermittent unhandled error elsewhere in this package. There is no workaround to apply; the suite
simply does not report it.

## Leads

- **Verified by instrumented run (stacks)**: all four pending timers are scheduled at
  `@heroui/react/dist/components/toast/toast-queue.js:202` (`ExitAwareToastQueue.close`), reached from
  `ExitAwareToastQueue.clear` (line 222) and `ToastQueue.clear` (line 282) under
  `Function.toastFn.clear` (line 408) — that is, from `toast.clear()`.
- **Verified by instrumented run (stacks)**: all four outside-`act` updates run
  `ExitAwareToastQueue.notifyExitSubscribers` (toast-queue.js:182) from `ExitAwareToastQueue.clear`
  (line 227), into `forceStoreRerender` and `scheduleUpdateOnFiber` in react-dom.
- **Verified in the code**: `Toasts.test.tsx:62-66` is a `finally` that runs `toast.clear()` first and
  `act(() => root.unmount())` second, with the `toast.clear()` call outside any `act`.
- **Verified in the code**: the queue is a module-level singleton inside `@heroui/react` — `toast.clear()`
  reaches it directly, and `close()` keeps its timers in the queue's own `exitTimers` map — so unmounting the
  root cannot cancel them.
- **Verified in the code**: `exitDuration` is the 300 ms the pending timers show, and `close()` only takes the
  timer path when `window.matchMedia` reports no reduced-motion preference; the test's `stubWidth` installs a
  `matchMedia` stub that answers `false` to that query.
- **Verified by repeated runs, elsewhere**: the same class of defect in
  `packages/ui/src/browser/ChangeList.test.tsx` — a React update scheduled outside `act` that is still in the
  scheduler when the environment is disposed — surfaced as `ReferenceError: window is not defined` in 3 of 18
  full runs of `packages/ui` on `main`, thrown at react-dom's `performWorkOnRootViaSchedulerTask` on its first
  statement, `schedulerEvent = window.event`. That file is being fixed on its own branch; this ticket is a
  different root cause in a different file.
- **Inferred**: the pending timers do no React work when they fire, because the queue's React subscriber goes
  with the unmounted root — which is why this shows up as a risk rather than as a failure today.
- **Inferred**: because the queue outlives any root, the two halves (the outside-`act` notify and the pending
  timers) are one defect — the test hands work to a singleton and ends before that singleton is quiet — rather
  than two independent ones.
- **Not traced**: whether `toast.clear()` can be made to settle the queue synchronously, and whether the queue
  exposes any way to wait for its exits. Only the call sites above were read.

## Acceptance criteria

- [ ] The instrumented run described above reports no pending timer for `Toasts.test.tsx` at teardown.
- [ ] The instrumented run reports no "not wrapped in act" record for `Toasts.test.tsx`.
- [ ] The test still asserts the toast region's placement class for all four widths (390, 639, 640, 1280), and
  still fails if a placement is wrong.
- [ ] The toast queue holds no queued and no exiting toast once the file's last test has finished, so the next
  file to run cannot see one.
