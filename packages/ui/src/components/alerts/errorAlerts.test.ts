// @vitest-environment jsdom
import { expect, it } from "vitest";

import { errorAlerts } from "./errorAlerts";

// The jsdom environment is what makes test/setup.ts drain the queue after every test.

it("queues two alerts that differ only in their string title", () => {
  errorAlerts.report({ title: "Could not copy the path", message: "The clipboard could not be written to.", recovery: "clipboardRetry" });
  errorAlerts.report({ title: "Could not copy the project name", message: "The clipboard could not be written to.", recovery: "clipboardRetry" });
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not copy the path", "Could not copy the project name"]);
});

it("merges an alert that repeats a pending one, title included", () => {
  errorAlerts.report({ title: "Could not copy the path", message: "The clipboard could not be written to.", recovery: "clipboardRetry" });
  errorAlerts.report({ title: "Could not copy the path", message: "The clipboard could not be written to.", recovery: "clipboardRetry" });
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
});

it("treats an alert with a title and one without as different, and merges two without", () => {
  errorAlerts.report({ message: "Copy failed", recovery: "clipboardRetry" });
  errorAlerts.report({ message: "Copy failed", recovery: "clipboardRetry" });
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  errorAlerts.report({ title: "Could not copy the path", message: "Copy failed", recovery: "clipboardRetry" });
  expect(errorAlerts.getSnapshot()).toHaveLength(2);
});

it("queues a copy failure again once the identical pending one has been acknowledged", () => {
  const request = { title: "Could not copy the path", message: "Copy failed", recovery: "clipboardRetry" } as const;
  errorAlerts.report(request);
  errorAlerts.acknowledge();
  errorAlerts.report(request);
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
});
