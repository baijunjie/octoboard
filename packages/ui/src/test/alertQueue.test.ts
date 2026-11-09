// @vitest-environment jsdom
import { act, createElement, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { afterAll, afterEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { acknowledgeAllAlerts } from "./alertQueue";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterAll(() => {
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

describe("local queue assertions", () => {
  afterEach(() => {
    expect(errorAlerts.getSnapshot()).toHaveLength(1);
  });

  it("acknowledges the whole queue and leaves global cleanup until after both local hook kinds", () => {
    errorAlerts.report({ message: "First failure" });
    errorAlerts.report({ message: "Second failure" });
    acknowledgeAllAlerts();
    expect(errorAlerts.getSnapshot()).toEqual([]);
    errorAlerts.report({ message: "Left for global cleanup" });
    onTestFinished(() => expect(errorAlerts.getSnapshot().map((alert) => alert.message)).toEqual(["Left for global cleanup"]));
  });
});

describe("global queue cleanup", () => {
  it("leaves the teardown alert queued until the global cleanup runs", () => {
    expect(errorAlerts.getSnapshot()).toEqual([]);
    function ReportsOnUnmount() {
      useEffect(() => () => { errorAlerts.report({ message: "Unmount failure" }); }, []);
      return null;
    }
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => expect(errorAlerts.getSnapshot().map((alert) => alert.message)).toEqual(["Unmount failure"]));
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    act(() => root.render(createElement(ReportsOnUnmount)));
  });

  it("keeps cleaning the queue used by static imports after modules are reset", () => {
    expect(errorAlerts.getSnapshot()).toEqual([]);
    errorAlerts.report({ message: "Before module reset" });
    vi.resetModules();
    onTestFinished(() => expect(errorAlerts.getSnapshot().map((alert) => alert.message)).toEqual(["Before module reset"]));
  });

  it("starts with an empty queue", () => {
    expect(errorAlerts.getSnapshot()).toEqual([]);
  });
});
