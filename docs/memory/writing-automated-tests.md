# Writing automated tests

The rules for writing tests (lean cases, breaking the rule a test is named for, races, the fourth provider, real runs
and captures, time, jsdom teardown and window-wide state, fixtures) are in `docs/conventions/tests.md`; the helpers for
lazy surfaces and dialog focus say on their doc comments when to use them. What follows is how to carry some of them
out, and how to read the failures they prevent.

## Produce another tool's failure for real against a throwaway target

A failure the daemon reads from a tool such as a package manager is captured from a real run, not written from memory
(npm 10 ends a failure with an advice block and a pointer to its log, so "the last line that looks like an error" picks
a fragment that says nothing). Install into a `--prefix` under a temporary directory, made read-only for a permission
failure, and build the fixture from that output with the tool's version beside it.

## The load proof

Inject the delay before the assertion as a busy loop or `Atomics.wait` on a `SharedArrayBuffer`, never a `setTimeout`:
on a fake clock that never fires, so the test hangs instead of showing anything. Write `--sequence.shuffle=true` with
its value when file filters follow. A shape that has worked for a `packages/ui` test: the file run alone, three copies
of it run at once with `--sequence.shuffle=true --maxWorkers=4`, and a run with the delay.

## `fakeTimers()` fakes animation frames too

It fakes them as sinon's 16 ms frames (`FAKE_FRAME_MS`), so a frame never runs ahead of a 0 ms timer and a
`requestAnimationFrame` stub installed earlier is replaced. A test on `framesAsTimers()` that also fakes the clock does
so with `fakeTimersKeepingFrames()` (`packages/ui/src/test/clock.ts`).

## A `window is not defined` crash names the wrong file

Work a jsdom test left running fires after its file is over and is reported against whichever file was running then; it
does not reproduce when that file is run alone, and the count varies run to run. Look for a test anywhere in the package
that left work running, and judge the fix over repeated full runs (one such leak showed about 3 times in 18), never one
clean run. A "not wrapped in act" warning out of a passing file is the audible half of the same leak. The instrumented
run that hunts these wraps `globalThis.setTimeout`, which a fake clock replaces, so it reports nothing for a test on a
fake clock; such a test is covered only by `fakeTimers()`'s teardown, which fails on any timer left.
