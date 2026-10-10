> Severity: Minor

# The file viewer's tests let the renderer's lazy import resolve outside `act`

## Symptom

`packages/ui/src/viewer/FileViewer.test.tsx` renders the code surface, whose renderer is a `React.lazy`
component, in tests that never wait for that import, so React retries the suspended render three times
outside `act` — while the tests go on asserting against the tree it is about to change.

## Reproduction steps

Nothing shows in the suite's output, so the reproduction is an instrumented run. Rebuilding the instrument:

1. In `packages/ui`, write a vitest setup file (any name; it must sit inside the package, because Vite's
   file-system allow-list rejects a setup file outside the package root) that wraps `console.error` and, when
   the first argument contains "not wrapped in act", appends `new Error().stack` to a file outside the package
   (`appendFileSync`), together with `globalThis.__vitest_worker__.filepath` and
   `expect.getState().currentTestName` so each record says where it came from.
2. Write a vitest config beside `packages/ui/vite.config.ts` that spreads that config's resolved value and
   adds `test: { setupFiles: ["./<the setup file>"] }`. Vitest 5 has no `--setupFiles` command-line option, so
   the config is the only way in.
3. Run `npx vitest run --config <that config>` in `packages/ui`.
4. Read the log: three records come from `FileViewer.test.tsx`, each with `pingSuspendedRoot` as the frame
   below React's warning.

The count came out the same on each instrumented run made, and the trigger is an import resolving in a
microtask, so neither load nor the order files run in is involved. A plain
`npx vitest run` in `packages/ui` reports 68 files and 615 tests passing with no warning and no error about
this, so the suite gives no sign of it.

## Expected vs. actual

- **Expected**: a test that renders the code surface has the suspended render settled before it asserts, and
  leaves no retry for React to make on its own. The file opts into React's test contract at its line 13
  (`IS_REACT_ACT_ENVIRONMENT = true`), and React's own rule for that mode — reported by the
  "An update to ... inside a test was not wrapped in act(...)" warning it prints — is that every update goes
  through `act`; an update React schedules outside `act` goes to the real scheduler, where the test no longer
  decides when it lands. The same shape in `packages/ui/src/browser/ChangeList.test.tsx` is what makes that
  file throw `ReferenceError: window is not defined` as a vitest unhandled error in roughly one full run in
  six (see Leads).
- **Actual**: three suspense retries are scheduled outside `act`, between and during the tests that render a
  diff.

## Environment

- `packages/ui` under vitest 5.0.3, jsdom 30.1.2 (per-file, via the file's `// @vitest-environment jsdom`),
  default isolation (one worker per test file), React and react-dom 19.3.0.
- macOS 26.2, Node 22.22.2. Nothing about the reproduction looks platform-specific, but it was only run here.
- The file mocks `./renderer` with an async `vi.mock` factory, so the lazy import resolves through the mock
  rather than through the real rendering library, which does not run under jsdom.

## Scope of impact

No user is affected and nothing currently fails. What is at risk is the suite's trustworthiness: an assertion
about the code surface can read a tree that React is about to replace, which is how a test comes to pass or
fail for the wrong reason, and the retries are of the same kind that already produces an intermittent
unhandled error elsewhere in this package. The file's own teardown is sound — it unmounts its root in
`afterEach` — and no timer of its own is left pending, so the retries are the only loose end. There is no
workaround to apply; the suite simply does not report it.

## Leads

- **Verified by instrumented run (stacks)**: all three records sit directly on `pingSuspendedRoot`
  (`react-dom/cjs/react-dom-client.development.js:20302`), which is React's path for "a promise the suspended
  render was waiting on has resolved" — so the update comes from a suspense retry, not from a timer.
- **Verified by instrumented run**: the same run reports no pending `setTimeout` for this file at teardown, so
  the retries are not waiting on a timer of the test's own.
- **Verified by instrumented run**: all three are attributed to the test that was current when they fired, the
  first case of the `it.each` at `FileViewer.test.tsx:229-241`
  ("offers the diff layout for an added file, which has one side"). The attribution says which test was
  running at that moment, not which render suspended, so the originating render may be an earlier test's.
- **Verified in the code**: `packages/ui/src/viewer/CodeSurface.tsx:22-23` declares `HighlightedFile` and
  `RenderedDiff` with `lazy(() => renderer().then(...))`, and line 155 renders the surface's children inside
  `<Suspense fallback={null}>`. That lazy import is the promise a render suspends on.
- **Verified in the code**: the `it.each` at `FileViewer.test.tsx:229-241` is synchronous — it calls `show()`,
  which renders inside `act`, and asserts straight away — while the test just above it
  (`FileViewer.test.tsx:208`, "shows it is loading until the renderer has drawn the code") does
  `await act(() => new Promise((resolve) => setTimeout(resolve, 0)))` after rendering.
- **Verified by repeated runs, elsewhere**: the same class of defect in
  `packages/ui/src/browser/ChangeList.test.tsx` — a React update scheduled outside `act` that is still in the
  scheduler when the environment is disposed — surfaced as `ReferenceError: window is not defined` in 3 of 18
  full runs of `packages/ui` on `main`, thrown at react-dom's `performWorkOnRootViaSchedulerTask` on its first
  statement, `schedulerEvent = window.event`. That file is being fixed on its own branch; this ticket is a
  different root cause in a different file.
- **Inferred**: a synchronous test that renders a lazily-backed surface cannot have the lazy child in the tree
  when it asserts, since the import resolves no earlier than a later microtask; what it sees inside the
  `Suspense` boundary is the `null` fallback.
- **Not traced**: which of the file's tests each retry belongs to, and whether any current assertion changes
  its result once the retry lands. Only the warning stacks and the two call sites above were read.

## Acceptance criteria

- [ ] The instrumented run described above reports no "not wrapped in act" record for `FileViewer.test.tsx`.
- [ ] The instrumented run still reports no pending timer for that file at teardown.
- [ ] Every assertion the file makes about the code surface holds against the tree as it stands once the lazy
  import has resolved: re-reading the asserted DOM after the retry gives the same answer.
- [ ] The tests still fail when the diff layout control is offered for a one-sided change or withheld from a
  two-sided one, so they were not weakened into passing either way.
