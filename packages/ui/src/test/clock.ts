// The clock helpers every `packages/ui` test shares: the fake clock and its teardown, the one yield that lets queued
// promise work finish, and the one wait on the real clock.
import { act } from "react";
import { onTestFinished, vi } from "vitest";

/** The timer that really fires, kept from before any test fakes the clock, so `afterQueuedWork` still returns under it. */
const realSetTimeout = globalThis.setTimeout;

/** Vitest 5 uses sinon fake-timers: `getTimeToNextFrame` schedules frames at the next multiple of 16 ms. */
export const FAKE_FRAME_MS = 16;

/** Rounds `runOutTimers` runs before it gives up: more than the longest chain of timers and frames a component sets in
 * turn here, so a poll that re-arms itself for ever fails at the bound instead of hanging the run. */
const RUN_OUT_ROUNDS = 20;

/** Real time added to a library delay waited out on the real clock. The library's own timer is armed no later than the
 * wait starts, so the margin only covers the granularity of the host's timers, not the work between the two. */
const REAL_CLOCK_MARGIN_MS = 50;

/** Ticks to the last currently pending timer inside `act`; timers armed in turn that fall due before that point also run.
 * This may jump far forward. Use for an owner that keeps arming timers (a relative time ticker or a marquee),
 * after which a later step must stop it before `runOutTimers` can drain the clock. */
export function runPendingTimers(): void {
  act(() => void vi.runOnlyPendingTimers());
}

/**
 * Runs the fake clock's pending timers and animation frames out, and those they set in turn, inside `act`, until none
 * is left: react-aria hands focus over, and a popover's fade is written, a frame or two after a press or a close.
 * Throws when timers are still left after `RUN_OUT_ROUNDS`. It never uses `vi.runAllTimers()`, which does not end on a
 * timer that re-arms itself. Needs `fakeTimers()` first.
 */
export function runOutTimers(): void {
  for (let round = 0; round < RUN_OUT_ROUNDS && vi.getTimerCount() > 0; round += 1) runPendingTimers();
  if (vi.getTimerCount() > 0) throw new Error(`${vi.getTimerCount()} fake timers still pending after ${RUN_OUT_ROUNDS} rounds: one re-arms itself`);
}

/** What the running test's teardown runs before it drains, or `undefined` while no test has faked the clock. */
let beforeDrains: (() => void)[] | undefined;

/**
 * Fakes the clock for the rest of the test; call it before mounting, so that every timer the code arms is on it. When
 * the test finishes it runs `beforeDrain` (what unmounts what the test mounted), then the timers still pending, and
 * those they set in turn, and requires that none is left, before the real clock comes back: a timer left armed would
 * fire into the next test. Use `runOutTimers` to drain in the middle of a test.
 * A mid-test install is appropriate only before the operation arms the timers being tested; existing real timers
 * must have settled first. Pass `beforeDrain` to unmount owners that may keep arming timers.
 *
 * Called again in the same test, it installs the clock afresh with the new options, dropping pending timers;
 * the one teardown runs every `beforeDrain` given.
 *
 * Vitest runs `onTestFinished` hooks after `afterEach`, last registered first. A test whose pending timers need a
 * stub or a global (a stubbed `requestAnimationFrame`, a fake socket) must take it off in an `onTestFinished`
 * registered before `fakeTimers()`, not in `afterEach`, or the run-out fires its timers into the real thing.
 * `options.now` and `options.toFake` are those of `vi.useFakeTimers`.
 */
export function fakeTimers(options: { now?: number | Date; toFake?: NonNullable<Parameters<typeof vi.useFakeTimers>[0]>["toFake"]; beforeDrain?: () => void } = {}): void {
  const { beforeDrain, ...install } = options;
  vi.useFakeTimers(install);
  if (beforeDrains) {
    if (beforeDrain) beforeDrains.push(beforeDrain);
    return;
  }
  const mine: (() => void)[] = (beforeDrains = beforeDrain ? [beforeDrain] : []);
  onTestFinished(() => {
    try {
      act(() => void mine.forEach((run) => run()));
      runOutTimers();
    } finally {
      beforeDrains = undefined;
      vi.useRealTimers();
    }
  });
}

/**
 * One real 0 ms macrotask: it runs after every microtask already queued, so a chain of `await`s of any length, in the
 * code under test and in a mocked daemon's replies, has run to its end by then. It is an ordering yield, not a stretch
 * of time, for code that finishes on promise chains alone; anything that waits on a timer or a frame needs a fake clock
 * (`fakeTimers`) or its own wait. Not wrapped in `act`; `flushPromises` is, for what renders. It still returns where
 * the clock is faked.
 */
export function afterQueuedWork(): Promise<void> {
  return new Promise<void>((resolve) => realSetTimeout(resolve, 0));
}

/** `afterQueuedWork` inside `act`, so what the promises set in state is rendered when it returns. */
export function flushPromises(): Promise<void> {
  return act(afterQueuedWork);
}

/**
 * Waits `ms` of real time plus `REAL_CLOCK_MARGIN_MS`. Only for the named helpers that wait out a library delay the
 * test cannot fake (see "Tests that depend on time" in the agent-unit-test skill); a test calls those, not this.
 */
export function waitOnRealClock(ms: number): Promise<void> {
  return new Promise<void>((resolve) => realSetTimeout(resolve, ms + REAL_CLOCK_MARGIN_MS));
}

/**
 * Fakes timers while keeping `framesAsTimers`' 0 ms frame stub. A full fake clock replaces that stub with sinon's
 * 16 ms frames, changing the ordering a dialog's focus handoff needs.
 */
export function fakeTimersKeepingFrames(): void {
  fakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"] });
}

/** Whether sinon faked frames: fake-timers' `hijackMethod` attaches `clock` to each installed method (Vitest 5). */
export function framesFaked(): boolean {
  return typeof requestAnimationFrame === "function" && "clock" in requestAnimationFrame;
}
