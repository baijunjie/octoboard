# Writing automated tests

## Write only the unit tests a rule needs, one case per rule

The user wants tests lean and has had over-written ones trimmed. Test a module's rules, not every variant of them:
one representative case per rule, in a table (`it.each`) when several rules share one shape, rather than a
`describe` block per behaviour with a handful of near-duplicate cases each.

## Check a new test by breaking the rule it is named for

Before calling a test done, make the rule its name claims fail — empty the constant the production path reads, feed it
the input the name says is ignored, delete the guard — and confirm that this test, not some other, reports it. Two
shapes pass otherwise and both get written here: a case whose input never contains the thing the name says is ignored,
which asserts nothing about ignoring it; and a list or allowlist a production path depends on that no test pins, so
emptying it leaves the whole suite green.

Run that check per element where the input is a shared fixture standing for a set — a table of variables, of payloads,
of flags — rather than once over the fixture as a whole: drop each element in turn and watch the assertion fail. An
aggregate assertion passes on behalf of whichever elements it is actually sensitive to, so one element that no
assertion can see, or whose payload turns out to be inert against the fixture's own shape, is decoration, and the
fixture's claim to cover the set is what the next person will trust.

A third shape is this window's tooltips: only one is ever open at a time (react-aria keeps one open tooltip for
the whole window), so an assertion that counts them — "exactly one `[role=tooltip]` is up" — cannot fail when a
mark gains a tooltip it should not have, because the new trigger replaces the open one instead of adding a second.
Every check of a tooltip, in a jsdom test as in a gallery hover step, names whose tooltip is up from its text, and
where none should open asserts that there is none.

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

## Seed the agent's model list before a daemon test gives anything a model or an effort

Applies to any daemon test — coordinator, control socket or console session tool — that creates or edits a console
or a project, or opens a session, with a model or a reasoning effort in it. The daemon checks every such value
against the list the agent offers, and for Codex and Grok that list comes from running the CLI installed on the
machine; a list that cannot be read checks nothing. So an unseeded test passes on a machine without the CLI, or
whose catalogue happens to hold the test's model, and fails on another — a made-up model id like `gpt-y` is refused
wherever the CLI is installed. Call `test_support::offer_models` for that agent before the call under test, with the
models and efforts the test uses. It seeds the default account only; a test that gives a model under another account
is checked against that account's own list.

## Test a daemon budget by lowering it, never by asserting its value

Applies to daemon tests of a limit in `apps/daemon/src/browse/budget.rs` whose real figure no small fixture reaches. A
test asserting the constant equals its figure fails when the figure is retuned and passes when the check enforcing it
is deleted, so it tests nothing. Have the production code read the limit through a getter that a `#[cfg(test)]`
`thread_local!` override can lower — `tree_diff::tree_diff_stdout` with `TREE_DIFF_STDOUT_OVERRIDE` is the shape — set
the override around the call in the test, clear it after, and assert what happens past the limit. The override is per
thread, so it reaches only code running on the test's own thread: the browse fixtures' `serve` calls in directly,
while anything moved onto another thread (`spawn_blocking`, a request through the coordinator) reads the real figure.

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

Where the code classifies an agent's record line by line — which lines are the person speaking, which
are the conversation — one session's capture is not enough, and a hand-written fixture is worth
nothing: both hold only the shapes you already expected. Before writing the reader, survey every
record of that agent on the machine, across all the CLI versions they span and including long,
compacted sessions: tally each distinct value of the fields that tell lines apart (`type`, `origin`,
attachment and synthetic-line markers, event types) and build the capture from a real line for every
value the reader must keep or drop. A reader built otherwise here passed its tests while missing
messages typed mid-turn, which carry no user line at all, taking background-task notifications for
the user, and losing the part of a conversation that compaction moved out of the record file.

## A jsdom test leaves nothing running, and the cleanup is bound to the test that armed it

Applies to the `packages/ui` component tests (`vitest` under jsdom). Work a test leaves running outlives the file:
the timer fires once the file's tests are over, its `setState` lands outside `act`, React hands the update to the
real scheduler, and the environment is disposed before that work runs — the scheduler then dies reading `window`,
and vitest reports an unhandled `ReferenceError: window is not defined` with the warning that it "might cause false
positive tests".

- Clearing the document (`document.body.replaceChildren()`) is not teardown: it leaves the tree mounted, so no
  effect cleanup runs and the timers the tree armed stay armed — a list's status region (`StatusAnnouncer`) delays
  its first write by one. Unmount the root instead, which also takes a popover portalled to `<body>` with it.
- Register the cleanup inside the mounting helper with `onTestFinished`, bound to the test that mounted the root,
  rather than as a file-level `afterEach` the next file has to remember to write. Unmounting is itself a render, so
  it goes through `act`.
- A root is not the only thing a test arms. Whatever it set going it also stops or awaits before it ends — a
  module-level singleton's own queue and timers, such as HeroUI's toast queue, which a later unmount cannot reach.
- Settling HeroUI's toast queue in a test that has queued a toast: fake the timers for that test (restored with
  `onTestFinished`), run `toast.clear()` and `vi.runOnlyPendingTimers()` together inside one `act`, and assert after
  the unmount that `toast.getQueue().visibleToasts` is empty. The assertion is the load-bearing half rather than the
  fake timers: faking alone moves the queue's exits onto a virtual clock that `vi.useRealTimers()` then discards,
  which hides the leak instead of settling it. An empty `visibleToasts` covers the queue's own timers too, because
  `close()` parks one per toast and a toast leaves `visibleToasts` only inside that timer's callback. Two near-misses
  to turn down: the queue's exported `destroy()`, advertised for HMR and test teardown, clears those timers but
  leaves every toast in `visibleToasts`; and having the test's `matchMedia` stub answer `true` to
  `(prefers-reduced-motion: reduce)` takes a synchronous close path needing no timers at all, but stops exercising
  the path the app runs.
