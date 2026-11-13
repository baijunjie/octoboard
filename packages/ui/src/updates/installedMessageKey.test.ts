import { expect, it } from "vitest";

import { installedMessageKey } from "./installedMessageKey";

it.each([
  ["in-place", true, "updates.notice.installed"],
  ["in-place", false, "updates.notice.installedUnknown"],
  ["installer", true, "updates.notice.downloaded"],
  ["installer", false, "updates.notice.downloadedUnknown"],
] as const)("names the notice's message for an update of a %s updater, version known %s, as %s when nothing blocks the restart", (updater, known, key) => {
  expect(installedMessageKey(updater, "notice", known, false)).toBe(key);
});

it.each([
  ["in-place", true, "settings.updates.version.installed"],
  ["in-place", false, "settings.updates.version.installedUnknown"],
  ["installer", true, "settings.updates.version.downloaded"],
  ["installer", false, "settings.updates.version.downloadedUnknown"],
] as const)("names the row's message for an update of a %s updater, version known %s, as %s when nothing blocks the restart", (updater, known, key) => {
  expect(installedMessageKey(updater, "settings", known, false)).toBe(key);
});

it.each(["in-place", "installer", "none"] as const)("names the notice's sentence about the agent's update, with the version or without, whatever the updater is (%s)", (updater) => {
  expect(installedMessageKey(updater, "notice", true, true)).toBe("updates.notice.installedAgentUpdate");
  expect(installedMessageKey(updater, "notice", false, true)).toBe("updates.notice.installedAgentUpdateUnknown");
});

it.each(["in-place", "installer", "none"] as const)("names the row's sentence about the agent's update, with the version or without, whatever the updater is (%s)", (updater) => {
  expect(installedMessageKey(updater, "settings", true, true)).toBe("settings.updates.version.installedAgentUpdate");
  expect(installedMessageKey(updater, "settings", false, true)).toBe("settings.updates.version.installedAgentUpdateUnknown");
});
