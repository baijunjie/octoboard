// @vitest-environment jsdom
import { describe, expect, it, onTestFinished } from "vitest";

import { errorAlertsUnderTest } from "../components/alerts/errorAlertsUnderTest";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// This file never imports the alert queue statically: the first test loads it with `import()`, as a test that mounts
// something lazily does, and the shared setup's cleanup still has to empty it before the next test.
const loadQueue = async () => (await import("../components/alerts/errorAlerts")).errorAlerts;

// The order is pinned: each case checks what the setup's cleanup did after the one before.
describe("the cleanup after a queue loaded inside a test", { shuffle: false }, () => {
  it("registers the queue for the cleanup when a test loads it, and leaves an alert raised there queued until the test ends", async () => {
    expect(errorAlertsUnderTest.size).toBe(0);
    const queue = await loadQueue();
    queue.report({ message: "Raised by a test that loaded the queue itself" });
    expect(errorAlertsUnderTest.has(queue)).toBe(true);
    expect(queue.getSnapshot()).toHaveLength(1);
    onTestFinished(() => expect(queue.getSnapshot()).toHaveLength(1));
  });

  it("finds the queue empty in the next test, though the file loaded it only inside the one before", async () => {
    const queue = await loadQueue();
    expect(queue.getSnapshot()).toEqual([]);
  });
});
