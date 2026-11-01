// @vitest-environment jsdom
import { expect, it, onTestFinished, vi } from "vitest";

import { forwardAlerts } from "./alertTestSupport";
import { errorAlerts } from "./errorAlerts";

/** Installs a forwarder to a fresh sink, taken down when the test finishes. */
function forwarded() {
  const sink = vi.fn();
  onTestFinished(forwardAlerts(sink));
  return sink;
}

it("forwards through a test's own mock of report once, which keeps doing what the test made it do", () => {
  const own = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
  onTestFinished(() => own.mockRestore());
  const sink = vi.fn();
  const stop = forwardAlerts(sink);
  errorAlerts.report({ message: "boom" });
  expect(sink.mock.calls).toEqual([["boom"]]);
  expect(own).toHaveBeenCalledTimes(1);
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  stop();
  // Back as the test left it once no host forwards.
  expect(errorAlerts.report).toBe(own);
  errorAlerts.report({ message: "later" });
  expect(sink).toHaveBeenCalledTimes(1);
  expect(own).toHaveBeenCalledTimes(2);
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
});

it("forwards again once vi.restoreAllMocks() has taken the forwarder away", () => {
  forwarded();
  vi.restoreAllMocks();
  const sink = forwarded();
  errorAlerts.report({ message: "boom" });
  expect(sink.mock.calls).toEqual([["boom"]]);
});

it("hands every host each message once and still queues the alert once", () => {
  const first = forwarded();
  const second = forwarded();
  errorAlerts.report({ message: "boom" });
  expect(first.mock.calls).toEqual([["boom"]]);
  expect(second.mock.calls).toEqual([["boom"]]);
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
});
