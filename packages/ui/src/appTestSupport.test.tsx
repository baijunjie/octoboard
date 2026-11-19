// @vitest-environment jsdom
import { expect, it, onTestFinished } from "vitest";

import { resetAppPreferences } from "./appTestSupport";
import { visibility } from "./layout/panelVisibility";
import { defaultStack } from "./panels/stackState";
import { closedReportsPreference, stacksPreference } from "./panels/usePanelStacks";
import { remoteDevicePreference } from "./remoteDaemons/remoteDevicePreference";
import { scheduledTaskPreference } from "./scheduledTasks/scheduledTaskPreference";
import { consolePreference } from "./store";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("resets every window preference held in memory after storage alone has been cleared", () => {
  onTestFinished(() => localStorage.clear());
  stacksPreference.set({ project: { open: [], shares: {} }, report: { open: [], shares: {} } });
  closedReportsPreference.set(["closed-session"]);
  consolePreference.set("other-console");
  remoteDevicePreference.set("other-device");
  scheduledTaskPreference.set("other-task");
  visibility.sidebar.set(false);
  visibility.aside.set(false);
  localStorage.clear();
  resetAppPreferences();
  expect(stacksPreference.get()).toEqual({ project: defaultStack("project"), report: defaultStack("report") });
  expect(closedReportsPreference.get()).toEqual([]);
  expect(consolePreference.get()).toBeUndefined();
  expect(remoteDevicePreference.get()).toBeUndefined();
  expect(scheduledTaskPreference.get()).toBeUndefined();
  expect(visibility.sidebar.get()).toBe(true);
  expect(visibility.aside.get()).toBe(true);
});
