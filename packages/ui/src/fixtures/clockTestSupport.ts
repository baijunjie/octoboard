// The fixtures' step drivers over the fake clock `fakeTimers` (`../test/clock`) installs: they fire its timers one at a
// time until a step settles.
import { act } from "react";
import { vi } from "vitest";

/** The most timers `settle` fires for one step: a step polls every 50 ms for seconds, so it needs a few hundred; one
 * that outruns this is stuck, not slow. */
const MAX_TIMERS = 5000;

/** Fires the fake clock's timers one at a time until `pending` settles, then returns what it settled to (or throws what it
 * rejected with), so a step that polls or waits is run to its end without the test guessing how long that takes. `commit`
 * wraps each firing: a React test passes `act`, so what one timer did is committed and on the page before the next fires,
 * as a browser's clock would leave it. Fails loudly where the step waits on no timer, or on more than `MAX_TIMERS`. */
export async function settle<T>(pending: Promise<T>, commit: (fire: () => Promise<void>) => PromiseLike<unknown> = (fire) => fire()): Promise<T> {
  let finished = false;
  const done = () => void (finished = true);
  // This also handles a rejection of `pending`, so where `settle` gives up and leaves it, a later one is not reported as unhandled.
  pending.then(done, done);
  for (let fired = 0; !finished; fired++) {
    if (fired === MAX_TIMERS) throw new Error(`The step was still running after ${MAX_TIMERS} timers`);
    if (vi.getTimerCount() === 0) {
      // Let what the step awaits settle before concluding that nothing will move it.
      await commit(() => vi.advanceTimersByTimeAsync(0).then(() => undefined));
      if (!finished && vi.getTimerCount() === 0) throw new Error("The step is waiting on no timer");
      continue;
    }
    await commit(() => vi.advanceTimersToNextTimerAsync().then(() => undefined));
  }
  return pending;
}

/** `settle` for a React test: each timer fires inside its own `act`, so what it did (a tooltip opening, a popover
 * mounting) is committed and on the page before the next fires, and a step that looks at it sees it, with no guess at how
 * far the clock was advanced in one go. */
export const settleInAct = (pending: Promise<unknown>): Promise<void> => settle(pending, (fire) => act(fire)).then(() => undefined);
