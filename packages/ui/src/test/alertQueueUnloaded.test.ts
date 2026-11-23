// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { errorAlertsUnderTest } from "../components/alerts/errorAlertsUnderTest";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// This file never loads the alert queue, so the shared setup's cleanup has nothing to empty: it must still run after
// every test, and must not load the queue's module graph to find out (which would register the queue).
type Registered = Parameters<typeof errorAlertsUnderTest.add>[0];
const watcher = (() => {
  let acknowledged = 0;
  let queued = 1;
  const queue = {
    getSnapshot: () => ({ length: queued }),
    acknowledge: () => { queued -= 1; acknowledged += 1; },
  } as unknown as Registered;
  return { queue, acknowledged: () => acknowledged };
})();

// The order is pinned: each case checks what the setup's cleanup did after the one before.
describe("the cleanup in a file that never loads the queue", { shuffle: false }, () => {
  it("starts with no queue registered, so the cleanup of the first test has loaded nothing", () => {
    expect(errorAlertsUnderTest.size).toBe(0);
    // Stands for a queue the test loaded: the cleanup that follows this test has to reach it.
    errorAlertsUnderTest.add(watcher.queue);
  });

  it("has had the cleanup of the test before empty what was registered", () => {
    expect(watcher.acknowledged()).toBe(1);
    errorAlertsUnderTest.delete(watcher.queue);
  });

  it("still has nothing registered after the cleanups of the tests before, so none loaded the queue", () => {
    expect(errorAlertsUnderTest.size).toBe(0);
  });
});