- A test that fakes timers can show it left none pending only by asserting `vi.getTimerCount()` is 0 itself. The
  instrumented run this class of leak is hunted with wraps `globalThis.setTimeout`, which `vi.useFakeTimers()`
  replaces, so it reports nothing pending for such a test whether or not that test ever ran its timers.

The symptom names the wrong file, so do not start from the one it is reported against: the crash is attributed to
whichever file was running when the stray work fired, it does not reproduce when that file is run alone, and the
count varies run to run because it is a race. Look instead for a test anywhere in the package that left work
running, and judge the fix over repeated full runs — one such leak showed up about 3 times in 18 — never over a
single clean one. A "not wrapped in act" warning out of a file that passes is the audible half of the same leak and
is tracked down the same way, not shrugged at.

## Settle a lazily imported surface before reading it, and never take a clean whole-file run as evidence

Applies to the `packages/ui` component tests (`vitest` under jsdom) that render a component sitting behind a
`React.lazy` boundary — the viewer's code surface and its Markdown document, the project browser.

- A test that renders such a surface and then asserts synchronously reads the `Suspense` fallback rather than what
  the renderer draws, so it passes or fails on whether the node it happens to look at sits outside the boundary;
  React also retries the suspended render on its own afterwards, outside `act`.
- Settle the import after the first render that needs it, inside `act`, around a **real** `setTimeout(resolve, 0)`:
  it lands several microtask hops after that render, so a bare `await` does not reach it. A test under
  `vi.useFakeTimers()` therefore cannot settle this way and has to assert only on what sits outside the boundary,
  and for the same reason the settle must not be folded into a shared render helper that such tests also call.
- A `lazy` object caches its resolution for the module's lifetime, so only the **first** test in a file that renders
  that object ever suspends; every later one is rescued by it and passes on file order alone. A clean whole-file run
  therefore says nothing — check a test of this kind by running it on its own (`vitest run <file> -t "<name>"`), which
  leaves the rest skipped and the cache cold. The caching is per `lazy` object, not per module: two exports built with
  `lazy` give one file two separate first tests, and a test that renders one surface and then the other settles twice.

## A dialog test waits out the dialog's own mount refocus before any step whose focus it asserts

Applies to `packages/ui` component tests (`vitest` under jsdom) that mount a modal — every HeroUI `Modal` and
`AlertDialog`, so `Dialog` and `ConfirmDialog` too, is react-aria's `Dialog` and so runs `useDialog`. Half a second
after mounting, unless a control inside already had focus, `useDialog` moves focus by itself: if focus is then on the
dialog or on `<body>` it blurs the dialog and focuses it again, one frame later under virtual clicks. Focus passes
through `<body>` whenever a select's list closes on its way back to the trigger, so a step around that moment loses
its focus to the dialog, and whether a step meets it depends only on how fast the test ran up to it: the test passes
alone and fails under the full suite's load.

Call `trackMountRefocus()` (`packages/ui/src/dialogs/dialogTestSupport.ts`) right after the mount, and await what it
returns before the first step whose focus the test asserts — immediately after the mount is simplest. Skip the await
in a test on `vi.useFakeTimers()`, where it would never resolve.

## A jsdom test cannot tell where focus ends after a session is selected

Applies to `packages/ui` component tests, which run under jsdom without the terminal. In the app, selecting a live
session attaches the terminal to it, and that attach (`attach` in `packages/ui/src/terminal/TerminalController.ts`)
focuses the terminal after whatever the sidebar or a hook did with focus. So a jsdom test asserting that focus stays
on a sidebar control after a selection passes while the app does the opposite. Assert focus in jsdom only for moves
that select no session; judge where focus lands after a selection in the real app.

## A jsdom test cannot tell what a selection over the viewer's code copies

Applies to the `packages/ui` viewer tests (`vitest` under jsdom), which mock the rendering library
(`vi.mock("./renderer")`) with a plain `<pre>` in the light DOM. The library itself draws a file's code inside a
shadow root of its own, which a document selection does not enter, so the mock takes away the one condition that
decides what a Select All or a copy over a code frame — or over a rendered document that holds fenced blocks — puts
on the clipboard: a handler that copies the prose and a gap where every block of code was passes its jsdom test, and
a browser check that only shows the selection scoped to the right region passes as well.

So assert in jsdom only what such a handler does to the DOM, and settle what a copy carries by copying for real in a
browser over the page and reading the clipboard back.

## A jsdom test's fixture must not stand in for what is under test

Applies to the `packages/ui` component tests (`vitest` under jsdom). Where the fixture models the geometry or the
wiring that the rule under test is about, every case passes whatever the production code does, and the rule can be
deleted or moved to the wrong place without a single failure.

- **Tokens and geometry**: jsdom loads no stylesheet, so `getComputedStyle(...).getPropertyValue("--x")` is `""` and
  `parseFloat` of it `NaN`. Set every custom property the code under test reads on `document.documentElement` in the
  test itself, and keep stubbed element geometry (`offsetLeft`, `getBoundingClientRect`) at what the current CSS lays
  out — a fixture left at the layout from before a change exercises geometry the app no longer has. Have the code
  under test throw on a token that does not parse rather than defaulting (`|| 0`), so an unset token fails loudly
  instead of silently becoming the geometry the rule exists to correct.
- **Wiring**: render the real component that owns the behaviour, and the real control the behaviour keys on, rather
  than a test host that calls the hook and attaches the listeners itself. Which component has to call the hook, and
  what the control has to carry for a selector to find it, are part of the rule; against a hand-wired host they are
  invisible, and the whole file stays green with the call back on the wrong element.
