# Writing automated tests

## Write only the unit tests a rule needs, one case per rule

The user wants tests lean and has had over-written ones trimmed. Test a module's rules, not every variant of them:
one representative case per rule, in a table (`it.each`) when several rules share one shape, rather than a
`describe` block per behaviour with a handful of near-duplicate cases each.

When trimming existing tests as redundant: a test that pins a past defect is not redundant just because another test
covers the same code path — keep it unless the user agrees to drop it; a merge into a table keeps every case and every
side assertion the merged tests made, or the test that holds one stays as it is; and a test that cannot fail as named
is not deleted as a duplicate but given the missing assertion or renamed to what it does check.

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

A test that something is refused asserts the refuser's own message as well as the failure. The operation the test
drives may be refused by something else first, and the test then passes with the guard gone: git refuses
`git branch -f` on a branch checked out in a worktree before any hook runs, so a hook test that drives it proves
nothing about the hook.

A jsdom lookup that finds nothing does not fail either: `find(...)!` checks nothing at runtime, and a negative
assertion on what it returned holds whatever the markup — `parent.contains(undefined)` is `false`, so "the email is not
inside the name's element" passed with the email anywhere. Such a lookup comes back empty unnoticed under a stand-in
that renders its children as bare text, as the `AgentAccountsSection` tests' `FadeOverflow` and `MarkedPath` stand-ins
(`packages/ui/src/settings/accounts/agentAccountsSectionTestState.tsx`) do: the text they draw has no element of its own.
Assert that the lookup found something before asserting about it, or assert on the text of an element the markup
still has (the row, the name line) rather than on a leaf found by its text.

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

What the UI reads back out of a session's terminal buffer — the links it finds in the output — is
checked the same way before its tests are trusted: replay a real capture's bytes into a real
`@xterm/xterm` `Terminal`, at more than one width, and see which rows it gives; the committed tests
may then build rows by hand, in the shapes the replay showed. Two shapes are missed otherwise. An
agent that wraps its own text — Claude Code does, with a real line break and the paragraph's indent —
leaves rows xterm does not mark as wrapped. And a session's output is replayed into the terminal at
its current width, not the width the program laid it out for, so a row "reaching the last column"
says nothing about whether the program broke the line there.

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
- react-aria's tooltip warm-up is such a singleton, with no way to reset it: showing any tooltip sets one flag for the
  whole window, and closing it leaves the flag set for 500 ms (`TOOLTIP_COOLDOWN` in react-stately 3.50.0's
  `useTooltipTriggerState`), during which the next tooltip opens with no delay. So a test that counts on a tooltip
  still waiting out its delay starts cold itself — waits 600 ms of real time first, before faking any timers, as the
  "still waiting to show" case in `packages/ui/src/components/menus/ActionMenu.test.tsx` does — or it passes alone and fails
  after any test that closed a tooltip. A test that fakes timers and shows a tooltip runs them out before restoring
  real ones, as `finish` in `packages/ui/src/components/hint/AnchoredTooltip.test.tsx` does, or the flag stays set for the
  rest of the file.
- A timer armed before `vi.useFakeTimers()` stays on the real clock, and advancing the fake one never fires it. The
  window's status region (`StatusAnnouncer`) arms its first-write delay (`FIRST_WRITE_DELAY_MS`) when it mounts and
  writes nothing until it has passed, so a test that mounts `App`, or any component holding a status region, and only
  then fakes timers to drive an announcement sees the region stay empty whenever less than that delay passed in real
  time: it passes under the full suite's load and fails alone. Fake the clock before mounting, or wait the delay out
  on the real clock inside `act` before faking, as `firstWriteDelayPassed` in
  `packages/ui/src/App.projectActions.test.tsx` does.
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

Call `trackMountRefocus()` (`packages/ui/src/components/dialog/dialogTestSupport.ts`) right after the mount, and await what it
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
